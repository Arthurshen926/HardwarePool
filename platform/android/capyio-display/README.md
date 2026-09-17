# CapyIO Android Display Sink seam

This crate models the bounded lifecycle that a future Kotlin/JNI adapter will
apply to an Android `MediaCodec` decoder configured with an application-owned
output `Surface`.

It deliberately contains no Android SDK call, JNI layout, Gradle project,
manifest, permission, foreground service, socket, codec implementation, APK or
device deployment. Its decoder trait consumes borrowed complete access units
synchronously, so the Rust sink cannot retain encoded frame payloads or build
an unbounded queue.

The first contract is AVC Annex-B. Configuration carries bounded SPS/PPS,
startup and post-flush input require a key frame, discontinuities force a flush
plus codec-specific-data resubmission, format changes require stop/configure
with a newer epoch, and stop clears the output surface through the platform
decoder boundary.
