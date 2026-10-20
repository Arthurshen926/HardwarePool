# ADR 1525: Android-target JNI and Java contract compile gate

Status: implemented; independent review and exact-head hosted validation pending.
Date: 2026-10-20

## Decision

Extend the existing explicitly scoped Linux validation workflow with a separate
20-minute Ubuntu 24.04 job. Compile the actual `aarch64-linux-android` JNI shared
library, not merely the host-testable coordinator. Reuse Rust 1.97.1 through the
existing toolchain action and require the runner's already-installed NDK
29.0.14206865, matching ADR 1407. Select its API-29 ARM64 Clang/archiver directly,
matching the Android app's minimum SDK. No cargo-ndk installation is needed.

Require preinstalled JDK 17 and compile the existing dependency-free Java
node/native-LAN contract sources with `--release 17`, plus the real
AndroidNativeCatalogRuntime wrapper with generated JNI headers. Check generated
native names against the actual Android shared library exports. This catches
missing/renamed exported entry points; it is not proof of JNI runtime calling
convention/argument behavior, Java class loading on Android or hardware behavior.
The wrapper's static native library load is never executed on the Linux JVM.

Run the same four contract main classes named in node-contract/build.gradle:
audio lifecycle, capability catalog, capability lifecycle and native LAN. Enable
assertions and retain the existing Rust golden-wire fixture property. Native LAN
includes real synthetic loopback UDP and bounded workers, so it runs only on the
hosted runner, not in the socket-restricted development workspace. No physical
audio, Android framework services or user data is used.

## Prerequisites and resources

The official [Ubuntu 24.04 runner inventory](https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md)
listed JDK 17 and NDK 29.0.14206865 when inspected on 2026-10-20. Runner images
can change; missing pinned tools are a hard prerequisite failure, not an excuse
to auto-install or silently choose another NDK. No SDK manager, license acceptance,
Gradle/AGP resolution, APK signing, credential lookup, emulator or device tool is
invoked. Rust toolchain/Android standard library and locked crates may download
through the existing official Rust tooling and configured Cargo registries. A
cold job can require hundreds of MB and several minutes; that is an estimate,
not a measured cost. The build is capped at 15 minutes within the 20-minute job.

The development workspace currently has a JRE but no javac, Android Rust std or
SDK/NDK. Local checks can validate the workflow/export checker and ordinary host
code only. Android compilation and Java execution must be confirmed by the
exact published commit's hosted job, with failures fixed rather than suppressed.

## Scope and remaining gates

No permission/foreground-service manifest, credential, wire protocol, signing,
installation or deployment behavior changes. Scoped compile corrections may be
required where Android-only code was previously uncompiled; record those changes
and preserve runtime policy. This gate builds ARM64 JNI but does not assemble an
APK or compile all Android-framework Java app classes. Real Android/PipeWire
capture/render, permissions, device lifecycle and performance remain separate
acceptance gates. Passing Java synthetic UDP contracts is not physical audio
acceptance or evidence that the lab UDP transport is authenticated/encrypted.
