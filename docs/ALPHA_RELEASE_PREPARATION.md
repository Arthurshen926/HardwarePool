# Alpha release preparation

Version: 0.5.24-alpha.1. Feature frozen; publicationApproved remains false.

This slice establishes the component model, compatibility presentation and current
project documentation. It does not claim a finished unified system installer.

| Work | Status |
| --- | --- |
| Default native paths | Audio Share/MicYou idle actions moved to compatibility diagnostics; SensorServer remains explicit compatibility access; sources preserved |
| Component model | Core/Extras selection, dependencies, file hashes, operation plans and explicit unknown system versions implemented |
| Documentation | README, CONTRIBUTING and SECURITY describe current Alpha instead of bootstrap-only limitations |
| Separate binary packages | Core binaries/APK rebuilt from one source candidate; local Core/Extras archives and unsigned installer generated; dependency/license clearance remains pending |
| Public source export | Allowlisted, history-free local review candidate generated; inventory and structural checks available; privacy/binary review remains open |
| Unified system deployment | App file lifecycle exists; service/driver/camera operations remain individually reviewed prerequisites |
| Publication | No release, tag, commit, push or production signing performed |

Next delivery gates are public-source privacy review, third-party/source completeness,
and driver/optional-engine source builds. The independent Core application build
and separately inventoried Extras are complete for local debug use. Excluding GPL DLLs alone does not prove the remaining APK/EXE is independent.
No public package may require weakening Windows security policy. Existing local
lab evidence does not establish new-machine deployment or public driver signing.

Validation results for this slice are recorded under
`test-results/release-preparation-*`. Physical media tests are not repeated for
this metadata/presentation change; the prior evidence remains in
[the follow-up report](ALPHA_FOLLOWUP_20261003.md).

## Validation on 2026-10-03

`cargo xtask alpha-check` passed: Rust format/check/strict Clippy/tests, Adapter
smoke, docs/manifests/repository validation, frontend build and 89 frontend tests,
and Android capyioCheck (92 tasks). Two component-model tests passed; the current
local application bundle passed read-only hash verification. Final structure and
`git diff --check` passed. The first attempt stopped on CRLF in newly generated
metadata/documents; files were normalized to LF and the complete Gate rerun passed.

No device installation, driver/service mutation, APK deployment, publishing or
Git history change occurred. The checked local application bundle is the previous
hardware-tested build; this turn's UI changes have build/test evidence, not an
updated installed executable. This earlier check preceded the local profile packages recorded below.

## Earlier mixed-input Core / Extras delivery (2026-10-03)

Generated under `target/alpha-profiles/20261003-065851-363514/`:

- `Core.zip`: 16 inventoried files, rebuilt embedded Desktop, Broker/native helpers,
  default Android APK with only first-party JNI; no display engine or ViGEm sidecar.
  SHA-256 `06aed303b9ac28f5f606e0ede8d84caed97754c0c0868d5062dcba467deaf50d`.
- `Experimental-Extras.zip`: 60 inventoried optional files, including a complete
  older experimental APK and its GameStream native receiver. No automatic overlay.
  SHA-256 `d2456f61dac03f76315c1e03c03bbbf2d157e1accc9aae4997386fb4e2ee1166`.
- `CapyIO-local-20261003-065946-1647-setup.exe`: Core app-file installer, built but
  not executed. SHA-256 `ABD04C2E5D7E0807279D403024F8604005FCE8FE2181469AFADD0A1F1115C443`.

The default software Gate APK initially had a different sandbox debug signer. Core
was rebuilt with the existing user debug key and verified against the experimental
APK signer `29307555e525e46a29150bb85d0b73e3fde7270865b215d07b5a95805cc41007`.
Core APK SHA-256 is `B1BA60F11150D0A3C29A5BF01763C1FD562F96B182A4FBE1B29B5D533BCC0F59`.
No APK was installed. The existing device application/pairing remains unchanged.

Inputs are mixed: Desktop and Core APK rebuilt; other helpers and Extras reuse
hash-verified prior artifacts. Manifests say `mixed-inputs-explicit-hashes-not-clean-source-proof`
and sourceRevision is null. Shared Rust compatibility/experimental wrappers remain
compiled. These packages must not be described as a clean public release or as
having automatically resolved GPL/source-correspondence obligations.

