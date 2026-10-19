# Source review candidate

This is a local pre-publication review workflow, not a release or approval gate.
It does not initialize a repository, rewrite history, commit, push or publish.

```text
python scripts/export_source_candidate.py
python scripts/export_source_candidate.py --verify target/source-export/<run>
python target/source-export/<run>/candidate/scripts/validate_repository.py
cargo metadata --manifest-path target/source-export/<run>/candidate/Cargo.toml --no-deps --format-version 1 --offline
```

[The export policy](../release/source-export-policy.json) enumerates source roots
and required root/configuration files. Git supplies tracked plus non-ignored new
file names; present working-tree bytes are copied. Tracked deletions are not
restored from history. Build outputs, personal configuration, known credential
file types and agent-session directories are excluded. AGENTS.md instructions
remain because repository validation requires them; the separate agent master
prompt/session records are excluded. Licenses and third-party records are retained.

Outputs live below target/source-export: `candidate/`, `manifest.json` and
`review.json`. The inventory records SHA-256 for every copied file and binds the
policy hash. The verifier rejects modified/unlisted files and Git history.
The review report records category, filename and line only, never matched values.
It scans private-key/AWS-key patterns, personal paths, machine names, user SIDs and
private/CGNAT network addresses. Binary resources are explicitly queued for review.

The initial review found environment references in both historical documentation
and live lab scripts. Many private-network addresses are synthetic test fixtures;
icons and the Gradle wrapper are examples of legitimate binary inputs. Classify
these by purpose before removing or replacing them. A blanket textual rewrite
could change accepted endpoint fixtures, exact-target lab safety checks or license
records. No such rewrite is performed by the exporter.

Repository structure, manifest integrity and Cargo metadata checks establish
source-tree shape, not a clean compile, optional native-engine source closure,
license clearance or privacy certification. No finding is automatically waived.
`publicationApproved` stays false even if the heuristic scan finds no secrets.
The original local development tree and unpushed history are excluded from the
user-approved public source snapshot. See [the 2026-10-19 handoff](CLOUD_SYNC_HANDOFF_20261019.md).
That source synchronization does not approve a package or formal release.

## Next review work

1. Parameterize active scripts that embed lab identity, with existing target and
   rollback checks preserved; distinguish synthetic test addresses from real ones.
2. Prepare public documentation separate from historical lab evidence, maintaining
   valid references and requirement traceability.
3. Review binary source assets and third-party/license completeness, then build
   the candidate without relying on the original workspace's generated files.
4. Re-export and re-scan; publication remains a separately authorized action.

Evidence for the current local run is under test-results/source-export-*.
Focused tests cover exclusions, non-disclosing findings and tamper/unlisted-file
rejection. Neither the export nor these tests access physical devices.

## Deployment-target cleanup (2026-10-03)

The integrated service updater and four staging/repair callers no longer default
to a real lab IP or user SID. Pass `-LocalIpv4`, `-AndroidIpv4` and, for wrappers,
`-ControlUserSid` explicitly. Account repair also requires
`-ExpectedPreviousUserSid`. Query the intended ordinary user's SID separately;
do not substitute the elevated account. IPv6/malformed addresses are rejected.

`stage_display_peer_lab.ps1` saves the selected endpoints/SID in prepared-stage.json.
Deployment requires those same values, exact prior service ImagePath and unchanged
artifact hashes before firewall/service actions. Old manifests without target
binding require a new reviewed preparation; they are not silently adopted.
Historical stage directories and engine configuration are still required inputs.
These commands remain controlled-lab tools, not generic public installation.

`scripts/test_lab_target_parameters.ps1` checks the real parsed parameter blocks
with valid and invalid synthetic inputs; deployment bodies are never executed.
All five parameter contracts and repository/diff validation passed. No service,
firewall, driver or device was modified. Historical evidence and synthetic fixture
addresses remain subject to separate public-document review.

## Classification and independent foundation build (2026-10-03)

