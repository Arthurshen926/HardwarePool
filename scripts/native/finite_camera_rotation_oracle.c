/*
 * Hosted-only, hardware-free pixel oracle for FiniteCameraTransform.
 * The GLSL, interleaved vertices, and all three matrices are exported by the
 * dependency-free Java contract test from the SAME production class used by
 * Android. There is intentionally no shader or production-matrix copy here.
 *
 * Independent reference: discrete inverse pixel rotations, an ideal 405x720
 * portrait fit in the 1280x720 frame (left=437.5), and AOSP no-mirror producer rotation/vertical-flip/filter-inset semantics.
 * CPU reference code never reads the exported geometry/texture/producer matrices.
 * Every output pixel and alpha is checked. Flat source neighborhoods and bars
 * are byte-exact; bilinear transitions allow <=3 RGB levels for implementation
 * rounding. Only pixel centers exactly on the two half-pixel fit edges may be
 * either opaque black or sampled color, and every row must contain exactly 405
 * nonblack pixels. No pixel is omitted from checking.
 *
 * Scope: Mesa llvmpipe EGL/GLES2 algorithm evidence, NOT Android SurfaceTexture
 * producer semantics, Camera2/MediaCodec behavior, device pixels, or performance.
 * EGLImage import uses synthetic RGBA, not Android native/YUV buffers.
 */
#define _POSIX_C_SOURCE 200809L
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <GLES2/gl2ext.h>
#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define WIDTH 1280
#define HEIGHT 720
#define CASES 24
#define PIXELS ((size_t)WIDTH * HEIGHT)
#define BYTES (PIXELS * 4)
#define EDGE_TOLERANCE 3

struct test_case {
    char name[80];
    int front, sensor, rotation, producer;
    GLfloat geometry[16], texture[16], producer_matrix[16];
};
struct counts {
    size_t exact, interpolated, boundary, bars, alpha_errors, rgb_errors, rows;
};
struct renderer {
    EGLDisplay display;
    EGLContext context;
    EGLSurface surface;
    EGLImageKHR image;
    PFNEGLDESTROYIMAGEKHRPROC destroy_image;
    GLuint program, source_texture, external_texture;
    GLint geometry, texture, producer;
};
static const char *producer_names[] = {"raw", "inset1", "inset2"};
static unsigned char *pattern, *actual;

