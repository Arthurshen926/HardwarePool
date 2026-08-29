# CAPY-MIC-001G — Trusted MicYou host configuration

Status: local functional acceptance complete; merge pending; release qualification moved to plan 0021

Owner: Codex and project owner

Created: 2026-08-29

Depends on: `CAPY-MIC-001E`, `CAPY-MIC-001F`

## Objective

Make the microphone Quick Action usable after one explicit host-side
provisioning step, without requiring environment variables on every desktop
launch and without giving the WebView executable-path or raw endpoint-ID
authority.

## In scope

1. Add a versioned, deny-unknown-fields trusted host configuration stored at a
   fixed user-local CapyIO path.
2. Add a host CLI that probes a separately supplied pinned MicYou executable,
   resolves an explicitly selected stable endpoint ID and derives its current
   expected name before writing configuration.
3. Let the Tauri host load complete environment overrides for development or
   otherwise load the fixed configuration file automatically.
4. Reuse the existing MicYou Adapter validation on every process start; a
   persisted enumeration index is forbidden.
5. Keep configuration source, executable path and endpoint identity outside
   the WebView DTO and ordinary diagnostics.

## Out of scope

- downloading, building, copying or distributing the GPL MicYou executable;
- choosing an endpoint from an ambiguous display name;
- adding a WebView filesystem picker or configuration-write command;
- installing/updating the driver or APK, changing permissions or running a
  physical phone test;
- production machine-wide provisioning, installer or service ownership.

## Acceptance criteria

1. Unknown schema versions/fields, incomplete overrides, unspecified bind
   addresses, invalid ports and malformed endpoint identities fail closed.
2. Provisioning selects by exact stable ID and derives the name from a bounded,
   already validated MicYou inventory.
3. The default file path is derived only from the trusted host environment and
   is never supplied by the WebView.
4. A loaded file creates the same Runtime-owned microphone Quick Action as a
   complete development override.
5. Debug output and Quick Action DTOs do not disclose the executable path or
   raw endpoint ID.
6. Unit tests, desktop tests, repository validation and full CI pass.

## Safety

This slice edits source and runs build/tests only. It does not write the actual
user configuration, run a third-party executable, install a driver/APK, change
permissions/services, reboot, sign, commit, push or create a pull request.

## Completion evidence

- `capyio-micyou-host-config` implements the fixed schema-v1 file, complete
  development override, bounded parser, redacted debug output and create-new
  persistence.
- `capyio-micyou-config provision` probes the separately supplied executable,
  selects an exact stable endpoint ID and derives the name; `validate` repeats
  the pinned version/capability/current-endpoint checks.
- Tauri automatically loads the complete override or fixed file and constructs
  the same Runtime-owned microphone Route. Its serialized Quick Action exposes
  only the validated bind IP/port guidance.
- Five configuration tests and 31 desktop library tests passed; four unrelated
  physical tests remained explicitly ignored.
- Targeted all-target check and Clippy with warnings denied passed.
- `cargo xtask ci` passed on 2026-08-29, including workspace checks/tests,
  documentation/manifests, Adapter smoke, repository validation and frontend
  typecheck/build.
- A separately authorized controlled-lab step provisioned and validated the
  fixed user-local file, then CAPY-MIC-001H proved that normal Quick Action
  construction uses it for a complete physical lifecycle without exposing the
  executable or endpoint identity to the WebView.
- That acceptance did not install a driver/APK, change Android permissions,
  restart Windows audio services or reboot the host.
