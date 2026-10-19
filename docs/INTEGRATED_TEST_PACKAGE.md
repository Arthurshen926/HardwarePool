# Integrated local-debug package

The current entrypoint is `scripts/build_integrated_test_package.ps1` and emits
schema-v3 local-only bundles. Follow [LOCAL_DEBUG_UPGRADE.md](LOCAL_DEBUG_UPGRADE.md)
for required signer/third-party inputs, integrity verification, the single
`capyio-desktop.exe` entrypoint and explicit upgrade/rollback commands.

The bundle includes the complete unified Android APK (including display JNI),
embedded-UI Windows executable, service, audio helpers, display worker, camera
and touchpad helpers, Xbox companion, and explicitly hash-pinned display DLLs.
Display workers come from the matching current debug build, not older service
staging directories. Third-party display/Xbox artifacts remain explicit local
debug inputs; no public redistribution or production signing is implied.

ADB is only optional deployment/diagnostic tooling. CapyIO discovery, authenticated
control and hardware data use the actual node network; Android wireless-debugging
ports and ADB forwarding are not normal runtime prerequisites. The historical
touchpad helper filename retains `adb-lab` but is not a request to enable tunnels.

Neither packaging nor verification installs drivers, APKs, services, permissions,
certificates, firewall rules or signing keys. Existing system projections retain
their separately approved deployment prerequisites. Hashes establish byte
integrity, not publisher authenticity or end-to-end hardware acceptance.

`build_local_delivery_package.ps1` and `verify_integrated_test_package.ps1` retain
the earlier schema-v2 **limited** package format for existing callers. They do
not include the new complete companion inventory and should not be used to claim
the full integration has been delivered.
