#define _POSIX_C_SOURCE 200809L
#include "avc_protocol.h"
#include <errno.h>
#include <libavcodec/avcodec.h>
#include <libavutil/buffer.h>
#include <libavutil/frame.h>
#include <libavutil/mem.h>
#include <libavutil/pixfmt.h>
#include <limits.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/time.h>
#include <unistd.h>

/* This executable has no URLs, demuxers, network, devices or arbitrary
 * commands. Linux rlimits/alarm bound even malformed streams stuck in a decoder
 * call. The parent owns/reaps us and imposes tighter per-operation deadlines.
 */
static int fail(const char *reason) {
  fprintf(stderr, "AVC helper: %s\n", reason);
  return -1;
}
static uint32_t be32(const uint8_t *p) {
  return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) |
         ((uint32_t)p[2] << 8) | p[3];
}
static uint64_t be64(const uint8_t *p) {
  return ((uint64_t)be32(p) << 32) | be32(p + 4);
}
static void put32(uint8_t *p, uint32_t n) {
  p[0] = n >> 24;
  p[1] = n >> 16;
  p[2] = n >> 8;
  p[3] = n;
}
static void put64(uint8_t *p, uint64_t n) {
  put32(p, n >> 32);
  put32(p + 4, n);
}
static int exact_read(uint8_t *p, size_t n) {
  while (n) {
    ssize_t r = read(STDIN_FILENO, p, n);
    if (r < 0 && errno == EINTR)
      continue;
    if (r <= 0)
      return -1;
    p += r;
    n -= (size_t)r;
  }
  return 0;
}
static int exact_write(const uint8_t *p, size_t n) {
  while (n) {
    ssize_t r = write(STDOUT_FILENO, p, n);
    if (r < 0 && errno == EINTR)
      continue;
    if (r <= 0)
      return -1;
    p += r;
    n -= (size_t)r;
  }
  return 0;
}
static int response(uint32_t kind, uint64_t seq, uint64_t pts,
                    const uint8_t *pixels) {
  uint8_t h[DEC_HEADER] = {0};
  memcpy(h, "CAN1", 4);
  put32(h + 4, kind);
  put32(h + 8, pixels ? DEC_NV12_BYTES : 0);
  put64(h + 16, seq);
  put64(h + 24, pts);
  return exact_write(h, sizeof(h)) ||
                 (pixels && exact_write(pixels, DEC_NV12_BYTES))
             ? -1
             : 0;
}
struct bits {
  const uint8_t *p;
  size_t n, bit;
  int bad;
};
static uint32_t bits(struct bits *b, unsigned n) {
  uint32_t v = 0;
  if (n > 32 || b->bit > b->n * 8 || n > b->n * 8 - b->bit) {
    b->bad = 1;
    return 0;
  }
  while (n--) {
    v = (v << 1) | ((b->p[b->bit / 8] >> (7 - b->bit % 8)) & 1);
    b->bit++;
  }
  return v;
}
static uint32_t ue(struct bits *b) {
  unsigned zeros = 0;
  while (!b->bad && !bits(b, 1)) {
    if (++zeros > 30) {
      b->bad = 1;
      return 0;
    }
  }
  return b->bad ? 0 : ((1u << zeros) - 1 + bits(b, zeros));
}
static int32_t se(struct bits *b) {
  uint32_t u = ue(b);
  return (u & 1) ? (int32_t)((u + 1) / 2) : -(int32_t)(u / 2);
}
/* Remove emulation prevention bytes, rejecting noncanonical/truncated escapes.
 */
