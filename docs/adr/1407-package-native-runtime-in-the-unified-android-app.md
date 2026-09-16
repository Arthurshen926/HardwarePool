# ADR 1407: Package the native Runtime in the unified Android app

Status: accepted

Date: 2026-09-16

## Context

ADR 1406 provides a buildable JNI catalog Runtime, but leaving its library and
Java wrapper outside the unified APK would preserve two disconnected products.
The Service must own the same native lifecycle as the hardware catalog before
physical integrated testing is meaningful.

## Decision

The Android App build runs the pinned Cargo NDK toolchain for `arm64-v8a`,
packages `libcapyio_android_jni.so`, and limits this development APK to that
ABI. `AudioNodeService` creates one `AndroidNativeCatalogRuntime` after hardware
inventory and Adapter construction, refreshes it after catalog lifecycle
changes, and closes it during Service destruction.

Sharing selection uses a pre-commit callback: the projected catalog must be
accepted by native Runtime before Java publication intent commits. A rejected
native replacement therefore cannot leave the UI selection ahead of Core.
High-rate gamepad/touchpad control updates notify presentation only; they do not
serialize or replace the catalog. Catalog refreshes are content-deduplicated.

The build has an explicit APK composition check for the manifest, DEX, shared
Vue entrypoint and ARM64 JNI library. It does not install or start the APK.

## Consequences

- One APK now contains the shared Android UI, unified hardware Service, Camera2
  service and Rust catalog Runtime.
- Catalog/native ownership follows Service lifetime rather than Activity
  lifetime.
- The current native owner remains disconnected; catalog transport and the
  missing unified IMU/gamepad/touchpad remote Routes remain later slices.
- x86/x86_64 emulators are not supported by this development APK.

## Build dependency note

The build uses the already provisioned `cargo-ndk` 4.1.2 command and Android NDK
29.0.14206865 to compile the existing Rust crate. They are build tools, not APK
runtime dependencies. A checked-in prebuilt `.so` was rejected because it
would hide source/toolchain drift; Java-only reimplementation was rejected
because it would create a second Runtime.
