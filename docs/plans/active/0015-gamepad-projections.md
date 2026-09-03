# CAPY-GAMEPAD-003 — Gamepad system projections

Status: active

Owner: Codex

Created: 2026-08-29

Requirements: `FR-SCEN-003`, `FR-PORT-004`, `NFR-STAB-002..004`,
`NFR-MAINT-001..004`

## Objective

Project the portable complete gamepad-state stream into deterministic external
controller protocols while keeping IMU, touch contacts and reverse haptics as
independent typed Ports. Start with hardware-free fixed-packet evidence before
any external process, USB/IP client or Windows driver operation.

## Slices

1. `CAPY-GAMEPAD-003A` (complete): compose already-normalized semantic control
   updates into complete fixed-epoch `GamepadState` snapshots and prove the DSU
   dual-input loopback path.
2. `CAPY-GAMEPAD-003B` (complete): pin the VIIPER v0.7.0 standalone-server contract and
   implement deterministic normalized-controls to Xbox 360 20-byte input-report
   projection plus exact 2-byte rumble-feedback parsing.
3. `CAPY-GAMEPAD-003C1` (complete): add a bounded localhost-only read-only VIIPER
   compatibility probe with fixture-server tests. This covers exact management
   framing, response bounds and absolute I/O deadlines without creating a
   resource or entering the device stream.
4. `CAPY-GAMEPAD-003C2` (complete): keep create-bus, add-fixed-Xbox-device, stream
   handshake, initial neutral, fixed-epoch sequence/gap guard and explicit
   neutral/cleanup lifecycle inside one owning transaction.
5. `CAPY-GAMEPAD-003D` (complete): compose the fixed-epoch gamepad Route with
   the external Adapter lifecycle. A real VIIPER/USB/IP lab remains separately
   authorized.
6. `CAPY-GAMEPAD-004A` (complete): add the pinned motion-capable DualShock 4
   control+IMU codec with strict fixed-point and feedback parsing.
7. `CAPY-GAMEPAD-004B` (hardware-free complete): add an owned DS4 VIIPER
   session, exact USB/IP `054c:09cc` identity selection, and an explicit desktop
   DSU motion-only mode so a physical controller without motion can be combined
   with the phone in a DSU-aware emulator.
8. `CAPY-GAMEPAD-004C` (Runtime and desktop ingress complete; activation/
   physical gate pending): two independent typed Routes now compose controls
   and IMU into one owned DS4 session, including optional one-shot USB/IP
   attachment, paired fail-closed cleanup and fixed-epoch retry. The desktop
   exposes explicit Xbox/DS4 selection, identity-specific read-only preflight
   and a bounded complete-state ingress with counters. Connecting that ingress
   to the owning DS4 Worker and running the separately authorized physical
   Windows gate remain pending. DualSense, adaptive triggers and IMU-to-mouse/
   stick mapping are explicitly out of scope. A bounded DS4 rumble Route remains
   a later optional slice after the forward DS4 path is physically proven.
9. `CAPY-GAMEPAD-005A` (complete): add a top-level shared Vue Controller surface,
   desktop simulated touch source, complete-state inspector and an explicitly
   labeled IPv4-loopback DSU projection fixture. This is a hardware-free input
   and Projection diagnostic, not Android or game compatibility evidence.
10. `CAPY-GAMEPAD-005B` (physical verification in progress): add an isolated,
   foreground-only native Android Controller Lab with complete multi-touch
   controls and SensorManager acceleration/gyroscope capture; carry its bounded
   complete-state UDP frames into the desktop inspector and existing DSU dual-
   input Worker. Moving the lab behind the unified Android Node module/lifecycle
   boundary remains integration work after that common host lands.
11. `CAPY-GAMEPAD-007A` (complete): use the complete DS4 controls+IMU
   projection by default and optionally mirror only its controls into an owned
   ViGEm Xbox 360 direct-XInput companion. The companion uses a fixed binary
   process boundary, shares host lifecycle and safe-neutral cleanup, and never
   maps IMU to controls. Physical WGI evidence selected DS4-only as the default
   because both tested Xbox alternatives were live in native XInput but stale
   in WGI on the authorized host.

