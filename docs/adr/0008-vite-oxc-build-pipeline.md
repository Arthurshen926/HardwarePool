# ADR 0008: Use the Vite 8 Oxc build pipeline

Status: accepted for bootstrap repair

## Context

HP-BOOT-003 type checking passed, but the first Vite 8.0.16 production build
failed because the offline bootstrap explicitly selected esbuild minification.
Vite 8 no longer installs esbuild by default, deprecates the esbuild minifier
option, and uses Oxc for JavaScript transformation and client minification.

A temporary test with the current esbuild peer dependency confirmed a second
incompatibility: esbuild 0.28 no longer performs the required destructuring
transforms for the configured `safari13` target. Keeping the deprecated path
would therefore require pinning an older transformer or changing the declared
browser targets.

This decision affects only the GUI build pipeline. It does not enter Core,
audio callbacks, network, Broker or driver code.

## Decision

Use Vite 8's Oxc minifier explicitly and retain the existing browser/WebView
targets. Do not add esbuild as a direct dependency.

## Alternatives considered

- Add esbuild 0.28.2: rejected after its target transform failed during the
  production build.
- Pin an older esbuild: rejected because it preserves a deprecated Vite path and
  creates an unnecessary direct build dependency.
- Raise the browser targets: rejected because build-tool repair should not
  silently reduce the intended WebView compatibility range.
- Disable minification: viable for diagnostics, but it would not exercise the
  intended production pipeline.

## Consequences

- The frontend follows the supported Vite 8 transformation/minification path.
- No dependency lifecycle-script approval is needed for esbuild.
- The configured targets must be covered by production-build evidence and can
  be reconsidered when actual Tauri desktop and Android WebView baselines are
  recorded.
