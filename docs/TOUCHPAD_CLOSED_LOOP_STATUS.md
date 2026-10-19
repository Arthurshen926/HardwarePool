# Touchpad closed-loop status — 2026-09-18

## 2026-09-19 debug-launch and gesture transport follow-up

### Integrated touchpad offline regression

On the TB323FU, the unified Android UI reported the touchpad offline with
`Android Adapter reported a bounded failure` and
`CAPY.ANDROID.TOUCHPAD_RECEIVER_UNAVAILABLE`. This was the legacy local-lab
quick action starting an ADB-reverse receiver at `127.0.0.1:61000`, not a
failure of the touchscreen itself. The touchpad quick action no longer offers
start/retry; the supported path is to publish the local Source, connect an
authenticated Windows node, request the remote touchpad resource and approve
the Route on Android. A previous Route failure no longer marks a present
touchscreen as offline, and the UI exposes a clear-error action.

The corrected debug APK (SHA-256
`66E3F7CDF6B8CC9E6FEF8529983F0274520D2345E04ECEEEC2A99ABF58876851`)
was installed on the authorized TB323FU Android 16 tablet. WebView bridge
inspection showed the touchpad idle, available and selectable; selecting it
for sharing succeeded, and the legacy start operation was rejected. Desktop
UI build, Android capability composition/compile, Android APK assembly and
Rust touchpad capture tests passed. Reinstalling the APK disconnected the
prior desktop control session; this follow-up does **not** establish a new
Windows Route or physical gesture success rate. Reconnect the same paired
desktop UI to the Android app's current control port before that test.

### Windows pairing identity mismatch

A later user-launched elevated CapyIO window was verified to run the expected
`target/debug/capyio-desktop.exe`, yet displayed the paired name `Android 设备`
and attempted TLS with pinned peer certificate SHA-256 prefix `<redacted-cert-prefix>`.
The readable pairing directory under the interactive user's
`LOCALAPPDATA/CapyIO/Control/Pairing` instead names `Lenovo Y700` and pins
prefix `<redacted-cert-prefix>`. These are distinct pairing records; an address, port or
discovery refresh cannot reconcile them. The exact reason for the alternate
store selection remains unverified because the elevated process environment
and token are not readable from the sandboxed diagnostic terminal. Windows
DPAPI also binds credential decryption to the protecting user identity.

Settings now refreshes paired metadata on entry and offers an explicit refresh
button. It exposes the current process SID and selected pairing-store directory
without exposing credential bytes, allowing the elevated and ordinary windows
to be compared directly. A truly different Windows identity must use the
original account or complete a fresh target-bound pairing invitation in that
identity; private keys must not be copied or automatically trusted. This is a
diagnostic and recovery-path fix, not yet proof of a successful reconnect from
the user's elevated window.

The newly built standalone release EXE (SHA-256
`2965E66D56140E251ED4DC2243FC09A8227E3892CD6A8275B7F3CB70BABBB053`)
opened in a separate window and showed the `Lenovo Y700` pairing from the
current user store, while the still-running elevated debug window continued
to show `Android 设备`. This reinforces a process-identity/store difference;
the release launch did not perform a TLS connection or VHF projection test.

After the user closed the elevated window, the same embedded-UI build was
placed at `target/debug/capyio-desktop.exe` (SHA-256
`E27C5EBE91BB050C1DD6EFF3D22A9B2CFC564130CB34B62A74E93C520FD79066`).
A normal launch of that exact debug EXE displayed the new refresh control and
the `Lenovo Y700` pairing; its test window was then closed. The elevated
post-replacement window and TLS connection still require a separate check.

The user subsequently opened the rebuilt EXE elevated and read its diagnostics:
the process SID is the `local-user` account (`...-1001`) and the selected store is
`%USERPROFILE%/AppData/Local/CapyIO/Control/Pairing`. Thus an alternate
Windows account or store path is **not established for the new elevated
process**. The earlier window's different paired name remains historical
evidence, not proof of why that prior process differed. At this point the
Android foreground Service still listens on its CapyIO control port, and the
Windows host can reach that TCP port; authentication from the rebuilt elevated
window must still be attempted before claiming connection recovery.

