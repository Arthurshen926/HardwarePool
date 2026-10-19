# Shared resource navigation — iteration 1

Status: implemented presentation slice, 2026-09-17. Network discovery,
unified pairing and remote occupancy arbitration remain pending.

## Delivered scope

Windows Tauri and Android package the same Vue source. The three primary pages
are 本机共享 (local sharing), 远程资源 (remote resources) and 设置 (settings).
Quick Actions remain the existing scoped host operations, now opened as one
resource detail at a time. Controller, touchpad and display are subordinate
panels. Navigation alone never starts an Adapter or changes a Route.

The landing page has no architecture diagram, raw Route table or IMU fixture.
Settings contains version checks, native diagnostics and the explicitly labeled
development workspace. Fixture capabilities and simulated Quick Actions are
excluded from ordinary resource catalogs/action lists. The optional controller
simulator remains explicitly labeled. Narrow layouts retain the mock badge.

Real peer catalogs are grouped by stable Node ID, not display name. Offline
catalog entries remain observable; no inferred user/lease count is displayed.
Capability availability is not represented as globally synchronized occupancy.
Source checkboxes still mean local publication intent only. Sink publication,
peer grants and hardware lifecycle are not silently changed by this iteration.

Audio network configuration is in Settings; camera selection and audio counters
are in matching Quick Action details. Existing hand-controller configuration
remains alongside its live panel and is reachable from Settings. Display keeps
its existing prerequisite/import and independent host-control component.

The UI remembers only the last primary page when localStorage is available.
Disabled storage is tolerated. Existing Android `?view=quick` links resolve to
local sharing unless a valid saved page exists; controller/touchpad/workspace
deep links remain available. Returning to a page never restores an active
hardware operation. Input panels retain their existing neutralization on unmount.

## Validation

- `node --test scripts/resource_navigation.test.mjs scripts/display_control_ui.test.mjs`
  covers legacy entry links, invalid/saved pages, fixture exclusion, stable-ID
  grouping, offline retention and the existing independent Display controls.
- `pnpm --filter @capyio/desktop build` typechecks the shared Vue and builds assets.
- Browser review checks navigation, keyboard focus, detail/back and collapsed
  diagnostics; this is not Android or Windows physical-media evidence.

Completed checks: all five Node tests and the shared typecheck/build passed.
Browser review confirmed three-page navigation, Display detail/back, focus on
the new page heading, remembered primary page after reload, and initially
collapsed diagnostics. At a 390x844 viewport, navigation targets were 44 px high
and document scroll width did not exceed viewport width. Browser logs contained
no warnings/errors during this review. Documentation validation and diff checks
passed.

Windows package: `target/integrated-test/20260917-145411-9731/`, including the
desktop executable and its three required sibling helpers. The manifest records
revision `b42092b` with tracked modifications; this is not a clean release build.
The desktop SHA-256 is
`7559B703F054390495B4008D524DA4055B83223FCD65BDEF24B71EC6919D1A71`.

Android offline `assembleDebug`, `verifyIntegratedDebugApk` and
`verifyGameStreamDebugApk` passed with
`-PcapyioGameStreamStageRoot=target/android-moonlight-unified`. The initial
sandboxed invocation failed reading SDK `core-lambda-stubs.jar`; the identical
offline build outside the sandbox passed. The APK is at
`platform/android/app/build/outputs/apk/debug/app-debug.apk`, retains version
0.5.11-dev and records `b42092b-ui-iteration1-dirty`. No new APK installation or
physical-media acceptance was performed for this presentation slice.
APK SHA-256:
`1AF29EFA5B0B8DEAD239C0E789DBC5A823C2748BDA2566992D96A311BE28020A`.

## Next slices

1. Extend resource/action associations beyond the current exact Android catalog
   compatibility binding, including host-provided concrete audio-device choices.
2. Complete first-run pairing/credential issuance around the reviewed discovery,
   authenticated connection and live peer-catalog boundary.
3. On Windows, the shared settings page may persist one inventory-validated
   discovery interface and automatically connect only candidates already marked
   paired by the native host. Manual disconnect suppresses reconnect for the
   current UI session; unpaired/revoked candidates are never promoted.
4. Bind each resource operation to a selected peer and negotiate its existing
   Adapter-owned endpoints through that connection.
5. Add owner-authoritative occupancy/leases, conflict feedback and separately
   scoped remote-configuration grants. Never infer occupancy from catalog count.
6. Validate identical navigation on packaged Android/Windows and repeat actual
   hardware acceptance independently of presentation tests.

## Iteration 2 — resource cards