static void fail(const char *message) {
    fprintf(stderr, "finite_camera_rotation: FAIL %s\n", message);
    exit(EXIT_FAILURE);
}
static void require(int condition, const char *message) {
    if (!condition) fail(message);
}
static void gl_check(const char *where) {
    GLenum error = glGetError();
    if (error != GL_NO_ERROR) {
        fprintf(stderr, "finite_camera_rotation: GL error 0x%x at %s\n", error, where);
        exit(EXIT_FAILURE);
    }
}
static void egl_check(EGLBoolean result, const char *where) {
    if (result != EGL_TRUE) {
        fprintf(stderr, "finite_camera_rotation: EGL error 0x%x at %s\n", eglGetError(), where);
        exit(EXIT_FAILURE);
    }
}
static int extension(const char *list, const char *name) {
    if (!list) return 0;
    size_t length = strlen(name);
    for (const char *p = list; (p = strstr(p, name)) != NULL; p += length) {
        if ((p == list || p[-1] == ' ') && (p[length] == ' ' || p[length] == '\0')) return 1;
    }
    return 0;
}
static void path_join(char *out, size_t capacity, const char *dir, const char *name) {
    int n = snprintf(out, capacity, "%s/%s", dir, name);
    require(n > 0 && (size_t)n < capacity, "artifact path too long");
}
static char *read_source(const char *dir, const char *name) {
    char path[4096];
    path_join(path, sizeof(path), dir, name);
    FILE *file = fopen(path, "rb");
    require(file != NULL, "missing exported shader");
    require(fseek(file, 0, SEEK_END) == 0, "shader seek failed");
    long length = ftell(file);
    require(length > 0 && length <= 65536, "invalid exported shader length");
    rewind(file);
    char *source = malloc((size_t)length + 1);
    require(source != NULL, "shader allocation failed");
    require(fread(source, 1, (size_t)length, file) == (size_t)length, "shader read failed");
    require(fclose(file) == 0, "shader close failed");
    require(memchr(source, '\0', (size_t)length) == NULL, "embedded NUL in shader");
    source[length] = '\0';
    return source;
}
static int parse_integer(const char *text) {
    char *end = NULL;
    errno = 0;
    long number = strtol(text, &end, 10);
    require(errno == 0 && end != text && *end == '\0' && number >= 0 && number <= 360,
            "invalid case integer");
    return (int)number;
}
static GLfloat parse_float(const char *text) {
    char *end = NULL;
    errno = 0;
    float number = strtof(text, &end);
    require(errno == 0 && end != text && *end == '\0' && isfinite(number),
            "invalid exported finite float");
    return number;
}
static void read_inputs(const char *dir, GLfloat vertices[16], struct test_case cases[CASES]) {
    char path[4096], line[8192];
    path_join(path, sizeof(path), dir, "vertices.txt");
    FILE *file = fopen(path, "r");
    require(file != NULL, "missing exported vertices");
    for (size_t i = 0; i < 16; ++i) {
        char word[80];
        require(fscanf(file, "%79s", word) == 1, "missing exported vertex component");
        vertices[i] = parse_float(word);
    }
    char trailing;
    require(fscanf(file, " %c", &trailing) == EOF, "unexpected extra vertices");
    require(fclose(file) == 0, "vertices close failed");
    path_join(path, sizeof(path), dir, "cases.tsv");
    file = fopen(path, "r");
    require(file != NULL, "missing exported cases");
    require(fgets(line, sizeof(line), file) != NULL, "missing case header");
    require(strcmp(line, "# name facing sensor rotation producer geometry[16] texture[16] producer[16]\n") == 0,
            "unexpected case schema");
    unsigned seen[2][4][3] = {{{0}}};
    size_t count = 0;
    while (fgets(line, sizeof(line), file)) {
        require(strchr(line, '\n') != NULL && count < CASES, "oversized or extra case");
        char *tokens[53], *save = NULL, *word = strtok_r(line, " \t\r\n", &save);
        size_t n = 0;
        while (word) {
            require(n < 53, "extra case fields");
            tokens[n++] = word;
            word = strtok_r(NULL, " \t\r\n", &save);
        }
        require(n == 53, "missing case fields");
        struct test_case *test = &cases[count++];
        require(strlen(tokens[0]) < sizeof(test->name), "case name too long");
        require(strspn(tokens[0], "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-")
                == strlen(tokens[0]), "unsafe case name");
        strcpy(test->name, tokens[0]);
        require(strcmp(tokens[1], "front") == 0 || strcmp(tokens[1], "back") == 0,
                "unknown camera facing");
        test->front = strcmp(tokens[1], "front") == 0;
        test->sensor = parse_integer(tokens[2]);
        test->rotation = parse_integer(tokens[3]);
        require(test->sensor < 360 && test->sensor % 90 == 0, "non-cardinal sensor");
        require(test->rotation == test->sensor,
                "exported rotation violates independent MirrorNONE sensor convention");
        test->producer = -1;
        for (int p = 0; p < 3; ++p) if (strcmp(tokens[4], producer_names[p]) == 0) test->producer = p;
        require(test->producer >= 0, "unknown producer transform");
        require(!seen[test->front][test->sensor / 90][test->producer]++, "duplicate coverage case");
        for (size_t i = 0; i < 16; ++i) {
            test->geometry[i] = parse_float(tokens[5 + i]);
            test->texture[i] = parse_float(tokens[21 + i]);
            test->producer_matrix[i] = parse_float(tokens[37 + i]);
        }
    }
    require(!ferror(file) && fclose(file) == 0 && count == CASES, "24 coverage cases are mandatory");
}
static GLuint compile_shader(GLenum type, const char *source) {
    GLuint shader = glCreateShader(type);
    glShaderSource(shader, 1, &source, NULL);
    glCompileShader(shader);
    GLint success = 0;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &success);
    if (!success) {
        char log[4096];
        glGetShaderInfoLog(shader, sizeof(log), NULL, log);
        fprintf(stderr, "production shader compilation: %s\n", log);
        fail("production shader compile failed");
    }
    gl_check("compile production shader");
    return shader;
}
static void make_pattern(void) {
    pattern = malloc(BYTES);
    actual = malloc(BYTES);
    require(pattern && actual, "pixel allocation failed");
    for (int y = 0; y < HEIGHT; ++y) for (int x = 0; x < WIDTH; ++x) {
        unsigned char *p = pattern + ((size_t)y * WIDTH + x) * 4;
        /* An asymmetric colored grid, not a rotationally symmetric checkerboard. */
        p[0] = (unsigned char)(40 + ((x / 80) * 23 + (y / 60) * 13) % 160);
        p[1] = (unsigned char)(48 + ((x / 80) * 11 + (y / 60) * 29) % 152);
        p[2] = (unsigned char)(56 + ((x / 80) * 31 + (y / 60) * 7) % 144);
        if (x % 80 < 3 || y % 60 < 3) { p[0] = 216; p[1] = 216; p[2] = 216; }
        /* Four dissimilar corner blocks retain broad exact interior probes. */
        if (x < 192 && y < 144) { p[0] = 232; p[1] = 32; p[2] = 40; }
        if (x >= 1088 && y < 144) { p[0] = 40; p[1] = 224; p[2] = 56; }
        if (x < 192 && y >= 576) { p[0] = 48; p[1] = 72; p[2] = 232; }
        if (x >= 1088 && y >= 576) { p[0] = 232; p[1] = 208; p[2] = 32; }
        /* Off-center F with unequal arms plus a separate offset dot. */
        if ((x >= 344 && x < 368 && y >= 390 && y < 582)
                || (x >= 344 && x < 496 && y >= 558 && y < 582)
                || (x >= 344 && x < 448 && y >= 474 && y < 498)
                || (x >= 530 && x < 562 && y >= 418 && y < 450)) {
            p[0] = 248; p[1] = 40; p[2] = 208;
        }
        p[3] = 255;
    }
}
static void init_renderer(struct renderer *r, const char *dir, const GLfloat vertices[16]) {
    const char *client_extensions = eglQueryString(EGL_NO_DISPLAY, EGL_EXTENSIONS);
    require(extension(client_extensions, "EGL_MESA_platform_surfaceless"), "missing EGL_MESA_platform_surfaceless");
    require(extension(client_extensions, "EGL_EXT_platform_base"), "missing EGL_EXT_platform_base");
    PFNEGLGETPLATFORMDISPLAYEXTPROC get_display =
            (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
    PFNEGLCREATEIMAGEKHRPROC create_image =
            (PFNEGLCREATEIMAGEKHRPROC)eglGetProcAddress("eglCreateImageKHR");
    r->destroy_image = (PFNEGLDESTROYIMAGEKHRPROC)eglGetProcAddress("eglDestroyImageKHR");
    PFNGLEGLIMAGETARGETTEXTURE2DOESPROC image_target =
            (PFNGLEGLIMAGETARGETTEXTURE2DOESPROC)eglGetProcAddress("glEGLImageTargetTexture2DOES");
    require(get_display && create_image && r->destroy_image && image_target, "missing EGLImage entry point");
    r->display = get_display(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    require(r->display != EGL_NO_DISPLAY, "surfaceless display unavailable");
    EGLint major, minor;
    egl_check(eglInitialize(r->display, &major, &minor), "initialize surfaceless software display");
    const char *vendor = eglQueryString(r->display, EGL_VENDOR);
    require(vendor && strstr(vendor, "Mesa"), "non-Mesa EGL vendor forbidden");
    const char *egl_extensions = eglQueryString(r->display, EGL_EXTENSIONS);
    require(extension(egl_extensions, "EGL_KHR_image_base"), "missing EGL_KHR_image_base");
    require(extension(egl_extensions, "EGL_KHR_gl_texture_2D_image"), "missing EGL_KHR_gl_texture_2D_image");
    egl_check(eglBindAPI(EGL_OPENGL_ES_API), "bind GLES API");
    const EGLint config_attrs[] = {EGL_SURFACE_TYPE, EGL_PBUFFER_BIT, EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT,
        EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8, EGL_BLUE_SIZE, 8, EGL_ALPHA_SIZE, 8,
        EGL_DEPTH_SIZE, 0, EGL_STENCIL_SIZE, 0, EGL_SAMPLE_BUFFERS, 0, EGL_NONE};
    EGLConfig config;
    EGLint configs = 0;
    egl_check(eglChooseConfig(r->display, config_attrs, &config, 1, &configs), "choose RGBA8 config");
    require(configs == 1, "RGBA8 pbuffer config unavailable");
    const EGLint attributes[] = {EGL_RED_SIZE, EGL_GREEN_SIZE, EGL_BLUE_SIZE, EGL_ALPHA_SIZE};
    for (size_t i = 0; i < 4; ++i) {
        EGLint size = 0;
        egl_check(eglGetConfigAttrib(r->display, config, attributes[i], &size), "read color precision");
        require(size == 8, "exact RGBA8 framebuffer required");
    }
    const EGLint pbuffer_attrs[] = {EGL_WIDTH, WIDTH, EGL_HEIGHT, HEIGHT, EGL_NONE};
    const EGLint context_attrs[] = {EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE};
    r->surface = eglCreatePbufferSurface(r->display, config, pbuffer_attrs);
    require(r->surface != EGL_NO_SURFACE, "pbuffer creation failed");
    r->context = eglCreateContext(r->display, config, EGL_NO_CONTEXT, context_attrs);
    require(r->context != EGL_NO_CONTEXT, "GLES2 context creation failed");
    egl_check(eglMakeCurrent(r->display, r->surface, r->surface, r->context), "make software context current");
    const char *renderer = (const char *)glGetString(GL_RENDERER);
    const char *version = (const char *)glGetString(GL_VERSION);
    require(renderer && strstr(renderer, "llvmpipe"), "llvmpipe software renderer required");
    require(version && strstr(version, "OpenGL ES"), "GLES context required");
    require(extension((const char *)glGetString(GL_EXTENSIONS), "GL_OES_EGL_image_external"),
            "missing GL_OES_EGL_image_external (no skip permitted)");
    fprintf(stderr, "finite_camera_rotation: EGL %d.%d vendor=%s renderer=%s GL=%s\n", major, minor, vendor, renderer, version);
    char *vertex_source = read_source(dir, "vertex.glsl");
    char *fragment_source = read_source(dir, "fragment.glsl");
    GLuint vertex = compile_shader(GL_VERTEX_SHADER, vertex_source);
    GLuint fragment = compile_shader(GL_FRAGMENT_SHADER, fragment_source);
    r->program = glCreateProgram();
    glAttachShader(r->program, vertex);
    glAttachShader(r->program, fragment);
    glLinkProgram(r->program);
    GLint linked = 0;
    glGetProgramiv(r->program, GL_LINK_STATUS, &linked);
    if (!linked) {
        char log[4096];
        glGetProgramInfoLog(r->program, sizeof(log), NULL, log);
        fprintf(stderr, "production shader link: %s\n", log);
        fail("production shader link failed");
    }
    glDeleteShader(vertex);
    glDeleteShader(fragment);
    free(vertex_source);
    free(fragment_source);
    glUseProgram(r->program);
    GLint position = glGetAttribLocation(r->program, "aPosition");
    GLint uv = glGetAttribLocation(r->program, "aTextureCoord");
    r->geometry = glGetUniformLocation(r->program, "uGeometryMatrix");
    r->texture = glGetUniformLocation(r->program, "uTextureMatrix");
    r->producer = glGetUniformLocation(r->program, "uProducerMatrix");
    GLint sampler = glGetUniformLocation(r->program, "sTexture");
    require(position >= 0 && uv >= 0 && r->geometry >= 0 && r->texture >= 0 && r->producer >= 0 && sampler >= 0,
            "missing production shader attribute/uniform");
    glVertexAttribPointer((GLuint)position, 2, GL_FLOAT, GL_FALSE, 4 * sizeof(GLfloat), vertices);
    glVertexAttribPointer((GLuint)uv, 2, GL_FLOAT, GL_FALSE, 4 * sizeof(GLfloat), vertices + 2);
    glEnableVertexAttribArray((GLuint)position);
    glEnableVertexAttribArray((GLuint)uv);
    glActiveTexture(GL_TEXTURE0);
    glGenTextures(1, &r->source_texture);
    glBindTexture(GL_TEXTURE_2D, r->source_texture);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, WIDTH, HEIGHT, 0, GL_RGBA, GL_UNSIGNED_BYTE, pattern);
    glFinish();
    gl_check("upload synthetic RGBA source");
    const EGLint image_attrs[] = {EGL_GL_TEXTURE_LEVEL_KHR, 0, EGL_IMAGE_PRESERVED_KHR, EGL_TRUE, EGL_NONE};
    r->image = create_image(r->display, r->context, EGL_GL_TEXTURE_2D_KHR,
            (EGLClientBuffer)(uintptr_t)r->source_texture, image_attrs);
    require(r->image != EGL_NO_IMAGE_KHR, "synthetic RGBA EGLImage creation failed");
    glGenTextures(1, &r->external_texture);
    glBindTexture(GL_TEXTURE_EXTERNAL_OES, r->external_texture);
    glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_EXTERNAL_OES, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    image_target(GL_TEXTURE_EXTERNAL_OES, (GLeglImageOES)r->image);
    glUniform1i(sampler, 0);
    glViewport(0, 0, WIDTH, HEIGHT);
    glDisable(GL_BLEND);
    glDisable(GL_DITHER);
    glDisable(GL_DEPTH_TEST);
    glDisable(GL_STENCIL_TEST);
    glDisable(GL_SCISSOR_TEST);
    glDisable(GL_CULL_FACE);
    glPixelStorei(GL_PACK_ALIGNMENT, 1);
    glValidateProgram(r->program);
    GLint valid = 0;
    glGetProgramiv(r->program, GL_VALIDATE_STATUS, &valid);
    require(valid == GL_TRUE, "external OES production program validation failed");
    gl_check("initialize external OES render pipeline");
}
static void render(const struct renderer *r, const struct test_case *test, float clear_alpha) {
    glClearColor(0, 0, 0, clear_alpha);
    glClear(GL_COLOR_BUFFER_BIT);
    glUniformMatrix4fv(r->geometry, 1, GL_FALSE, test->geometry);
    glUniformMatrix4fv(r->texture, 1, GL_FALSE, test->texture);
    glUniformMatrix4fv(r->producer, 1, GL_FALSE, test->producer_matrix);
    glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
    glFinish();
    gl_check("draw production shaders with exported matrices");
    glReadPixels(0, 0, WIDTH, HEIGHT, GL_RGBA, GL_UNSIGNED_BYTE, actual);
    gl_check("read back all RGBA pixels");
}
static int clamp(int n, int upper) { return n < 0 ? 0 : n >= upper ? upper - 1 : n; }
static const unsigned char *texel(int x, int y) {
    return pattern + ((size_t)clamp(y, HEIGHT) * WIDTH + clamp(x, WIDTH)) * 4;
}
static void independent_uv(const struct test_case *test, double u, double v, double *su, double *sv) {
    /* Independent AOSP MirrorNONE reference, for BOTH lens facings:
       SurfaceTexture P = FlipV * Crop * R(sensor). The production texture
       matrix must be identity: applying sensor rotation a second time is wrong.
       No exported matrix is read while computing these reference coordinates. */
    switch (test->sensor) {
        case 0: *su = u; *sv = v; break;
        case 90: *su = 1 - v; *sv = u; break;
        case 180: *su = 1 - u; *sv = 1 - v; break;
        case 270: *su = v; *sv = 1 - u; break;
        default: fail("invalid independent rotation");
    }
    double ix = (double)test->producer / WIDTH;
    double iy = (double)test->producer / HEIGHT;
    *su = ix + (1 - 2 * ix) * *su;
    *sv = 1 - (iy + (1 - 2 * iy) * *sv);
}

static int reference_sample(const struct test_case *test, double u, double v, unsigned char rgb[3]) {
    double su = 0, sv = 0;
    independent_uv(test, u, v, &su, &sv);
    double sx = su * WIDTH - .5, sy = sv * HEIGHT - .5;
    int x = (int)floor(sx), y = (int)floor(sy);
    double fx = sx - x, fy = sy - y;
    const unsigned char *a = texel(x, y), *b = texel(x + 1, y);
    const unsigned char *c = texel(x, y + 1), *d = texel(x + 1, y + 1);
    for (int channel = 0; channel < 3; ++channel) {
        double top = a[channel] * (1 - fx) + b[channel] * fx;
        double bottom = c[channel] * (1 - fx) + d[channel] * fx;
        rgb[channel] = (unsigned char)lround(top * (1 - fy) + bottom * fy);
    }
    /* An extra one-texel guard prevents round-off at a color discontinuity from
       being classified as an exact interior. Every guarded edge is still checked. */
    for (int dy = -1; dy <= 2; ++dy) for (int dx = -1; dx <= 2; ++dx)
        if (memcmp(a, texel(x + dx, y + dy), 3) != 0) return 0;
    return 1;
}
static struct counts compare(const struct test_case *test, int diagnostics) {
    struct counts counts = {0};
    int portrait = test->sensor % 180 != 0;
    double fit_width = portrait ? 405.0 : WIDTH;
    double left = (WIDTH - fit_width) / 2;
    for (int y = 0; y < HEIGHT; ++y) {
        size_t colored = 0;
        for (int x = 0; x < WIDTH; ++x) {
            const unsigned char *pixel = actual + ((size_t)y * WIDTH + x) * 4;
            int black = pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0;
            if (!black) ++colored;
            if (pixel[3] != 255) ++counts.alpha_errors;
            double center = x + .5;
            int boundary = portrait && (center == left || center == left + fit_width);
            int outside = center < left || center > left + fit_width;
            unsigned char expected[3] = {0};
            int exact = 1, mismatch = 0;
            if (outside) {
                ++counts.bars;
                mismatch = !black;
            } else {
                exact = reference_sample(test, (center - left) / fit_width,
                        (y + .5) / HEIGHT, expected);
                if (boundary) ++counts.boundary;
                else if (exact) ++counts.exact;
                else ++counts.interpolated;
                if (!(boundary && black)) for (int c = 0; c < 3; ++c) {
                    int difference = abs((int)pixel[c] - (int)expected[c]);
                    if (difference > (exact ? 0 : EDGE_TOLERANCE)) mismatch = 1;
                }
            }
            if (mismatch) {
                if (diagnostics && counts.rgb_errors < 6)
                    fprintf(stderr, "%s mismatch x=%d y=%d actual=%u,%u,%u expected=%u,%u,%u exact=%d boundary=%d\n",
                            test->name, x, y, pixel[0], pixel[1], pixel[2], expected[0], expected[1], expected[2], exact, boundary);
                ++counts.rgb_errors;
            }
        }
        if (colored != (size_t)(portrait ? 405 : WIDTH)) ++counts.rows;
    }
    return counts;
}
static int passed(struct counts counts) {
    return counts.alpha_errors == 0 && counts.rgb_errors == 0 && counts.rows == 0
            && counts.exact > 150000 && counts.interpolated > 1000
            && counts.exact + counts.interpolated + counts.boundary + counts.bars == PIXELS;
}
static void write_ppm(const char *dir, const char *name, const unsigned char *data) {
    char path[4096];
    path_join(path, sizeof(path), dir, name);
    FILE *file = fopen(path, "wb");
    require(file != NULL, "cannot write pixel artifact");
    require(fprintf(file, "P6\n%d %d\n255\n", WIDTH, HEIGHT) > 0, "cannot write PPM header");
    for (int y = HEIGHT - 1; y >= 0; --y) for (int x = 0; x < WIDTH; ++x)
        require(fwrite(data + ((size_t)y * WIDTH + x) * 4, 1, 3, file) == 3, "cannot write PPM pixel");
    require(fclose(file) == 0, "cannot close pixel artifact");
}
static void require_negative(const struct renderer *r, const struct test_case *oracle,
        const struct test_case *mutated, const char *name, float clear_alpha) {
    render(r, mutated, clear_alpha);
    struct counts c = compare(oracle, 0);
    require(!passed(c) && c.rgb_errors + c.alpha_errors + c.rows > 100, "oracle mutation was not detected");
    printf("{\"kind\":\"negative\",\"name\":\"%s\",\"rejected\":true,\"rgb_errors\":%zu,\"alpha_errors\":%zu,\"row_errors\":%zu}\n",
            name, c.rgb_errors, c.alpha_errors, c.rows);
}
static void mutation_controls(const struct renderer *r, const struct test_case cases[CASES]) {
    const struct test_case *base = NULL, *opposite = NULL;
    for (int i = 0; i < CASES; ++i) {
        if (!cases[i].front && cases[i].sensor == 90 && cases[i].producer == 0) base = &cases[i];
        if (!cases[i].front && cases[i].sensor == 270 && cases[i].producer == 0) opposite = &cases[i];
    }
    require(base && opposite, "mutation controls missing base cases");
    struct test_case mutation = *base;
    memcpy(mutation.producer_matrix, opposite->producer_matrix, sizeof(mutation.producer_matrix));
    require_negative(r, base, &mutation, "wrong_rotation_sign", 1);
    mutation = *base;
    /* Reflect sampled U after the producer transform, which MirrorNONE forbids. */
    for (int column = 0; column < 4; ++column) mutation.producer_matrix[column * 4] *= -1;
    mutation.producer_matrix[12] += 1;
    require_negative(r, base, &mutation, "unrequested_mirror", 1);
    mutation = *base;
    mutation.geometry[0] = 1;
    require_negative(r, base, &mutation, "stretched_portrait", 1);
    mutation = *base;
    /* Deliberately add a second 90-degree texture rotation. This is a fault
       injection, not a production matrix or a reference implementation. */
    mutation.texture[0] = 0;
    mutation.texture[1] = 1;
    mutation.texture[4] = -1;
    mutation.texture[5] = 0;
    mutation.texture[12] = 1;
    require_negative(r, base, &mutation, "double_sensor_rotation", 1);
    require_negative(r, base, base, "transparent_black_bars", 0);
}

static void cleanup(struct renderer *r) {
    glDeleteTextures(1, &r->external_texture);
    egl_check(r->destroy_image(r->display, r->image), "destroy owned EGLImage");
    glDeleteTextures(1, &r->source_texture);
    glDeleteProgram(r->program);
    gl_check("delete owned GL resources");
    egl_check(eglMakeCurrent(r->display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT), "release context");
    egl_check(eglDestroyContext(r->display, r->context), "destroy context");
    egl_check(eglDestroySurface(r->display, r->surface), "destroy pbuffer");
    egl_check(eglTerminate(r->display), "terminate owned display");
    egl_check(eglReleaseThread(), "release EGL thread");
}
int main(int argc, char **argv) {
    require(argc == 3, "usage: finite_camera_rotation_oracle EXPORTED_INPUT_DIR ARTIFACT_DIR");
    require(getenv("CAPYIO_HOSTED_CAMERA_ROTATION") && strcmp(getenv("CAPYIO_HOSTED_CAMERA_ROTATION"), "1") == 0
            && getenv("GITHUB_ACTIONS") && strcmp(getenv("GITHUB_ACTIONS"), "true") == 0
            && getenv("RUNNER_ENVIRONMENT") && strcmp(getenv("RUNNER_ENVIRONMENT"), "github-hosted") == 0,
            "execution is restricted to the explicitly opted-in disposable GitHub-hosted gate");
    require(getenv("LIBGL_ALWAYS_SOFTWARE") && strcmp(getenv("LIBGL_ALWAYS_SOFTWARE"), "1") == 0
            && getenv("GALLIUM_DRIVER") && strcmp(getenv("GALLIUM_DRIVER"), "llvmpipe") == 0
            && getenv("EGL_PLATFORM") && strcmp(getenv("EGL_PLATFORM"), "surfaceless") == 0,
            "forced surfaceless llvmpipe environment is required");
    GLfloat vertices[16];
    struct test_case cases[CASES];
    read_inputs(argv[1], vertices, cases);
    make_pattern();
    write_ppm(argv[2], "source-pattern.ppm", pattern);
    struct renderer renderer = {0};
    init_renderer(&renderer, argv[1], vertices);
    size_t total_exact = 0, total_edges = 0, total_bars = 0;
    int failures = 0;
    for (int i = 0; i < CASES; ++i) {
        render(&renderer, &cases[i], 1);
        struct counts c = compare(&cases[i], 1);
        int ok = passed(c);
        failures += !ok;
        total_exact += c.exact;
        total_edges += c.interpolated;
        total_bars += c.bars;
        printf("{\"kind\":\"case\",\"name\":\"%s\",\"facing\":\"%s\",\"sensor\":%d,\"rotation\":%d,\"producer\":\"%s\",\"passed\":%s,\"pixels\":%zu,\"exact_interior\":%zu,\"interpolation\":%zu,\"boundary\":%zu,\"black_bar\":%zu,\"rgb_errors\":%zu,\"alpha_errors\":%zu,\"row_errors\":%zu}\n",
                cases[i].name, cases[i].front ? "front" : "back", cases[i].sensor, cases[i].rotation,
                producer_names[cases[i].producer], ok ? "true" : "false", PIXELS, c.exact, c.interpolated,
                c.boundary, c.bars, c.rgb_errors, c.alpha_errors, c.rows);
        if (!ok || (!cases[i].front && cases[i].producer == 0)) {
            char name[128];
            int n = snprintf(name, sizeof(name), "%s%s.ppm", ok ? "render-" : "failure-", cases[i].name);
            require(n > 0 && (size_t)n < sizeof(name), "image artifact name too long");
            write_ppm(argv[2], name, actual);
        }
    }
    fflush(stdout);
    if (failures) { cleanup(&renderer); fail("one or more mandatory pixel cases failed"); }
    mutation_controls(&renderer, cases);
    cleanup(&renderer);
    free(pattern);
    free(actual);
    printf("{\"kind\":\"summary\",\"passed\":true,\"cases\":24,\"negative_controls\":5,\"pixels\":%zu,\"exact_interior\":%zu,\"interpolation\":%zu,\"black_bar\":%zu,\"opaque_alpha\":true,\"owned_cleanup\":true}\n",
            PIXELS * CASES, total_exact, total_edges, total_bars);
    puts("CAPYIO_FINITE_CAMERA_ROTATION_PIXELS_PASS cases=24 negative_controls=5 pixels=22118400 software=llvmpipe external_oes=true owned_cleanup=true");
    return EXIT_SUCCESS;
}
