/* Owned uinput boundary. IPC is private, bounded, and accepts only this device's
 * fixed event vocabulary; no paths, descriptors or ioctls come from the peer. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <linux/input.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#ifndef CAPYIO_FAKE_UINPUT
#include <libevdev/libevdev.h>
#include <libevdev/libevdev-uinput.h>
#endif
#define MAX_EVENTS 64
#define MAX_LIFETIME_MS 300000
struct event { uint16_t type, code; int32_t value; };
static volatile sig_atomic_t stopped;
static uint64_t absolute_expiry = UINT64_MAX, last_realtime;
static void stop_signal(int number) { (void)number; stopped = 1; }
static int64_t monotonic_ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts)) return -1;
    return (int64_t)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}
static uint64_t realtime_ms(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) || ts.tv_sec < 0) return 0;
    return (uint64_t)ts.tv_sec * 1000 + (uint64_t)ts.tv_nsec / 1000000;
}
static int authority_valid(void) {
    uint64_t now = realtime_ms();
    if (!now || now < last_realtime || now >= absolute_expiry) return 0;
    last_realtime = now;
    return 1;
}
static uint16_t le16(const unsigned char *p) { return (uint16_t)(p[0] | (uint16_t)p[1] << 8); }
static uint32_t le32(const unsigned char *p) { return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24; }
/* Pipes are nonblocking; one absolute deadline bounds the entire record, not
 * each byte, so a slow/partial writer cannot renew its authority. */