The default Windows store path is now obtained from the effective process
user's `FOLDERID_LocalAppData` known folder instead of trusting an inherited
`LOCALAPPDATA` environment variable. Explicit test-store construction is
unchanged. This prevents a mismatched process token/environment from silently
reading another profile's pairing metadata that its DPAPI identity cannot use.

### Tablet gesture and haptic follow-up

The Lenovo Y700 user reported that most gestures work but the tablet's own
three-finger system action sometimes wins, four-finger Windows recognition is
inconsistent, and local feedback amplitude is fixed. The immersive Activity
already hides system bars and requests WebView gesture exclusion. Android/OEM
mandatory gestures are not a permission an ordinary application can revoke;
the UI now states that limitation rather than promising full interception.

The initial unified-UI attempt held the third down for 90 ms, but comparison
with `CAPY-PTP-003T` showed that this was **not** the validated standalone
policy. The follow-up restores the standalone principle: every pointer
down/up is published immediately, while MOVE is settled for 24 ms at first
contact and 72 ms after an added contact. The 700-per-mille spatial attenuation
from the standalone path was already present in the reused Android Rust capture
session (`AndroidTouchpadMotionPolicy::RECOMMENDED`); the WebView had omitted the
MOVE settling. This correction still needs a measured physical success-rate
comparison on the Y700.

Android Settings now exposes a 0–100% touchpad feedback slider. The value is
validated by the narrow WebView bridge and stored in app-private preferences;
0 disables feedback, and supported amplitude control scales the existing tap
and drag pulses. Devices lacking amplitude control may not vary perceived
strength. The unified debug APK (SHA-256
`C3FA6AD14645731DAF7CD6254E41BB687ED4F2037F20EFB0F1DD271BF2D7391C`)
was installed on the authorized TB323FU Android 16 tablet without changing
permissions or system gesture settings. Shared UI build, Android Java compile
and `assembleDebug` passed. On device, the setting changed from 50 to 70,
accepted 0 and rejected 101. A separate 70% value survived a force-stop and
relaunch; the original 50% was then restored. Actual three-/four-finger OS
recognition and perceived vibration
amplitude still require user-observed physical repetition.

The reported `127.0.0.1` refusal was reproduced as a build-mode mismatch:
the Tauri debug configuration points at the Vite development server, while a
plain Cargo-built EXE does not embed the frontend. Rebuilding the shared UI and
`capyio-desktop` with `tauri/custom-protocol` produced a standalone debug EXE
(SHA-256 `BC052B4616CBED7121513577439847C3A6D1BC904E1DE7676716A0B2FEAC3CF5`).
Its real elevated window loaded the Settings page and the persisted Lenovo Y700
pairing instead of an HTTP refusal. The integrated-test package script now uses
the same embedded-UI feature for the desktop binary; this script change was
syntax-checked, not followed by a full package rebuild while the running EXE
was in use.

An authorized Android-to-Windows paired touchpad data gate then opened a fresh
consented Route and the immersive Android surface. A WebView touch-injection
matrix delivered one-finger tap/move, two-finger tap/vertical pan, three-finger
horizontal swipe and four-finger horizontal swipe. Each produced the expected
one-to-four active contacts, advancing Source sequence and an empty released
frame; the pinned mTLS/VHF receiver gate stayed live and ended with
`CAPYIO_TOUCHPAD_DATA_PASSED`. Local lab output is under
`target/lab-evidence/CAPY-INTEGRATED-touchpad-gestures-all.txt`. This is
transport/lifecycle evidence, **not** proof that Windows generated a click,
right-click, wheel event, desktop switch or haptic effect. A further targeted
Windows-visible-effect run could not be completed from the automation token:
starting a new elevated test process failed before launch with `0xc0000142`,
and the already elevated GUI did not accept automated edit-field input. Those
user-visible gesture outcomes remain open.

## Desktop pairing refresh and certificate-error follow-up

The user's settings screenshot confirms the debug desktop executable and the
expected peer/endpoint, but reports TLS `BadSignature`. Normal-user and elevated
control diagnostics succeeded against that endpoint. The UI displayed a different
paired display name from the persisted directory. Read-only inspection confirmed
the running GUI executable and its `LOCALAPPDATA` pairing base; no alternate
account/store path was found. These observations do not yet prove why the GUI
retained different metadata or establish GUI connection acceptance.