Review entries now carry a purpose group without waiving the underlying finding:
documentation, explicit test-fixture paths, implementation, binary resources or
credential patterns. Inline Rust tests still appear under implementation-review;
the grouping is triage, not evidence that those entries contain live configuration.
After removing personal user paths from FIRST_RUN_WINDOWS and TOOLCHAIN examples,
the snapshot contains 356 entries: 127 documentation, 155 implementation, 22
explicit test fixtures and 52 binary resources. Documentation examples now resolve
the current user's tool paths using environment variables.

The binary inventory is 48 PNGs, one ICO, one ICNS and two Gradle wrapper JARs.
No PNG text/EXIF chunks were found. The JARs are byte-identical (SHA-256
`497c8c2a7e5031f6aa847f88104aa80a93532ec32ee17bdb8d1d2f67a194a9c7`),
contain GradleWrapperMain and only Gradle/META-INF entries. This establishes format
and identity between the copies, not independent upstream authenticity or license
clearance. The touchpad bridge now pins the same Gradle 9.5.0 distribution checksum
as the main Android project; no toolchain version was changed or downloaded.
Evidence: `test-results/source-assets-review.json`.

Candidate `target/source-export/20261003-083137-949357/candidate` compiled and
passed 57 tests (0 failures) for capyio-core, capyio-runtime and capyio-protocol,
using a fresh target/independent-source-build directory and offline dependency
cache. Protocol generation used the candidate's own .proto files. No first-party
artifacts from the development target directory were reused. Candidate integrity
and structural validation passed after the build. Evidence:
`test-results/source-independent-rust.log` and `source-export-integrity.log`.

This foundation result is supplemented by the full Core build below. Driver and
optional native-engine builds from the candidate remain separate gates. The offline Cargo dependency cache is still an environmental
prerequisite; this is not a hermetic or bit-reproducible release build.

## Independent Core application build (2026-10-03)

Candidate `target/source-export/20261003-083340-910854` (1720 files), manifest
SHA-256 `3e92840b3141d6201bd3fbd3d5967649866ae61213d3295ecf46ceae568cda8d`,
was copied to `target/source-build/20261003-083635-8400/src`. Frozen offline pnpm
installation, frontend, Windows Desktop, seven Core helpers, and Android ARM64 JNI
plus Core debug APK all built successfully with fresh first-party outputs.
All exported source input hashes remained unchanged after building.
The helper build ran as a separate follow-up in the same fresh Cargo target;
`result.json` records its success and all nine artifact hashes. The reusable entry
`scripts/verify_source_build.ps1` now includes that step and input-integrity audit.

Build entry (existing SDK/JDK/toolchain caches and debug key are prerequisites):

```powershell
./scripts/verify_source_build.ps1 -CandidateDirectory <export-directory> -AndroidDebugKeystore <existing-debug-keystore>
```

Evidence: `target/source-build/20261003-083635-8400/result.json` and sibling step
logs. APK SHA-256 is
`dff788e12762906c1e5e3936da6946d56970db2d4af2e8fad68dfbfc167b9c2b`;
apksigner verified identity
`29307555e525e46a29150bb85d0b73e3fde7270865b215d07b5a95805cc41007`.
APK native inventory contains only the first-party ARM64 JNI library.
This is an offline-cache build, not a hermetic or bit-reproducible release.

`build_alpha_profiles.py --first-party-build <result.json>` requires successful
steps, unchanged source inputs, and matching hashes for all Core binaries/APK.
Current packaging scripts/documentation are separate working-tree inputs, copied
and hashed in the package inventory; the build source manifest covers the binaries.
Extras explicitly retain reused-integrated-artifact provenance.
New local package: `target/alpha-profiles/20261003-084521-313788`.
Core.zip SHA-256: `4ea50daa347f539df994bec4928d2385424092538d615113fae56fbc3b0e4a52`.
Unsigned installer: `CapyIO-local-20261003-084544-5876-setup.exe`, SHA-256
`d0040851c634ee4ab6c1432189ce123418f1804114f1d751c074e9960b562c64`.
Installer was compiled only; no device, service or driver deployment occurred.
The new binaries have build/package validation, not a new hardware acceptance run.

Public privacy review (356 snapshot entries), third-party provenance/license and
corresponding-source review, driver/optional engine source builds, production
signing and public release approval remain open. Earlier device acceptance does
not automatically certify this newly built package.


