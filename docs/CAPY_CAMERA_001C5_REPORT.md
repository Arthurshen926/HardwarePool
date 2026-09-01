# CAPY-CAMERA-001C5 Windows AVC decode report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Base commit: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: C3-guarded Annex-B H.264 to bounded packed NV12 in the Windows
  loopback lab

## Outcome

`capyio-vcamdroid-adapter` now owns a Windows-only wrapper around the inbox
Media Foundation H.264 decoder MFT. The wrapper configures full input metadata
and Annex-B SPS/PPS, repeats those parameter sets in the first key-frame sample
after stream start or discontinuity, negotiates NV12 and handles synchronous
backpressure, output type changes, flush and drain. Output timestamps map back
to the accepted CAVC source sequence. Strided decoder buffers are normalized to
bounded packed NV12.

The existing receiver exposes this only through explicit `--decode-nv12` lab
mode. Each decoded frame is size-checked, hashed with FNV-1a and immediately
dropped. No pixels or encoded access units are retained, and no shared-memory
producer, COM registration, virtual-camera activation or driver operation is
invoked.

## Automated evidence

- `cargo test -p capyio-vcamdroid-adapter --all-targets`: nine tests passed.
- Config tests cover the canonical 1280x720 NV12 byte bound, Annex-B-only input
  and missing-start-code rejection.
- The existing CAVC framing/streaming/transactional-guard tests remain green.
- `cargo check -p capyio-vcamdroid-adapter --all-targets`: passed during the
  implementation cycle.

The first device decode attempt correctly failed at the 64-sample pending bound
because SPS/PPS were present only on the media type. Windows' decoder still
waited for parameter sets in the elementary stream. C5 therefore prefixes the
same bounded Annex-B sequence header to the first key frame after every decoder
start/flush. A later debug-mode run decoded frames but showed artificial sender
loss while hashing large frames; final physical evidence uses the optimized
receiver.

## Physical Windows/Android evidence

The already installed, hash-recorded Camera Lab APK on the explicitly selected
`100.66.157.119:33909` vivo V2419A / PD2419 was used without reinstall or
permission changes. The receiver ran as:

```text
cargo run --release -p capyio-vcamdroid-adapter --bin capyio-avc-lab-receiver -- --decode-nv12 --max-access-units 90
```

The authorized `tcp:38173 tcp:38173` ADB reverse mapping was re-established for
the run. The visible Activity was started and its `START CAMERA` button pressed.
The receiver reported:

```text
stream=81240382115de16fabc7277f26f116ea
epoch=89513292604920
size=1280x720 fps=30 bitrate=4000000
access_layout=AnnexB config_layout=AnnexB
access_units=90 key_frames=3 discontinuities=1
payload_bytes=6294877 last_sequence=91
decoded_frames=90 decoded_bytes=124416000
first_checksum=93a1c21993ea4869
last_checksum=f8ed18c181c4113e
last_source_sequence=91
```

The decoded byte total is exactly 90 × 1280 × 720 × 3/2. Distinct first/last
checksums establish changing decoder output buffers without asserting what the
camera viewed. After the run the app was force-stopped, Camera Service reported
`Active Camera Clients: []`, the reverse mapping was removed and the subsequent
reverse list was empty. The installed app remains present and force-stopped.

## Remaining gates

C5 proves real phone AVC reaches the inbox Windows decoder and becomes valid
packed NV12. It does not yet publish those frames through
`CameraProducerHost`, switch registered activation away from the deterministic
fixture or prove a Windows camera consumer sees the phone stream. Those are the
next integration and controlled system-boundary gates.
