# Integrated alpha packaging report

The Windows desktop now defaults to an NSIS bundle. The verified local command
`pnpm --filter @capyio/desktop tauri build --bundles nsis` produced
`target/release/bundle/nsis/CapyIO_0.1.0_x64-setup.exe` and the standalone
`target/release/capyio-desktop.exe`.

SHA-256 evidence for this build:

- desktop executable: `07608C215D65DA88412C4E099C045D902522368A3D8AE0D560AB4DB3C3D5E067`
- NSIS installer: `938716C9C3B6B40C2648E27B2990FC02D7E836AF635AF6B11DEA197CFFEC11D1`
- debug APK: `341E330E1046094FF23AA2B50587BA0BBD55BCA2013F98C7BF2AAB86D0769107`

The Android `capyioCheck` build produced
`platform/android/app/build/outputs/apk/debug/app-debug.apk`. This APK is signed
with the Android debug identity and is installable for alpha testing; it is not
a production-signed release. No production signing key, Windows Authenticode
certificate, update feed or rollback service is configured. Therefore Windows
SmartScreen warnings and manual Android sideload confirmation remain expected.

Upgrade evidence currently covers `adb install -r` preserving the app-private
Node identity and previously granted runtime permissions on TB323FU / Android
16. The final installer remains `NotSigned` under `Get-AuthenticodeSignature`.
Windows installer upgrade/uninstall, failed-update recovery and signed update
provenance remain release gates rather than inferred passes.
