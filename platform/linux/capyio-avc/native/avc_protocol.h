#ifndef CAPYIO_DECODER_PROTOCOL_H
#define CAPYIO_DECODER_PROTOCOL_H
#include <stdint.h>
/* Private local binary protocol, not a network or compatibility interface.
 * Every integer is big endian. Input/output header is exactly 32 bytes:
 * magic[4], kind:u32, payload_len:u32, flags/reserved:u32, sequence:u64, pts_us:u64.
 * Input CAD1: CONFIG=1 (combined Annex B SPS/PPS), AU=2, EOS=3.
 * AU flags: KEY=1, DISCONTINUITY=2. Other records flags=0.
 * Output CAN1: READY=1, FRAME=2, EOS=3; reserved=0.
 * CONFIG gets READY (sequence=pts=0); AU gets exactly one NV12 FRAME;
 * EOS gets EOS (echo sequence/pts). No partial-success response on invalid AU.
 * Errors close the process nonzero, diagnostics only to stderr.
 */
#define DEC_HEADER 32u
#define DEC_MAX_AU (4u * 1024u * 1024u)
#define DEC_MAX_CSD (64u * 1024u)
#define DEC_MAX_INPUT (32u * 1024u * 1024u)
#define DEC_MAX_FRAMES 120u
#define DEC_WIDTH 1280u
#define DEC_HEIGHT 720u
#define DEC_PIXELS (DEC_WIDTH * DEC_HEIGHT)
#define DEC_NV12_BYTES (DEC_PIXELS * 3u / 2u)

/* Closed startup policy, not a configurable relaxation of parser limits. */
#define DEC_PROFILE_LEGACY 0u
#define DEC_PROFILE_FINITE_720P30 1u
#define DEC_MAX_LIFETIME_MS 60000u
#define DEC_FINITE_FPS 30u
#define DEC_FINITE_FRAME_BURST 3u
#define DEC_FINITE_BYTE_RATE (2u * 1024u * 1024u)
#define DEC_FINITE_BYTE_BURST (4u * 1024u * 1024u)
#define DEC_TOKEN_SCALE UINT64_C(1000000000)

struct dec_bucket {
  uint64_t tokens, capacity, rate, last_ns;
};
struct dec_budget {
  uint32_t profile, max_frames, frames;
  uint64_t max_input, total;
  struct dec_bucket frames_rate, bytes_rate;
};
static inline int dec_budget_init(struct dec_budget *out, uint32_t profile,
                                  uint32_t lifetime_ms, uint64_t now_ns) {
  if (!out || lifetime_ms == 0 || lifetime_ms > DEC_MAX_LIFETIME_MS ||
      (profile != DEC_PROFILE_LEGACY && profile != DEC_PROFILE_FINITE_720P30))
    return -1;
  uint64_t frames = DEC_MAX_FRAMES, bytes = DEC_MAX_INPUT;
  if (profile == DEC_PROFILE_FINITE_720P30) {
    /* Widen before arithmetic; the independently validated 60s bound proves
     * these operations fit even when the Rust caller/CLI is bypassed. */
    frames = ((uint64_t)lifetime_ms * DEC_FINITE_FPS + 999u) / 1000u;
    bytes = (uint64_t)lifetime_ms * DEC_FINITE_BYTE_RATE / 1000u +
            DEC_FINITE_BYTE_BURST;
    if (!frames || frames > 1800u)
      return -1;
    if (bytes > DEC_MAX_INPUT)
      bytes = DEC_MAX_INPUT;
  }
  uint64_t frame_capacity = DEC_FINITE_FRAME_BURST * DEC_TOKEN_SCALE;
  uint64_t byte_capacity = DEC_FINITE_BYTE_BURST * DEC_TOKEN_SCALE;
  *out = (struct dec_budget){
      profile, (uint32_t)frames, 0, bytes, 0,
      {frame_capacity, frame_capacity, DEC_FINITE_FPS, now_ns},
      {byte_capacity, byte_capacity, DEC_FINITE_BYTE_RATE, now_ns}};
  return 0;
}
static inline int dec_bucket_charge(struct dec_bucket *bucket, uint64_t count,
                                     uint64_t now_ns) {
  if (now_ns < bucket->last_ns || count > UINT64_MAX / DEC_TOKEN_SCALE)
    return -1;
  uint64_t elapsed = now_ns - bucket->last_ns;
  if (bucket->rate && elapsed > UINT64_MAX / bucket->rate)
    return -1;
  uint64_t refill = elapsed * bucket->rate;
  if (refill > UINT64_MAX - bucket->tokens)
    return -1;
  uint64_t available = bucket->tokens + refill;
  if (available > bucket->capacity)
    available = bucket->capacity;
  uint64_t cost = count * DEC_TOKEN_SCALE;
  if (cost > available)
    return -1;
  bucket->tokens = available - cost;
  bucket->last_ns = now_ns;
  return 0;
}
/* The caller invokes this using only the fixed header, before payload read or
 * decoding. PTS, sequence gaps and discontinuity never enter the rate clock. */
static inline int dec_budget_charge(struct dec_budget *budget, uint32_t kind,
                                     uint32_t payload_bytes, uint64_t now_ns) {
  if ((kind != 1 && kind != 2 && kind != 3) || payload_bytes > DEC_MAX_AU ||
      (kind == 1 && payload_bytes > DEC_MAX_CSD) ||
      (kind == 3 && payload_bytes != 0) ||
      (kind == 2 && budget->frames >= budget->max_frames))
    return -1;
  uint64_t charge = payload_bytes;
  if (budget->profile == DEC_PROFILE_FINITE_720P30)
    charge += kind == 1 ? 84u : 56u; /* Complete CAVC, not 32-byte CAD1. */
  if (charge > budget->max_input || budget->total > budget->max_input - charge)
    return -1;
  struct dec_budget next = *budget;
  if (budget->profile == DEC_PROFILE_FINITE_720P30 &&
      (dec_bucket_charge(&next.bytes_rate, charge, now_ns) ||
       dec_bucket_charge(&next.frames_rate, kind == 2 ? 1 : 0, now_ns)))
    return -1;
  next.total += charge;
  if (kind == 2)
    next.frames++;
  *budget = next;
  return 0;
}
#endif