The existing seven Android resource cards now contain their configuration/use
entry and independent operation status alongside publication selection. Exact
schema-4 Node/key/class/inventory checks associate the existing local actions;
remote, simulated, unknown and ambiguous entries never receive that binding.
This is a presentation compatibility table, not a new host authority or a
general resource-to-action protocol. Unmatched actions remain available as
fallback entries, so inventory changes do not silently remove functionality.

Bound actions are removed from the duplicate local action list. Controller and
touchpad surfaces are opened from resource details, with their own existing
start/stop controls on the panel. Microphone/speaker details show only the
selected direction. Source publication intent and Sink-only resources are
distinguished explicitly. The seven known local action titles and summaries
are Chinese; unknown action text is preserved.

The seven-test focused suite includes SSR rendering of the actual catalog
component, publication checkbox/busy state, collapsed technical information,
and absent operation controls for unbound inventory. Binding tests reject
foreign Nodes, simulated resources, wrong classes, missing/duplicate actions
and duplicate capabilities. Shared Vue typecheck/build passes. These checks
do not establish remote resource arbitration or physical sensor operation.

Iteration 2 builds: Windows package
`target/integrated-test/20260917-152256-2872/`; Android
`platform/android/app/build/outputs/apk/debug/app-debug.apk`, revision label
`b42092b-ui-iteration2-dirty`, SHA-256
`016D8B3234733606ED0115F3BEDAA282E775A4314765A387C38C72D3337E2960`.
Offline APK assembly, integrated-package verification and GameStream-package
verification passed. These supersede the iteration-1 artifacts above. This
iteration did not install the APK or rerun physical hardware tests.

## Iteration 3 — detail workflow and return navigation

Resource Start/Stop and configuration links precede metadata and telemetry.
Opening a Controller/Touchpad panel or audio settings retains the exact resource
detail as the Back destination. Primary-page navigation clears subordinate
history and action identity; at most eight in-memory entries are retained.
Back changes presentation only and never calls a host start/stop operation.
This is application-button navigation, not native Android system-Back handling.

Audio settings opened from a resource expand immediately and select the matching
microphone/speaker direction. Active-direction editing remains disabled. The
form explicitly labels defaults as examples: its current API does not read back
saved addresses/ports. Adding that read-only host contract remains follow-up
work; the UI does not invent or cache authoritative connection values.

All nine focused tests pass, covering resource-specific return, root navigation,
direct entry, repeated clicks, bounded history, actual audio form rendering,
direction selection and active-direction lockout. Shared frontend typecheck/build
passes. No device operation or production connection feature is implied.

Build artifacts: Windows package
`target/integrated-test/20260917-153237-1851/`; Android APK at the same output
path above, revision label `b42092b-ui-iteration3-dirty`, SHA-256
`CB334BB152EEB6690F8DA7F68D735A368B25EABB978B7C135703293E3DEFF1FE`.
Both package builds and the Android integrated/GameStream archive checks passed.
Documentation validation and diff checks passed. APK installation, native system
Back behavior and physical media regression were not tested in this iteration.

## Iteration 4 — authoritative saved audio settings

The Android form now reads the native saved settings for the selected audio
direction, replacing the iteration-3 limitation. Missing saved values, missing
legacy bridge support and read failures have separate messages. Saving compares
readback with the submitted values before reporting confirmation. Reads do not
start hardware or prove reachability. Pending responses are ignored after a
direction change or unmount. Windows and browser backends report unsupported.
ADR 1431 records the narrow read-only contract and security boundary.

Validation: all 11 focused Node tests passed (resource navigation, actual Vue
SSR rendering, independent Display controls and strict audio readback parsing).
Shared frontend typecheck/build and documentation validation passed. Android
offline `capyioCheck`, `verifyIntegratedDebugApk` and `verifyGameStreamDebugApk`
passed, including lint, 33 audio-peer cases and the existing capability,
lifecycle, LAN and display contract suites. These are software checks, not
physical-media acceptance. No APK installation occurred in this iteration.

Latest Windows package: `target/integrated-test/20260917-154530-8928/`.
Keep its desktop executable and sibling helpers together. Latest Android APK:
`platform/android/app/build/outputs/apk/debug/app-debug.apk`, revision label
`b42092b-ui-iteration4-dirty`, SHA-256
`8C93F0BD3B392DA74F02C7D629AB33D4BA1CC6CB5BBD153EF60DE1C29CFD2819`.
These are development artifacts with uncommitted changes, not release builds.
Mounted-form asynchronous interactions still require runtime UI verification;
SSR and DTO tests do not establish those interactions on a real WebView.

## Iteration 5 — remote catalog resource details