static int transfer(int fd, void *bytes, size_t length, int writing, int64_t deadline) {
    unsigned char *p = bytes;
    while (length && !stopped) {
        int64_t now = monotonic_ms();
        if (now < 0 || now >= deadline || !authority_valid()) return -1;
        uint64_t remaining = absolute_expiry - last_realtime;
        int timeout = (int)(deadline - now);
        if (remaining < (uint64_t)timeout) timeout = (int)remaining;
        struct pollfd wait = { .fd = fd, .events = writing ? POLLOUT : POLLIN };
        int ready = poll(&wait, 1, timeout);
        if (ready < 0 && errno == EINTR) continue;
        if (ready <= 0 || (wait.revents & (POLLERR | POLLNVAL))) return -1;
        ssize_t count = writing ? write(fd, p, length) : read(fd, p, length);
        if (count < 0 && (errno == EINTR || errno == EAGAIN)) continue;
        if (count <= 0) return -1;
        p += count; length -= (size_t)count;
    }
    return length ? -1 : 0;
}
static const unsigned device_keys[] = {
    BTN_TOUCH, BTN_TOOL_FINGER, BTN_TOOL_DOUBLETAP,
    BTN_TOOL_TRIPLETAP, BTN_TOOL_QUADTAP, BTN_TOOL_QUINTTAP
};
static const struct { unsigned code; int maximum, resolution; } device_axes[] = {
    {ABS_MT_SLOT, 4, 0}, {ABS_MT_TRACKING_ID, INT_MAX, 0},
    {ABS_MT_POSITION_X, 12000, 100}, {ABS_MT_POSITION_Y, 7000, 100},
    {ABS_MT_PRESSURE, 65535, 0}, {ABS_X, 12000, 100},
    {ABS_Y, 7000, 100}, {ABS_PRESSURE, 65535, 0}
};
static const char device_path[] = "/dev/uinput";
#define DEVICE_OPEN_FLAGS (O_WRONLY | O_CLOEXEC | O_NONBLOCK)
#define DEVICE_PROPERTY INPUT_PROP_POINTER
struct backend {
    int fd;
#ifndef CAPYIO_FAKE_UINPUT
    struct libevdev_uinput *device;
#else
    unsigned writes;
    unsigned fail_at;
    unsigned frames;
    enum { FIXTURE_NORMAL, FIXTURE_PARTIAL_WRITE, FIXTURE_DELAYED_COMMIT,
           FIXTURE_BACKPRESSURE } mode;
#endif
};
#ifndef CAPYIO_FAKE_UINPUT
static int enable_abs(struct libevdev *device, unsigned code, int maximum, int resolution) {
    struct input_absinfo info = { .maximum = maximum, .resolution = resolution };
    return libevdev_enable_event_code(device, EV_ABS, code, &info);
}
#endif
static int backend_create(struct backend *backend) {
    backend->fd = -1;
#ifdef CAPYIO_FAKE_UINPUT
    const char *mode = getenv("CAPYIO_FAKE_MODE");
    if (!mode || !strcmp(mode, "normal")) backend->mode = FIXTURE_NORMAL;
    else if (!strcmp(mode, "partial-write")) backend->mode = FIXTURE_PARTIAL_WRITE;
    else if (!strcmp(mode, "delayed-commit")) backend->mode = FIXTURE_DELAYED_COMMIT;
    else if (!strcmp(mode, "backpressure")) backend->mode = FIXTURE_BACKPRESSURE;
    else return -1; /* Includes explicit fail-create and every unknown mode. */
    const char *fail = getenv("CAPYIO_FAKE_FAIL_CREATE");
    if (fail) return -1;
    const char *at = getenv("CAPYIO_FAKE_FAIL_WRITE");
    backend->fail_at = at ? (unsigned)strtoul(at, NULL, 10) : 0;
    backend->writes = 0; backend->frames = 0; backend->fd = 42;
    fprintf(stderr, "CREATE\n");
    fprintf(stderr, "OPEN %s %d\n", device_path, DEVICE_OPEN_FLAGS);
    fprintf(stderr, "PROPERTY %d\n", DEVICE_PROPERTY);
    for (unsigned i = 0; i < sizeof(device_keys)/sizeof(device_keys[0]); ++i)
        fprintf(stderr, "KEY %u\n", device_keys[i]);
    for (unsigned i = 0; i < sizeof(device_axes)/sizeof(device_axes[0]); ++i)
        fprintf(stderr, "AXIS %u %d %d\n", device_axes[i].code, device_axes[i].maximum, device_axes[i].resolution);
    return ferror(stderr) ? -1 : 0;
#else
    struct libevdev *device = libevdev_new();
    if (!device) return -1;
    libevdev_set_name(device, "CapyIO authorized remote touchpad");
    libevdev_set_id_bustype(device, BUS_VIRTUAL);
    int rc = libevdev_enable_property(device, DEVICE_PROPERTY);
    for (unsigned i = 0; !rc && i < sizeof(device_keys)/sizeof(device_keys[0]); ++i)
        rc = libevdev_enable_event_code(device, EV_KEY, device_keys[i], NULL);
    for (unsigned i = 0; !rc && i < sizeof(device_axes)/sizeof(device_axes[0]); ++i)
        rc = enable_abs(device, device_axes[i].code, device_axes[i].maximum, device_axes[i].resolution);
    if (!rc) {
        backend->fd = open(device_path, DEVICE_OPEN_FLAGS);
        if (backend->fd < 0) rc = -1;
    }
    if (!rc) rc = libevdev_uinput_create_from_device(device, backend->fd, &backend->device);
    libevdev_free(device);
    if (rc && backend->fd >= 0) { close(backend->fd); backend->fd = -1; }
    return rc;
#endif
}
static int backend_write(struct backend *backend, struct event event) {
#ifdef CAPYIO_FAKE_UINPUT
    if (++backend->writes == backend->fail_at) { fprintf(stderr, "WRITE_FAILED\n"); return -1; }
    return fprintf(stderr, "EVENT %u %u %d\n", event.type, event.code, event.value) < 0 ? -1 : 0;
#else
    return libevdev_uinput_write_event(backend->device, event.type, event.code, event.value);
#endif
}
static void backend_destroy(struct backend *backend) {
    if (backend->fd < 0) return;
#ifdef CAPYIO_FAKE_UINPUT
    fprintf(stderr, "DESTROY\n");
#else
    libevdev_uinput_destroy(backend->device);
    close(backend->fd);
#endif
    backend->fd = -1;
}
/* Best effort after any uncertain write. Destroy is unconditional even when
 * release writes fail. Hard process kill closes the fd but cannot emit this. */
