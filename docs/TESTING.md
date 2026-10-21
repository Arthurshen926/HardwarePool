Warning: truncated output (original token count: 38152)
Total output lines: 2467

# CapyIO Testing Strategy

Local `cargo xtask ci`, `cargo xtask alpha-check` and hosted static checks also run
`python -m unittest discover -s scripts -p "test_*.py"`. These existing tests cover
source export/review validity, Core/Extras artifact boundaries and component plans.
They use temporary fixture files, do not build/install packages or contact devices,
and need no Windows service or Android SDK. Keep tests under this pattern portable;
physical deployment checks remain separate explicitly invoked scripts.

The component inventory fixtures additionally reject empty/malformed inventories,
unlisted files, duplicate/case-aliased and escaping paths, linked roots/manifests/
files/directories, and size/hash mismatches. Missing files remain explicit rather
than verifying an empty set. A valid nested schema-v3 inventory with uppercase
hashes still passes. Symbolic-link tests report a skip on hosts that cannot create
fixture links; that skip is not evidence of Windows junction enforcement. These
tests never execute payloads and do not replace the Windows package-shape gate.
Portable synthetic Windows reparse-attribute tests cover the Python 3.11-compatible
check and rejection before any manifest/payload read. A Windows-only test creates
and removes a junction between temporary directories; non-Windows hosts explicitly
skip this real-junction case rather than claiming Windows filesystem evidence.

`scripts/test_local_debug_installer.ps1` checks the real file-only NSIS installer
with fake payloads in an isolated workspace target: installation, overwrite refusal,
recoverable removal and restore. It does not execute payloads or deploy system
components. Release inventory generation and coverage are in RELEASE_EVIDENCE.md.

Windows window lifecycle can be checked after building the embedded UI:
`cargo build -p capyio-desktop --bin capyio-desktop --features tauri/custom-protocol`,
then `scripts/test_desktop_host_lifecycle.ps1 -Executable target/debug/capyio-desktop.exe`.
Close existing nodes first. The script launches its own UI and separate Host, checks snapshot/version/argument
boundaries, closes and interrupts its UI, checks the unchanged Host PID and response,
then invokes the native quit menu and requires both processes to exit.
It does not start physical Routes or install components; this is not media continuity
or device acceptance. Failure leaves the test process available for diagnosis.

`cargo xtask alpha-check` is the strict Windows/Android candidate software gate
(ADR 1509): format, complete workspace check/Clippy/tests including Desktop,
repository/manifests/Adapter checks, mandatory frontend build and Node tests,
and Android capyioCheck. Missing tools fail instead of skipping. Physical soak,
deployment/signing and publication approval are separate gates; see
ALPHA_CONSOLIDATION.md. Compatibility microphone DTO tests use the pure
projection seam so an installed native service cannot override their fixture.

ADR 1486 adds touchpad lease-policy and renewal tests covering use across the
old five-minute boundary, final grant expiry, lost claims and exact correlated
replies with intervening catalogs/heartbeats. The optional paired touchpad soak
uses a fixed 370-second hold and the production renewal helper; it must be
reported separately from hardware-free tests and visible gesture acceptance.

ADR 1484's reverse-control loopback test uses fresh dual-role certificates,
Windows accepted/Android dialed catalog exchange and three simultaneous
heartbeat rounds in both directions. It does not prove Windows listener UI
startup on a physical host, Android APK reverse dialing, or a touchpad Route.

ADR 1485 tests that explicit renewal replaces only the selected peer record,
preserves unrelated peers, and rejects an unknown peer. Windows account
credential preflight and Android invitation import still require a physical
user-session check before claiming old-pair migration success.

ADR 1478 extends `:app:capabilityCompositionTest` with background-waiting
ownership independent of publication and hardware. On the authorized Y700,
the updated debug APK kept the same PID and TCP 43193 reachable after Home;
`dumpsys` reported the Node Service as `connectedDevice` foreground with no
Activity binding. Under the paired Windows account the control-only gate
passed pinned mTLS, catalog exchange (two capabilities) and heartbeat in this
background state. Disabling waiting removed foreground ownership and the port
became unreachable after Home. This short run does not establish OEM-policy,
reboot, long-duration or touchpad data behavior.

ADR 1479 reuses the shared peer-list parser tests for bounded three-field
metadata and exclusion of credential fields. The authorized Y700 debug APK
returned one Windows peer with exactly `nodeId`, `displayName` and `revoked`
from the WebView bridge; Remote Resources rendered its display name while
truthfully reporting no current authenticated connection. This does not prove
Android outbound dialing or Windows resource consumption.

ADR 1480 adds a desktop enrollment regression that issues fresh Windows and
Android control bundles and authenticates their pinned TLS 1.3 identities in
both dial directions on loopback. It asserts fresh ordered Node/Session
bindings. The test uses no stored user credentials and does not prove the
production listener/dialer lifecycle or migrate old pairs.

ADR 1481 adds a Windows inbound loopback regression using a newly issued pair:
non-TLS bytes are rejected, then the same listener accepts a pinned Android
identity, admits the catalog, acknowledges heartbeat and closes. The test
caught a Windows nonblocking-accepted-socket TLS abort; the TLS boundary now
sets accepted sockets to blocking mode with finite read/write timeouts.
This remains a hardware-free CLI-gate test, not an Android-initiated device run.

ADR 1482 adds three Android-host loopback cases: valid pinned Windows TLS
acceptance and catalog exchange with no Route; unpaired/invalid credential
rejection before a network attempt; and wrong Windows leaf rejection with no
Session admission. These tests do not exercise JNI, the physical Android
network stack or any shared UI control. The `:app:buildRustJni` Android ARM64
release compilation also passed; it is compile evidence, not a device run.

ADR 1475 adds `:app:controlRecoveryContractTest` for Android listener failure
isolation. Invalid peer handshakes must not terminate the accept loop; cleanup
failure remains terminal and cannot advertise a live listener. Device evidence
must show an invalid handshake followed by paired mTLS on the same port.

ADR 1474: `:app:capabilityCompositionTest` covers publication-only foreground
ownership and remote-stop generation isolation. The Y700 background-publication
and unpublication lifecycle was physically checked; see
`docs/TOUCHPAD_CLOSED_LOOP_STATUS.md` for precise evidence and remaining limits.

ADR 1473: run `cargo test -p capyio-control-tls -p capyio-control-link
-p capyio-protocol` and `cargo test -p capyio-remote-touchpad-adapter --features
tls --test secure_receiver`. These cover real loopback TLS with fake Sinks,
pin/ALPN/binding rejection, authorization/lease admission and cleanup. They do
not prove Windows cursor movement. The opt-in desktop diagnostic
`--verify-paired-touchpad-data <node> <ip> <control-port>
--acknowledge-desktop-input` uses the native receiver and requires resource
consent; its transport success alone still does not prove physical gestures.

ADR 1448 control TLS tests reject an incorrect paired leaf on both sides of a
real mTLS connection despite valid CA chains, and reject missing certificates
or using an issuer as the leaf. Catalog/lease success fixtures explicitly pin
their peer leaves. Run `cargo test -p capyio-control-tls -p capyio-control-link`.

ADR 1455 adds real loopback mTLS tests in which both endpoints converge on one
new non-nil Session binding without prior Session coordination. The listener
rejects a request that changes either configured Node identity after successful
mTLS, and unit coverage rejects non-zero reserved binding bytes. These tests do
not prove application Runtime admission, reconnect or multi-peer scheduling.

ADR 1456 extends the loopback composition so both initially independent
Runtimes transactionally admit the shared Session and opposite peer. Tests
assert exact IDs, one Ready Session, empty provisional resource catalogs and
rollback for wrong-local and existing-peer cases. Catalog exchange remains
separately covered. Testkit then closes an active multi-Route Session, requires
live Routes Offline, preserves a Stopped Route, marks the peer Offline and
proves repeated close is a no-op. Reconnect and process-owned lifecycle remain
unproven.

ADR 1457 adds Windows host regression for recovering a paired record, retaining
its exact platform store, matching a fresh discovery endpoint and producing the
closed dial inputs. Desktop Clippy/tests plus shared UI typecheck/build cover the
command and Node-ID-only button. The lower layers retain real loopback mTLS,
fresh admission, catalog exchange and teardown tests, but this is not a physical
Windows-to-Android connection result: the current Android APK has no common
control listener or advertisement.

The 1458 Windows release gate runs the complete Tauri frontend/release compile,
records the EXE size and SHA-256, then starts it for five seconds and terminates
the exact process. See `CAPY_WINDOWS_DESKTOP_RELEASE_BUILD_1458_REPORT.md`.
This proves a directly runnable unsigned EXE can be produced, not an installer,
signature, upgrade or physical cross-device acceptance.

ADR 1449 tests absent-store read-only startup, bounded malformed input,
wrong-owner rejection, closed credential-handle paths, atomic directory/blob
replacement and a real current-user DPAPI credential round trip. Desktop tests
recover paired metadata and reject corrupt state without partial admission.
Run `cargo test -p capyio-windows-pairing-store -p capyio-desktop` on Windows.

