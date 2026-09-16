# Integrated Windows test directory

Run `powershell -NoProfile -ExecutionPolicy Bypass -File
scripts/build_integrated_test_package.ps1` from the repository. This builds the
shared UI and the three explicit Windows binary targets offline, then creates a
new timestamped directory under `target/integrated-test`. Existing test packages
are preserved. Launch `capyio-desktop.exe` inside that directory.

The adjacent `manifest.json` records the source commit, tracked-change flag,
build time and SHA-256 of each copied executable. A dirty build is explicitly
identified; it is not represented as an exact committed release. This first
package contains the camera and touchpad helpers used by the desktop, not a
complete audio/gamepad installer. Existing audio services, installed drivers,
WebView2 and separately configured ADB remain prerequisites. Their list is
included in the manifest. The shared UI now has a version/diagnostics panel;
its Windows backend reports the embedded package revision (including a dirty
suffix), build time, and presence of the exact ADB and receiver executables.
The no-argument diagnostic command cannot select paths or launch processes.
File presence does not prove a running service, connected phone or permission
to access a driver. Android 0.5.6 supplies native version/code, optional build
revision/time and Binder attachment through the same diagnostics DTO. Build
revision/time default to `unrecorded` unless the build environment supplies
`CAPYIO_BUILD_REVISION` and `CAPYIO_BUILD_TIME`; they are never inferred from
Windows. Deeper service/device checks remain follow-up work; Browser Mock
explicitly reports simulated state.

The authorized phone update on 2026-09-16 installed 0.5.6-dev, version code 18.
The installed base APK and final local build share SHA-256
`6A9F0CAF002EB0D6B5D58CF98576503A3DA3FB30535A417892568094E7E488EC`.
Android `capyioCheck` passed, including 242 composition, 22 catalog, 40 lifecycle,
36 audio and 126 native-LAN assertions. This proves build/package deployment,
not a new physical media or diagnostic-panel rendering acceptance.

The camera receiver currently decodes AVC to NV12 and does not register a
system camera. A uniquely installed VHF interface selects VHF; missing VHF
selects synthetic input. Ambiguous interfaces, failed discovery and unsupported
platforms now fail before reverse mapping or helper creation, as ADR 1410
requires. VHF requires an elevated Windows application.

Validation on 2026-09-16: package build passed; eight receiver unit tests passed
(one physical test ignored); desktop all-target Clippy passed with warnings
denied. No physical acceptance is inferred from this build. The current
microphone WASAPI error still requires an execution-account comparison before
attributing it to the driver.
