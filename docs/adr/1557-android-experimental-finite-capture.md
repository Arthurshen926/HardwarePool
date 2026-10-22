# ADR 1557: Explicit experimental Android finite capture eligibility

Status: implementation and hardware-free acceptance; not physical-device proof.

The orientation-zero-only restriction below describes this initial slice.
[ADR 1558](1558-android-finite-camera-pixel-orientation.md) supersedes that restriction
with an actual finite-only GPU pixel-normalization stage, not a metadata relabel.

## Scope

ADR 1556 added the actual Rust finite sender, but intentionally left Java capture
and advertisement legacy-only. This increment adds an explicit, in-memory Android
camera setting for the existing `cavc-avc-finite-720p30-v1` descriptor. Legacy is
still the default. No new media protocol, permission, foreground service, automatic
Start, resource grant, lease renewal or fallback is introduced.

The experimental option requires an explicitly selected camera with sensor
orientation exactly zero. Missing orientation, 90/180/270-degree sensors and dynamic
orientation maps are not treated as zero. There is no pixel rotation stage. This
restriction likely excludes ordinary phone cameras; the option is not presented
as general phone compatibility or an unlimited continuous-camera mode.

## Eligibility is not output verification

A metadata-only probe pins one selected camera and one AVC encoder. It requires
exact 1280×720 output, an advertised fixed [30,30] AE range and sufficient stream
minimum-frame-duration support, surface input, AVC Baseline/Level 3.1, and support
for the exact configured size, rate and bitrate. Automatic source substitution,
zoom-based lens switching and codec fallback are not allowed for this profile.
The same pinned selection is revalidated before opening capture.

These checks establish eligibility to attempt a finite session, not proof that
requested encoder keys were honored or that a receiver delivered pixels. Only
an explicitly enabled eligible profile may be advertised. Selecting legacy again
restores legacy advertisement; a finite failure does not silently downgrade under
the same consent. Source/profile/quality changes stop the current camera use and
invalidate its pending generation before publishing a changed catalog. A new
session still requires fresh existing authorization. Unsupported finite selection
retains an unavailable local placeholder, with neither the legacy nor finite format
available for sharing. It does not violate the registry’s nonempty-module contract.
Live settings controls remain disabled until Stop; a direct edit during asynchronous
shutdown can be rejected and require an explicit retry, not seamless live switching.

## Actual output gate

Finite capture requests the exact size and AE range, an explicitly selected
Baseline encoder, zero B frames, and BT.709 limited-range SDR. Missing or mismatched
required actual MediaFormat fields fail closed for finite capture, without changing
legacy behavior. Before any finite configuration/media reaches JNI, a bounded
portable Java validator checks actual Annex-B SPS/PPS and AU slice headers:

- Baseline 8-bit 4:2:0, progressive uncropped 1280×720 and matching parameter IDs.
- Explicit BT.709 limited-range VUI and fixed 30 fps bitstream timing.
- Immutable parameter sets, IDR-first ordering, sequence/PTS continuity and no
  B/SP/SI slices or unsupported parameter-set changes.
- Existing payload bounds, bounded bit parsing and terminal failure with no retry.

Only bounded parameter syntax and the first three slice-header fields are parsed;
full slice decoding, SEI interpretation and pixel correctness remain receiver work.
This is intentionally a stricter sender subset than the Linux decoder's acceptance
of some unspecified VUI fields. Identical parameter sets may repeat; mutation is
not hidden by the legacy first-config cache. Actual-format verification must occur
before a codec-config buffer can authorize output. A parsed bitstream is not proof
of physical sensor color accuracy or successful decoding on a real phone.

The existing two-record reservation-before-copy budget remains. AU admission uses
read-only payload views, bounded NAL counts and a small slice-header scratch buffer;
parameter-set parsing uses bounded sub-megabyte transient scratch. This remains
within the existing 64 MiB application-owned payload budget, not a total JVM/GC or
codec-driver allocation bound. There is no whole-session accumulation. Rust independently enforces original Source authority,
monotonic duration, frame/byte work ceilings and rate buckets; Linux retains its
independent native decoding and output gates. No Java status or counter synthesizes
receiver Active or consumer delivery. The existing 32 MiB total byte ceiling and
native CPU/work limits can end a session before its separate maximum deadline.

## Evidence and boundaries

Hardware-free Java tests use fake capability descriptions, committed non-personal
AVC fixtures and fake transports. They exercise eligible/ineligible metadata,
actual SPS/PPS/AU admission, invalid output producing no records, mutation, finite
failure and unchanged legacy behavior. Hosted CI compiles the actual Android Java
against its preinstalled SDK and ARM64 JNI, and retains the production Rust sender
through native Linux/PipeWire synthetic graph tests. These are distinct proofs:
there is no Camera2 device execution, phone deployment, physical capture, user
keyring access, persistent credential creation or Android permission change.

Primary API references:

- [Camera characteristics](https://developer.android.com/reference/android/hardware/camera2/CameraCharacteristics), including supported AE ranges and dynamic sensor orientation.
- [Capture request](https://developer.android.com/reference/android/hardware/camera2/CaptureRequest), including AE target range and stream-duration constraints.
- [Video capabilities](https://developer.android.com/reference/android/media/MediaCodecInfo.VideoCapabilities), whose size/rate support is not a measured output guarantee.
- [Media format](https://developer.android.com/reference/android/media/MediaFormat), distinguishing requested format keys from actual output format.
