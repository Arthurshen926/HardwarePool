# CAPY-CAMERA-001C26 — registered late-producer handoff

Date: 2026-08-31

Status: implementation, exact release packaging, focused Windows tests,
warnings-denied Clippy and full repository CI complete. No COM DLL deployment,
virtual-camera registration, APK operation or physical camera run occurred in
this slice.

## Trigger

Registered Media Foundation activation previously selected its frame provider
exactly once. A valid production Global mapping selected live shared frames; an
absent mapping selected the deterministic fixture permanently. Consequently a
Windows application that activated `CapyIO Camera` before the phone/receiver
published its mapping could never transition from the fixture to that later
live producer without reopening the media source.

## Outcome

- Activation behavior is unchanged when the fixed, validated production
  mapping already exists.
- When `OpenFileMappingW` reports only exact file-not-found, the registered
  source starts a late-bound provider. It emits the existing deterministic
  720p30 color-bar fixture as an explicit offline placeholder.
- The provider checks only the fixed production mapping after a fixed
  15-placeholder-frame countdown. The check runs through the existing Media
  Foundation serial work queue; `RequestSample` does not wait for it or perform
  network, codec, file or control-plane work.
- The first validated shared frame switches the provider one-way to live mode.
  Its payload is retained, while stream ID, epoch, sequence and timestamp are
  rebased onto the already active virtual-camera timeline. The handoff frame is
  marked discontinuous.
- After live mode begins, an empty publication interval retains the existing
  bounded asynchronous request and 5 ms retry behavior. Placeholder frames are
  never interleaved into a live stream.
- Access denial, malformed shared state and every mapping error other than exact
  file-not-found still fail closed.

The late-bound provider accepts no caller-controlled mapping name in production.
Its Local mapping target exists only inside the feature-gated test surface.

## Automated evidence

- `cargo test -p capyio-windows-camera-mf` — PASS. Seven library tests include:
  - placeholder output before the mapping exists;
  - exact fixed-count probing and late local-test mapping attachment;
  - output identity, epoch, sequence and timestamp continuity;
  - discontinuity on the first live frame;
  - no placeholder interleaving after live mode starts;
  - fail-closed behavior for an invalid mapping target.
- `cargo clippy -p capyio-windows-camera-mf --all-targets -- -D warnings` — PASS.
- Repository structural validation pins the fixed interval, production-only
  target, asynchronous provider selection, handoff and fail-closed tests.
- `cargo xtask ci` — PASS, including workspace format/check/Clippy/tests,
  documentation/manifests, Adapter smoke, repository validation and desktop
  typecheck/build.
- `scripts/capyio-camera-live-lab-preflight.ps1` — PASS against the newly
  hash-locked artifacts and clean host state without elevation or mutation.

## Exact release artifacts

- Receiver, 249,856 bytes (unchanged from C24):
  `0A7315F806B249BE9FFCDAEBCF326399AF7B063FF487B19EB6898CA7B54E2967`.
- Virtual-camera lab, 323,072 bytes:
  `6E1156864F9B668DC1A39A2F308569CC0F69C396D1FDD5515A6095481A0F31A2`.
- COM DLL, 192,512 bytes:
  `527C3A1D4A233BB9553BF0757BA91AA090814727E26E884A3280551372FC10CA`.

The parameter-free read-only preflight and administrator deployment script now
pin these exact C26 artifacts. Preparing and hashing them made no system change.

## Remaining evidence and limits

A separately authorized system regression is required to prove that an ordinary
Windows application can open the camera while only the placeholder exists and
then observe later V2419A pixels without closing or reopening that application.

This slice does not make camera registration persistent, move ownership into a
Windows service, detect producer death after the one-way handoff, add an offline
brand/status overlay, change Android lifecycle/permissions, or add pairing and
encrypted transport.