The following regression slices retain the earlier DSU-first and Xbox physical
evidence alongside `004`:

11. `CAPY-GAMEPAD-001D` (complete): bind the DSU Worker to one Runtime-owned,
   fixed-epoch IMU Route with typed failure ownership, cleanup-before-lifecycle
   ordering, retry and unrelated-Route isolation.
12. `CAPY-GAMEPAD-001E` (complete): add a bounded SensorServer-to-DSU lab command,
   explicit signed axis mapping, local port preflight and an end-to-end
   two-WebSocket/UDP fixture. A physical phone plus Cemu/Dolphin observation is
   now the next manual gate, not an implementation prerequisite.
13. `CAPY-GAMEPAD-006A` (driver deployed; restart complete): run the authorized
    real VIIPER gate, pin and install signed usbip-win2 v0.9.7.7 on the approved
    host with a restore point, exact package/signature evidence, no automatic
    restart and a resolved uninstall fallback.
14. `CAPY-GAMEPAD-006B` (user-mode complete): add a bounded no-shell
    usbip-win2 client, exact VIIPER Xbox export validation, one-shot attachment,
    owned-port detach and Runtime activation/cleanup composition. Real `list`
    passes before restart; mutating attachment remains post-restart work.
15. `CAPY-GAMEPAD-006C` (complete): expose a parameter-free desktop read-only
    preflight for the pinned VIIPER and usbip-win2 boundaries. The UI reports
    dependency readiness, exact Xbox export count/bus and sanitized failures,
    while attach and all driver/restart operations remain unreachable.
16. `CAPY-GAMEPAD-006D` (complete): after the operator-completed restart, prove
    exact Windows PnP and live XInput button/axis delivery from the physical
    Android source, then harden one-shot ownership with exact post-attach and
    per-second port-identity checks plus exact detach.

## 003B acceptance

- the upstream repository, release, revision, license and reviewed paths are
  recorded before protocol code is added;
- no upstream source, generated client or binary is imported or linked;
- every representable semantic button, D-pad direction, stick endpoint and
  trigger endpoint maps to the pinned little-endian packet layout;
- touchpad and paddle buttons fail closed rather than disappearing;
- source-axis orientation is explicit and configurable without UI geometry;
- reserved packet bytes remain zero;
- rumble feedback accepts exactly two bytes and preserves both raw motor
  intensities without inventing a duration or haptics lifecycle;
- all tests and repository validation pass without starting VIIPER or touching
  USB/IP.

## 004A acceptance

- the pinned VIIPER `dualshock4` device contract and reviewed paths are recorded
  before the codec is connected to a real session;
- complete normalized controls plus a separately typed canonical IMU sample map
  to the fixed 31-byte DS4 stream packet with explicit source-axis policy;
- SI acceleration and angular velocity use the pinned fixed-point scales and
  fail closed on non-finite or `i16`-overflowing output;
- touch contacts remain inactive until a separate touch Port is deliberately
  routed, while touchpad click remains representable;
- DSU motion-only operation remains independently available and never requires
  an Xbox, DS4 or synthetic controls source;
- the codec and lifecycle fixtures do not start VIIPER, attach USB/IP or mutate
  a Windows device; those remain a separately approved physical gate.

## 003C1 acceptance

- configuration accepts only an explicit loopback socket, non-zero port,
  bounded response size and positive bounded connect/I/O deadlines;
- the sole management request is exactly `ping\0` on one fresh connection and
  JSON is read only to connection close within the configured byte limit and
  one absolute I/O deadline;
- ping rejects the wrong server identity or any release other than exact
  `0.7.0`;
- malformed/trailing/oversized API responses, remote Problems, timeout and
  non-loopback configuration fail explicitly;
- tests connect only to a repository fixture listener. They never connect to,
  start or mutate a real VIIPER/USB-IP installation.

## 003C2 acceptance

- ADR 0042 resolves the fixed-revision auto-attach mismatch by requiring an
  explicit caller assertion that localhost auto-attach is independently
  disabled before the mutating API is reachable;
