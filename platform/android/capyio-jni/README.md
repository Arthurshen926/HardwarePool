# CapyIO Android JNI boundary

This crate exposes primitive-only JNI entrypoints for the private touchpad
bridge, the local Android catalog Runtime and paired-directory validation.
Android framework objects and Core memory layouts never cross the boundary.

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

ADR 1407 packages and instantiates this library from the unified Service. This
crate still does not open a socket, authenticate a peer or authorize a Route.

ADR 1451 reuses the shared `capyio-pairing` schema through a bounded byte-array
JNI call. Java owns only no-backup `AtomicFile` placement; Rust validates the
local Node relationship, schema, peer bounds and duplicate identities and
returns canonical non-secret JSON. This does not enroll or connect a peer.

## Explicit finite secure microphone profiles

ADR 1564 binds Android live secure-mono eligibility to the exact selected
`pcm-s16le-48000-mono-tls13-v1` (60 seconds) or
`pcm-s16le-48000-mono-tls13-300s-v1` (300 seconds) descriptor. The closed schema-v2
Java DTO keeps `pinned-tls13-mono-v1` transport and carries that exact format;
remaining grant time never selects a profile. Legacy catalog defaults and JNI
method signatures are unchanged.

The native authenticated preparation, durable one-shot Route use and AudioJob
pin the profile. Capture admission intersects the original monotonic preparation
end with the exact profile cap and original Source lease. Reaping, delayed Start,
settings changes or a new offer cannot renew that authority. Two fixed PCM slots,
960-byte records and the existing rate/burst/250 ms transport bounds are unchanged.

Socket-free policy tests cover exact selection, DTOs, deadlines and admission.
The 19 mandatory `audio_job::tests::hosted` cases additionally require actual
synthetic pinned TLS execution on the disposable hosted runner, including both
profile caps, preparation/profile mismatch, lease/grant/catalog mutation,
pending/post-TLS cancellation and finite expiry. Compilation is not TLS evidence;
none of these cases proves physical Android AudioRecord compatibility.
