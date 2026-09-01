# ADR 0046: Use jni-rs for the Android touchpad boundary

Status: accepted

## Context

CAPY-PTP-002N and 002O keep Android pointer lifecycle mapping and private
touchpad packet creation in deterministic Rust. CAPY-PTP-002U needs a real,
buildable Android boundary that can project framework `MotionEvent` values into
those existing DTOs without copying Rust layouts into Kotlin or moving Android
framework types into Core.

Raw JNI is an unsafe ABI with local-reference, exception, thread-attachment and
panic-unwind obligations. Hand-writing that surface directly against `jni.h`
would duplicate those mechanics and increase the amount of locally maintained
unsafe code.

## Decision

Use `jni` 0.22.4 (`jni-rs`) only in the Android-target portion of the new
`capyio-android-jni` composition crate. The exact version is locked in
`Cargo.lock`.

- Purpose: FFI-safe `EnvUnowned`, lifetime-bound Java references, primitive
  array copies, Java exception construction and byte-array returns.
- Upstream: <https://github.com/jni-rs/jni-rs>
- License: MIT OR Apache-2.0, compatible with this Apache-2.0 repository.
- Maintenance: 0.22.4 is the selected current release and supports the
  FFI-safe `EnvUnowned`/`Env` split used here.
- Imported code: none; Cargo resolves the upstream crate and its locked
  transitive dependencies.

The JNI contract is version 1 and contains only strings, integers, booleans,
primitive arrays and private packet byte arrays. `MotionEvent` itself remains in
Kotlin. Each session is creating-thread-owned, JNI panics are contained, and the
bridge emits no log, socket or file I/O on the event path.

The build-only Android library uses Android Gradle Plugin 9.3.1 with Gradle
9.5.0 and JDK 17. These tools are not linked into or shipped as runtime product
dependencies. The module declares no permission, Activity, service or receiver.

## Consequences

- Host tests can exercise the complete DTO-to-private-packet composition without
  a JVM or device.
- `cargo-ndk` produces a real `arm64-v8a` shared library and Gradle verifies the
  Kotlin/JNI packaging boundary in an AAR.
- Android network ownership, authenticated transport, foreground lifecycle and
  an application UI remain separate slices.
- The crate contains an explicit FFI export boundary; Rust Core and the existing
  Android mapping crate remain free of JNI and unsafe code.

## Alternatives considered

- Raw `jni.h` bindings: rejected because they add avoidable unsafe reference,
  exception and attachment maintenance.
- Serialize `MotionEvent` through JSON or Protobuf: rejected because continuous
  pointer events do not belong on the control path and would allocate a larger,
  less precise contract.
- Mirror Rust structs in Kotlin/native memory: rejected because representation
  coupling would make both sides evolve unsafely.
- Implement gesture recognition on Android: rejected because Windows Precision
  Touchpad semantics require forwarding contacts, not pre-recognized gestures.
