# ADR 0001: Rust for the shared Core and Runtime

Status: accepted for bootstrap

## Context

The project needs memory-safe cross-platform domain logic, networking support, FFI options, deterministic tests, and integration with Tauri. Platform drivers remain separate.

## Decision

Use stable Rust for the OS-independent Core, Runtime, Protocol conversion, CLI, Tauri backend and future user-mode Broker where practical.

## Consequences

- Core can compile across desktop and mobile targets.
- Windows kernel driver still uses the supported WDK language/toolchain and is not required to share Rust code.
- Android permission/service glue remains Kotlin.
- FFI boundaries require explicit C-compatible contracts when needed.
