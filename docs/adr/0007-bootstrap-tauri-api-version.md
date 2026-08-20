# ADR 0007: Use the published Tauri JavaScript API baseline

Status: accepted for bootstrap repair

## Context

The offline bootstrap declared `@tauri-apps/api` version 2.11.2. During
HP-BOOT-001, npm dependency resolution failed because that version was not
published. The npm registry reported 2.11.1 as the latest available release in
the 2.11 line. The separately versioned `@tauri-apps/cli` 2.11.2 declaration is
available and does not need to change.

The JavaScript API package supplies the typed WebView-to-Tauri invocation API.
It does not enter the Core, real-time audio, network, Broker or driver paths.

## Decision

Pin `@tauri-apps/api` to 2.11.1 and retain `@tauri-apps/cli` at 2.11.2. Generate
and retain `pnpm-lock.yaml` from this published baseline.

Registry metadata for `@tauri-apps/api` 2.11.1 identifies the upstream as the
Tauri repository and the license as `Apache-2.0 OR MIT`, which is compatible
with this repository's Apache-2.0 license.

## Alternatives considered

- Keep 2.11.2: rejected because no matching npm package exists.
- Use an open version range: rejected because the bootstrap requires a
  reproducible first lockfile and should not silently consume a future API.
- Move all Tauri packages to another release line: deferred because no current
  compatibility failure requires the broader upgrade.
- Remove the JavaScript API dependency: rejected because the shared UI invokes
  the narrow Tauri command boundary through this package.

## Consequences

- The frontend dependency graph can be resolved reproducibly.
- CLI and JavaScript API patch versions differ by one; this is explicit and
  must be covered by the UI typecheck, web build and Tauri desktop build.
- A later version alignment is a separate dependency update with its own build
  evidence.
