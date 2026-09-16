# CapyIO Android JNI boundary

This crate exposes primitive-only JNI entrypoints for the private touchpad
bridge and the local Android catalog Runtime. Android framework objects and
Core memory layouts never cross the boundary.

ADR 1406 adds `AndroidCatalogRuntimeV1`. It decodes the schema-v1 catalog from
the unified Java host, constructs a real `NodeRuntime`, and permits
transactional same-Node replacement only while disconnected. The Android JNI
registry owns each instance behind a positive handle until explicit close.

The matching `AndroidNativeCatalogRuntime` Java class is compile-checked, and
the ARM64 `.so` can be built with:

```text
cargo ndk -t arm64-v8a -o target/android-jni build \
  -p capyio-android-jni --release --offline
```

The unified APK does not yet package or instantiate this library. This crate
does not open a socket, authenticate a peer or authorize a Route.
