# CAPY-CAMERA-001C23 — bounded ADB-free trusted-LAN camera lab

Date: 2026-08-31

Status: implementation, offline Android build, focused tests, full repository
CI and clean-package preflight complete. An authorized exact-package run proved
the no-ADB trusted-LAN decode/publication path and temporary camera enumeration;
ordinary Windows Camera pixels and final Android stop remain incomplete.

## Outcome

The camera lab no longer requires ADB reverse as its only representable
transport. Blank Android input remains the compatible loopback/ADB path. A user
may instead enter one canonical Windows IPv4 literal in RFC1918, link-local or
100.64.0.0/10 space; the outbound sender then connects to fixed TCP port 38173.
The value is bounded, not persisted and never resolved through DNS.

The Windows receiver exposes trusted-LAN mode only when both an exact local bind
IPv4 and a different exact phone peer IPv4 are supplied. It binds only that
interface, keeps the port fixed and closes every other peer before CAVC parsing.
`capyio-camera-virtual-lab trusted-lan-live-hold <bind-ipv4> <phone-ipv4>`
passes only those values to the fixed sibling receiver and retains the existing
mapping readiness, 60-second Session/CurrentUser camera hold, liveness checks
and cleanup behavior.

This is an explicit plaintext trusted-lab mode. It is not production pairing,
authentication or encryption. ADR 0042 records that boundary and the remaining
mutual-authentication, authenticated-encryption, Route/Session/replay and
downgrade requirements.

## Implementation evidence

- `CameraTransportEndpoint` is Android-free, immutable and covered by the
  no-dependency contract test. It accepts only the two closed modes and returns
  defensive address copies.
- The Activity adds one length-bounded, non-persisted destination field and
  disables it throughout start/stream/permission states.
- Camera and codec callbacks remain socket-free; the existing bounded exporter
  worker alone connects and writes.
- Receiver CLI tests cover paired bind/peer arguments, exact address classes,
  fixed port, distinct addresses and IPv4-only exact peer admission.
- Windows orchestration tests cover both the parameter-free ADB path and the
  exact two-address trusted-LAN command.
- Repository validation pins the endpoint contract, non-name-resolving Android
  socket, exact receiver allowlist, non-wildcard bind and closed orchestration.
- No dependency, CAVC record, Core/Protocol type, Android permission, service,
  driver or firewall change was added.

## Automated evidence

- Android offline verification:
  `gradle --offline contractTest :app:assembleDebug :app:lintDebug` — PASS.
  The known host JDK ZipFS/R.jar sandbox issue required the existing approved
  out-of-sandbox offline build path; no network was enabled.
- APK manifest inspection with Android Build Tools 36.0.0 — exactly CAMERA and
  INTERNET; no new permission.
- `cargo test -p capyio-vcamdroid-adapter --bin capyio-avc-lab-receiver` —
  5 passed.
- `cargo test -p capyio-windows-camera-mf --bin capyio-camera-virtual-lab` —
  5 passed.
- `python scripts/validate_repository.py` — PASS, 84 Requirement IDs.
- `cargo xtask ci` — PASS, including format, workspace check, Clippy with
  warnings denied, tests, docs/manifests, Adapter smoke and desktop build.
- `scripts/capyio-camera-live-lab-preflight.ps1` — artifact hashes, deployment
  lock, ProgramData/CLSID state, TCP 38173 and lab-process cleanliness all PASS.

## Exact artifacts

- Android debug APK, 2,720,370 bytes:
  `EA8061284E93A3A49D75D343A761F1C7A8502731ECAB3C1B312990342EC8C914`.
- `capyio-avc-lab-receiver.exe`, 249,856 bytes:
  `D2CFE27A17E8A9317096679265A318B898A5704CBF868D4ECC6EADAA0BAED9AB`.
- `capyio-camera-virtual-lab.exe`, 322,048 bytes:
  `9358936D8EFC471503E1063AFE549215AAEA3BADB7437BD2D941BC0435530871`.
- `capyio_windows_camera_mf.dll`, 190,464 bytes:
  `0AA8C2F8119059EBF0087E04AC8CBAAAF290C8E641C4535C18E7BC69F6375EE4`.

The C23 deployment and preflight used these exact hashes. The current scripts
may pin a later package; historical reports retain the artifacts they actually
tested.

## Authorized physical evidence

The exact C23 APK was installed on the approved V2419A and the exact Windows
package was temporarily deployed on `DESKTOP-AT8EVE9`. ADB was used only for
installation; no `adb reverse` mapping was created. Private interface/device
addresses are intentionally omitted from repository evidence.

The trusted-LAN receiver admitted the configured phone peer and reported:

- one canonical 1280x720, 30 fps, 4 Mbit/s Annex-B stream;
- 358 accepted access units and 12 key frames;
- 358 decoded and 358 Global-mapping publications;
- distinct first/last decoded checksums
  `e37cd5e7428fefe0` / `8908ca9301f2524c`;
- one exact `CapyIO Camera` Session/CurrentUser enumeration match; and
- `decoder_low_latency=true`, zero maximum pending decoder samples and bounded
  decode/publication timing aggregates.

This proves physical Android capture/encode, no-ADB trusted-LAN transfer,
Windows decode, shared publication and Media Foundation enumeration. The
Windows inbox Camera application was opened after the fixed hold had ended, so
it displayed `NoCamerasAreAttached`; no C23 ordinary-application pixel claim is
made.

The run exposed short initial/reconnect windows and an error-precedence defect:
an external consumer could retain the mapping while `live-hold` returned the
earlier receiver-exit error, causing later preflight refusal. C24 fixes those
bounds and makes cleanup failure authoritative. The exact C23 Windows DLL/CLSID
deployment was subsequently removed; ProgramData, registry, TCP 38173 and lab
process checks all returned clean.

## Physical evidence still required

The remaining completion evidence is one coordinated run that:

1. observes advancing samples and visibly changing pixels in an ordinary
   Windows camera application;
2. explicitly stops the Android foreground capture; and
3. confirms mapping, registration, listener, process and ProgramData/CLSID
   cleanup after the consumer closes.

If Windows blocks the exact-interface listener, any narrowly scoped inbound
firewall rule and its removal require a separate explicit approval. Private
device/interface addresses are intentionally not retained in repository
evidence.
