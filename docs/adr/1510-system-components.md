# ADR 1510: System component inventory and release preparation

Status: accepted for the user-requested release-preparation slice, 2026-10-03.

Unify delivery identity and operation planning, not kernel drivers. A release-level
component manifest is separate from Adapter manifests: the latter still contain
no privileged deployment commands. Core/domain/protocol remain unchanged.

The ordered manifest describes app, Host process mode, Broker, audio, touchpad,
camera and optional display/gamepad. Only app file operations have existing closed
handlers. Others are prerequisite-only until their target-specific deployment and
rollback gates are implemented. The initial CLI only checks files and emits plans;
it never executes a manifest-provided command or reads pairing credentials.
Unknown installed versions stay null. File hashes do not establish service/driver
registration, signing or hardware readiness. Pairing/settings are preserved.

Audio Share, MicYou and SensorServer remain compatibility sources and compiled
boundaries where currently referenced, but are not default external runtime
prerequisites or bundled upstream programs. Idle legacy audio actions move to
Workspace diagnostics; running actions remain reachable for Stop. This retains
compatibility baselines required by ADR 0041 rather than deleting them.

Core and Extras are selection profiles, not proof of license independence. The
existing integrated package still includes experimental components. A public Core
binary needs dependency and APK-native-library auditing before it can be labeled
free of optional integrations. No public tree, history rewrite or publication is
performed by this slice.

## Local package profiles

The next slice adds a schema-v3 Core file profile and a separate, non-installable
Extras archive. Old schema-v3 manifests without a profile retain integrated
validation. Core requires native audio/camera helpers but rejects optional runtime
files; its APK must contain only the first-party JNI library. Extras has its own
inventory, source hashes and experimental APK, not an automatic overlay. Existing
app installation tools can install Core files in a new directory. Runtime-only
upgrade across profiles is rejected because it would leave optional files behind.
This is a local-debug delivery boundary; shared Rust wrappers remain compiled,
and neither profile is approved for public distribution or legal clearance.

## Source review export

Export the present working tree through an explicit path policy, including new
source files, without Git history, local build outputs or credential files.
Preserve source and license bytes; do not mass-rewrite IPs/SIDs or historical lab
records to make a scan green. Record per-file hashes and findings by category,
filename and line, never matched secret values. A review candidate is not a public
release. Private-environment references and binary assets require review even when
repository structure and Cargo workspace metadata resolve without Git history.

## Explicit lab deployment identity

The service updater and its four checked-in callers now require explicit IPv4
endpoints. Staging/repair callers require the intended user SID; repair additionally
requires the expected old SID. No current elevated identity is substituted for the
requesting user. Display peer preparation persists these targets and deployment
rejects changed targets or older manifests without target binding before firewall
changes. Existing exact service-baseline checks, artifact hashes, elevation and
rollback recording remain. Historical stage paths stay local-lab prerequisites;
this does not turn these scripts into a public system installer.


## Source build evidence and packaging

Core application binaries can be built from an immutable, hash-inventoried source
candidate in a disposable copy and fresh first-party output directories. Dependency
caches and existing SDK/compiler tools are explicit prerequisites, so this is not
hermetic or bit-reproducible build certification. Packaging checks all eight Windows
binaries and the APK against the build report and records its hash; Extras retain
separate reused-artifact provenance. Current packaging documents/scripts are
independently inventoried inputs rather than implicitly covered by the source hash.

The build entry performs APK signature verification and records the public signer
fingerprint without storing the keystore or passwords in the report. Packaging
rejects a supplied signer that contradicts this evidence. Earlier reports retain
the separate apksigner-check workflow; no historical evidence is upgraded in place.
These checks never authorize device deployment, driver installation or publication.
