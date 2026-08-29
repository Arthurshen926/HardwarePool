# CAPY-MIC-001E — Stable MicYou endpoint identity report

Date: 2026-08-29

Status: completed in PR #14; release qualification remains separate

## Outcome

MicYou output selection no longer persists a reorderable one-based Windows
device index. The CapyIO Adapter stores a bounded Core Audio endpoint ID plus
its expected display name, resolves the current CPAL index immediately before
each launch and passes an exact ID/index/name tuple to the reviewed local CLI.
The CLI validates that tuple both before and after audio startup.

The index remains necessary only because the pinned CPAL version exposes
duplicate localized names without the Windows endpoint ID. It is now an
ephemeral coordinate, not endpoint identity.

## Contract and failure behavior

- the required capability is `device-stable-id-v1`;
- the versioned inventory carries current one-based index, stable endpoint ID
  and display name with explicit count and size bounds;
- persisted configuration has no output-device index;
- duplicate IDs, missing IDs, renamed endpoints, malformed rows, control
  characters and inconsistent ID/index/name tuples fail closed;
- reordering an otherwise identical inventory resolves the configured ID to
  its new index;
- display names remain diagnostic confirmation and are not treated as unique.

The local-only pinned MicYou CLI patch uses `IMMDevice::GetId` through
`windows` 0.61.3 (MIT OR Apache-2.0). That crate was already present
transitively in the pinned source. No MicYou source, patch or binary is copied
into or distributed by this repository.

## Automated evidence

- `cargo test -p capyio-micyou-adapter`: 5 unit tests and 2 fixture integration
  tests passed; one physical/real-CLI test remains ignored by default;
- the ignored exact real-CLI probe passed with `CAPYIO_MICYOU_CLI` pointing to
  the separately built pinned executable;
- `cargo test -p micyou-cli --no-run`: passed for the local pinned source;
- `cargo build -p micyou-cli --release`: passed;
- final `cargo xtask ci`: passed, including workspace format/check/Clippy/tests,
  documentation and manifest validation, repository structural validation,
  Adapter smoke, desktop typecheck and production UI build;
- local CLI SHA-256:
  `6FAEC6AFCBA0A21D574035D4E9B05CFD967B7D1C69C2256937D2195C54E8AE5D`.

## Physical regression

On the approved `DESKTOP-AT8EVE9` lab, the private microphone ingress had
endpoint ID
`{0.0.0.00000000}.{c7e4cce3-d84e-4985-bca2-bed51be21971}` and currently
resolved to one-based index 5. Launching with the correct ID/name but index 6
failed before the server or lock started. Launching the mutually consistent
tuple started TCP control on 8554 and UDP audio on 8555.

With the authorized Android MicYou client connected, an ordinary Windows CPAL
capture session received 47,520 samples over 99 callbacks with RMS
`0.00126867` and peak `0.00500488`. Capture-ring diagnostics retained
`last_error = 0`. After explicit phone disconnect, a new 47,520-sample capture
reported exact-zero RMS and peak, again with `last_error = 0`.

No driver, APK, permission, Windows audio-service, boot-policy or reboot change
was made for this slice.

## Remaining work

The stable selector closes endpoint reordering as a local functional risk. It
does not finish Gate 8 release qualification. Remaining work includes
Runtime/UI lifecycle integration, Android lock/background and permission-
revocation tests, normal-reboot survival, latency/loss/glitch characterization,
and legal/distribution decisions for the GPL process and local patch.
