# Finite Linux camera executable candidate — 2026-10-21

## Scope

This source candidate is based on accepted touchpad commit
`fda6080eb9e58844ebe52d6d8c957c22fd6de9da`. ADR 1536 adds an explicit actual
`capyio-node linux-camera --trusted-lab` entry, composing the existing secure
camera owner and fixed native decoder/PipeWire helpers. It does not alter the
legacy audio CLI, create an Android camera picker, grant physical permissions,
install packages/devices or change system security/routing.

The boundary uses strict private framed bootstrap, exact local request/Route
approval, a real Linux Sink and authenticated selected Android Source catalog,
independent finite grants, both leases and dynamic-port pinned camera TLS.
Source objects also reject positional arrays, duplicate and unknown fields.
Queued pixels do not promote the Runtime beyond Starting. Limits remain canonical
720p30 rotation-zero Annex B, 60 seconds, 32 MiB and 120 frames without renewal.

## Local checks actually run

- Linux host library: 45 tests passed, including both camera catalog/schema tests.
- Actual node camera subprocess: 13 tests passed, one hosted-only graph test
  intentionally ignored outside its explicit private-daemon harness. The tests
  use real pinned control/data and Android camera consent/lease policy with
  ephemeral synthetic identities, not physical Camera2 capture.
- Boundaries exercised include exact/stale/malformed approval, independent Android
  denial, strict config, same-paired-node missing catalog Source, control/data
  pins, denied/missing Source lease, no helpers before data binding, local Stop,
  preapproval EOF and partial bootstrap deadlines.
- Native node feature forwarding and both separately built sibling helpers
  compiled with the existing isolated official development headers/runtime.
  Production helper binaries were not executed against a camera or user graph.
- Strict default and native node all-target Clippy passed, as did native host
  library Clippy and the native node test-target build.
- Launcher Python harness: eight pure assertion tests passed. They reject wrong
  parentage, malformed/duplicate markers, missing readiness, incorrect terminal
  success, and distinguish handshake-only from actual graph evidence.
- Documentation traceability passed (88 unique requirements); adapter manifests
  passed (two manifests); Rust formatting passed.
- Existing audio regressions: capture 14 passed/one hosted-only ignored; playback
  26 passed/two hosted-only ignored; passive preflight one passed.

## Local limitations and required hosted evidence

The actual private PipeWire graph was not run locally: required daemon/tools
(`/usr/bin/pipewire`, `/usr/bin/pw-dump`) are absent. The prerequisite check fails,
not skips into a success marker. No alternate access route or host installation
was used. Native compile and pure harness tests are not pixel-delivery proof.

Existing `linux_host` regression execution initially lacked the mock sibling
binaries. After building them, ten tests passed but the two unchanged Unix-local
payload/batch lifecycle tests still failed at their first expected Active Route
(the response had no result Route). The exact cause was not established locally;
this is an observed failure, not a claimed pass or proven security restriction.
Their code and production mock paths are unchanged and remain required hosted
regressions. The complete workspace CI suite was not represented as locally green.

The existing secure-camera workflow retains its old owner/decoder cases and adds
required named node subprocess tests plus actual executable private-daemon cases:
pixels, local Stop, remote Stop, control EOF, Source revoke, malformed CAVC,
finite expiry, and stalled handshake. The harness checks actual node child
identity, exact native-helper parentage/executable paths, no premature media
children, 20 independent NV12 frames, graph/sentinel isolation and owned teardown.
Handshake-only evidence explicitly reports `real_graph=false`. All existing seven
jobs remain required. Publication and hosted results must be recorded separately;
this report does not predeclare hosted acceptance.

Physical camera, Java Camera2/permission UI, third-party app compatibility, device
installation and input/uinput/kernel behavior remain deferred.