The desktop previously cached pairing metadata for its entire process lifetime.
It now reloads the existing local pairing store before returning paired-device
metadata and before manual/discovered dial selection. Updated handles and pins
are selected together; revoked, removed, or unreadable records cannot fall back
to cached trust. No network certificate is automatically adopted and no pairing
material is replaced by this fix. This refresh governs new dials; it does not
claim cross-process immediate revocation of already active Sessions.

Regression coverage replaces a record while the host remains alive, exercises
both dial paths, checks handle/pin/name consistency, and rejects revocation,
corruption and empty trust. Desktop unit tests: 82 passed, 10 physical tests
ignored. All-target desktop Clippy and shared UI build passed. Three peer UI
tests passed, including a distinct certificate-failure explanation instead of
treating `BadSignature` as a firewall/port problem.

The rebuilt normal-user GUI was subsequently exercised through its actual
settings form (input event and Connect button), not the verification CLI. It
displayed the current persisted peer name, authenticated successfully, reported
one control connection and synchronized the remote catalog. The remote page
showed the Android touchpad Source, speaker Sink and vibration Sink. No resource
was started. This closes the observed normal-user GUI control-connect check,
not elevated VHF input, owner consent, gestures or disconnect-with-held-contact
acceptance. The reason the previous process held different metadata remains
unproven; no CA/pin replacement or validation bypass was used.

Standalone debug build command:
`cargo build -p capyio-desktop --features tauri/custom-protocol --offline`.
The custom-protocol feature embeds the built frontend; a plain Cargo build can
instead depend on the unavailable development web server. Verified EXE SHA-256:
`4CDA5CAD6683D44645025444F63CCDA773FA7F98ECED3D8474FAE04ADF54677C`.
The temporary WebView inspection endpoint listened only on loopback and was
closed with the test process after checking the remote page; no persistent
debugging environment variable was configured.

An elevated user retry later reported the same Node ID under an unexpected
display name and a TLS `BadSignature`, while the persisted pairing directory
still named `Lenovo Y700`. Read-only process inspection proved the elevated GUI
ran as the same Windows user with the same executable and `LOCALAPPDATA` base.
The exact elevated CLI verification then authenticated and received all three
capabilities, so neither certificate replacement nor relaxed validation is
justified. The remaining GUI-only pre-handshake operation was native VHF
interface discovery. Control connection now authenticates the peer first and
only then probes/registers the local touchpad Sink before preparing the catalog.
This enforces a cleaner trust boundary and prevents failed endpoints from
triggering driver discovery. It was not established as the concrete cause.

The later endpoint audit established that `<LAB_IPV4_01>:40657` was the current
Android wireless-debugging/ADB endpoint, while the visible CapyIO Android page
reported control port `43193`. The rebuilt executable failed closed against the
ADB port (it is not the CapyIO protocol) and authenticated against `43193`,
receiving three capabilities. Windows and Android non-secret pairing directory
records also carry the same `pairedAtMs` value, proving they came from the same
invitation. The certificate-error UI now explains that `BadSignature` can mean
the user reached a different TLS service and asks for the CapyIO control port
before suggesting pairing repair. The exact port entered in the earlier failing
elevated GUI was not retained, so this is strong endpoint evidence rather than
a claim that the historical click was directly observed.

## Connection timeout hotfix (ADR 1475)

The user's repeated Windows timeout was reproduced against the actual Android
port while its foreground Service remained alive. TCP/TLS bytes reached the
tablet, but its control accept thread was absent. A peer-level exception exited
the entire Java loop and left the listening socket advertised. Restart alone
did not prevent the next rejected connection from recreating the defect.

Fixed per-connection exception isolation and explicit native link reconciliation
without replacing the listener/port or bypassing authentication. A terminal
worker no longer exposes a valid listen port/discovery state. The precise
original exception is unknown and is not attributed to a firewall/certificate.

Installed hotfix APK SHA-256:
`756452D6782622196ECC21BF837C474B2E708E2FAAEA059B2406602E2A20E15A`.
On the Y700, paired catalog and heartbeat succeeded three times on the same
listener: after a TCP probe closed without TLS, after malformed TLS bytes, and
on a subsequent clean reconnect. Each diagnostic returned three capabilities;
no hardware/data Route was started. The accept thread remained alive afterward.
The user was asked to retry the Windows settings form against the freshly
observed control port; diagnostic success is not substituted for that GUI result.

