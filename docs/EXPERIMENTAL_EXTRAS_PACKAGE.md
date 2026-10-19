# Experimental Extras local package

Optional display engine resources, gamepad sidecar and a complete experimental
Android debug APK are inventoried separately from Core. Third-party provenance is
in THIRD_PARTY.yml. This archive is not a standalone application or installer.

Do not blindly copy it over Core: automatic overlay is not implemented. The APK is
a full application replacement, not an Android plugin; it may come from an older
integrated build. Compare hashes, version and signer before a separately approved
installation. Installing it changes the app's native-library/license boundary.
No device installation or service configuration happens when extracting this ZIP.

Display needs the existing IDD and approved Broker engine configuration. Gamepad
needs the separately installed projection backend. Audio Share, MicYou and
SensorServer upstream applications are not included; their compatibility wrappers
and setup guidance remain in the source repository.

GPL/native display source correspondence, build provenance and redistribution
review remain release gates. This archive is local-debug-only and is not approved
for public distribution. Its manifest binds the matching Core manifest hash and
records every file hash; it does not prove installation or hardware readiness.

Verify with Python 3.11+: `python verify_alpha_extras.py . --core <Core-directory>`.
The paired Core manifest hash must match; neither this check nor extraction deploys anything.