static int rbsp(const uint8_t *p, size_t n, uint8_t *out, size_t *out_n) {
  unsigned zeros = 0;
  size_t w = 0;
  for (size_t i = 0; i < n; i++) {
    uint8_t x = p[i];
    if (zeros >= 2 && x == 3) {
      if (i + 1 >= n || p[i + 1] > 3)
        return -1;
      zeros = 0;
      continue;
    }
    if (zeros >= 2 && x < 3)
      return -1;
    out[w++] = x;
    zeros = x == 0 ? zeros + 1 : 0;
  }
  *out_n = w;
  return w ? 0 : -1;
}
static int trailing(struct bits *b) {
  if (b->bad || b->bit >= b->n * 8 || b->n * 8 - b->bit > 8 || bits(b, 1) != 1)
    return -1;
  while (b->bit < b->n * 8)
    if (bits(b, 1))
      return -1;
  return b->bad ? -1 : 0;
}
static int more_rbsp(struct bits b) { return trailing(&b) != 0; }
static void hrd(struct bits *b) {
  uint32_t count = ue(b);
  if (count > 31) {
    b->bad = 1;
    return;
  }
  bits(b, 4);
  bits(b, 4);
  for (uint32_t i = 0; i <= count; i++) {
    ue(b);
    ue(b);
    bits(b, 1);
  }
  bits(b, 5);
  bits(b, 5);
  bits(b, 5);
  bits(b, 5);
}
static int vui(struct bits *b) {
  if (bits(b, 1)) {
    unsigned ar = bits(b, 8);
    if (ar == 255) {
      unsigned w = bits(b, 16), h = bits(b, 16);
      if (w != h || !w)
        return -1;
    } else if (ar != 1 && ar != 0)
      return -1;
  }
  if (bits(b, 1))
    bits(b, 1);
  if (bits(b, 1)) {
    bits(b, 3);
    if (bits(b, 1))
      return -1;
    if (bits(b, 1)) {
      for (unsigned i = 0; i < 3; i++) {
        unsigned color = bits(b, 8);
        if (color != 1 && color != 2)
          return -1;
      }
    }
  }
  if (bits(b, 1)) {
    if (ue(b) > 5 || ue(b) > 5)
      return -1;
  }
  if (bits(b, 1)) {
    uint32_t units = bits(b, 32), scale = bits(b, 32);
    unsigned fixed = bits(b, 1);
    if (!units || !fixed || (uint64_t)units * 60 != scale)
      return -1;
  }
  unsigned nal = bits(b, 1);
  if (nal)
    hrd(b);
  unsigned vcl = bits(b, 1);
  if (vcl)
    hrd(b);
  if (nal || vcl)
    bits(b, 1);
  bits(b, 1);
  if (bits(b, 1)) {
    bits(b, 1);
    if (ue(b) > 16 || ue(b) > 16 || ue(b) > 16 || ue(b) > 16)
      return -1;
    if (ue(b) != 0 || ue(b) > 4)
      return -1;
  }
  return b->bad ? -1 : 0;
}
struct config {
  uint8_t sps[DEC_MAX_CSD], pps[DEC_MAX_CSD];
  size_t sps_n, pps_n;
  uint32_t sps_id, pps_id;
};
static int sps(const uint8_t *p, size_t n, uint32_t *id) {
  uint8_t raw[DEC_MAX_CSD];
  size_t len;
  if (n < 4 || n > sizeof(raw) || rbsp(p + 1, n - 1, raw, &len))
    return -1;
  struct bits b = {raw, len, 0, 0};
  unsigned profile = bits(&b, 8);
  unsigned constraints = bits(&b, 8);
  unsigned level = bits(&b, 8);
  if (constraints & 3 || level > 41)
    return -1;
  *id = ue(&b);
  if (*id > 31)
    return -1;
  if (profile == 100) {
    if (ue(&b) != 1 || ue(&b) != 0 || ue(&b) != 0)
      return -1;
    bits(&b, 1);
    if (bits(&b, 1))
      return -1;
  } else if (profile != 66 && profile != 77)
    return -1;
  if (ue(&b) > 12)
    return -1;
  uint32_t poc = ue(&b);
  if (poc == 0) {
    if (ue(&b) > 12)
      return -1;
  } else if (poc == 1) {
    bits(&b, 1);
    se(&b);
    se(&b);
    uint32_t count = ue(&b);
    if (count > 16)
      return -1;
    for (uint32_t i = 0; i < count; i++)
      se(&b);
  } else if (poc != 2)
    return -1;
  if (ue(&b) > 4)
    return -1;
  bits(&b, 1);
  uint32_t mw = ue(&b), mh = ue(&b);
  if (mw != 79 || mh != 44 || !bits(&b, 1))
    return -1;
  bits(&b, 1);
  /* No implicit crop/resize. Canonical 720p is already macroblock aligned. */
  if (bits(&b, 1)) {
    if (ue(&b) || ue(&b) || ue(&b) || ue(&b))
      return -1;
  }
  if (bits(&b, 1) && vui(&b))
    return -1;
  return trailing(&b);
}
static int pps(const uint8_t *p, size_t n, uint32_t sps_id, uint32_t *pps_id) {
  uint8_t raw[DEC_MAX_CSD];
  size_t len;
  if (n < 2 || n > sizeof(raw) || rbsp(p + 1, n - 1, raw, &len))
    return -1;
  struct bits b = {raw, len, 0, 0};
  *pps_id = ue(&b);
  if (*pps_id > 255 || ue(&b) != sps_id)
    return -1;
  bits(&b, 1);
  if (bits(&b, 1) || ue(&b) != 0)
    return -1; /* no fields/FMO */
  if (ue(&b) > 3 || ue(&b) > 3)
    return -1;
  bits(&b, 1);
  if (bits(&b, 2) != 0)
    return -1; /* weighted B prediction is unsupported */
  int32_t qp = se(&b), qs = se(&b), chroma = se(&b);
  if (qp < -26 || qp > 25 || qs < -26 || qs > 25 || chroma < -12 || chroma > 12)
    return -1;
  bits(&b, 1);
  bits(&b, 1);
  if (bits(&b, 1))
    return -1; /* redundant pictures unsupported */
  if (more_rbsp(b)) {
    bits(&b, 1); /* transform_8x8_mode_flag */
    if (bits(&b, 1))
      return -1; /* scaling matrices outside this slice */
    chroma = se(&b);
    if (chroma < -12 || chroma > 12)
      return -1;
  }
  return trailing(&b);
}
/* NAL iteration is linear and bounded. Annex B must start at byte zero. */
static size_t prefix(const uint8_t *p, size_t n, size_t i) {
  if (i + 3 <= n && p[i] == 0 && p[i + 1] == 0 && p[i + 2] == 1)
    return 3;
  if (i + 4 <= n && p[i] == 0 && p[i + 1] == 0 && p[i + 2] == 0 &&
      p[i + 3] == 1)
    return 4;
  return 0;
}
static int validate_nals(const uint8_t *p, size_t n, struct config *c,
                         int initial, int key) {
  size_t i = 0;
  unsigned count = 0, slices = 0, first = 0;
  int idr = -1;
  uint32_t last_mb = 0;
  if (!prefix(p, n, 0))
    return -1;
  while (i < n) {
    size_t pre = prefix(p, n, i);
    if (!pre || ++count > 256)
      return -1;
    size_t start = i + pre, end = start;
    while (end < n && !prefix(p, n, end))
      end++;
    size_t trim = end;
    while (trim > start && p[trim - 1] == 0)
      trim--;
    if (trim <= start)
      return -1;
    size_t len = trim - start;
    unsigned type = p[start] & 31;
    if ((p[start] & 128) ||
        ((type == 7 || type == 8 || type == 5) && !(p[start] & 96)))
      return -1;
    if (type == 7) {
      if (initial) {
        if (c->sps_n || sps(p + start, len, &c->sps_id))
          return -1;
        memcpy(c->sps, p + start, len);
        c->sps_n = len;
      } else if (len != c->sps_n || memcmp(c->sps, p + start, len))
        return -1;
    } else if (type == 8) {
      if (initial) {
        if (!c->sps_n || c->pps_n || pps(p + start, len, c->sps_id, &c->pps_id))
          return -1;
        memcpy(c->pps, p + start, len);
        c->pps_n = len;
      } else if (len != c->pps_n || memcmp(c->pps, p + start, len))
        return -1;
    } else if (!initial && (type == 1 || type == 5)) {
      uint8_t raw[129];
      size_t raw_n;
      size_t take = len - 1;
      if (take > 128) {
        take = 128; /* bounded slice prefix; do not cut an escape sequence */
        if (p[start + take] == 3 && p[start + take - 1] == 0 &&
            p[start + take - 2] == 0)
          take++;
      }
      if (rbsp(p + start + 1, take, raw, &raw_n))
        return -1;
      struct bits b = {raw, raw_n, 0, 0};
      uint32_t mb = ue(&b), slice = ue(&b), pi = ue(&b);
      if (b.bad || slice > 9 || slice % 5 == 1 || slice % 5 == 3 ||
          slice % 5 == 4 || pi != c->pps_id || mb >= 3600 ||
          (type == 5 && slice % 5 != 2))
        return -1;
      if (!slices && mb != 0)
        return -1;
      if (slices && mb <= last_mb)
        return -1;
      if (mb == 0)
        first++;
      last_mb = mb;
      slices++;
      if (idr != -1 && idr != (type == 5))
        return -1;
      idr = type == 5;
    } else if (!initial &&
               (type == 6 ||
                type == 9)) { /* bounded SEI/AUD: codec remains validator */
    } else
      return -1;
    i = end;
  }
  return initial ? (!c->sps_n || !c->pps_n ? -1 : 0)
                 : (!slices || first != 1 || idr != !!key ? -1 : 0);
}
static int plane(const AVFrame *f, unsigned plane_index, unsigned rows,
                 unsigned columns, const uint8_t **ptr, unsigned *stride) {
  if (!f->data[plane_index] || f->linesize[plane_index] < (int)columns)
    return -1;
  AVBufferRef *buf = av_frame_get_plane_buffer((AVFrame *)f, (int)plane_index);
  if (!buf)
    return -1;
  uintptr_t base = (uintptr_t)buf->data, pos = (uintptr_t)f->data[plane_index];
  size_t used =
      (size_t)(rows - 1) * (unsigned)f->linesize[plane_index] + columns;
  if (pos < base || pos - base > buf->size || used > buf->size - (pos - base))
    return -1;
  *ptr = f->data[plane_index];
  *stride = (unsigned)f->linesize[plane_index];
  return 0;
}
static int canonical(const AVCodecContext *ctx, const AVFrame *f, uint8_t *out,
                     int key, int64_t pts) {
  if (av_frame_get_side_data(f, AV_FRAME_DATA_DISPLAYMATRIX) ||
      av_frame_get_side_data(f, AV_FRAME_DATA_STEREO3D) ||
      av_frame_get_side_data(f, AV_FRAME_DATA_SPHERICAL))
    return -1;
  if (f->flags & (AV_FRAME_FLAG_CORRUPT | AV_FRAME_FLAG_DISCARD))
    return -1;
  if (f->width != DEC_WIDTH || f->height != DEC_HEIGHT || ctx->has_b_frames ||
      f->pict_type == AV_PICTURE_TYPE_B || f->pts != pts ||
      f->decode_error_flags || f->repeat_pict || f->crop_top ||
      f->crop_bottom || f->crop_left || f->crop_right)
    return -1;
#ifdef AV_FRAME_FLAG_INTERLACED
  if (f->flags & AV_FRAME_FLAG_INTERLACED)
    return -1;
#else
  if (f->interlaced_frame)
    return -1;
#endif
#ifdef AV_FRAME_FLAG_KEY
  if (!!(f->flags & AV_FRAME_FLAG_KEY) != !!key)
    return -1;
#else
  if (!!f->key_frame != !!key)
    return -1;
#endif
  if ((f->color_range != AVCOL_RANGE_UNSPECIFIED &&
       f->color_range != AVCOL_RANGE_MPEG) ||
      (f->colorspace != AVCOL_SPC_UNSPECIFIED &&
       f->colorspace != AVCOL_SPC_BT709) ||
      (f->color_primaries != AVCOL_PRI_UNSPECIFIED &&
       f->color_primaries != AVCOL_PRI_BT709) ||
      (f->color_trc != AVCOL_TRC_UNSPECIFIED &&
       f->color_trc != AVCOL_TRC_BT709))
    return -1;
  const uint8_t *y, *u, *v;
  unsigned ys, us, vs;
  if (plane(f, 0, 720, 1280, &y, &ys))
    return -1;
  if (f->format == AV_PIX_FMT_NV12) {
    if (plane(f, 1, 360, 1280, &u, &us))
      return -1;
    for (unsigned row = 0; row < 360; row++)
      memcpy(out + DEC_PIXELS + row * 1280, u + (size_t)row * us, 1280);
  } else if (f->format == AV_PIX_FMT_YUV420P) {
    if (plane(f, 1, 360, 640, &u, &us) || plane(f, 2, 360, 640, &v, &vs))
      return -1;
    for (unsigned row = 0; row < 360; row++)
      for (unsigned col = 0; col < 640; col++) {
        out[DEC_PIXELS + row * 1280 + col * 2] = u[(size_t)row * us + col];
        out[DEC_PIXELS + row * 1280 + col * 2 + 1] = v[(size_t)row * vs + col];
      }
  } else
    return -1;
  for (unsigned row = 0; row < 720; row++)
    memcpy(out + row * 1280, y + (size_t)row * ys, 1280);
  return 0;
}
static int limits(uint32_t lifetime_ms) {
  struct rlimit memory = {512u * 1024u * 1024u, 512u * 1024u * 1024u},
                cpu = {10, 10},
                file = {256u * 1024u * 1024u, 256u * 1024u * 1024u},
                core = {0, 0};
  if (setrlimit(RLIMIT_AS, &memory) || setrlimit(RLIMIT_CPU, &cpu) ||
      setrlimit(RLIMIT_FSIZE, &file) || setrlimit(RLIMIT_CORE, &core))
    return -1;
  if (signal(SIGPIPE, SIG_IGN) == SIG_ERR ||
      signal(SIGALRM, SIG_DFL) == SIG_ERR ||
      signal(SIGXCPU, SIG_DFL) == SIG_ERR ||
      signal(SIGXFSZ, SIG_DFL) == SIG_ERR)
    return -1;
  sigset_t unblocked;
  if (sigemptyset(&unblocked) || sigaddset(&unblocked, SIGALRM) ||
      sigaddset(&unblocked, SIGXCPU) || sigaddset(&unblocked, SIGXFSZ) ||
      sigprocmask(SIG_UNBLOCK, &unblocked, NULL))
    return -1;
  if (!lifetime_ms || lifetime_ms > 60000)
    return -1;
  struct itimerval timer = {0};
  timer.it_value.tv_sec = lifetime_ms / 1000;
  timer.it_value.tv_usec = (lifetime_ms % 1000) * 1000;
  return setitimer(ITIMER_REAL, &timer, NULL);
}
int capyio_avc_main(uint32_t lifetime_ms) {
  int status = 1;
  AVCodecContext *ctx = NULL;
  AVPacket *packet = NULL;
  AVFrame *frame = NULL;
  uint8_t *input = NULL, *pixels = NULL;
  struct config *cfg = NULL;
  uint64_t last_seq = 0, last_pts = 0;
  unsigned frames = 0;
  size_t total = 0;
  int configured = 0;
  if (limits(lifetime_ms))
    return 1;
  av_log_set_level(AV_LOG_ERROR);
  input = av_mallocz(DEC_MAX_AU + AV_INPUT_BUFFER_PADDING_SIZE);
  pixels = av_malloc(DEC_NV12_BYTES);
  cfg = calloc(1, sizeof(*cfg));
  packet = av_packet_alloc();
  frame = av_frame_alloc();
  if (!input || !pixels || !cfg || !packet || !frame)
    goto done;
  for (;;) {
    uint8_t h[DEC_HEADER];
    if (exact_read(h, sizeof(h))) {
      fail("truncated input or missing EOS");
      goto done;
    }
    if (memcmp(h, "CAD1", 4)) {
      fail("bad IPC magic");
      goto done;
    }
    uint32_t kind = be32(h + 4), n = be32(h + 8), flags = be32(h + 12);
    uint64_t seq = be64(h + 16), pts = be64(h + 24);
    if (n > DEC_MAX_AU || total > DEC_MAX_INPUT - n ||
        pts > INT64_MAX / 1000u) {
      fail("input budget or timestamp");
      goto done;
    }
    total += n;
    if (exact_read(input, n)) {
      fail("truncated payload");
      goto done;
    }
    memset(input + n, 0, AV_INPUT_BUFFER_PADDING_SIZE);
    if (kind == 1) {
      if (configured || flags || seq || pts || !n || n > DEC_MAX_CSD ||
          validate_nals(input, n, cfg, 1, 0)) {
        fail("unsupported config");
        goto done;
      }
      const AVCodec *codec = avcodec_find_decoder(AV_CODEC_ID_H264);
      if (!codec)
        goto done;
      ctx = avcodec_alloc_context3(codec);
      if (!ctx)
        goto done;
      ctx->thread_count = 1;
      ctx->thread_type = 0;
      ctx->max_pixels = DEC_PIXELS;
      ctx->err_recognition = AV_EF_EXPLODE | AV_EF_BITSTREAM | AV_EF_BUFFER;
      ctx->strict_std_compliance = FF_COMPLIANCE_STRICT;
      ctx->pkt_timebase = (AVRational){1, 1000000};
      ctx->extradata = av_mallocz(n + AV_INPUT_BUFFER_PADDING_SIZE);
      if (!ctx->extradata)
        goto done;
      memcpy(ctx->extradata, input, n);
      ctx->extradata_size = (int)n;
      if (avcodec_open2(ctx, codec, NULL) < 0)
        goto done;
      configured = 1;
      if (response(1, 0, 0, NULL))
        goto done;
      continue;
    }
    if (!configured || !seq || seq <= last_seq || pts < last_pts ||
        (last_seq && kind != 3 && pts == last_pts)) {
      fail("order or state");
      goto done;
    }
    if (kind == 3) {
      if (n || flags || !frames || seq != last_seq + 1) {
        fail("invalid EOS");
        goto done;
      }
      int r = avcodec_send_packet(ctx, NULL);
      if (r < 0 && r != AVERROR_EOF) {
        fail("EOS send failed");
        goto done;
      }
      r = avcodec_receive_frame(ctx, frame);
      if (r != AVERROR_EOF) {
        fail("unsupported delayed output at EOS");
        goto done;
      }
      if (response(3, seq, pts, NULL))
        goto done;
      status = 0;
      goto done;
    }
    if (kind != 2 || !n || (flags & ~3u) || frames >= DEC_MAX_FRAMES ||
        ((last_seq ? seq != last_seq + 1 : seq != 1) && !(flags & 2)) ||
        (!last_seq && !(flags & 1)) || ((flags & 2) && !(flags & 1)) ||
        validate_nals(input, n, cfg, 0, flags & 1)) {
      fail("unsupported AU");
      goto done;
    }
    if (flags & 2)
      avcodec_flush_buffers(ctx);
    if (av_new_packet(packet, (int)n) < 0)
      goto done;
    memcpy(packet->data, input, n);
    packet->pts = (int64_t)pts;
    packet->dts = (int64_t)pts;
    if (flags & 1)
      packet->flags |= AV_PKT_FLAG_KEY;
    int r = avcodec_send_packet(ctx, packet);
    av_packet_unref(packet);
    if (r == AVERROR(EAGAIN)) { /* Strict no-delay profile must have fully
                                   drained prior input. */
      fail("unexpected pending decoder output");
      goto done;
    }
    if (r < 0) {
      fail("decode rejected AU");
      goto done;
    }
    unsigned outputs = 0;
    for (;;) {
      r = avcodec_receive_frame(ctx, frame);
      if (r == AVERROR(EAGAIN))
        break;
      if (r < 0 || ++outputs > 1 ||
          canonical(ctx, frame, pixels, flags & 1, (int64_t)pts)) {
        fail("unsupported decoded frame");
        goto done;
      }
      av_frame_unref(frame);
    }
    if (outputs != 1) {
      fail("unsupported delayed/no-output AU");
      goto done;
    }
    if (response(2, seq, pts, pixels))
      goto done;
    last_seq = seq;
    last_pts = pts;
    frames++;
  }
done:
  av_frame_free(&frame);
  av_packet_free(&packet);
  avcodec_free_context(&ctx);
  av_free(input);
  av_free(pixels);
  free(cfg);
  return status;
}