Checks passed: Java `controlRecoveryContractTest`, Android Lint/assemble,
JNI all-target Clippy, accepted-control tests (now including bad peers before
valid TLS on the same listener), formatting and document validation. No driver,
manifest permission, firewall or stored pairing material was changed.

## Latest physical follow-up: Y700 connection ownership and native data gate

ADB `<LAB_IPV4_01>:46493` connected. The process survived the earlier ADB loss.
An independent application defect was reproduced: leaving an idle unshared
Activity destroyed the bound Node Service and changed its control port on
return. ADR 1474 fixes the explicitly published waiting state with the existing
visible connected-device foreground Service, without manifest changes.

- On device, selecting touchpad publication then pressing Home retained the
  same Service and control port 39671. `isForeground=true`, `startRequested=true`,
  and paired mTLS catalog/heartbeat passed with three capabilities. Returning,
  unpublishing and pressing Home removed that idle Service. No input Route ran
  during this test. This does not diagnose Android wireless-debugging shutdown.
- The elevated real VHF diagnostic first exposed a host bug passing a relative
  TTL to Runtime's absolute-expiry API. Fixed with an absolute timestamp and a
  regression test. Its next run exposed double-release after data closure;
  Stop now precedes data teardown, and an exact correlated Denied release is
  accepted only after authoritative Stopped status. Android distinguishes the
  explicit remote cancellation from transport failure and ignores stale worker
  completion when a new generation owns the touchpad.
- Two final elevated native data gates returned `CAPYIO_TOUCHPAD_DATA_PASSED`
  after 15-second mTLS runs, without driver installation. Logs (local artifacts):
  `artifacts/touchpad-route-data-final-20260918.log` and
  `artifacts/touchpad-route-held-stop-20260918.log`.
- First gate: Android became Active and automatically displayed the touchpad
  heading. The UI bridge accepted 20 short contact-motion updates and explicit
  release. The subsequent read was idle with zero contacts. This exercises the
  real native receiver but does not independently measure Windows cursor motion.
- Held-contact follow-up: the long CDP observation exceeded the evaluator's
  timeout. Native diagnostic completion passed, but the complete held-contact
  trace was not captured; do not mark that physical acceptance passed. A later
  UI read showed Failed with zero contacts, not a retained active contact. Its
  additional lifecycle transition is not attributed to a verified cause.
- Latest installed APK SHA-256:
  `87F072B26B63A91EBC3A014B064D4AF06A4C06FA1C5C1E271CFAC085F0A0951B`.
  Android composition: 283 assertions passed; Lint/build passed. Desktop library:
  81 passed, 10 physical tests ignored; Clippy, formatting and doc checks passed.
  Restored touchpad publication to its original false value and removed the
  temporary `tcp:9223` debugging forward. No Windows driver/policy was changed.

Still required: Windows GUI-path validation, independently observed cursor and
gestures, abrupt network-loss/held-contact native cleanup, online first pairing,
tablet-initiated use and Windows confirmation. The complete desired workflow
is not claimed done. Older sections below retain historical test/build evidence.

## Latest slice: dynamic encrypted data connection (ADR 1473)

This section supersedes the missing receiver/data-composition notes below;
older APK hashes and physical evidence remain historical.

- Added a separate pinned mTLS touchpad stream, exact Route/Stream/epoch binding,
  dynamic listener offer and service-owned Android record session. No ADB data
  forwarding or fixed lab identity is used by this new path.
- Windows registers a separate native AdapterManaged Sink, checks projection
  access, acquires its local lease and opens the existing VHF/synthetic factory.
  Logical Start alone no longer marks this path Active: an initial record must
  reach the receiver and be acknowledged. Missing offers fail closed.
- Owner confirmation is polled on the Windows control worker; the shared UI
  displays waiting/cancel and Android opens its touchpad panel on activation.
  Declining or cancelling a pending proposal cannot turn into authorization.
- Explicit stop interrupts bounded data I/O; receiver exit closes its Sink and
  removes the managed worker/Route. Fixed Java-normalized 10,000-unit coordinate
  scaling (not 65,535 input units).
