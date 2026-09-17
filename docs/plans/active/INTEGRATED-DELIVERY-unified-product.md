# Unified product delivery, including Display

Status: active

Owner: Codex

Date: 2026-09-17

## Authorized scope

The owner supersedes the earlier Display exclusion and requests all previously
developed capabilities in one Android APK and one Windows application. Work
continues without phone access when needed. The owner subsequently designated
a Lenovo TB323FU tablet and explicitly authorized APK installation and testing;
the first software and physical checkpoint is recorded in
`docs/CAPY_Y700_INTEGRATION_REPORT.md`.
Source integration is local; existing display worktree changes are preserved.

## Ordered acceptance slices

1. Import Display contracts, transport, platform adapters and tests while
   preserving the integrated audio/camera/input baseline.
2. Merge Windows service and worker ownership, preserving independent audio and
   display routes and exact-target display cleanup.
3. Compose Android display decoding and Surface ownership into the unified APK;
   use shared UI controls instead of the standalone receiver launcher.
4. Integrate display and dependency status into the common capability catalog
   and shared Vue presentation; eliminate misleading fixture/product states.
5. Complete ordinary-user onboarding, authenticated peer catalog/authorization,
   per-capability configuration, error recovery and packaged launch paths.
6. Produce reproducible test packages with dependency/source/license manifests,
   automated regression and explicit remaining physical acceptance items.

## Software checkpoint — 2026-09-17

### Native display connection-bundle import — 0.5.11

The Android shared card now opens the system file picker and installs an exact,
bounded five-entry CPD1 or GameStream bundle without exposing credentials to the
WebView. App-private staging preserves an existing configuration after invalid
input; eight pure Java cases cover installation and rejection. A Windows script
creates the fixed archive without temporary plaintext files or overwrite.

The TB323FU installed version 0.5.11-dev (23), SHA-256
`4A3A5DC7C0B0A4885EC74EBC44FE3543582D70053EEF6BC9DBA85BD070CEE1FB`.
The shared bridge opened Android DocumentsUI and returned safely after cancel;
all capabilities remained Idle and the debug forward was removed. There is no
issued credential bundle yet, so this closes user-controlled import plumbing,
not enrollment, Windows sender deployment or rendered display media.

### Single-source Windows/Android display binding

The Windows service now converts the exact validated schema-3 service selection
to Android's 17-field GameStream expectation file without reading credential
bytes. A bounded wrapper combines that metadata with an explicitly selected
client credential set. The service gate passes 78 tests with two child fixtures
ignored, plus strict Clippy. A five-entry experimental Y700 bundle was created
locally with SHA-256
`F34B6295A0A740B0F1D66B81DF5148E46031AD91535DB24AA53F5DBBBC8D3665`.
It contains a laboratory client private key. With explicit authorization it was
copied temporarily to Download, imported into app-private storage and deleted
from public storage. The unified Windows service and APK then completed a real
HEVC Display E2E on TB323FU at 2640x1216, 120 Hz; Android logged 8 presented
frames and the retained screenshot showed 456. The final service state is
Running with Display Stopped. Production enrollment and release packaging remain
open.

- Seven Display Rust packages pass offline tests. Combined Windows service
  passes `cargo check --offline -p capyio-windows-service --all-targets` and
  56 library tests (two subprocess fixture entrypoints intentionally ignored).
- Unified Android source sets now compile the display Surface and decoder;
  manifest keeps exactly one launcher and an unexported Display Activity.
- `capyioCheck --offline` passes after locale-stable decoder states and complete
  Chinese resource coverage were added. This is software-only evidence.
- Windows display startup/control wiring, shared UI/catalog actions, native
  GameStream packaging and credential enrollment remain incomplete. Neither
  the imported standalone launcher nor lab configuration files are delivery UX.

## Completion boundary (unchanged)

### Strict Android display provisioning and failed-connect cleanup — ADR 1428

The unified receiver now rejects unknown, duplicate (including escaped aliases),
missing and out-of-bound properties instead of accepting an ambiguous config.
Shared preflight checks the selected file's structure without reading private
key contents. TLS setup closes its owned socket on unsuccessful configuration,
connect, handshake or ALPN verification. Successful setup transfers ownership;
the primary error remains visible if close also fails. CPD1 temporary key arrays
are wiped after copying, and GameStream binding validation precedes key loading.