Run `node --test scripts/trusted_endpoints_ui.test.mjs` for ADR 1476. It
proves bounded, non-secret endpoint hints, revoked/unpaired filtering,
manual-disconnect suppression and retry cooldown. It does not prove a live
network reconnect, interactive pairing or Android-originated dialing.

Run `cargo test -p capyio-android-jni control_bind_tests` for ADR 1477. It
proves an occupied preferred control port falls back without replacing the
occupying listener. A physical Android Service restart and post-restart
authenticated reconnect are still required before claiming live persistence.

ADR 1450 adds a dependency-free Android instrumentation APK. On a separately
authorized device it verifies real Keystore AES-GCM creation, replacement,
recovery, tamper rejection, closed handles, bounds and cleanup. CI assembles the
test APK but cannot count assembly as Keystore execution. The identified-device
result is recorded in `CAPY_ANDROID_KEYSTORE_1450_REPORT.md`.

ADR 1451 adds native canonicalization tests and extends that physical gate with
an isolated `AtomicFile` root. It covers absent read-only startup, canonical
commit/reload, rejection without replacement, stored corruption and cleanup.
The result is recorded in `CAPY_ANDROID_PAIRING_DIRECTORY_1451_REPORT.md`.

ADR 1452 adds schema-5 contract cases for ready/failed states and a physical
WebView readback after Service restart. The readback must show ready/zero peers
while transport and catalog exchange remain disconnected. See
`CAPY_ANDROID_PAIRING_HOST_1452_REPORT.md`.

ADR 1459 adds an active-only pairing projection test proving revoked records do
not cause credential recovery. The full Android gate compiles the JNI symbol,
Service recovery path, ARM64 library and APK and runs lint plus hardware-free
contracts. A populated-directory Keystore run remains a physical-device gate;
see `CAPY_ANDROID_ACTIVE_CREDENTIAL_ADMISSION_1459_REPORT.md`.

ADR 1460 adds real loopback mTLS tests for Android's acceptor-ordered initial
catalog exchange and for rollback after an authenticated wrong-Session catalog.
See `CAPY_ANDROID_ACCEPTED_CATALOG_1460_REPORT.md`.

ADR 1461 extends the same loopback gate through the real Android control
listener constructor, canonical pairing selection and typed credential decode.
It also proves zero/multiple active records fail closed. See
`CAPY_ANDROID_CONTROL_LISTENER_1461_REPORT.md`.

ADR 1462 makes the full Android gate compile/package the JNI registry, Service
worker and teardown ordering. Shared snapshot contracts cover disconnected and
authenticated catalog states. Physical accept and teardown remain device/peer
tests; see `CAPY_ANDROID_SERVICE_CONTROL_1462_REPORT.md`.

ADR 1463 adds a send-only ephemeral-socket transport test and makes the Android
gate cross-compile/package the periodic announcer and discovery-state UI
projection. Physical multicast delivery remains an environment gate; see
`CAPY_ANDROID_DISCOVERY_1463_REPORT.md`.

ADR 1464 adds a two-generation real mTLS reconnect test covering exact Session
close, retained peer state, fresh Session admission and duplicate-live rejection.
See `CAPY_CONTROL_RECONNECT_1464_REPORT.md`.

ADR 1465 adds protocol round trips plus a real mTLS link request/acknowledgement
test for the exact Session-bound heartbeat. See
`CAPY_CONTROL_HEARTBEAT_1465_REPORT.md`.

ADR 1466 makes the Windows controller and Android Service package the same
heartbeat primitive and automatic Session cleanup. Rust link tests retain the
real mTLS request/acknowledgement evidence; desktop checks and Android
`capyioCheck` cover both platform compositions. Physical half-open timing remains
a device gate; see `CAPY_CONTROL_HEARTBEAT_OWNER_1466_REPORT.md`.

ADR 1467 adds a real mTLS ordering test in which a CatalogSnapshot is dispatched
and applied while a heartbeat acknowledgement is pending. Desktop compilation
and lifecycle regression tests cover the worker composition, while Android
`capyioCheck` packages the dirty-publication Service/JNI path. A physical
UI-toggle-to-peer-inventory test remains required; see
`CAPY_LIVE_CATALOG_SYNC_1467_REPORT.md`.

ADR 1469 adds codec tests for exact target binding, round trip, truncation and
trailing-byte rejection. Desktop/JNI compilation checks the issuer and native
import boundary; Android `capyioCheck` compiles the Keystore-first importer and
shared file-picker UI. Installing and launching an APK is not evidence that an
invitation was imported or that mTLS reconnect succeeded; those remain explicit
physical acceptance observations. See `CAPY_PAIRING_INVITATION_1469_REPORT.md`.

Alpha packaging evidence and its signature/recovery limits are recorded in
`CAPY_ALPHA_PACKAGING_REPORT.md`. Merely producing an NSIS executable or a
debug-signed APK is not evidence of Authenticode, production Android signing,
installer upgrade/uninstall correctness or update rollback.

The current TB323FU lifecycle evidence and its explicit cross-device exclusions
are recorded in `CAPY_ANDROID_TB323FU_REGRESSION.md`.

ADR 1470 requires manual-endpoint parsing tests plus the same authenticated
catalog/heartbeat path used after discovery. Endpoint reachability alone is not
an authentication pass; a physical pass must observe the peer catalog and a
matching Session-bound heartbeat acknowledgement.

ADR 1468 adds pure shared-UI selection cases for paired-only automatic connect,
connected/manual-suppression exclusion and retry cooldown. The production UI
typecheck/build verifies the startup composition; physical multicast and
reconnect remain device/network gates. See
`CAPY_AUTOMATIC_PAIRED_CONNECT_1468_REPORT.md`.

ADR 1453 tests exact typed-bundle round trips and rejection of malformed,
truncated, extended, empty, excessive-chain and oversized input. Windows tests
exercise a real DPAPI typed round trip and reject protected arbitrary plaintext;
control TLS tests map the same type into both endpoint identities. Android
`capyioCheck` compiles the ARM64 JNI validator and instrumentation APK, whose
physical gate must prove Keystore round trip/replacement plus rejection before
storage. Run `cargo test -p capyio-control-credentials -p
capyio-control-tls -p capyio-windows-pairing-store -p capyio-android-jni` and
Android `gradlew.bat capyioCheck`. APK assembly is not physical Keystore
execution; the 2026-09-18 attempt could not run because the previously supplied
wireless-debug endpoint refused the connection.

ADR 1447 adds Testkit local-catalog replacement cases and an Android JNI host
test with an existing peer/Session. They verify identity/ownership rejection,
idempotent updates, independent Route preservation, epoch invalidation, affected
lease cleanup and no automatic restart. Run `cargo test -p capyio-testkit` and
`cargo test -p capyio-android-jni`; Android `capyioCheck` validates the native
build and Service call site. Connected physical acceptance remains separate.

ADR 1446 adds the desktop candidate observer regression (including expiry after
listener Stop) and `node --test scripts/discovery_candidates_ui.test.mjs` for
bounded untrusted candidate readback. These checks do not establish physical
discovery, pairing or a usable remote resource.

ADR 1454 extends those regressions with active and revoked paired-directory
records. The native projection must report `paired`, `revoked` and `unpaired`
without changing authenticated connection/catalog state; the TypeScript parser
accepts only those closed values. This remains local metadata classification,
not certificate or Session authentication.

ADR 1445 uses real loopback mTLS connections with synthetic catalogs to test
bidirectional publication-filtered catalog exchange and Runtime admission,
deselection updates, peer loss between receive/apply, cross-link work rejection
and explicit close. Run `cargo test -p capyio-control-link`. These are host
composition tests, not physical Android/Windows application connection evidence.

ADR 1444 control TLS regression tests cover terminal truncated/malformed input,
partial-write timeout, untouched-channel local rejection and a real loopback
mTLS failure followed by successful exchange on a fresh authenticated channel.
Run `cargo test -p capyio-control-tls`. This is transport evidence, not Android/
Windows application pairing, automatic reconnect or Route epoch acceptance.

ADR 1443 adds read-only Windows interface inventory and shared UI selection
tests. Run `cargo test -p capyio-desktop network_interfaces` and
`node --test scripts/discovery_interfaces_ui.test.mjs`. These cover bounded
enumeration, excluded address/state classes, malformed DTOs, ambiguous-network
selection and stale-choice removal, not multicast delivery or peer pairing.

## Principles

- Tests prove explicit requirements and failure isolation.
- Core/Protocol/Adapter DTO tests run without hardware.
- Platform and driver claims require identified environments and retained
  evidence.
- Mock UI/Sidecar behavior is visibly simulated.
- Tests are not removed or weakened to hide migration defects.

## Unified commands

```text
cargo xtask doctor
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask adapter-smoke
cargo xtask ci
cargo xtask demo
cargo xtask imu-demo
cargo xtask android-check
cargo xtask android-doctor --serial <explicit-serial>
cargo xtask android-baseline --serial <explicit-serial>
cargo xtask android-collect --serial <explicit-serial>
```

Frontend uses `corepack pnpm typecheck` and `corepack pnpm build`.

## Foundation unit tests