- one public open operation re-probes exact compatibility, creates one bus,
  adds only the default Xbox 360 type, validates the returned identity, opens
  the stream immediately and writes neutral before returning `Running`;
- once a positive owned bus ID is known, every later open failure attempts
  bounded bus removal and retains both the primary and cleanup errors;
- state validation and encoding precede sequence consumption; a gap emits
  neutral before recovery, epoch advance emits neutral before new data, and
  sequence exhaustion emits neutral and latches;
- two-byte rumble, zero-byte timeout, one-byte truncation and clean peer close
  remain distinct; stream failures are terminal;
- explicit idempotent stop orders neutral, socket shutdown and owned-bus
  removal, while Drop performs socket shutdown only;
- fixture tests use only `127.0.0.1:0`; no real VIIPER, USB/IP or system device
  is touched.

## 003D acceptance

- the Windows composition boundary registers a typed StandardPort gamepad Sink
  and owns one VIIPER Worker for one Runtime `ExternalProtocol` Route;
- the Route reaches Active only after exact probe, provisioning, stream
  handshake and initial neutral all succeed;
- Runtime epoch is the Worker's fixed stream epoch; an Offline retry creates a
  fresh Worker and uses a strictly newer epoch;
- per-frame contract/codec rejection remains non-terminal, while open failure,
  terminal stream failure and sequence exhaustion create typed gamepad-only
  Problems;
- upstream disconnect and explicit stop complete neutral/owned-bus cleanup
  before Offline/Stopped; cleanup and lifecycle errors are both retained;
- fake protocol tests prove that every injected gamepad failure leaves a
  simultaneously Active IMU Route unchanged;
- raw rumble polling remains un-routed liveness data; no reverse haptics Route,
  real VIIPER process, USB/IP attachment or system device is used.

## 001D/001E acceptance

- Runtime `Starting` returns the epoch used by SensorServer assembly; only an
  exact-epoch anchor plus successful IPv4-loopback bind can activate the Route;
- queue pressure is observable and non-terminal, while a stopped/failed Worker
  is joined before a typed projection Problem and Offline transition;
- upstream disconnect and explicit stop release the UDP endpoint before their
  Runtime transition; retry uses a strictly newer epoch;
- occupied-port, source-epoch and disconnect fixtures leave an unrelated
  Active IMU Route and its diagnostics unchanged;
- the lab command accepts only explicit phone/ports, a bounded sample count and
  two validated signed axis permutations; output does not retain the phone IP;
- automated end-to-end acceptance crosses two real fixture WebSockets, the
  SensorServer assembler, Runtime Route, DSU Worker and a real UDP subscriber;
- physical acceptance additionally requires an observed emulator subscription,
  delivered motion packets, zero loss/error counters, manual axis-direction
  checks and clean process/port teardown.

## 006A/006B/006C acceptance

- the authorized host/package/signature/restore point/install log, published
  INF names and exact rollback are retained without changing Secure Boot,
  BitLocker, test signing, boot configuration or Driver Verifier;
- installation never automatically restarts Windows and attachment does not
  run while the installer has a pending-restart requirement;
- the user-mode client launches only an absolute `usbip.exe` directly, accepts
  exact version `0.9.7.7` and an explicit IPv4-loopback server, and bounds every
  command's duration plus stdout/stderr;
- only the VIIPER-derived `bus-device` export with VID:PID `045e:028e` may be
  attached, and the exact command includes `--once` rather than persistence;
- mutation requires an explicit caller assertion covering the verified
  deployment, completed required restart and separate attachment authorization;
- attachment returns one owned hub port; explicit stop neutralizes the VIIPER
  stream, detaches only that port, then removes the owned VIIPER bus;
- a USB/IP-enabled Route reaches Active only after VIIPER initial neutral and
  the exact Windows attachment both succeed; failure remains gamepad-only;
- fixtures cover closed arguments/parsers and ordered owned-port lifecycle;
  real pre-restart evidence stops after `list`, never `attach`.
- the WebView supplies no executable, endpoint, bus or port input to preflight;
  the host uses only fixed loopback endpoints and a fixed absolute CLI path;
