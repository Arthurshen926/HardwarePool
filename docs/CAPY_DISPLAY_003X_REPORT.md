# CAPY-DISPLAY-003X report — Phase 1 display closeout

Date: 2026-09-16

## Outcome

The single-display Phase 1 media path is complete at the controlled-lab level. One
authorized Android Display Sink can become an independent Windows virtual
monitor, receive its desktop through the mature GameStream-derived video-only
engine, decode HEVC in hardware and present frames. Windows remains responsible
for choosing extend or duplicate; CapyIO does not implement separate media
paths for those topology choices.

The production projection direction uses the pinned external SudoVDA driver
rather than the earlier CapyIO IDD experiment. The media Adapter uses the
pinned Vibepollo-derived display-only boundary and excludes audio, input,
discovery, UPnP and foreign-monitor cleanup. The Android receiver uses the
Moonlight common-C-derived GameStream path behind the CapyIO application and
credential boundary.

The Windows service authenticates the receiver over TLS 1.3, binds Route,
Stream, epoch, Capability and exact output identity, launches the hash-locked
worker in the authorized interactive session, and retains the worker channel
as the media lease. The final V2419A run used HEVC hardware decoding at
2640x1216 and 120 Hz and observed frames presented.

Vibepollo coexistence is now demonstrated rather than inferred. Its virtual
display and the CapyIO display remained active together. The optional ADR 0078
control temporarily disabled the Philips physical monitor while retaining both
virtual monitors, then restored the physical path. Sunshine's main process and
session survived; its WGC helper reinitialized capture at each global topology
change, so a brief disturbance remains expected during the toggle.

## Evidence

- Final display E2E:
  `target/evidence/single-display-gamestream-e2e-final/result.json` and
  `closeout-result.json` (media, topology and aggregate `passed: true`).
- Final physical-display policy:
  `target/evidence/physical-display-suppression-closeout/result.json`
  (`passed: true`).
- Legacy-driver retirement:
  `target/evidence/legacy-capyio-display-retirement/retirement.json`
  (`passed: true`) and its hash-locked rollback package.
- Final teardown restored automatic `CapyIOBroker`, removed
  `CapyIO-SingleDisplayGameStream-E2E`, and left no display worker.
- Windows display library: 70 tests passed.
- Windows service: 37 tests passed, two physical gates ignored by design, and
  five real worker-handshake tests passed.
- Desktop host: 24 tests passed and four explicitly physical tests remained
  ignored outside their lab gates.
- Workspace `check`, `clippy`, `test`, document validation, manifest validation,
  Vue type-check and production build passed for the completed slices.

The first closeout audit found that the SudoVDA media run passed while Windows
later collapsed the restored desktop to the Sunshine path after the temporary
SudoVDA target departed. The worker now captures the pre-projection active path
identities, waits for the exact SudoVDA LUID/target to remain absent, merges the
captured paths into a fresh topology and requires ten seconds of stable restored
state. Restoration is retried up to five times so the bounded topology jogs of
another active display manager cannot win the final write. The service allows
the interactive worker 45 seconds to finish this teardown before forcing exit.

The final elevated rerun passed both independent postconditions. The V2419A
presented a hardware-decoded HEVC frame at 2640x1216 and 120 Hz, and the sorted
active topology before and after the run was exactly the same: PHL 278E1 plus
the existing Sunshine virtual monitor. `closeout-result.json` reports
`media_passed: true`, `topology_restored: true` and `passed: true`. Teardown
removed the temporary CapyIO SudoVDA target, task and worker, while preserving
the running Sunshine session and restoring the automatic CapyIO broker.

The obsolete `Root\CapyIOVirtualDisplay` development device and `oem174.inf`
package were then retired from this host after their exact identities and
rollback package were recorded. The old `CapyIO Phone Display` / wired-display
entry was therefore not a second production display; it was the superseded
custom IDD. The machine now has no active or installed CapyIO custom display
device. Retirement evidence and the retained rollback command are in
`target/evidence/legacy-capyio-display-retirement/retirement.json`.

## Scope boundary

This closes the single-display controlled-lab implementation, not the entire
CapyIO product release. The following work is deliberately outside this
closeout:

- same-profile multi-display service integration remains frozen by ADR 0051;
- HDR negotiation and end-to-end HDR correctness have not been proven;
- unattended crash recovery for temporary physical-display suppression is not
  implemented, so that control is not persisted;
- packaging, installer UX, certificate enrollment and consumer one-click
  onboarding remain whole-product integration work;
- a topology change can make a coexisting capture engine briefly reinitialize,
  even though it does not require either virtual display to be removed.

The selected projection backend is the pinned SudoVDA package. The retired
CapyIO IDD source remains in the repository as historical/frozen work and is
not used by the Phase 1 production path.
