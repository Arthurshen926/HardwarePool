# ADR 1533: Bounded offline AVC into the existing Linux camera owner

- Status: accepted offline software slice; no physical or network claim
- Date: 2026-10-21

## Decision and boundary

Add optional `platform/linux/capyio-avc`, not a second projection or listener.
`RuntimeAvcCamera` owns `RuntimeCamera` and one fixed-name sibling decoder child.
The constructor first admits the existing Runtime Session, Route, finite consent,
local Sink lease, stream and epoch through `RuntimeCamera::start`. Each decoder
wait polls that exact owner; each returned frame goes through its production
`submit` validation. No decoder writes PipeWire directly. Stop, invalid input,
decoder failure, EOS, expiry and Drop stop both owned children. Unrelated routes
are not stopped. Runtime mutation stays explicit through `runtime_mut`.

Input is one complete bounded offline CAVC record, not a blocking generic reader
on the control owner thread. Reuse adapter `decode_record`, `AvcRecordGuard` and
`AvcStreamKey`. Unique config, stream/epoch, sequence, PTS and EOS remain checked.
Network authentication, Camera2 access, listeners, Android live transport and
physical capture remain separate work. This private child media protocol is not
an Adapter Sidecar JSON-RPC control endpoint or a public network protocol.

## First supported contract

Only explicit Annex B CSD/access units, 1280x720 at 30fps, rotation zero,
progressive 8-bit 4:2:0, BT.709 limited. Combined CSD is at most 64 KiB; AU 4 MiB.
The helper inspects bounded SPS/PPS/slice headers before decoder allocation,
then validates actual AVFrame dimensions, progressive flags, pixel format,
color fields, keyframe, PTS, crop and errors. Supported profiles are Baseline/Main/High with at most four references, no
scaling matrices, FMO, redundant pictures or weighted-B prediction.
Identical repeated SPS/PPS are
allowed; changed parameter sets are rejected. No scaling, rotation, tone mapping,
libswscale, libavformat, demuxer, URL or generic ffmpeg runtime command exists.
Unspecified bitstream color fields inherit the verified fixed CAVC contract;
explicit conflicting fields fail. YUV420P to NV12 is checked plane copy and UV
interleave, not a color conversion. Negative/short/out-of-buffer strides fail.

This slice deliberately requires no-delay, exactly one decoded picture per AU.
Real send/receive operations are drained to EAGAIN with a bounded output count;
zero/multiple outputs, decoder reorder/B-frames and delayed EOS frames fail
explicitly. It does not equate arbitrary H.264 packets with frames. Only actual
IDR slices can substantiate keyframe/recovery flags; discontinuity flushes prior
references. The whole owner stops after a violation; no corrupted stream resumes.
PTS uses checked microseconds x1000, at most i64::MAX, with the existing exact
33,333,333ns frame duration. Rational timing improvements are out of scope.

## Resource and lifecycle bounds

- Parent lifetime: 1 ms..60 s; per-IPC operation absolute deadline: 2 s.
- Decoder independently enforces the requested lifetime with ITIMER_REAL.
- Child limits: 512 MiB address space, 10 CPU seconds, 256 MiB regular file output,
  no core dump; one decoding thread and no frame threading.
- Input cumulative 32 MiB, maximum 120 frames, at most 256 NALs per AU, bounded
  Exp-Golomb and SPS loops. Returned NV12 frame is exactly 1,382,400 bytes.
- Parent owns a private Unix socketpair, no filesystem socket or listener. Its
  endpoint is nonblocking, preserves partial record offsets, and polls authority
  between progress attempts. Neither partial input nor partial output is dropped
  or reused as a new record. Complete frames alone reach camera backpressure.
- Fixed regular sibling helper must match executable owner and not be group/world
  writable; no peer-supplied program/path/environment. Environment is cleared.
- Owned kill/reap is bounded to 1 s; cleanup failure is returned, not called success.
  Decoder death stops projection; projection revocation stops decoder.

## Dependency and distribution decision

Default builds need no FFmpeg headers or library. `native-avc` opts into C code
linking libavcodec/libavutil; `native-pipewire` additionally composes the existing
camera native feature. C is isolated in the decoder executable; the Rust owner
continues to forbid unsafe code. `avcodec_find_decoder(AV_CODEC_ID_H264)` selects
only the native H.264 decoder.

Use official distribution development packages for isolated software validation,
record their exact versions, source, hashes and actual library configuration and
license. This is a pragmatic test dependency, NOT an LGPL redistribution claim:
distribution FFmpeg builds often enable GPL components. No FFmpeg source, decoder
binary, libraries or release package are redistributed by this slice, and no
release/license acceptance flow is introduced. Before any distribution, a
separate dependency/license review must select and record an appropriate build.
A minimal pinned official shared FFmpeg build with GPL/nonfree/autodetect/network
and unrelated components disabled is an alternative, deferred from this slice.

Official upstream: https://ffmpeg.org/ ; API state machine:
https://ffmpeg.org/doxygen/7.1/group__lavc__encdec.html ; license/configuration:
https://ffmpeg.org/legal.html . No local modifications to upstream libraries.

## Verification and truthful claims

`fixtures/linux-avc/solid-720p30.cavc` contains only generated solid colors.
`scripts/generate_linux_avc_fixture.py` records encoder command, version and
configuration, input and output hashes and decoded NV12 pixel oracle. The
GPL-enabled ffmpeg/libx264 encoder is a fixture-generation tool only, never a
product subprocess. Reproduction is pinned by the recorded tool version/config;
a newer encoder may generate different compressed bytes.

Pure Rust admission/header tests run in default builds. Native decoder tests
explicitly opt into isolated execution and verify pixel planes, order/PTS,
malformed bitstreams, resource expiry, revoke, child exit and cleanup. Compile
success and ignored listings are not native execution evidence. The separate Linux AVC workflow requires eight private
PipeWire graph scenarios through this same `RuntimeAvcCamera`; its pixels
scenario verifies 20 actual frames with an independent native consumer. The
existing camera owner graph is accepted on commit 23bcc2f0. The AVC-composed
graph passed its own hosted result at commit `30d0a59058b5e2426652fa5f3e16ee1c9ffb0457`,
[run 39564578158](https://github.com/Arthurshen926/HardwarePool/actions/runs/39564578158). Decoder-only evidence is
not system-camera delivery or Android physical-device evidence.