static int cleanup(struct backend *backend) {
    if (backend->fd < 0) return 0;
    int failed = 0;
#ifdef CAPYIO_FAKE_UINPUT
    fprintf(stderr, "CLEANUP_BEGIN\n");
#endif
    /* BTN_TOUCH must be first in this complete synchronization frame. */
    for (unsigned i = 0; i < sizeof(device_keys)/sizeof(device_keys[0]); ++i)
        failed |= backend_write(backend, (struct event){EV_KEY, device_keys[i], 0}) != 0;
    for (int slot = 0; slot < 5; ++slot) {
        failed |= backend_write(backend, (struct event){EV_ABS, ABS_MT_SLOT, slot}) != 0;
        failed |= backend_write(backend, (struct event){EV_ABS, ABS_MT_TRACKING_ID, -1}) != 0;
    }
    failed |= backend_write(backend, (struct event){EV_ABS, ABS_PRESSURE, 0}) != 0;
    failed |= backend_write(backend, (struct event){EV_SYN, SYN_REPORT, 0}) != 0;
    backend_destroy(backend); return failed ? -1 : 0;
}
#ifdef CAPYIO_FAKE_UINPUT
/* The fixture runs the real parser/ACK/cleanup loop. Only its backend and this
 * deterministic fault boundary are fake. It never opens or ioctls a device.
 * Wait on the parent's private input descriptor so Stop/EOF aborts before any
 * delayed events can commit, rather than sleeping past a cancelled owner. */