Every real, unbound catalog capability now opens an identity-preserving detail
page on both platforms. The page shows its owning Node, online/availability
state and exact Port directions/profiles. It deliberately reports occupancy and
consumer identity as unknown because the current catalog has no lease data.
For a remote resource it also states that authenticated authorization and Route
control are unavailable instead of presenting an inert Start button. Catalog
refresh removal produces an explicit unavailable state. Navigation back returns
to the exact local/remote parent and never calls hardware lifecycle operations.

The remote page also exposes an explicit refresh of the host's already-admitted
Runtime snapshot, a local refresh timestamp, and online/offline Node counts.
Offline catalogs remain visible and are labeled as retained diagnostic state.
This refresh does not send discovery traffic, authenticate a peer or reconnect
a Session, and its wording intentionally avoids claiming those operations.

The diagnostic snapshot Route DTO now carries exact Source/Sink Capability IDs
plus authorization state/expiry, rather than forcing the UI to join resources
by display name. A resource detail lists Routes known to this Node and counts
its active Routes, while explicitly warning that zero locally known Routes is
not proof of global availability. Full occupancy requires an owner-authoritative
lease protocol and cannot be derived from catalog or UI state.

Iteration-5 verification passed: 12 focused shared-UI tests, Vue typecheck and
production asset build, all 82 desktop library tests (72 passed, 10 physical
tests correctly ignored), documentation validation, Android `capyioCheck`,
integrated APK verification and GameStream APK verification. The Android APK
is `platform/android/app/build/outputs/apk/debug/app-debug.apk`, revision
`b42092b-ui-iteration5-dirty`, SHA-256
`D0F391D0862C399A692203102F568DC6965B4B2DB3B9FAAE3653CBC264AC37A6`.
The Windows package is `target/integrated-test/20260917-155753-8230/`; its
desktop executable SHA-256 is
`2CB4D252A6BA89BB4C20F7CF7BFEE9DF5ACE6B9403C8A4F00B6F70CCD734C32E`.
No APK installation, physical media run or discovery traffic occurred.

## Iteration 6 — one preferred address, isolated feature authority

Settings now accepts one canonical private/overlay IPv4 as a local preferred
peer. It pre-fills Android audio only when that direction has no native saved
peer, and pre-fills the Android controller target without storing its token.
Changing the preference does not overwrite a separately edited controller
target. Clear removes only the UI preference; native audio, controller tokens,
ports, display credentials and running hardware remain untouched.

The strict versioned profile rejects public, loopback, noncanonical, oversized
and unknown-field input. It is explicitly labeled as neither pairing nor a
connection. ADR 1433 records this interim trusted-network boundary. Automatic
candidate discovery and authenticated configuration exchange remain separate.

Iteration-6 verification passed: all 14 focused UI/profile tests, shared Vue
typecheck/build, documentation validation, Android `capyioCheck` and both APK
composition checks. No APK installation or physical network test occurred.
Android APK SHA-256:
`E410D28F6A1F6C1E20D82A8A768EC30636CEECD1694688C050112DF604EE23E4`.
The Windows package is `target/integrated-test/20260917-161415-6074/`;
desktop executable SHA-256:
`62059337AC3A8D70FA497B20FF548973A8BE63EF4A899119797A52AE92F92182`.

## Iteration 7 — owner-authoritative occupancy foundation

ADR 1434 adds explicit Runtime Port leases. Local Ports may be exclusive or
bounded-shared; claims bind exact Port, Route, Ready Session and finite expiry.
Configured Ports reject Route start without a claim. Stop, Offline, catalog
invalidation and peer loss release Route-scoped claims; expiry moves the Route
Offline and advances its epoch. Storage is capped at 512 policies and 1,024
claims.

The desktop diagnostic snapshot now carries exact local lease claims. Resource
details display these separately from active Route counts and continue to state
that remote/global occupancy is unknown. Android schema-4 snapshots omit the
optional field until the Android Runtime host owns real Route leases. No remote
lease request or permission UI is implied by this iteration.

Iteration-7 verification passed: full non-desktop workspace tests, workspace
check and warnings-denied Clippy, eight Runtime unit tests, 19 testkit tests,
82 desktop library tests (72 passed, ten physical tests ignored), shared Vue
build, Android `capyioCheck` and both APK composition checks. No APK installation
or physical lease exchange occurred. Android APK SHA-256:
`495CB37145D7A9CEA0DF2C10277D85A73FB681DBE36650A81A0F96AF073755B3`.
Windows package: `target/integrated-test/20260917-163139-2414/`; desktop
executable SHA-256:
`6A2B4F3C43133D7199F8F18A754792C41FFC6CAC1809244916138FC841D03BCD`.