### Repeatable build entry consolidation

The build entry resolves caller-relative keystore, SDK and JDK paths before
changing directories. Windows Cargo steps use `--offline --locked`; the existing
frozen pnpm and offline Gradle settings remain. It now runs apksigner as an explicit
`android-signature` step and records the verified certificate SHA-256 in result.json.
`-AndroidBuildToolsVersion` defaults to the existing 36.0.0 SDK tools; missing tools
fail before copying/building. No automatic SDK download or signing-key creation occurs.
Packaging compares the signer with this field when available. Older reports still
require the separately supplied signer verification described above; no historical
report or package is rewritten to imply this newer check ran previously.


The consolidated entry was executed end-to-end using a caller-relative keystore
path against the same immutable candidate. Run
`target/source-build/20261003-090204-1865/result.json` records six successful steps,
nine artifact hashes, unchanged source inputs and the expected debug signer.
Android ran 40 tasks (all executed). Packaging's source-build input check accepted
this real report and APK native inventory; the three focused profile tests,
PowerShell syntax, repository structure and diff checks passed.
Log: `test-results/source-build-repeat.log`. This verifies the build entry, without
replacing or installing the already inventoried Core package above.


### Implementation versus fixture review

The scanner now recognizes JavaScript/TypeScript `*.test.*` files and nested
README/AGENTS documents. Findings remain visible; this only changes the work queue.
Credential patterns retain priority even in tests and documentation. Inline Rust
tests remain implementation-review until their context is reviewed explicitly.
Do not remove RFC1918/CGNAT policy constants merely to reduce the count.

The Desktop gamepad address placeholder no longer repeats the lab host address.
The Android touchpad README uses an explicitly selected `CAPYIO_ADB_SERIAL` instead
of an old wireless-debug endpoint. These changes do not alter connection defaults,
permissions or hardware lifecycle. Historical physical evidence is preserved.
Validation: four exporter tests, Desktop typecheck, repository structural validation
and diff check passed. Public-source and binary/license clearance remain open.
The updated scan has 355 entries: 132 documentation, 129 implementation, 42 explicit
test fixtures and 52 binary resources. The reduced implementation queue reflects
classification, not 26 resolved vulnerabilities. Prior build/package reports remain
bound to their earlier source manifests; this newer source snapshot is not covered
by those full-build claims.


### Hash-bound review decisions

`release/source-review-decisions.json` records 15 inspected findings as
`retain-explained`: six display-only IPv4 placeholders, seven CGNAT range
messages/comments, one explicit example configuration and one upstream patch
context comment. The record stores file/line/category, the full file SHA-256 and
reason; it does not reproduce matched personal values. This disposition explains
why the source is retained, not public-release or third-party license approval.

The exporter keeps every finding and adds `reviewStatus` plus the matching reason.
A decision applies only to identical file bytes, path, line and category. Changed
files automatically return to pending review. Review counts remain separate from
scan groups and publicationApproved remains false. Historical reports stay intact.
Five exporter tests pass, including changed-source and different-path invalidation;
repository structure and diff checks pass. No runtime code or device is changed.

The updated snapshot has 356 findings: 15 explained and 341 pending. One new
synthetic address in the decision-invalidation test accounts for the increase
from 355; findings are not suppressed to lower the total.


### Build-cache retention (2026-10-03)

Independent builds now require at least 10 GiB free on the repository drive before
creating an output directory. This is an entry threshold, not a guaranteed peak
space bound. Cargo incremental compilation is disabled for disposable builds.
After signature/input verification, eight Windows executables and the APK are
retained under `artifacts/windows` and `artifacts/android`, with hashes in the same
report (`artifactLayout=retained-v1`). Packaging supports both this layout and old
reports that point inside `cargo-target/debug`.

Successful builds remove only that run's validated `cargo-target` directory by
default; use `-KeepBuildCache` for debugging. Source copies, final artifacts and
logs remain. Failed runs retain their cache for diagnosis and need deliberate
cleanup. No historical run, source candidate, installed application, SDK or shared
dependency cache is automatically removed. This prevents repeated successful
candidate builds from accumulating full compiler caches.