Two final offline capyioCheck runs passed, including 13 new property cases,
five new unconnected-socket cases, the existing 14 prerequisite cases, shared UI,
Lint and native package verification. Documentation validation and diff whitespace
checks passed. These tests do not validate a real certificate or TLS peer.

The authorized TB323FU update installed the unified 0.5.10-dev (22) APK with
SHA-256 `233BD64E281D116C16EED44B32AF1D0D2C13B34EC7B22DA64973B0F6A2D155C8`.
Activity launch returned COLD/ok. The retained WebView reported the shared display
section, native component loaded, configuration absent and Open rejected.
Window policy then confirmed keyguard showing and screen off. Audio bridge data
was empty because the Activity was stopped/unbound; no AudioNodeService was
present, and camera state was stopped. This is not visible-screen interaction,
audio regression or display presentation evidence. No unlocking, recording,
camera/display capture or credential provisioning was performed. The temporary
debug forward was removed.

Ordinary-user configuration/enrollment remains open; this slice closes parser
and resource-ownership defects discovered while reviewing that workflow.

### Shared Windows display controls — ADR 1427

The shared display panel now uses parameter-free Tauri commands for service
Status/Start/Stop. A dedicated optional requester-scoped pipe keeps display
independent of audio and exposes no launch/configuration authority to UI.
The controlled service starts idle and waits for Start; Stop returns Stopping
while the service retains cleanup ownership. Unconfigured/old/inaccessible
services show unavailable controls, not successful display readiness.

Five new service tests passed, including real local test-pipe round trips,
duplicate ownership rejection, explicit-start failure and nonblocking Stop.
The service library now passes 77 tests (two subprocess fixtures intentionally
ignored). Two UI contract tests cover every state/action mapping and malformed
DTO rejection. Shared UI typecheck/build, Tauri check and warnings-denied Clippy
for service all-targets and Tauri library passed.

No installed service configuration or tablet APK was changed. The installed
service therefore has not acquired these controls yet. Physical display output,
credential enrollment, detailed media metrics, signed packaging and ordinary-
user setup remain unfinished; compile and local IPC tests do not close them.

The full Tauri library regression passed 72 tests with ten physical tests
intentionally ignored. Unified Android `capyioCheck --offline
-PcapyioGameStreamStageRoot=target/android-moonlight-unified` passed, including
Lint, native artifact/notice verification and the rebuilt shared UI assets.
The final stop/admission lock-order change passed the five focused control
tests and all-target service Clippy again. `git diff --check` passed.

Built, not deployed: Windows debug service SHA-256
`93B6A8A002501635136B4FE07D0A7368C2E3FA1FC3FB5DEF97585086120CBCD3`;
Android debug APK SHA-256
`094910CB027DBBABE2CEEE289A81F6A8E8F3DC7366393D2EAD8B956D013F9900`.
The APK retains version 0.5.10-dev (22); the installed tablet still has the
earlier hash recorded below. This UI refresh is not a new physical acceptance.

### Independent Windows service display execution — ADR 1426

The unified service now accepts an optional trusted-host absolute display
configuration path. Its background owner constructs Windows resources on the
worker thread, polls independently of audio and retains a private cancellation
signal. Display failure leaves active audio unchanged. Startup timeout requests
only display cancellation, prevents overlapping restart and retains the failure.
Service exit attempts both audio and display cleanup and reports final failure.

Eleven new hardware-free tests cover this composition, including the existing
audio runtime alongside a failed display host. `cargo test --offline -p
capyio-windows-service --lib` passed 72 tests with two intentionally ignored
subprocess fixture entrypoints; all five display worker handshake tests passed.
All-target Clippy with warnings denied and repository validation passed.

An initial compile rejected an unnecessary Send bound on the thread-local
host return value; removing that bound preserves thread affinity without unsafe
implementations. No installed service configuration, driver or tablet changed.
The audio control pipe is still audio-only: display controls/media snapshots
in the shared UI, credential enrollment and current-device output remain open.
Cooperative shutdown is tested, not a hard bound for a frozen native API.

The debug service executable built successfully with SHA-256
`DFFD60CCEB26CA9536E99A2F36DC2A00AB1B4EEC1FFE2126BCA27CBAF4674B63`.
Documentation traceability validation and `git diff --check` also passed.
The executable has not replaced the installed service.

