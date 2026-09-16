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
included in the manifest; automatic runtime dependency inspection and an
in-application build-information page remain follow-up work.

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
