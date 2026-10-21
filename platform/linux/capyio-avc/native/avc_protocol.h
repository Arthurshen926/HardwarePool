#ifndef CAPYIO_DECODER_PROTOCOL_H
#define CAPYIO_DECODER_PROTOCOL_H
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
#endif