static int fixture_before_frame(struct backend *backend, int64_t deadline) {
    if (fprintf(stderr, "FRAME_BEGIN %u\n", ++backend->frames) < 0) return -1;
    if (backend->frames != 3) return 0;
    if (backend->mode == FIXTURE_PARTIAL_WRITE) {
        backend->fail_at = backend->writes + 2;
        return 0;
    }
    int delay = backend->mode == FIXTURE_DELAYED_COMMIT ? 100 :
        backend->mode == FIXTURE_BACKPRESSURE ? 1000 : 0;
    if (!delay) return 0;
    if (fprintf(stderr, "COMMIT_WAIT %u\n", backend->frames) < 0) return -1;
    int64_t until = monotonic_ms() + delay;
    while (!stopped && authority_valid()) {
        int64_t now = monotonic_ms();
        if (now < 0 || now >= deadline) break;
        if (now >= until) return 0;
        int remaining = (int)((until < deadline ? until : deadline) - now);
        struct pollfd parent = { .fd = 0, .events = 0 };
        int ready = poll(&parent, 1, remaining);
        if (ready < 0 && errno == EINTR) continue;
        if (ready != 0) break;
    }
    fprintf(stderr, "FRAME_ABORTED %u\n", backend->frames);
    return -1;
}
#endif
static int valid_event(struct event event, int final) {
    if (event.type == EV_SYN) return final && event.code == SYN_REPORT && event.value == 0;
    if (final) return 0;
    if (event.type == EV_KEY) {
        if (event.value != 0 && event.value != 1) return 0;
        return event.code == BTN_TOUCH || event.code == BTN_TOOL_FINGER ||
            event.code == BTN_TOOL_DOUBLETAP || event.code == BTN_TOOL_TRIPLETAP ||
            event.code == BTN_TOOL_QUADTAP || event.code == BTN_TOOL_QUINTTAP;
    }
    if (event.type != EV_ABS) return 0;
    if (event.code == ABS_MT_TRACKING_ID) return event.value >= -1;
    if (event.value < 0) return 0;
    switch (event.code) {
    case ABS_MT_SLOT: return event.value <= 4;
    case ABS_MT_POSITION_X: case ABS_X: return event.value <= 12000;
    case ABS_MT_POSITION_Y: case ABS_Y: return event.value <= 7000;
    case ABS_MT_PRESSURE: case ABS_PRESSURE: return event.value <= 65535;
    default: return 0;
    }
}
int capyio_touchpad_main(void) {
#ifdef CAPYIO_FAKE_UINPUT
    setvbuf(stderr, NULL, _IONBF, 0);
    /* The optional trace may be a pipe. Lost trace capacity must fail closed,
     * never block commit/stop or permit unbounded diagnostic buffering. */
    int trace_flags = fcntl(STDERR_FILENO, F_GETFL);
    if (trace_flags < 0 || fcntl(STDERR_FILENO, F_SETFL, trace_flags | O_NONBLOCK)) return 1;
#endif
    stopped = 0; absolute_expiry = UINT64_MAX; last_realtime = 0;
    struct sigaction action = { .sa_handler = stop_signal };
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGTERM, &action, NULL) || sigaction(SIGINT, &action, NULL)) return 1;
    signal(SIGPIPE, SIG_IGN);
    for (int fd = 0; fd <= 1; ++fd) {
        int flags = fcntl(fd, F_GETFL);
        if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK)) return 1;
    }
    int64_t start = monotonic_ms();
    unsigned char header[24];
    if (start < 0 || transfer(0, header, sizeof(header), 0, start + 2000)) return 1;
    uint32_t lifetime = le32(header + 4), idle = le32(header + 8);
    if (memcmp(header, "HPT1", 4) || le32(header + 12) || lifetime < 1 ||
        lifetime > MAX_LIFETIME_MS || idle < 10 || idle > 2000) return 1;
    absolute_expiry = (uint64_t)le32(header + 16) | (uint64_t)le32(header + 20) << 32;
    if (!authority_valid()) return 1;
    /* Lifetime starts before header reading, conservatively counting startup. */
    int64_t expires = start + lifetime, deadline = start + idle;
    if (deadline > expires) deadline = expires;
    if (monotonic_ms() >= deadline || !authority_valid()) return 1;
    struct backend backend = { .fd = -1 };
    if (backend_create(&backend)) return 1;
    unsigned char ack = 1;
    int result = 1;
    int64_t window = 0;
    unsigned in_window = 0;
    if (transfer(1, &ack, 1, 1, deadline)) goto done;
    for (;;) {
        unsigned char frame_header[8], encoded[MAX_EVENTS * 8];
        struct event events[MAX_EVENTS];
        if (transfer(0, frame_header, sizeof(frame_header), 0, deadline)) break;
        unsigned count = le16(frame_header + 4);
        if (memcmp(frame_header, "HPF1", 4) || le16(frame_header + 6) || count > MAX_EVENTS) break;
        if (!count) {
            if (!cleanup(&backend) && !transfer(1, &ack, 1, 1, deadline)) result = 0;
            break;
        }
        if (transfer(0, encoded, count * 8, 0, deadline)) break;
        int valid = 1;
        for (unsigned i = 0; i < count; ++i) {
            uint32_t value = le32(encoded + i*8 + 4);
            events[i] = (struct event){le16(encoded + i*8), le16(encoded + i*8 + 2),
                value <= INT_MAX ? (int32_t)value : -1};
            /* Only the exact two's-complement -1 is valid negative input. */
            if ((value > INT_MAX && value != UINT32_MAX) || !valid_event(events[i], i + 1 == count)) valid = 0;
        }
        if (!valid || stopped || !authority_valid() || monotonic_ms() >= deadline) break;
        /* 240 complete reports per one-second admission window; admission also
         * enforces the rate, but the device boundary does not trust its peer. */
        int64_t now = monotonic_ms();
        if (!window || now - window >= 1000) { window = now; in_window = 0; }
        if (++in_window > 240) break;
#ifdef CAPYIO_FAKE_UINPUT
        if (fixture_before_frame(&backend, deadline)) break;
#endif
        for (unsigned i = 0; i < count; ++i) {
            if (stopped || !authority_valid() || monotonic_ms() >= deadline || backend_write(&backend, events[i])) { valid = 0; break; }
        }
        if (!valid) break;
#ifdef CAPYIO_FAKE_UINPUT
        if (fprintf(stderr, "FRAME_COMMITTED %u\n", backend.frames) < 0) break;
#endif
        if (transfer(1, &ack, 1, 1, deadline)) break;
        deadline = monotonic_ms() + idle;
        if (deadline > expires) deadline = expires;
    }
done:
    if (cleanup(&backend)) result = 1;
    return result;
}
#if defined(CAPYIO_FAKE_UINPUT) && !defined(CAPYIO_FAKE_LIBRARY)
int main(void) { return capyio_touchpad_main(); }
#endif