### Unified debug native display receiver — 0.5.10

ADR 1425 adds an opt-in locally rebuilt ARM64 GameStream receiver to the same
APK. Offline staging checks pinned archives and the display-only patch; Gradle
checks source/build provenance hashes, ELF ABI, excluded audio/input sources,
the packaged binary hash and notice assets. Release remains explicitly blocked
until corresponding-source distribution requirements are closed.

TB323FU successfully installed and cold-started 0.5.10-dev (22), APK SHA-256
`D0706D37D941CF3BB9B9B868BA687E5E6EED4F1DC866F114063AE7665060B56E`.
The native library SHA-256 is
`86954468A338760578B1BA12AA7A96035FA8B3C8A1F4ED2911A857259FFDCE38`.
The bridge returned `nativeClientAvailable: true`, and the actual WebView
rendered the native component as loaded. Configuration is still absent:
`canOpen: false`, `CAPY.DISPLAY.CONFIGURATION_REQUIRED`. Audio and camera
remained stopped. No display launch or rendered-frame evidence is claimed.

The optional-native `capyioCheck --offline` and repository validator passed.
The release prerequisite task failed as intended at the distribution gate.
Physical inspection caught and corrected a hidden HTML template around the
native-status row before the final APK above was installed. Windows display
service/UI wiring, enrollment and end-to-end display validation remain open.

### Shared display receiver entry — 0.5.9

ADR 1424 exposes the same-APK Android receiver through CapyIOApi, with a shared
status panel and explicit transport/configuration/native-component prerequisites.
The Windows backend currently reports no Android receiver; it does not pretend
to expose a Windows sender. Missing configuration cannot launch the Activity.

The TB323FU test installed 0.5.9-dev (21), SHA-256
`9C51A128141F8A37060062E434F9065FBDF70DA467AB8AF16B471CC65BE8E6EB`.
Its panel reported `CAPY.DISPLAY.CONFIGURATION_REQUIRED`, disabled Open, and
the bridge independently rejected Open. MainActivity remained foreground,
audio stayed Stopped, and the temporary debug forward was removed. This is
negative prerequisite evidence, not a successful display-media test.

Shared UI build, Android `capyioCheck` (including 14 new display prerequisite
cases), manifest validation and documentation validation passed. Display
third-party records now identify the frozen MS-PL driver reference and optional
GPL native engine dependencies; no native engine binary was bundled.

### Runtime audio configuration — 0.5.8

ADR 1423 adds per-direction private/overlay endpoint settings through the common
Vue API and Android Node service. The generic APK no longer needs build-time
peer metadata for this trusted-lab workflow. It does not configure the Windows
service or establish authenticated pairing.

On TB323FU, the shared form saved microphone settings successfully; the bridge
rejected missing consent, public-IP peers, local-port collisions and changes to
an active speaker. Saving the stopped microphone did not stop the speaker.
Two-second tones before and after process recreation each produced 96,000
processed frames. The configured microphone then delivered 525 consecutive
packets / 504,000 payload bytes in a six-second receiver window, with zero
malformed, duplicate, wrong-peer or discontinuity counters. No recording was
retained. Audio was stopped after the run.

Installed APK: 0.5.8-dev (20), SHA-256
`92A429BFC61526EA1472F78B34151E7D5DE5D237409A75D2A2CD015F8E2441B5`.
Shared UI typecheck/build and `capyioCheck --offline` passed; the latter now
includes 29 pure Java audio endpoint cases. The subsequent source indentation
cleanup is nonfunctional. Display integration and automatic peer/service
configuration remain open, so this is not an overall completion claim.

### Display error-path cleanup follow-up

The single-display host now runs its owned worker shutdown and transport
disconnect after both successful and failed polling, rather than skipping
cleanup on an early `?` return. Final status preserves the original typed
failure; failed cleanup reports `CAPY.DISPLAY.CLEANUP_FAILED` instead of
`Stopped`. Unclassified host errors become `CAPY.DISPLAY.HOST_FAILED`.
Five hardware-free regression tests cover cleanup order/count and terminal
status. Panic recovery and physical topology restoration are not established
by these tests. Shared UI and service control wiring remain open.

Source import and unit tests do not establish consumer readiness. Delivery
requires the ADR 1422 gates. New real-device validation is deferred by the owner;
software work continues, and unverified physical behavior stays visible.