- Node has no global role and may own Source/Sink Ports;
- Capability/Adapter ownership and duplicate IDs;
- Source→Sink Profile compatibility;
- Source→Source, Sink→Sink and mismatched Profile rejection;
- valid/invalid Route transitions;
- opposite-direction Routes coexist;
- stopping one Route leaves another active;
- Adapter failure affects owned Routes only;
- catalog replacement after Adapter restart;
- complete peer-catalog admission is bound to one Ready Session and exact
  remote Node identity; rejection is transactional, while accepted endpoint
  removal offlines only dependent Routes;
- the dedicated CatalogSnapshot decoder requires a typed Session ID, exact
  payload and complete valid Node descriptor before returning a Core value;
  encoder/decoder regressions reject excessive descriptor counts and catalog
  strings before Core conversion;
- the shared control host composes decoder and Runtime admission, rejects
  mismatched Session/peer and recent duplicate IDs before mutation, does not
  consume an ID after Runtime rejection and bounds retention to 256;
- Port lease control round-trips exact Session/Route/Port identity and response
  correlation, rejects invalid TTL/response shapes, and the admitted host can
  acquire, renew and release only an owner-local Port on its bound Ready
  Session; non-local requests and replays fail closed;
- the unified admitted-host dispatcher routes CatalogSnapshot and
  PortLeaseRequest while the authenticated link additionally validates typed
  RouteStatus, RouteCommand and PortLeaseResponse messages;
- the live-link integration test completes a real mutual-TLS pending Route
  proposal, independent authorization, exclusive owner lease and lease-gated
  Start with exact Session/Route/Port correlation;
- the generic control TLS channel requires mutual TLS 1.3 with its dedicated
  ALPN, exact Session/server/client Node binding, bounded valid Envelopes and
  carries an owner lease request through the admitted control host end to end;
- the bounded post-pairing directory classifies exact fresh discovery matches,
  rejects unpaired/expired/revoked candidates, prevents silent re-pairing and
  stores only an opaque secure-storage handle plus peer certificate fingerprint;
- paired-directory schema-1 JSON round-trips deterministically while rejecting
  oversize input, unknown fields, unsupported schema and duplicate identities;
- Windows current-user DPAPI round-trips a bounded fixture credential, rejects
  a modified protected blob and applies size checks before platform access;
- the Windows pairing store performs read-only empty startup, bounded
  fail-closed directory recovery, closed handle-to-path mapping, atomic
  replacement and desktop restart admission without exposing credential bytes;
- Android Keystore device testing protects, replaces and recovers one bounded
  credential, rejects a changed authentication tag and removes its dedicated
  key/file without adding an application permission;
- Android paired-directory tests retain the shared Rust schema across JNI and
  prove atomic isolated-device persistence without enrolling a peer;
- Android Service startup owns the readable canonical directory and projects
  ready/failed plus bounded count without implying a connection;
- discovery ingress counts malformed frames against its fixed per-window rate
  budget, rejects backward time and admits only exact records;
- the explicit UDP host fixes private-scope multicast address, dynamic/private
  port, TTL and poll timeout, rejects invalid interfaces before binding and
  treats a receive timeout as an idle lifecycle poll;
- Windows discovery start/stop is a separate worker lifecycle, does not hold the
  Runtime lock while polling, clears listening state on exit and prevents a
  Node identity reset while active;
- outbound publication keeps selected Source-bearing Capabilities plus all
  Sink/Control-only Capabilities, prunes Adapter ownership, round-trips through
  the bounded Session encoder and rejects unknown, non-Source or Offline
  selections;
- Protocol catalog/Route/Problem round trips and enum/version failures;
- Adapter manifest validation;
- NDJSON framing, malformed/oversized messages and correlation;
- stdout/stderr limits are enforced while reading, including oversized input
  without a newline;
- timeout, late/unexpected response, malformed response and stdout closure
  poison the sequential Host, reap the child and reject later requests;
- generic Route prepare/start/stop/status contracts round trip without carrying
  continuous data payloads;
- child stdout machine-only behavior and bounded/truncated stderr;
- Requirement parser rejects duplicate, malformed and non-canonical IDs, and
  traceability covers every normative PRD ID;
- deterministic UI snapshot with four Routes.

## Fixture-first IMU tests

- committed JSONL envelopes validate Profile, timestamps, clock domain, epoch,
  sequence, SI units, coordinate frame, accuracy, calibration and sensor data;
- Panel and Recorder consume independent bounded queues from one fan-out;
- a full/stopped Recorder does not block Panel progress;
- gaps, duplicates, late samples, wrong streams, stale/future epochs, sequence
  exhaustion and recorder bounds have explicit regression tests;
- `cargo xtask imu-demo` replays the same compiled fixture through the headless
  node and emits numeric Panel plus JSONL Recorder evidence;
- the desktop Browser Mock and Tauri backend expose the same schema-v3 fixture
  summary and label it as simulated rather than live phone data.

These tests require no phone and make no SensorServer, APK, network or physical
timing claim.

## Video and input contract tests

- canonical video and input/haptics Profile helpers match the normative
  registry and replace stale mock-only names;
- packed NV12/BGRA candidates validate dimensions, reduced rational frame
  rate, closed colorimetry and derived payload bounds;
- negotiation requires exact complete candidates and performs no implicit
  resize, rotation, decode, color conversion or QoS rewrite;
- frame descriptors reject zero/stale epochs, wrong payload sizes and invalid
  end-of-stream payloads;
- input stream descriptors keep clock-domain allocation out of per-frame
  headers;
- the shared sequence guard rejects wrong stream, stale/future epoch,
  duplicate/late sequence, non-advancing epoch and sequence exhaustion, while
  reporting gaps;
- pointer/keyboard reset, empty touch snapshots, fixed gamepad neutral state and
  explicit haptics stop make fail-safe cleanup testable.
- the distinct touchpad Profile enforces positive himetric size, 3..=5 contacts,
  stable unique IDs per snapshot, declared geometry/pressure and closed button
  semantics;
- touchpad frame tracking suppresses initial, post-gap and post-epoch updates
  until explicit cancel-all, and timestamp validation is transactional;
- the committed synthetic fixture replays cancellation, one/two/four/five
  contacts, release and final cancellation with exact bounded metrics;
- the appended `Touchpad` Capability class round-trips as protocol enum value
  16 while touchpad frames stay outside the control envelope.

These tests prove only deterministic semantic contracts. They do not capture a
camera, inject OS input, register a virtual camera, start DSU/VIIPER, install a
driver/APK or prove ordinary-application compatibility.

## Windows camera fixture and projection contract tests

- the canonical fixture deterministically renders bounded 1280x720 30 fps NV12
  frames with rational timestamps and a pinned diagnostic checksum;
- the owned-frame queue enforces both frame and payload-byte limits, with
  explicit reject-newest or drop-oldest behavior and discontinuity evidence;
- the first Media Foundation plan can represent only session lifetime and
  current-user access with a bounded friendly name;
- sample timing maps an absolute source timestamp delta onto a QPC-correlated
  100 ns anchor and rejects wrong streams/epochs, duplicates and regressions;
- packed NV12 rows copy into a checked positive-stride destination and padding
  is cleared before the buffer can be published;
- first start/restart produce the required new/updated-stream, stream-started
  and source-started action order for the single canonical stream; a repeated
  start while already active preserves generation, pending requests and sample
  timeline state while producing updated/start events;
- sample requests use a four-entry fixed FIFO, reject overflow/out-of-order
  completion and remain pending after a transactional sequence failure;
- failure cancellation removes only the oldest accepted request and leaves the
  source started so the next pull can recover;
- stop/shutdown cancel outstanding requests, and registrar tests prove
  start/stop failures trigger terminal cleanup or an explicit cleanup-required
  state against an in-memory backend;
- the optional host probe calls only `MFIsVirtualCameraTypeSupported`;
- the Windows-only in-process COM test verifies source/stream attributes, the
  Legacy profile and provided-allocator contract, exact NV12 media type,
  first-start/restart event ordering, token identity, 2D buffer size/content,
  rational sample timing, pause/resume, stop and terminal shutdown behavior;
- class-factory tests verify the fixed CLSID, aggregation rejection, unsupported
  interface failure, server locking and unload only after source/stream release;
- backend contract tests prove the default object is inert, cleanup is
  idempotent and the plan remains session/current-user only;
- the opt-in `preflight` command calls `MFCreateVirtualCamera` and shutdown but
  never `Start`; the separately approved `roundtrip` command requires one
  enumeration match, two valid 30 fps NV12 samples through Source Reader and
  explicit cleanup. It bounds empty live-source reads, requires exact sample
  duration plus monotonic bounded downstream timestamps, and copies opaque
  downstream buffers into bounded CPU memory before checking NV12 luma.
- the separately approved `shared-roundtrip` keeps registration ownership in
  one parent and sequentially launches exactly two independent
  `consumer-probe` processes. Each child receives the parent's exact symbolic
  link through a bounded internal environment value, performs its own
  `MFEnumDeviceSources`/`IMFActivate` path and pulls two validated frames. The
  parent applies a separate 20-second deadline to each child, reports non-zero
  child exits and kills/reaps an unfinished child before registrar cleanup;
  command parsing rejects caller-supplied paths, identities, counts, timeouts
  and extra arguments. The host lab also records that simultaneous dual
  consumers still fail, so this gate does not claim concurrent fan-out.