- TLS, protocol and control-link suites pass (21 + 21 + 14 tests). Real TLS with
  fake Sink admission/cleanup has three passing integration tests. Desktop/JNI
  all-target Clippy passes. Shared Vue build and Android Lint/assembly pass.
- Y700 wireless ADB connected at `<LAB_IPV4_01>:40897`. The new data diagnostic
  correctly refuses this ordinary Windows token: existing VHF requires an
  elevated process. The user launched an elevated GUI, but Computer Use reports
  its integrity level is higher than the helper; automated input is unavailable.

**Not yet accepted:** physical Windows cursor/gesture delivery and held-contact
disconnect cleanup on this APK, online first pairing, tablet-initiated Routes,
Windows-side use confirmation, and remembered grants. The full requested
either-end initiation workflow is not finished. No driver/ACL/security-policy
change was made. Desktop CLI data readiness is not a physical-input pass.

Final artifacts in this slice:

- `target/debug/capyio-desktop.exe`: rebuilt with packaged shared web assets
  after the user closed the elevated GUI. Desktop library tests: 80 passed,
  10 physical tests ignored. JNI host tests: 17 passed. Formatting, Clippy,
  document traceability and manifest validation passed.
- Unified debug APK SHA-256:
  `8A104A176A8049B7B40837069BAE9A169CC8DF3589AE56FFFA88B0B5EEB74025`.
  ADB reported installation Success and started MainActivity on the Y700. Its
  subsequent PID read succeeded (17835), then the device went offline and the
  same wireless port refused reconnection. Post-install control/data regression
  is therefore not recorded as passed.
  The subsequently supplied ADB port `45423` also returned connection refused,
  including outside the sandbox; the previously observed CapyIO control port
  `36547` refused the read-only mTLS diagnostic as well.

Manual trial (paired devices only): run the rebuilt Windows EXE elevated for
the existing lab VHF; enable Android local touchpad publication; connect from
Windows Settings using the paired-device selector and Android's **CapyIO control
port**, not its wireless ADB port. In Windows remote resources open the tablet
touchpad and select Use. Confirm once on the tablet; Windows should proceed
without another click and Android should open its touchpad panel. Test actual
motion, one/two-finger gestures, Stop, cancellation before consent, retry, and
disconnect while holding a contact. Record failures; do not infer a physical
pass from an Active badge. New-device first pairing and tablet-initiated use
are not yet available through this flow.

## Follow-up: explicit resource consent

Implemented after the initial connection/cleanup slice:

- Touchpad publication no longer automatically authorizes incoming Routes.
  Android's shared UI exposes Allow once / Reject, a 60-second request deadline
  and a finite five-minute exact-Route grant. No remembered permission is added.
- Pending cannot acquire a lease. Repeated requests retain their original
  deadline. Denial, expiration, disconnection and publication loss fail closed.
- Windows preserves a pending Draft request for retry and preserves the remote
  grant expiry in its local mirror. It bounds lease TTL to that expiry and no
  longer reports Pending as denial or authorizes a Pending response locally.
- Real device testing exposed that the ordinary Windows touchpad Sink is still
  a StandardPort demo, incompatible with Android's AdapterManaged private
  contract. The UI now reports this missing integration explicitly. It is not
  solved by changing a fixture's label or marking it as real hardware.
- A dedicated diagnostic uses a clearly labeled simulated compatible Sink only
  to exercise the real Y700 resource consent UI. Explicit clicks on Reject and
  Allow once were both observed: rejection returned without a lease; approval
  completed logical Start/Stop and lease release. No input Adapter was opened.
- Updated installed APK SHA-256:
  `EBF2101FFF83AA594C3FF4764938D3E620C4553E7905055DB5252DA5A0EFB416`.
  ARM64 JNI, Java, shared Vue build, Android Lint and APK assembly passed.
  Host suites passed, including three new consent tests and the UI parser test.

Still missing: actual Windows receiver catalog/data Route composition, first
online device confirmation, reverse initiation and Windows-side use consent,
remembered grants, and real input/cleanup acceptance. Android-only consent is
not the complete bilateral permission workflow. Earlier evidence below pertains
to the preceding APK and remains labeled by hash.

## Scope and acceptance

