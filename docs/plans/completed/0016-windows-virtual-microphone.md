# CAPY-MIC-001 — Windows `CapyIO Microphone`

Status: completed (controlled-lab functional Gate); release qualification moved to plan 0021

Owner: Codex and project owner

Created: 2026-08-28

Completed: 2026-08-29 in PR #14, merge commit `2145f9f`

Depends on: `CAPY-MIC-000A`, completed Speaker Gate 7B

## Objective

Expose one independent Windows capture endpoint named `CapyIO Microphone` and
replace its SysVAD test-tone input with bounded 48 kHz mono PCM received through
a dedicated `CapyIO Microphone Ingress` render endpoint.

## In scope

1. Restore only the minimal SysVAD microphone endpoint beside the existing
   Speaker endpoint and prove the package builds.
2. Define a versioned, fixed-capacity frame ring whose absence produces
   silence without blocking AudioDG.
3. Add an ingress render APO that downmixes decoded MicYou output into the ring.
4. Add a capture APO that consumes PCM frames and overwrites the
   capture output with silence on underrun/stale generation.
5. Make the CapyIO Windows service own the shared mapping and diagnostics.
6. Connect the MicYou Adapter boundary, then validate ordinary-app recording.

## Out of scope

- network, codec or reconnect logic in the driver/APO;
- copying or linking MicYou GPL source;
- automatic driver/APK installation;
- AEC, denoise and AGC performance claims before the raw path is proven;
- public signing, packaging or redistribution.

## Architecture constraints

- the capture Route is independent of Speaker permissions and lifecycle;
- the real-time APO callback performs bounded copies/zero-fill and atomics only;
- missing Broker, phone loss, underflow and invalid generation degrade to
  silence, never a wait or audio-service failure;
- MicYou may decode to mono/stereo, but the first projection epoch stores 48
  kHz mono float32 frames; the shared audio network baseline remains signed
  16-bit PCM at its transport boundary;
- MicYou's private transport remains AdapterManaged outside the audio service.

## Acceptance criteria

1. WDK x64 Release build and INF validation pass.
2. The package exposes Speaker render, microphone ingress render and exactly
   one application-facing CapyIO capture endpoint.
3. With no Broker the microphone records silence, not the SysVAD tone.
4. A deterministic injected tone is recorded by an ordinary WASAPI application.
5. Phone audio is recorded by an ordinary application and disconnect returns
   to silence within the bounded underflow window.
6. Install/restart/uninstall and rollback evidence is retained under ADR 0029.

## Safety

Source/build work is unprivileged. Every install, upgrade, uninstall or driver
tool invocation still requires explicit approval for the exact package.

## Current progress

- enumeration-only capture miniport and INF declarations added;
- x64 Release WDK compile/link/API validation passed with MSVC `/W4 /WX`;
- independent x64 InfVerif `/u` and `/w` passed; the known WDK embedded
  `x86\\InfVerif.dll` loader defect remains visible as three managed-task errors;
- ADR 0037 selects paired ingress/capture endpoints instead of a third-party
  virtual cable or in-process MicYou protocol copy;
- service-owned capture ABI, paired miniports and both real-time APO directions
  are source/build complete (`CAPY_MIC_001B_REPORT.md`);
- signed `21.60.0.1` was deployed on the approved `DESKTOP-AT8EVE9` lab and all
  three endpoints enumerated with healthy PnP status, but both render endpoints
  inherited the same generic speaker bridge-pin name;
- `21.61.0.1` assigns/registers independent bridge-pin names and repairs Release
  packaging; deployment proved correct KS names but Windows still hard-codes
  both render endpoints as Speakers because both used the Speaker category;
- `21.62.0.1` proved that the private ingress can use a non-speaker category,
  but the standard Line Connector category still retained duplicate MMDevice
  names after endpoint regeneration;
- `21.63.0.1` gives the ingress a registered CapyIO-specific Category GUID,
  but deployment showed the topology interfaces still associated every
  endpoint with `KSNODETYPE_ANY`;
- `21.64.0.1` explicitly associates each topology interface with its real Pin
  Category, but deployment showed the ingress still inherited Speaker identity
  through the shared integrated-speaker Jack Description;
- `21.65.0.1` removes Speaker jack automation only from the private ingress
  topology and is signed/deployed with all PnP nodes and services healthy;
- Windows still presents both render flows with the same localized Speaker
  label. The interim MicYou selector therefore preserved duplicate names and
  validated a freshly probed index/name pair; CAPY-MIC-001E later replaced its
  persisted index with a stable Core Audio endpoint ID;
- the real CPAL probe rejected the ingress mix format because it inherited
  capture-only `COMMUNICATIONS` and `SPEECH` MFX modes. Signed `21.66.0.1`
  isolates the ingress mode list to `DEFAULT`, `MEDIA` and `MOVIE`; deployment
  and post-install enumeration evidence remain pending;
- `21.79.0.1` restored the pinned SysVAD capture Pin mode/attribute contract.
  A valid high-integrity CPAL comparison opened both a physical USB microphone
  and `CapyIO Microphone`; the earlier medium-integrity sandbox harness failed
  on both and is not valid driver evidence;
- `21.81.0.1` corrected the capture APO exact-format return contract and was
  hot-deployed without a reboot. Ordinary CPAL capture proved zero RMS/peak
  without an ingress producer, then a 997 Hz local ingress closure recorded
  216,000 samples with 191,519 non-zero samples and peak `0.25`;
- the pinned MicYou Debug APK is installed on the approved Android device and
  the user granted microphone and notification permissions. The current
  separately built CLI persists the bounded endpoint ID plus expected name,
  resolves the index at launch, validates the full tuple around audio startup
  and exposes `device-stable-id-v1`;
- the real Android-to-Windows path is proven through an ordinary CPAL capture
  client. On signed/deployed `21.82.0.1`, 47,520 live samples measured RMS
  `0.01841975` and peak `0.10452271`; the corrected per-session transport loss
  metric stayed bounded and began at `0.00%` on a fresh connection;
- disconnect testing exposed a stale-backlog defect in `21.81.0.1`: a newly
  opened capture application could replay frames retained while no consumer
  was attached. `21.82.0.1` synchronizes the consumer to the current producer
  sequence at attach. The physical test retained 16,320 old frames before the
  new capture opened, then returned 47,520 samples with exact zero RMS/peak;
- the stable-ID physical regression independently recorded 47,520 live samples
  with RMS `0.00126867` and peak `0.00500488`. An intentional correct-ID/wrong-
  index tuple failed before startup, while disconnect again yielded 47,520
  exact-zero samples;
- CAPY-MIC-001F now registers the Android Source and Windows Sink in the desktop
  Runtime and exposes a schema-v2 microphone Quick Action. Hardware-free tests
  require stable process-owned phone TCP presence before `Active`, retain typed
  disconnect/start/timeout Problems, advance retry epoch and preserve an
  unrelated active IMU Route;
- CAPY-MIC-001G adds fixed host-only configuration and validates the current
  patched CLI plus stable ingress identity on every start;
- CAPY-MIC-001H physically exercised the normal desktop Quick Action through
  ordinary-client PCM, bounded disconnect silence, fresh-process retry and
  stop. Its eight-second WAV contained non-zero audio in every one-second
  interval, and the project owner confirmed the recording was audible;
- exact-head hosted Linux, macOS, Windows, repository, UI and Windows Tauri
  checks passed on PR #14 before merge commit `2145f9f`;
- installer, lifecycle, security, performance and distribution work continues
  only under plan 0021.
