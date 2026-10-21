# Finite Linux camera executable acceptance — 2026-10-21

## Scope

Accepted source commit `bbf6531731f27e7243f10fa4a2e80ae780f14b74` builds on
touchpad `fda6080eb9e58844ebe52d6d8c957c22fd6de9da`. ADR 1536 adds an explicit actual
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

## Local limitations and subsequent hosted acceptance

The actual private PipeWire graph was not run locally: required daemon/tools
(`/usr/bin/pipewire`, `/usr/bin/pw-dump`) are absent. The prerequisite check fails,
not skips into a success marker. No alternate access route or host installation
was used. Native compile and pure harness tests are not pixel-delivery proof.

Existing `linux_host` regression execution initially lacked the mock sibling
binaries. After building them, ten tests passed but the two unchanged Unix-local
payload/batch lifecycle tests still failed at their first expected Active Route
(the response had no result Route). The exact cause was not established locally;
this is an observed failure, not a claimed pass or proven security restriction.
Their code and production mock paths are unchanged. The subsequent accepted
hosted Linux IPC job passed these legacy tests and the new camera tests; that
resolves the hosted gate, not the unexplained local outcome. The complete
workspace CI suite was not represented as locally green.

The existing secure-camera workflow retains its old owner/decoder cases and adds
required named node subprocess tests plus actual executable private-daemon cases:
pixels, local Stop, remote Stop, control EOF, Source revoke, malformed CAVC,
finite expiry, and stalled handshake. The harness checks actual node child
identity, exact native-helper parentage/executable paths, no premature media
children, 20 independent NV12 frames, graph/sentinel isolation and owned teardown.
Handshake-only evidence explicitly reports `real_graph=false`.

At accepted commit `bbf6531731f27e7243f10fa4a2e80ae780f14b74`, all seven hosted
jobs passed. [Secure-camera run 39576272282](https://github.com/Arthurshen926/HardwarePool/actions/runs/39576272282)
(job 119142453298) passed all eight actual-node scenarios, retaining all prior
secure-owner/decoder scenarios. The Linux IPC job 119142454487 passed the legacy
and new named subprocess cases. The initial candidate's exact stderr assertion
was affected by inherited CI backtrace settings; the accepted correction sets
both backtrace variables to zero only for fixture children. Production error
redaction and all exit/cleanup assertions remain unchanged. All 13 cases also
passed locally with both parent backtrace settings enabled.

The executable path is therefore exercised end to end: inherited bootstrap and
approval → actual pinned Android control/policy and synthetic secure CAVC →
production decoder → private PipeWire camera → independent pixel consumer.
This is more than library/example reachability, but is not user-session or
physical-camera acceptance. The application's Runtime remains Starting when
frames are merely queued; the independent consumer result is test evidence,
not automatic production application-consumption telemetry.

Physical camera, Java Camera2/permission UI, third-party app compatibility, device
installation and input/uinput/kernel behavior remain deferred.