Validation: Core/Extras hash and APK native-library checks, Core component check,
two profile boundary tests, existing full integrated bundle regression, and real
Core file install/upgrade/exact-manifest rollback/recoverable removal/restore passed.
Cross-profile runtime upgrade was rejected before mutation. The first sandbox
bundle test could not query Windows services; rerunning in the ordinary user
context passed. No service/driver or boot policy was changed. Repository structure
and diff checks passed. Logs: `test-results/release-profiles-*`.

Packaging entry: `scripts/build_alpha_profiles.py`; it requires an existing verified
integrated bundle, separately signer-verified Core APK with expected SHA-256, and
an explicit Desktop executable. It creates new directories only. Extras verification
is `python scripts/verify_alpha_extras.py <Extras> --core <Core>`.
Core uses the existing file installer/validator. See
[Core instructions](CORE_ALPHA_PACKAGE.md) and [Extras instructions](EXPERIMENTAL_EXTRAS_PACKAGE.md).
Next: audited source export and reproducible per-profile builds, then target-specific
system-component deployment; public publishing still requires separate authorization.

## Source export review

The next slice implements [source candidate export](SOURCE_EXPORT.md), explicit
path policy, per-file hashes and a non-disclosing review report. The candidate
retains source/license bytes; environment references are flagged for review rather
than silently rewritten. No public archive or new Git history is created.

Deployment-target cleanup now removes actual IP/SID defaults from the service
updater and all four checked-in callers. Prepared display stages bind the selected
target identity before deployment. Parameter-only tests passed without executing
system actions; see [source export notes](SOURCE_EXPORT.md).

The source candidate now passes an independent offline Core/Runtime/Protocol
compile and 57 tests in a fresh output directory. Review items are grouped by
purpose; two user-specific documentation paths are removed, and Android projects
share the pinned Gradle distribution checksum. Full Core application builds now
also pass; driver/optional native-engine builds and public-document/license review
remain open; see [evidence](SOURCE_EXPORT.md).

The current Core package uses all eight Windows executables and the APK from one
hash-bound source-candidate build, replacing the earlier mixed Core binary inputs.
The packaging command accepts `--first-party-build <result.json>`; stale or
incomplete evidence is rejected. Extras remain explicitly reused artifacts.
The unsigned local installer has been compiled without execution. Build report,
package hashes and remaining release gates: [source build evidence](SOURCE_EXPORT.md).


## Remaining acceptance boundaries

| Gate | Current boundary | Completion evidence required |
| --- | --- | --- |
| Source privacy | Latest candidate: 356 scan entries; 10 explanations still match file hashes, 346 pending | Reviewed dispositions and a new hash-bound export |
| Third-party delivery | Core excludes optional runtime payloads; shared wrappers remain compiled | Dependency/source/license inventory and required notices for actual artifacts |
| Drivers / Extras | Installed-lab prerequisites and reused optional binaries | Separate source-build, package/signing and target deployment evidence |
| New Core device acceptance | Source build and package integrity passed | Device regression against the exact new artifact hashes |
| Ordinary-user camera | Elevated Host remains the approved Alpha path | Separate ordinary-privilege implementation and acceptance |
| Lock / sleep | Deferred by user | Authorized physical recovery checks |
| Public release | Local unsigned/debug artifacts only | Production signing, cleared delivery gates and explicit publication approval |

Historical package hashes above are retained as evidence, not a recommendation to
replace the current package. The current package path and hash are maintained in
[LOCAL_ALPHA_CANDIDATE_20261003.md](LOCAL_ALPHA_CURRENT.md); avoid treating build success as deployment.


## Routine validation integration

The three existing Python test modules now run through both local CI/Alpha commands
and the hosted static workflow, using the same unittest discovery command. Ten
fixture-only tests passed locally, along with xtask compilation/format checks and
repository/diff validation. This change adds no new test suite, package build,
deployment step or dependency. Hosted execution itself has not been run locally.