Current priority: either device initiates, the tablet controls Windows through
the unified applications, and disconnect leaves no retained native contacts.
This acceptance is **not yet passed**. Other media feature integration is not
expanded in this slice. New data encryption is deferred per the current user
direction; existing mTLS control trust is retained. Plaintext touchpad records
remain restricted to the paired ADB loopback lab, not newly exposed on a LAN.

## Implemented in this slice

- Desktop Settings exposes the actual connection action above legacy per-feature
  address preferences. It selects native paired-device metadata rather than
  requiring UUID transcription, distinguishes CapyIO from ADB ports, and shows
  actionable failure hints while retaining the technical cause.
- Manual and discovered mTLS connection work runs off the Tauri UI thread.
  Backend trust, revoked-peer rejection and exact certificate binding remain.
  Manual/automatic frontend attempts no longer race with one another.
- Android touchpad workers own separate queues and sockets. Stop closes a
  blocked socket; stale generation tokens cannot close a retry. JNI handles are
  released on the worker on failures as well as ordinary completion.
- Held contacts are refreshed every 250 ms. The Windows lab receiver bounds
  reads to two seconds with active contacts, explicitly disconnects on boundary
  EOF, and still rejects partial records. Existing native Sink cleanup remains
  authoritative; no new driver was installed.

## Executed evidence

- Shared Vue typecheck and production web build: pass.
- Node connection/discovery UI tests: 9 pass.
- Desktop Rust library: 80 pass, 10 opt-in physical tests ignored.
- Remote-touchpad crate: 77 pass, 15 opt-in physical tests ignored.
- Desktop library and touchpad all-target Clippy, warnings denied: pass.
- Android transport-generation contract, Java compilation, Lint and debug APK:
  pass. Sandbox build initially failed accessing SDK `core-lambda-stubs.jar`;
  the same offline tasks passed in the approved normal-user build context.
- Real paired Y700 control diagnostic: mTLS, bound catalog exchange and
  heartbeat passed; two currently published capabilities were received.
- Installed APK SHA-256:
  `CE216F8CEC856C122B521D19B2E09EA6AFDFEE75B28B6B76B9905C69C56D4EB4`.
- On the installed Y700 APK, the loopback fault fixture withheld ACK three times.
  No false Active state was observed; Stop returned idle and closed the peer in
  466, 448 and 507 ms (host-observed bounds, not latency benchmarks).
  A fourth session accepted a held contact, observed three complete stationary
  frames, then stopped to idle with peer closure. No Windows input was injected.
  The gate's ADB reverse mapping was removed.
- Windows UI automation can read CapyIO but input actions repeatedly fail with
  `GetCursorPos: access denied (0x80070005)`. Consequently actual new-button
  operation and physical cursor/contact cleanup are **not** marked passed.

Debug desktop executable: `target/debug/capyio-desktop.exe` (packaged web assets).
The old running release executable was not replaced. Android package:
`platform/android/app/build/outputs/apk/debug/app-debug.apk`.

## Remaining work, in order

1. Verify the rebuilt desktop connection form in the interactive Windows
   session. The command-line result proves transport, not user-interface input.
   Overlay multicast remains unavailable; selecting an overlay adapter does not
   provide automatic discovery over that overlay.
2. Add online first-pair confirmation and per-resource consent. The existing
   offline `.cpi` invitation is not the proposed two-sided confirmation UX.
   Windows must independently consent before accepting tablet-initiated input.
3. Bind actual touchpad data endpoints and a fresh Route/Session/Stream epoch
   to authorization and owner leases. Remove fixed lab identities from that new
   path; do not treat a logical Active Route or a fixture Sink as working input.
4. Drive start/stop from either node over that admitted control connection,
   open the foreground tablet surface, expose real peer/resource/occupancy state,
   and clean up on denial, expiry, restart, suspension and transport loss.
5. Perform real one-/two-/multi-finger interaction and active-contact network-loss
   checks against the Windows projection. Validate no retained contacts, buttons
   or owned receiver/mapping after stop. The installed VHF privilege boundary and
   ordinary-user broker/package flow still need product integration.

No claim of zero-configuration discovery, online pairing, resource consent,
two-ended initiation, fully wired touchpad Route or final delivery is made here.
