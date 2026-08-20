# ADR 0002: Tauri 2 and Vue 3 for the shared UI

Status: accepted for bootstrap

## Context

A shared desktop/mobile UI is desirable, but platform permissions and long-running services remain native concerns.

## Decision

Use Vue 3 + TypeScript as the shared presentation layer and Tauri 2 as the native host. Define a small TypeScript API with Tauri and browser-mock implementations.

## Consequences

- Most views are shared across Windows, Linux, macOS and Android.
- Native Kotlin/Swift/Windows code is still required for platform operations.
- GUI closure does not define Runtime/session lifecycle.
- The WebView cannot receive broad shell/filesystem privileges by default.