- the read-only inventory omits unrelated exports, reports zero/one match and
  rejects multiple exact Xbox matches instead of selecting implicitly;
- desktop state distinguishes dependency offline, ready-without-export and
  unique-export-ready while never claiming an owned Windows attachment.
- post-restart attachment is accepted only while the returned owned port still
  resolves to the exact loopback server, VIIPER bus and Xbox identity; physical
  evidence includes PnP, XInput packet/button/axis movement and disappearance
  after exact detach.

## 005A acceptance

- the shared Controller view exposes D-pad, face/shoulder/system buttons, two
  radial sticks and two analog triggers without importing platform events into
  `capyio-input`;
- Pointer Events retain independent pointer ownership so a stick, D-pad and
  face button can be active together; release, cancellation, view teardown and
  explicit reset produce neutral state;
- the Tauri host accepts only a closed semantic update DTO and uses
  `GamepadStateComposer`; the WebView cannot supply a complete trusted state,
  stream ID, epoch or sequence;
- the read-only inspector exposes complete normalized controls plus stream
  epoch/sequence, while simulated source state remains visibly labeled;
- the optional DSU fixture accepts only a port and binds IPv4 loopback, exposes
  queue/subscriber/packet/error counters, and sends or requests neutral during
  start reset, cancellation and stop;
- browser mock and Tauri implement the same DTO contract; browser mock never
  opens a socket and labels DSU unsupported;
- automated tests prove invalid updates do not consume sequence, complete-state
  composition/reset and simulator-to-bounded-DSU-worker acceptance; desktop and
  844x390 responsive browser checks have no horizontal overflow or console
  errors.

## Safety boundary

- the no-executable/no-Android-operation boundary applies to completed slice
  `005A`; the operator separately authorized `005B` to resolve Android build
  dependencies, declare only `android.permission.INTERNET`, build the debug APK
  and install it on the explicitly connected vivo lab phone;
- the no-USB/IP-operation boundary applied through `005B`; the operator later
  authorized `006A` to install the exact signed usbip-win2 v0.9.7.7 package on
  `DESKTOP-AT8EVE9`, with no automatic restart;
- restart, first attachment, driver removal and every boot/security-policy
  change remain separately authorized operations; `006B` performs only a
  read-only real `list` before restart, and `006C` makes only that fixed probe
  reachable from the desktop status surface;
- no certificate, trusted-root, Secure Boot or test-signing change;
- no foreground-service declaration, microphone/camera permission, production
  signing key, device reset or Android system-setting change;
- no Runtime/UI/roadmap integration in the packet-codec slice;
- no commit, push or pull request without explicit human approval.

## 005B acceptance

- the Android app renders explicit D-pad, face/shoulder/system buttons, dual
  radial sticks and analog trigger regions with independent pointer ownership;
- Activity pause, touch cancellation, explicit stop and a desktop 350 ms peer
  timeout all publish/request a complete neutral controller state;
- SensorManager callbacks only update bounded in-process state; a separate
  foreground-owned sender samples that state at a bounded cadence and performs
  UDP I/O outside the callbacks;
- each datagram is at most 2 KiB and carries a closed version-1 schema, a
  desktop-generated hexadecimal lab token, session/sequence, complete controls
  and SI-unit acceleration/angular velocity with monotonic timestamps;
- the desktop rejects wrong tokens, unknown fields, unknown button bits,
  reserved/out-of-range axes, non-finite motion, invalid timestamps and replayed
  sequence values without retaining or displaying the peer IP;
- one accepted phone frame updates the same desktop inspector and the existing
  fixed-epoch DSU motion+controls Worker; queue pressure and neutral timeouts are
  observable counters;
- Android lint and APK assembly pass, the manifest contains only the authorized
  `INTERNET` permission, and the APK package/hash plus uninstall rollback are
  recorded before installation;
- physical acceptance requires the authorized phone to be present in ADB, the
  installed Activity to render in landscape, controls and IMU to change desktop
  live values, DSU controls/motion counters to advance, disconnect to neutralize
  and uninstall rollback to remain available.
