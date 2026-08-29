# CAPY-MIC-001E — Stable MicYou Windows endpoint identity

Status: local functional acceptance complete; merge pending; release qualification moved to plan 0021

Owner: Codex and project owner

Created: 2026-08-29

Depends on: `CAPY-MIC-001D`

## Objective

Replace the persisted MicYou one-based output-device index with a stable,
bounded Windows endpoint ID while retaining an index only as a freshly resolved
launch coordinate. This prevents an audio-service reload or endpoint hot-plug
from silently exchanging the two localized, same-name CapyIO render endpoints.

## In scope

1. Add a versioned `device-stable-id-v1` capability to the reviewed local
   MicYou CLI patch.
2. Return a bounded inventory of stable endpoint ID, current one-based index
   and expected display name.
3. Make the CapyIO Adapter store ID plus expected name, resolve the current
   index immediately before each spawn and fail closed on missing, duplicate or
   renamed identity.
4. Pass ID, current index and expected name to MicYou and validate the Windows
   endpoint inventory both before and after audio startup.
5. Prove hardware-free reorder behavior and repeat the physical microphone
   closure without a driver update or reboot.

## Out of scope

- changing the CapyIO driver package or installing another driver;
- exposing raw endpoint IDs to the WebView;
- treating a Windows endpoint ID as a CapyIO Node, Capability or Port ID;
- distributing the locally patched GPL executable;
- solving Windows friendly-name localization in this slice.

## Acceptance criteria

1. Persisted configuration contains no device index.
2. Reordering an otherwise identical inventory resolves the same endpoint ID to
   its new current index.
3. Missing, duplicate, malformed, renamed and control-character IDs fail
   before child spawn.
4. The child receives a mutually consistent ID/index/name tuple and validates
   it around audio startup.
5. Adapter tests, local MicYou tests, repository validation and full CI pass.
6. A physical Android-to-Windows capture records non-zero samples through the
   stable-ID selection; disconnect still returns to exact silence.

## Safety

Source, build and ordinary process tests are unprivileged. This slice does not
authorize a driver/APK update, Android permission change, reboot, signing,
commit, push or pull request.

## Completion evidence

- Adapter unit/fixture tests pass, including inventory reorder and duplicate-ID
  rejection.
- The ignored real-CLI probe accepts the separately built pinned MicYou CLI
  exposing `device-stable-id-v1`.
- On `DESKTOP-AT8EVE9`, endpoint ID
  `{0.0.0.00000000}.{c7e4cce3-d84e-4985-bca2-bed51be21971}` resolved to the
  current one-based index 5. Passing that ID with index 6 failed before server
  startup; the correct tuple started normally.
- Physical Android audio produced 47,520 captured samples with RMS
  `0.00126867` and peak `0.00500488`; after explicit disconnect, a new 47,520-
  sample capture had exact-zero RMS and peak.
- No driver, APK, permission, audio-service or boot-policy change was made in
  this slice.