- the 001B1E external ingress tests bind one stream/epoch, reject malformed,
  wrong-stream, wrong-epoch, non-advancing and end-of-stream frames, and prove
  the fixed drop-oldest bound plus discontinuity marking. The Windows
  in-process COM test feeds three caller-owned frames into a two-frame ingress,
  observes only the retained payload values in Media Foundation samples,
  verifies `MFSampleExtension_Discontinuity`, exact timing and a non-blocking
  `MF_E_NOTACCEPTING` result after the queue drains. This does not exercise a
  cross-process mapping or registered external-frame source.
- the 001B1F shared-ingress tests pin the 256-byte header, 64-byte slot header,
  three 1,382,464-byte slots, 4,147,648-byte total mapping and exact protected
  SDDL. They reject a second producer for the same name, prove two independent
  read-only consumers observe the same latest payload, mark a skipped
  publication discontinuous and spawn a separate test process that opens the
  mapping and verifies owned payload bytes. A provider-level MF test proves the
  shared consumer supplies the latest frame and returns
  `MF_E_NOTACCEPTING` without blocking once that publication is consumed. Test
  mappings use unique `Local\\...test...` names and vanish with their handles;
  they do not create the production `Global\\CapyIO.CameraIngress.v1` object or
  alter registered activation.
- the 001B1G composition test keeps the producer in its parent process and
  directly spawns the current test executable as an independent MF consumer.
  The child opens the unique mapping read-only, starts Media Foundation,
  supplies the platform video allocator, requests a sample, observes the
  parent's exact luma byte and canonical 333,333-tick duration, then verifies a
  second request fails fast with `MF_E_NOTACCEPTING`. This proves decoded bytes
  cross the process boundary and reach an MF sample; it does not use the
  registered class factory or Frame Server service token.
- the 001B1H extraction runs the fixed-layout, duplicate-owner, independent-
  reader and raw child-process tests in `capyio-windows-camera-share`, while the
  cross-process MF sample test remains in `capyio-windows-camera-mf`. Bounded
  test-only mapping-name APIs reject names outside the fixed local test
  namespace. Camera-host tests prove stopped publish rejection, start/publish/
  stop accounting, mapping disappearance after final release, idempotent stop,
  duplicate-owner failure without false activation and successful retry after
  the first owner releases. MF and host consumers also verify they can adopt
  the validated producer-bound stream ID and epoch.

The COM test creates only directly referenced objects inside its process after
`MFStartup`; it does not expose a class factory or call a virtual-camera API.
These normal tests and the probe do not register, enumerate, start, stop or
remove a system virtual camera. Approved host-specific lab evidence may
establish Frame Server delivery, but does not establish broad
ordinary-application compatibility.

Normal CI never deploys the COM DLL, writes HKLM, changes an ACL or invokes the
roundtrip/shared-roundtrip/cleanup system commands. Those results belong in the
host-specific 001B1C/001B1D/001B1E reports with the exact DLL hash and rollback
targets. The 001B1F mapping tests are ephemeral process tests and require no
registry or filesystem rollback.

The 001B1E release regression may repeat the approved fixture-backed system
`roundtrip` because the common `RequestSample` implementation changed. That
regression verifies no loss of the existing virtual-camera path; it does not
turn the process-local external ingress test into cross-process evidence.

## Android Camera2 build-only tests

CAPY-CAMERA-001C0 uses the standalone project below without installing an APK:

```text
cd platform/android/capyio-camera-app
gradle contractTest :app:assembleDebug :app:lintDebug
```

`contractTest` has no test-library dependency. It verifies explicit permission
request/grant/deny, visible start, pause-driven close, failure cleanup, retry
and bounded frame metadata. Android lint runs with warnings as errors; only the
repository-audited API 36 pin checks are disabled because changing the Android
toolchain pin is a separate task.

The APK manifest is then inspected with `aapt2 dump permissions`; the expected
and observed set is exactly `android.permission.CAMERA`. Normal repository
validation parses the source manifest offline, rejects any additional
permission or service declaration and checks the visible permission/pause/
secure-window controls. These checks do not prove a camera opens on a phone,
that device stream combinations work, or that an encoded frame reaches
Windows.

CAPY-CAMERA-001C1 extends the same no-dependency contract executable with AVC
configuration bounds, owned/read-only payload behavior, codec-parameter-set
bounds and deterministic drop-oldest queue behavior. `assembleDebug` and
strict lint compile the real Android `MediaCodec` owner against API 36. These
are compile/contract checks only: they do not instantiate a device codec,
connect its input Surface to Camera2, or assert a vendor encoder's negotiated
profile, level, color metadata or output framing.

CAPY-CAMERA-001C2 compiles and lints the Camera2/MediaCodec composition. Offline
validation requires common-size selection, the encoder Surface request target
and capture callback to remain present. One separately authorized Android 16 /
API 36 device run validated the advertised two-Surface combination, 1280x720
vendor AVC output and pause-driven camera release. OS MediaMetrics recorded 77…20152 tokens truncated…audio parameters, permissions, visible service,
  lock/background, focus, route changes and power saving;
- Windows user mode: endpoint enumeration, Broker restart, bounded IPC and
  sleep/resume;
- drivers: install/update/remove, service restart and reboot in an isolated
  VM/dedicated target or the ADR 0029 controlled local lab; project-only
  Verifier remains isolated by default and requires separate approval;
- end to end: IMU Panel/Recorder, audio both directions, camera, gamepad,
  independent Routes, disconnect/reconnect and clock epochs.

## Android audio node shell Gate

The integrated display prerequisite executable also runs ADR 1428's 13 strict
property-structure cases and five unconnected-socket ownership cases. Duplicate
escaped keys, unknown/missing fields, blank/overlong/control-character values
and input bounds fail closed. Socket tests distinguish successful ownership
transfer from exception/security/error cleanup and preserve primary failure
when close also fails. These are not certificate or real TLS handshake tests.

`cargo xtask android-check` is the explicit hardware-free 001C Gate. It runs a
dependency-free Java contract executable, Android Lint with warnings denied and
debug APK assembly. The 36 contract assertions cover Port direction, independent
microphone/speaker activation, one-direction Stop, failure isolation, later
generation Retry, stale completion rejection and bounded actual-format/Problem
fields.

ANDROID-A01B1/A01B2 add a second dependency-free contract executable with 16
assertions. It verifies stable module order, immutable catalog snapshots,
Port-owned direction, duplicate rejection, bounded/canonical identifiers and
the rules that an unattached Adapter or absent hardware cannot advertise an
available Capability.
The existing Android host then passes `lintDebug`, `assembleDebug` and
`verifyDebugEntrypoints` with the registry exposed by the service Binder. This
is compile/contract evidence only; the APK is not installed and pending camera,
gamepad, haptics and touchpad entries do not constitute attached Adapters.
The hardware-free host composition executable now has 39 assertions for four
modules, seven non-display Capabilities, attached-versus-pending counts,
Source/Sink direction, hardware presence/absence/unknown, permission state and
live audio, IMU and touch-gamepad lifecycle projection. A separate 40-assertion
lifecycle contract covers generation-bound start, stop, failure, Retry and
stale callback rejection plus complete gamepad-frame bounds, neutralization and
old-generation rejection for non-audio Android Adapters.
The root Android `capyioCheck` task includes this composition executable so the
registry cannot silently disappear from the aggregate Android Gate.

ANDROID-A01C1 adds a hardware-free shared-presentation packaging check. Gradle
builds the exact `apps/desktop` Vue source, stages it as `assets/capyio-ui`, and
requires an HTML entrypoint plus JavaScript and CSS files. The HTML check rejects
root-absolute `/assets/` references so the same Vite output is valid in Tauri and
an Android packaged-asset host. `capyioCheck` depends on this verification and
debug APK assembly; archive inspection additionally confirms all three artifact
classes are present. This is packaging evidence only: the native Activity is
unchanged, no WebView/Tauri observer bridge runs, and no APK is installed.

ANDROID-A01C2 extends the hardware-free composition executable from 39 to 51
assertions. It projects all seven non-display Capabilities into shared schema 5,
checks Android observer mode, node and capability counts, Source/Sink direction,
read-only sharing flags, JSON escaping and a disconnected-service snapshot.
Android Lint and debug compilation cover the local-only WebView host, its single
read-only JavaScript method, the native/shared view switch and Back fallback.
This still provides compile/contract evidence only; no WebView was rendered on
a physical device and no APK was installed.

ANDROID-A01D1 raises the composition executable to 70 assertions. The added
checks cover two schema-v2 `capability_control` Quick Actions, stopped/starting/
active projection, exact available operations, null Route identity, fixed IMU
and gamepad targets, and state-dependent rejection of unknown IDs, invalid
operations, shell-like text and invalid transitions. `capyioCheck` compiles the
real three-method JavaScript bridge and Service Binder calls, then runs Lint and
assembles the debug APK. No physical sensor was opened and no APK was installed.

