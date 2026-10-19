# System Components

The manifest [system-components.json](../release/system-components.json) defines
release-level IDs, required/optional selection, dependencies, file evidence,
operation policy and restart requirements. ADR [1510](adr/1510-system-components.md)
keeps this separate from runtime Adapter manifests and kernel driver boundaries.

```text
python scripts/system_components.py check
python scripts/system_components.py verify --bundle target/alpha-final-app-20261003
python scripts/system_components.py upgrade --profile core --bundle target/alpha-final-app-20261003
python scripts/system_components.py remove --profile all
```

All commands are read-only. Install/upgrade/rollback/remove emit plans; only app
file handlers are currently implemented by the referenced existing PowerShell
scripts. No command in this tool starts a service, registers COM, deploys a driver,
changes security settings or deletes user data. Removal plans reverse dependency
order. Extras dependencies reference Core components; Extras is not standalone.

`releaseVersion` is the intended release, not an installed driver version.
`installedVersion=null` and `systemState=not-probed` are intentional: the tool
cannot infer system installation from a package directory. `verify` hashes the
bundle's declared files and requires selected component files; it fails on missing
or changed files. Use the existing `verify_local_debug_bundle.ps1` for full bundle
shape validation, and `check_local_prerequisites.ps1` for read-only Windows presence
inventory. Neither is a functional hardware test or publisher-signature check.

The app/Host share one executable; do not install or remove it twice. Audio is one
package with speaker and microphone. Touchpad remains VHF/KMDF. Camera is a user-mode
Media Foundation/COM projection. Display and gamepad remain optional external
backends. Driver/COM/service operations require exact versioned artifacts, target
inventory and recorded rollback before adding executable handlers.

Local Core and Extras file packages now have separate inventories. Core uses the
schema-v3 `profile=core` validator; omitted profiles retain integrated validation.
Extras uses its own non-installable manifest and verifies a matching Core manifest
hash. Runtime-only upgrades across profiles are rejected; use a new directory.
See [delivery evidence](ALPHA_RELEASE_PREPARATION.md).
