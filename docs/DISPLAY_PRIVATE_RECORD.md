# CapyIO private display record v1

> Status: fixed AdapterManaged record boundary; not a public or secure network
> protocol

## Purpose and binding

This layout carries one complete AVC decoder configuration or one complete AVC
Annex-B access unit between reviewed display Adapters. It is never placed in the
JSON/Protobuf control path. A transport may carry it only after authenticating
the Session and Route and must bind the selected Display format, Stream ID and
epoch to its authenticated context.

The record supplies no authentication, encryption, replay window, rate limit,
congestion control, fragmentation, reconnect or peer authorization. `CPD1` is a
format discriminator, not proof of trust. A socket implementation is outside
this contract.

`DisplayTransportBinding` is the receive-side handoff from that future outer
transport. It retains typed Session, Route and Stream IDs, the selected epoch
and the exact selected format without serializing Session/Route into `CPD1`.
`BoundPrivateDisplayReceiver` then requires configuration before access units,
rejects a different Stream ID or epoch, rejects duplicate/regressing access-unit
sequences and becomes terminal after end-of-stream. Sequence gaps remain visible
to the decoder resynchronization gate. These types perform no cryptography and
are not proof that the caller actually authenticated or authorized the peer.

All integers are unsigned big-endian. Although the Rust semantic types use
`u64`/`u32`, values shared with Java are limited to `Long.MAX_VALUE` and
`Integer.MAX_VALUE`. A parser receives exactly one complete record and rejects
truncation and trailing bytes.

## Common header

| Offset | Bytes | Field | v1 rule |
|---:|---:|---|---|
| 0 | 4 | magic | ASCII `CPD1` |
| 4 | 1 | version | `1` |
| 5 | 1 | kind | `1` configuration, `2` access unit |
| 6 | 1 | flags | kind-specific; unknown bits fail |
| 7 | 1 | header bytes | total header including these 12 bytes |
| 8 | 4 | body bytes | excludes the complete header |

`record bytes = header bytes + body bytes` exactly. The absolute body limit is
16 MiB. Unknown versions, kinds, header lengths, flags or nonzero reserved bytes
fail closed.

## Decoder configuration record

Kind is `1`, flags are zero and header length is 68 bytes.

| Offset | Bytes | Field |
|---:|---:|---|
| 12 | 16 | UUID Stream ID in RFC 4122 byte order |
| 28 | 8 | positive stream epoch |
| 36 | 4 | decoded width |
| 40 | 4 | decoded height |
| 44 | 4 | reduced frame-rate numerator |
| 48 | 4 | reduced frame-rate denominator |
| 52 | 4 | selected maximum access-unit bytes |
| 56 | 1 | AVC profile |
| 57 | 1 | AVC level |
| 58 | 2 | zero reserved bytes |
| 60 | 4 | SPS bytes |
| 64 | 4 | PPS bytes |
| 68 | variable | Annex-B SPS followed by Annex-B PPS |

Profiles are `1=Baseline`, `2=Main`, `3=High`. Levels are `1=3.1`, `2=4`,
`3=4.1`, `4=4.2`, `5=5`, `6=5.1`, `7=5.2`. SPS and PPS are individually
limited to 64 KiB and their sum must equal the body length.

The Rust decoder requires width, height, frame rate, maximum access-unit size,
profile and level to equal the already authenticated selection. Other decoded
Display semantics such as colorimetry, cursor mode and latency remain in the
Route selection and are not renegotiated by this record.

## Access-unit record

Kind is `2` and header length is 60 bytes.

| Offset | Bytes | Field |
|---:|---:|---|
| 12 | 16 | UUID Stream ID in RFC 4122 byte order |
| 28 | 8 | positive stream epoch |
| 36 | 8 | sequence |
| 44 | 8 | presentation time in microseconds |
| 52 | 8 | positive duration in nanoseconds |
| 60 | variable | one complete AVC Annex-B access unit |

Flag bit 0 marks a key frame, bit 1 an explicit discontinuity and bit 2 the
empty end-of-stream record. Bits 3–7 are reserved and must be zero. Ordinary
payload begins with a four-byte Annex-B start code and is bounded by both the
selected per-stream maximum and 16 MiB. End-of-stream has an empty body and no
other flags.

## Compatibility evidence

The committed lowercase-hex fixtures under `fixtures/display/` are decoded by
Rust and parsed by the dependency-free Android Java harness.
Mutation tests cover bad magic/version/kind, header and body lengths, trailing
bytes, reserved values, authenticated-selection mismatch, Java numeric bounds,
codec configuration bounds, malformed access-unit semantics, wrong bound
stream/epoch, pre-configuration input, duplicate/regressing sequence and
post-terminal input.

The original two fixtures contain synthetic AVC-shaped bytes and prove framing
only. The `real_v1_*` pair contains SPS, PPS and one IDR encoded from FFmpeg's
deterministic 1280×720 `testsrc2` frame by the pinned evidence generator. The
generator reconstructs SPS+PPS+IDR, decodes exactly one YUV420p frame and records
its size and SHA-256 in `real_v1_fixture_manifest.json`. This proves one host
software decoder accepts the encoded bytes; it does not prove Android device
decoding, Surface rendering, visual fidelity, latency or interoperability with
an unreviewed encoder.