ANDROID-A01D2 raises the composition executable to 98 assertions. It covers
complete-state composition across buttons, d-pad, both sticks and triggers;
reset behavior; exact field bounds; rejection of unsupported buttons, malformed
unused fields and shell-like kinds; and stopped/active shared-UI JSON projection.
The aggregate Gate also compiles the synchronous generation-bound Binder
submission and packages the shared Controller code. This remains compile,
contract and package evidence: no APK was installed and no physical touch input
was exercised.

ANDROID-A01D3 raises the composition executable to 131 assertions. New checks
cover touchpad attachment, generation-bound Start/Stop, complete-frame sequence
advance, Stop neutralization, stale-generation rejection, zero-to-five contact
parsing, canonical integer syntax, coordinate/pressure bounds, duplicate IDs,
shell-like input rejection and shared-UI state projection. `capyioCheck` also
type-checks/packages the common Vue Touchpad surface and runs Android Lint. This
is hardware-free compile/package evidence; no APK was installed and no physical
multi-touch or Windows projection was exercised.

ANDROID-A01D4 raises the composition executable to 150 assertions. It verifies
five independent Android Capability Quick Actions, permission-required audio
projection, microphone/speaker target separation, lifecycle-valid operation
admission, malformed/unknown rejection, and bounded audio state JSON including
the actual negotiated format and processed-frame count. Lint/debug compilation
cover the bridge handoff back to the Activity permission path. Physical
permission dialogs and audio operation remain covered only by the earlier native
UI device evidence until the shared surface is installed and exercised.

ANDROID-A01D5 raises the composition executable to 170 assertions. It verifies
that the four attached hardware-present Android Sources (microphone, IMU,
gamepad and touchpad) are selectable, while speaker, haptics and the unattached
camera are not. It also covers select/unselect idempotence, exact Node and
capability identity, unknown/null/overlong rejection, and schema-4 selected-state
projection. These checks prove local publication-intent admission only; they do
not prove peer discovery, Route authorization or a data-plane connection.

ANDROID-A01E raises the composition executable to 182 assertions. It covers the
attached camera Source and CAVC format, hardware/permission/lifecycle projection,
Source-only publication eligibility, six independent Quick Actions and exact
state-dependent camera command rejection. Lint/debug compilation cover the
reused Camera2 service and narrow facade. On the authorized V2419A, the installed
unified APK rendered the common Vue UI from the APK-only HTTPS origin; a shared
camera Start held Camera ID 0 under `dev.capyio.android` in a camera foreground
service, and shared Stop removed the service and left `Active Camera Clients: []`.
No camera pixels were retained.

ANDROID-A01H raises the composition executable to 193 assertions. It covers the
attached non-shareable haptics Sink, Available lifecycle projection, exact
shared action admission and rejection, generation-bound command construction,
zero-strength rejection and the 10-second duration ceiling. Android compilation
and Lint cover the service-owned `Vibrator` projection and approved `VIBRATE`
permission. The physical confirmation pulse remains a designated-device check;
the V2419A vibration service recorded the exact `dev.capyio.android` 35 ms,
0.28-amplitude request and UI Stop returned the lifecycle to Idle with
`CurrentVibration: null`. The OS classified the pulse as
`ignored_for_settings`, so physical sensation is not claimed and the device
setting was not changed. The test does not claim a Windows feedback Route.

CAPY-INTEGRATED-ALPHA-002 raises the Android composition executable to 208
assertions. The added checks project one selected microphone Source into a
catalog that omits unselected IMU/gamepad/touchpad/camera Sources while retaining
speaker and haptics Sinks, and reject a Sink presented as publication intent.
They also prove that an already-selected IMU can be removed after entering
Offline, cannot be selected again while Offline and leaves no stale selection.
This is hardware-free local projection evidence, not transport delivery or peer
admission evidence.

ADR 1404 adds three `capyio-android-host` identity tests and raises the Android
Capability catalog contract to 22 assertions. Java and Rust match the same
Adapter/Capability/Port UUIDv5 vectors, prove Node and identity-domain
separation, and reject malformed or overlong platform keys. These tests prove
stable identifier derivation only; by themselves they do not construct a
complete Core NodeDescriptor or encode an Android CatalogSnapshot.

ADR 1405 raises `capyio-android-host` to 17 tests. Three tests construct a
complete Android DTO, validate stable Adapter ownership in Core, select one
microphone Source while retaining a speaker Sink, and round-trip the projected
catalog through the Session-bound publication encoder/decoder. Malformed
profiles, duplicate Capability keys and contradictory unattached/Available
state fail before publication. The decoder also rejects unknown JSON fields,
unsupported schema versions and messages above 64 KiB. The hardware-free Java
composition suite (221 assertions) validates the matching Service encoder and
its conservative per-Capability Route policy. Java output byte-matches the
same checked-in fixture decoded by a fourth Rust projection test; unified Android Java compilation
verifies the Binder entrypoint. This proves the local versioned handoff
contract, not JNI invocation, network delivery or physical hardware behavior.

ADR 1406 raises `capyio-android-jni` to seven host tests. Three catalog Runtime
tests construct the local Runtime from the exact Java fixture, prove same-Node
disconnected replacement, reject identity drift and malformed JSON
transactionally, and confirm there are no peers, Sessions or Routes. A separate
offline Cargo NDK release build verifies the ARM64 JNI exports, while Android
SDK compilation verifies the matching Java wrapper. These checks do not prove
APK packaging, Service activation or a physical JNI call.

ADR 1407 adds the reproducible packaging gate. `buildRustJni` compiles the
ARM64 release library from source before Android `preBuild`; `assembleDebug`
then packages it with the shared Vue assets and unified Java Service.
`verifyIntegratedDebugApk` opens the resulting archive and requires the
manifest, DEX, UI entrypoint and exact JNI library path. The Java composition
suite has 226 assertions, including rejection-before-selection-commit. This is
install-ready artifact evidence, not a physical-device acceptance result.

ANDROID-A01E2 type-checks the common Vue Camera2 settings panel and compiles/
lints its four-method Android bridge plus the reused inventory boundary. On the
authorized V2419A, the panel enumerated seven bounded choices across directly
openable IDs 0 and 1, selected `1@auto` with `clear` quality, and Camera Service
then reported active ID 1 for `dev.capyio.android`. Source/quality changes were
rejected while Active; UI Stop recorded ID 1 disconnect and left no camera
client. No frames were retained.

Offline repository validation additionally pins the Gradle/AGP contract and
checks the source manifest for the approved audio/camera/notification/foreground
permissions, non-exported platform services, disabled backup/cleartext and the
approved Internet permission. It also requires real `AudioRecord`/`AudioTrack` builders,
`START_NOT_STICKY` and foreground notification ownership. This static check is
not an APK or hardware result.

ANDROID-A01F3 additionally requires the unified manifest to declare the
`connectedDevice` foreground type, `FOREGROUND_SERVICE_CONNECTED_DEVICE` and
its normal `CHANGE_NETWORK_STATE` prerequisite. The hardware-free policy gate
proves that Starting, Active and Stopping IMU/gamepad/touchpad/haptics states
retain service ownership, while Stopped and Failed states do not. Physical
acceptance requires a capability-only run to retain the same service/PID and
typed notification after Home, show neutral UI-owned controls after reopening,
and remove the service after the last explicit Stop.

The local debug APK build and merged-manifest inspection do not themselves
prove device behavior. A separately authorized one-device run now proves the
positive permission flow, actual 48 kHz `AudioRecord`/`AudioTrack` parameters,
concurrent and independent Stop behavior, Activity-finish survival, persistent
notification action and complete service/audio cleanup. Permission denial and
revoke, visible indicator inspection, lock/process death, focus/routing,
long-duration background and vendor power policy remain physical tests. No
microphone recording is retained by the 001C shell.

Gate 7B first proves an unchanged pinned SysVAD build and an approved-target
install as a toolchain/enumeration baseline; synthetic SysVAD
WASAPI loopback is not real-PCM evidence. It then requires `CapyIO Speaker`
enumeration, explicit application selection, endpoint-associated render APO
PCM evidence, bounded ring-full/Broker-loss behavior, silence on the ordinary
physical/RDP endpoint, audio-service/reboot survival and clean uninstall. The
APO callback must have evidence of no blocking, allocation, file/network I/O
or ordinary logging. Repository validation prevents driver source from
appearing while the SysVAD record still declares `source_imported: false`.
ADR 0029 permits `LAB-WINDOWS-HOST` for the approved-target install only after
its recovery, exact-package and rollback preflight passes.

ADR 0051 adds a remote-recording compatibility regression. The speaker-only
case must keep the Android microphone `STOPPED` at zero frames, prove non-zero
Android speaker frames with zero malformed/wrong-source/drop counters, and
verify that a host-audio recorder cannot obtain SysVAD's synthetic tone from
`CapyIO Speaker`. The 21.85 build candidate intentionally rejects that WASAPI
loopback activation; silence or unsupported activation is acceptable, but a
tone is a failure. This test does not claim genuine system-loopback support.

The native Speaker service supervisor is tested independently from physical
audio. Configuration tests reject missing, unspecified, duplicate or
mode-incompatible endpoints. Readiness parsing accepts only the bounded
`native_virtual_speaker=true` first record, output readers remain bounded and
the Runtime can become `active` from native transport readiness while keeping
`receiverPresent=false`. Physical Android counters, not UDP process liveness,
prove remote receipt.

The native microphone slice tests the two halves independently before physical
composition. Capture-ring tests prove its fixed ABI, live-owner/stale-reclaim
rule, whole-block pressure behavior and exact S16LE-mono conversion. Native LAN
tests prove the microphone binding, explicit-peer UDP reassembly and a distinct
bounded readiness record. Service tests require an all-or-none microphone child
configuration, reject it outside native mode and keep startup transactional.
Android contract tests require `AudioRecord` to hand off only through the
bounded packetizer/queue/sender worker. Physical media acceptance requires
non-zero receiver/ring counters with MicYou absent; final projection acceptance
additionally requires a normal Windows recording client to produce a non-silent
WAV from the matching deployed service build.

The controlled 001F2 acceptance records exact Release/APK hashes rather than a
friendly endpoint name alone. It requires 384,000 samples for each eight-second
48 kHz mono run, a non-zero sample count plus RMS/peak, zero client errors,
release of both native UDP ports on Stop, new owners after Start and another
non-silent capture in the new generation. Android sender counters independently
confirm generated/sent packets, datagrams, pressure drops and partial bytes;
raw microphone WAVs remain ignored and are never repository evidence.

The 001G1 orchestration Gate additionally distinguishes pair readiness from
liveness. Unit tests require transactional initial startup, no Stop call on a
surviving child after partial health loss, transition from `active` back to
`starting` without a terminal Problem and unchanged failure/reap behavior when
no child remains. These tests do not replace physical simultaneous-media,
child-exit or reconnect evidence.

## Android read-only lab commands

Android commands require `--serial`; target order is never inferred. They use
an allow-list of `adb devices`, `getprop`, `wm size` and `dumpsys
sensorservice`, impose a four-megabyte process-output bound, and retain only
model/build-version plus bounded sensor-list fields. `android-baseline` prints
sanitized JSON; `android-collect` writes it only below ignored
`test-results/android/<run-id>/`. Neither command installs an APK, grants a
permission, starts a service or changes settings.

The separately authorized physical `CAPY-IMU-001B2` run used the fixed upstream
SensorServer v7.2.1 binary after its published SHA-256 matched. Live evidence
requires paired source timestamps, sequential envelopes, equal Panel/Recorder
counts, zero silent sequence repair, a second clean connection after graceful
close, and an explicit client failure when the phone service stops. Physical
addresses, pairing codes and raw device identifiers are not committed.

The authorized `CAPY-IMU-001B3A` desktop run additionally exercises the Tauri
start/read/stop DTO boundary. Acceptance requires a visible typed failure, a
later successful connection with changing numeric vectors and monotonically
growing sample count, and a stopped state that retains the last snapshot. The
desktop Rust physical test remains ignored by default and requires explicit
`CAPYIO_LIVE_IMU_IP` and `CAPYIO_LIVE_IMU_PORT` environment variables. Normal CI
does not require a phone or a reachable private endpoint.

`CAPY-IMU-001B3B` binds that worker to the same `NodeRuntime` that owns the
desktop Node. Loopback tests assert the staged Route lifecycle, retained
disconnect Problem, fresh retry epoch and explicit stop without a phone. The
ignored physical test asserts real paired samples drive the Route to `Active`
and shutdown reaches `Stopped`. The authorized lab run also confirmed that a
stale phone listener produces `Offline` rather than a false success, then
succeeds after the service is restarted. Private addresses are not retained in
repository evidence.

## Unified Android 0.5.4 physical integration

The explicitly authorized Android 16 run installed the unified debug APK with
SHA-256 `E1335AAD51CBE4DCCA76516326633A83AA0E3C4DAECD467AD5645D58E01AC2F4`.
All controls below came from `dev.capyio.android`; legacy input applications are
not prerequisites for these paths.

- Camera: the bounded Windows receiver accepted 90 1280x720 AVC access units,
  including three key frames and one discontinuity, and decoded all 90 into
  124,416,000 NV12 bytes. Start/Stop removed the receiver and reverse mapping.
- Touch-gamepad and standalone IMU: the physical gate accepted 151 controller
  frames with zero rejects and produced 267 DSU packets. It observed a
  non-neutral touch control and finite IMU values, then stopped cleanly.
- Speaker: a five-second Windows tone sent 500 packets / 1,000 datagrams and
  Android reported exactly 240,000 processed frames.
- Microphone: a bounded six-second restart processed 291,840 Android frames
  with no sender queue loss. The service-owned Windows helper held the exact
  `<LAB_IPV4_04>:40011` socket while the Tailscale adapter gained 636 unicast
  packets / 650,049 bytes. The protected capture-ring DACL returned Win32 error
  5 to a normal-user metrics observer, so final ring-write evidence remains
  open; no audio was recorded or retained.
- Touchpad: ADB reverse, handshake and the initial empty frame succeeded. The
  first contact reached `InjectTouchpadAction`, which returned Win32 error 5 in
  this token. The desktop maps the bounded helper diagnostic to
  `CAPY.WINDOWS.TOUCHPAD_INJECTION_DENIED`. The existing VHF path was not used,
  and no driver or system policy was changed.

Physical endpoint addresses, transient PIDs and pairing material are not
normative configuration. Normal CI remains device-free.

## Unified Android 0.5.5 entry and installed-VHF acceptance

The later `CAPY_INTEGRATED_MICROPHONE_056_REPORT.md` closes the short microphone
capture gate with APK 0.5.6. It also supersedes attributing `0x80070057` to the
virtual endpoint: a physical USB control fails identically in the restricted
account and both endpoints work in the actual user's session. Retain account
identity in all physical audio evidence. Fifty nonzero capture windows and a
zero-signal post-Stop capture prove this short path without recording audio.

The explicitly authorized update installed version code 17 with SHA-256
`728992717F6615EACE082B2FFB40D2D147846C45D6D18A90E1470C0635284FED`.
Cold launch opened the shared Quick Actions URL directly and respected status
bar/display-cutout insets. The shared `原生诊断` action opened the retained
native diagnostics page, and its `返回统一界面` action restored Quick Actions.

The installed-device touchpad test used a hash-pinned receiver through
`scripts/run_windows_touchpad_integrated_vhf.ps1`. After the unified Android
touchpad action connected, five injected contact updates plus handshake/close
produced seven processed/submitted VHF frames and transitions `0->1->2->0`;
the helper exited with `lab_status=complete` and `device_cleanup=closed`.
The test did not install or modify the already present driver. Unit coverage
also requires no-interface synthetic arguments and the stable VHF elevation
diagnostic.

The Android microphone action and Windows service still reached their active
network/runtime states. The repeat ordinary WASAPI capture was not accepted:
the endpoint enumerated as 48 kHz mono float32, but initialization returned
`0x80070057` across the format/period matrix. The attempted service restart
reported access errors, so completed restart and a driver regression are not
established. Execution-account differences still need investigation. No WAV
was created. A release candidate
must repeat a non-zero ordinary-client capture after the endpoint is repaired
or provide bounded service-owned capture-ring metrics.

## Data and timing quality

Signal tests measure latency, clipping, gaps, discontinuities, loss/repeat and
RMS. Drift tests record source/sink samples, queue water level and resampling
ratio rather than inferring drift from acoustic latency. Sensor tests preserve
clock domain, sequence, units, coordinate frame, accuracy and calibration.

## Evidence format

Android WebView automation distinguishes CDP transport replies from successful
evaluation. Protocol errors, JavaScript exceptions, missing/by-reference-only
results and premature socket closure fail the helper. JSON false/zero remain
false/zero; automation must separately assert application success. Run
`node --test scripts/android_webview_target.test.mjs scripts/android_webview_result.test.mjs`
for target isolation and result-parser regressions. These hardware-free checks
do not establish Android media or Windows projection behavior.

Display connection-bundle tests use synthetic bytes only. They cover exact ZIP
entry names/counts, per-entry and aggregate bounds, selected properties schema,
managed-directory replacement and preservation after rejection. They do not
claim valid X.509/PKCS#8 material, mTLS authentication or rendered frames. A
physical file-picker pass proves only that the shared UI reaches Android's
system document picker and returns safely after cancellation.

The Windows-to-Android display provisioning converter is tested against the
same typed schema-3 parser used by service startup. Tests assert all 17 emitted
fields and an invalid-codec rejection. Bundle inspection records entry names,
sizes and whole-archive digest only; private-key or certificate contents must
not be included in logs or evidence.

```text
test-results/<run-id>/
  manifest.json
  summary.md
  config.json
  metrics.jsonl
  runtime.log
  adapter-stderr.log
  platform/device inventories as applicable
  input/output recordings only when explicitly authorized
```

`manifest.json` records Git commit, versions, OS/device, Route/Profile/backend,
network mode, case and timestamps.

The projection-scoped camera Broker tests cover its closed Status/Start/Stop
schema, bounded request correlation, idempotent lifecycle, partial-start
rollback and unexpected child exit. A Windows-only round trip creates the
remote-rejecting pipe with a DACL naming the exact current user SID. Separate
tests require Windows to parse that bounded SID and reject malformed or
DACL-injection-shaped values. The physical cross-account UAC gate records the
ordinary requester and elevated administrator as different accounts, then
requires Stopped/Active/Stopped transitions, exact child presence while active,
and empty process plus ADB-reverse inventory after Stop. A locked phone does not
count as live-frame or Media Foundation consumer evidence.

## CI policy

Required before merge: Rust format, check, Clippy warnings denied, tests,
Protobuf build, docs/repository validation, manifest validation, Adapter smoke,
frontend typecheck/build and dependency/license review when dependencies change.
Hardware jobs may be manual but must attach evidence. Claims match actual runs.

The native DS4 application-consumer Gate compiles and self-tests the SDK-only
`RawGameController` probe without an attached device. Its no-device case must
fail closed. A physical pass is recorded only inside a separately authorized,
exact VID:PID USB/IP attachment window and requires a user-visible control
change; normal CI never attaches a controller.

Pull-request workflows targeting `main` explicitly check out
`github.event.pull_request.head.sha`; a synthetic merge commit is not substituted
for the submitted head. Rust format/check/Clippy/tests, documentation, manifests
and Adapter smoke run on Windows, Linux and macOS. The frontend uses the frozen
pnpm lockfile for typecheck/build. Windows additionally runs native Tauri Cargo
check/build.

Linux/macOS native Tauri packaging is an explicit merge-gate skip in the current
foundation: those runners still execute Rust Core/Adapter and web UI gates, but
they do not count as Tauri application build evidence. Adding non-Windows Tauri
packaging requires an explicit workflow and platform prerequisites; absence is
never reported as a pass.


ADR 1511 camera acceptance has an explicit ordinary-user entry:
`scripts/test_paired_camera_regression.ps1 -Executable <candidate Desktop> -ConsumerProbe
<camera virtual lab exe> -PeerNodeId <paired UUID> -PeerHost <approved address>
-ControlPort <current CapyIO port> -EvidenceRoot <new directory>`.
It refuses an existing Host and elevated caller, waits for real device consent,
requires increasing route-bound frame counters plus a separate MF consumer, then
checks Route/lease cleanup and Host exit. It stores counters/diagnostics, not video.
A failed consumer is a failed check even when the Route is Active. Do not confuse
ADB's wireless-debug port with the app's current control port. Execution requires
an available paired device and the explicitly configured allocation service; parsing
the script alone is not a physical pass.

The consumer executable must support `consumer-current-live-probe`: after the initial
format/timing checks it reads up to 90 more samples and requires at least three
whose middle-row color-bar signature differs from the deterministic placeholder.
The older two-sample `consumer-current-probe` only proves system activation and
sample delivery. It does not establish live content. This signature check does not
measure visual quality, latency or scene identity; raw frames are never saved.

## Linux headless Node management

ADR 1514 adds hardware-free `capyio-linux-host` unit tests and Linux-only
`capyio-node` subprocess tests. Run `cargo test -p capyio-linux-host -p capyio-node`.
They prove stable explicit identity, Linux snapshots, bounded local NDJSON,
malformed-input recovery, EOF, shutdown and oversized-request exit. Process tests
have bounded response/exit waits and kill/reap on failure. They do not establish
mock Route composition, secure peer connectivity or Linux hardware/projection.


## Linux mock subprocess lifecycle

ADR 1515 adds opt-in Source/Sink subprocess control and synthetic Runtime Route
acceptance. Build `cargo build -p capyio-mock-source -p capyio-mock-sink` before
`cargo test -p capyio-linux-host -p capyio-node`; `cargo xtask test` and core CI do
this explicitly. Tests require the known binaries beside the node executable.
Real-process tests cover start/stop/restart, event-bound saturation, owned mock
exit reconciliation on the next snapshot, EOF, oversized input and shutdown.
Injected controls cover wrong Route/epoch completion, partial-start rollback and
Stop failure. These prove lifecycle composition, not sample transfer or hardware.
The cloud environment lacks usable descendant `/proc` inventory; external PID-tree
cleanup was not independently observed. The existing Sidecar owner performs
kill/wait cleanup; arbitrary hostile child deadlines remain outside this scope.

ADR 1516's finite Linux payload tests require Unix-domain socket creation. Build
both mock binaries, then run `cargo test -p capyio-mock-source -p capyio-linux-host
-p capyio-node --all-targets`. Socket transfer, missing-peer timeout, oversized
record rejection and real subprocess receipt/cleanup tests remain mandatory.
Sandbox EPERM is an unmet prerequisite, never a passing test or lifecycle proof
of payload delivery. Socket-free parser/receipt tests can be reported separately.


The finite-fixture acceptance run for exact commit
`4210120b722cf834e432fc12f92772e7f8deb5e7` completed on GitHub-hosted Ubuntu 24.04
with Rust 1.97.1: [run 39277747051, job 118146302738](https://github.com/Arthurshen926/HardwarePool/actions/runs/39277747051/job/118146302738).
All 30 focused tests passed (13 host, seven mock library, ten node process), with
zero ignored/filtered tests and all eight explicit IPC guards satisfied. ADR 1516
records the retained log evidence and bounded acceptance scope. This hosted run
does not change the original cloud environment's socket restriction or establish
continuous streaming, physical hardware or production networking behavior.

ADR 1517 batch tests add socket-free ordering, exact binding/timing, aggregate
receipt, partial completion and transactional queue-full checks. Hosted tests
require actual four-record IPC, missing/partial sender timeouts, oversized
datagrams and all owner-cleanup exits. The dedicated workflow retains its eight
single-record guards and requires batch guards too. Exact implementation commit
`07b068e43ccf628326288d2918a2b90021984918` passed
[run 39280283094](https://github.com/Arthurshen926/HardwarePool/actions/runs/39280283094) on 2026-10-19:
39 focused tests (14 host, 13 mock library, 12 node subprocess), zero ignored or
filtered tests, and all 17 mandatory IPC guards including the paced whole-budget
regression. This is finite four-record batch acceptance, not continuous streaming,
hardware or production networking evidence. No restricted local socket retry is
allowed. A later documentation-only evidence commit is not the tested SHA.

## Linux Android-compatible lab playback

ADR 1518 adds nine socket-free Linux playback configuration/pump/argv tests and
four socket-capable subprocess/UDP tests. The dedicated Linux IPC workflow
requires their successful outcomes and the existing shared Rust/Java golden wire
regression. Synthetic UDP-to-child byte equality, EOF, early exit and full stdin
cleanup are not physical PipeWire or Android device acceptance. On the restricted
cloud host run `cargo test -p capyio-linux-audio --lib -- --skip ipc_` only; compile
the full suite without executing sockets. No actual audio was captured or played.

ADR 1519's Runtime-driven Linux audio tests exercise dynamic bindings, grant/lease
expiry across open/receive, lost lease, closed Session, changed grant, clock
regression, finite lifetime, stale owners and cleanup errors without sockets.
Use `cargo test -p capyio-linux-audio --lib -- --skip ipc_` in the restricted cloud.
The additional `ipc_dynamic_route_udp_pcm_stop_releases_socket_and_lease` test
must execute in hosted CI: real UDP and a synthetic child do not prove Android,
PipeWire, cryptographic admission or audible playback. Local all-target compile
is not an IPC pass. The caller-driven worker has no independent expiry watchdog.

ADR 1520 adds socket-free admission-policy tests and hosted-only real TLS owner
fixtures. The latter use ephemeral in-memory certificate pins, reciprocal offers,
UDP and an owned synthetic child to test readiness/Stop, wrong-Stream preflight,
disconnect, terminal timeout and Drop. They do not provision credentials or prove
physical Android/PipeWire, continuous low-jitter playback or secure UDP. Local
`--skip ipc_` runs deliberately exclude all socket-capable audio fixtures.

The ADR 1520 hosted gate also requires a paced trickled-frame regression against
the absolute TCP operation deadline, cancellation of the watcher on success, and
an actual TLS owner read clamped to the local Sink lease. Per-read timeouts alone
are not evidence of a bounded whole control operation.

ADR 1521 adds socket-free host-loop/Stop-channel and incremental-frame tests,
plus hosted-only actual TLS host supervision and audio-owner polling regressions.
These cover idle control with continued PCM delivery, authenticated polled offer,
remote Stop, expiry/disconnect and local lifecycle cancellation. Synthetic child
playback remains `/bin/cat`, not PipeWire. Run only the new host `playback::tests`
and TLS `incremental_poll` socket-free filters with `--skip ipc_` locally after checking exact names;
all `ipc_` cases require the socket-capable hosted job. No local skipped case is
counted as passing. ADR 1521 records the remaining executable/device acceptance.

ADR 1522 adds pure launcher config/framing/timer/key-cleanup checks and hosted-only
`linux_playback` subprocess tests. The latter use FIFO pipes and ephemeral test
credentials for real pinned TLS/catalog, nonce-bound consent, finite offer,
redaction and default CLI regression; no reciprocal media offer or PipeWire is
used. Additional `ipc_try_accept_*` cases verify whole handshake/bind deadlines.
Compile-only checks are not IPC passes. Run only `launch`, `launcher_io`,
`secret_der`, and `accept_deadline_check` pure filters locally; exact-head hosted
CI is the execution gate.

ADR 1526 adds pure `scripts/test_pipewire_loopback.py` signal/graph checks and
`capyio-linux-audio --example pipewire-pcm-fixture` unit tests. The explicit
fixture main and `scripts/run_pipewire_loopback.py` execute only in the separate
hosted `pipewire-loopback` job. They start a private actual PipeWire daemon and
synthetic null-sink graph, never hardware or a user's session. Do not run the
fixture main locally when IPC is sandbox-denied. Only the hosted waveform and
cleanup success marker proves software graph traversal; see ADR 1526 for limits.

ADR 1527 adds the explicitly ignored `hosted_pipewire_android_source_end_to_end`
command integration test. Compile it with `cargo test --locked -p capyio-node
--test linux_playback --no-run`; do not execute it locally under IPC restrictions.
The hosted private graph harness invokes the exact ignored test with opt-in and
private markers, and requires integrated control/UDP/PipeWire/Stop success. A
normal test-suite report listing this case as ignored is not acceptance evidence.

ADR 1529 adds incoming-Draft tests under `incoming_draft` in the existing
`capyio-node --test linux_playback` target, plus pure launcher admission/grant
predicates, actual Android `begin_audio_use` selection tests and catalog metadata
roundtrip coverage. Hosted Linux validation must enumerate and execute these
cases without skipping; the Android compile contract still exercises the shared
JNI coordinator path. The existing PipeWire graph gate continues to validate
Linux-led playback only. No incoming-mode sound or physical device result should
be inferred from negotiation/lease tests or Android-target compilation.

ADR 1530 adds pure `virtual_microphone` mode/argv/identity tests plus an explicit
private real-PipeWire source integration gate. This must prove direct Audio/Source
output-port readback by a separate recorder, not a renamed null-sink monitor.
Default playback remains separately gated. An ignored test listing, compilation,
or synthetic stdin backpressure test is not real virtual-microphone acceptance.
Implementation `510aea94` passed all three hosted gates in
[run 39453132776](https://github.com/Arthurshen926/HardwarePool/actions/runs/39453132776),
including all four virtual-source scenarios: waveform, no-consumer Stop, owned
child exit and server disconnect. These use Linux-led proposals; incoming-Draft
real-graph audio, physical testing and PulseAudio/ALSA compatibility remain
unverified. The ignored test is explicitly executed by the hosted harness; its
ordinary ignored listing alone remains insufficient evidence.

ADR 1531 adds `capyio-node --test linux_capture`: ordinary IPC cases exercise
finite bilateral stereo negotiation and denial/expiry/Stop cleanup. The explicitly
ignored graph fixture runs only under `scripts/run_pipewire_capture.py` against
a private actual PipeWire server, separately for waveform, child exit, server
disconnect, control disconnect and expiry. Each scenario requires its exact
success marker. `scripts/test_pipewire_capture.py` checks the distinct-channel
PCM oracle and graph/ownership rejection predicates without starting IPC.
Compilation, a normal ignored listing, or those pure tests are not graph evidence.
Android policy and shared Rust UDP reception are real; consumption is simulated,
not Java AudioTrack or device acceptance. Existing three hosted jobs remain gates.

Reverse implementation `e2316b06` passed all three hosted jobs in
[run 39560030792](https://github.com/Arthurshen926/HardwarePool/actions/runs/39560030792).
All five reverse graph scenarios emitted their required pass markers, and the
four prior virtual-microphone scenarios also passed. This validates private
synthetic PipeWire capture through the production Linux sender and Android Rust
policy/shared receiver with simulated consumption, not physical AudioTrack output.


## Linux native camera decoded-frame acceptance (2026-10-21)

Commit `23bcc2f00d21e5016ea221fee7a8c196264765ce` passed all four jobs in
[run 39563035324](https://github.com/Arthurshen926/HardwarePool/actions/runs/39563035324).
The `linux-native-camera` job passed six Runtime-owner tests, strict native
protocol/assembler/terminal-result checks, four pure graph-harness regressions,
and seven real private-PipeWire scenarios: pixels, no_consumer, expiry,
child_exit, server_disconnect, malformed and partial. A separate native consumer
verified 20 full synthetic canonical NV12 frames, including exact byte pattern,
Y/U/V distinction, stride, negotiated colorimetry and sequence order. The harness
checks approval before creation, exact Client/PID/PPid ownership, no autolinks,
and owned cleanup without removing an unrelated sentinel. Server-loss acceptance
checks a fresh graph for no stale reconnection instead of claiming old-server
sentinel continuity. Startup fault cases do not claim post-delivery failure tests.

This is synthetic decoded ingress through the production Runtime owner and native
projection, not Android AVC/network, portal/V4L2, physical camera, or third-party
application acceptance. Prior mono/virtual-microphone/reverse-stereo gates remain
separate retained evidence. See ADR 1532 for the optional dependency and lifetime
boundaries; cloud graph attempts denied EPERM were not treated as test passes.

ADR 1533 adds default `capyio-linux-avc` admission/header tests and opt-in actual
native decoder tests. `scripts/test_avc_native_helper.py` verifies generated
pixel hashes, sequence/PTS/EOS, malformed SPS/AU/IPC, actual orientation SEI,
expiry and cleanup against the built fixed sibling helper. Ignored Rust tests
must emit both exact `CAPYIO_AVC_*_PASS` markers when explicitly executed.

`Linux AVC validation` separately exercises `scripts/run_pipewire_avc.py` with
production `RuntimeAvcCamera` -> existing `RuntimeCamera` -> native projection ->
independent decoder-pixel consumer. It reuses the accepted camera private-graph
harness and checks both helper PIDs, exact links and unchanged sentinel. Eight
scenarios are required: pixels, no_consumer, expiry, child_exit, decoder_exit,
server_disconnect, revoke and malformed_cavc. Compiled fixtures and separate
component passes are not composed-graph evidence. The existing four Linux IPC
jobs and seven camera graph scenarios remain unchanged. This adds no real-device,
network-camera or physical-delivery acceptance claim.

ADR 1534 secure-camera software tests exercise distinct secure-offer validation,
legacy-wire compatibility, exact pinned TLS bindings and pre-allocation limits,
plaintext/wrong-ALPN/wrong-pin rejection, handshake/partial-record cancellation,
and actual shared Android Source consent/lease policy. The secure graph driver
must separately pass actual pinned control/data into the CAVC decoder and an
independent private PipeWire pixel consumer, including terminal cleanup scenarios.
Until that exact hosted candidate succeeds, compilation and isolated TLS tests
must not be reported as composed graph evidence. Camera2, physical devices and
general desktop/portal availability remain outside this acceptance.

Offline AVC implementation `30d0a590` passed all eight composed graph scenarios
in [run 39564578158](https://github.com/Arthurshen926/HardwarePool/actions/runs/39564578158),
including 20 exact decoded frames and both owned helpers' cleanup. Both actual
Rust IPC markers and ten native tests passed; the existing four jobs remained
green in [run 39564578050](https://github.com/Arthurshen926/HardwarePool/actions/runs/39564578050).
This is bounded synthetic offline camera evidence, not authenticated live Android
video or physical-device acceptance.

Secure camera implementation `e2be08dc1597aeedab4a7d0d111fd1fc24a8fc3b` passed all
eleven actual secure graph scenarios in [run 39570270828](https://github.com/Arthurshen926/HardwarePool/actions/runs/39570270828). This covers
actual Android Rust consent/Source lease policy, pinned control/data, synthetic
CAVC and the real Linux decoder/projection with independent 20-frame NV12 pixel
readback and cleanup. All six jobs across three workflows were green. The
Java/JNI Camera2 bridge is a separate implementation/validation increment; this
result does not establish physical capture or general desktop camera availability.

## Linux touchpad software-only acceptance (ADR 1535)

`cargo test --locked -p capyio-linux-touchpad` exercises the production bounded
projector and Runtime owner. Projector cases cover five contacts, reorder/removal,
fresh local IDs for reused or u32-max wire IDs, pressure/confidence, initial and
gap CancelAll, epoch/time validation and 4,096 deterministic randomized snapshots.
Owner cases use actual loopback mTLS, Android touchpad policy and a fake event
backend: before-Hello admission, missing leases, malformed Hello, delayed commits,
write fault, Stop/revoke/EOF/idle/finite expiry and ACK ordering.

`python3 platform/linux/capyio-touchpad/native/test_helper.py` compiles a fake
backend into the production helper loop. Shared descriptor tables are recorded,
and malformed/partial IPC, rate limits, missing/denied create, write failure,
EOF, SIGTERM and finite/idle cleanup are checked without any input device.
The separate hosted workflow compiles `native-uinput` with official libevdev
headers; it never runs the real helper. Neither fake traces nor compilation prove
kernel creation, desktop recognition, gestures or physical Android input. Those
remain deferred and require separate explicit OS testing authority.


## Finite camera executable acceptance

ADR 1536 adds actual capyio-node subprocess coverage over inherited framed bootstrap/approval and Android camera policy, alongside a private PipeWire consumer gate for synthetic secure CAVC. Required named negatives guard malformed config, approval, Source, certificate pins, leases and finite lifecycle. Independent pixels and helper parentage/teardown, rather than queued output, establish graph acceptance. Native compilation, portable subprocess tests and real private-graph execution must be reported separately.
