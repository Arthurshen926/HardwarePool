# ADR 1425: Rebuild the native display receiver for the unified debug APK

Status: accepted for explicitly selected local debug builds only

Date: 2026-09-17

Compose the existing fixed-source Moonlight Common C video-only receiver build
into the unified Android app. The new optional Gradle property
`capyioGameStreamStageRoot` names a stage below repository `target/`; it does
not accept an arbitrary binary. Default builds remain possible without the
optional native library. A selected stage is verified before packaging.

The imported scripts verify four fixed source archive SHA-256 values and the
unchanged reviewed patch hash `07badc78a5e6e0ec58627271b1fe4b55ef6dac7de1f41c39642e7a6e48191ff2`.
Builds use NDK 29.0.14206865, a recorded CMake compile database and explicit
video-only sources. Interrupted staging never recursively deletes existing
files; a new stage must be chosen. The original display worktree is unchanged.

The generated manifest records staged-source and CapyIO JNI source hashes,
compile database hashes and native artifact hashes. Gradle verifies these,
ARM64 ELF layout, and absence of the excluded audio/input source files from
the compile database. A final APK check compares the exact packaged library
hash and requires GPL/MIT notices plus the distribution-plan asset. Library
loading can now be reported even before a transport is configured; it remains
distinct from authentication or frame presentation.

This uses the dependencies and licenses already recorded in the Moonlight
distribution plan: GPL-3.0-only Common C, MIT ENet/nanors and the pinned
reference OpenSSL crypto archive. No new Android permissions, drivers or
upstream UI/audio/input subsystem are enabled. This private debug packaging
does not close public corresponding-source, complete notices or reproducible
OpenSSL source-build obligations. With this property set, Release prebuild
fails intentionally until the distribution gate is closed.

Alternatives were an opaque copied .so or rewriting the established receiver;
both discard useful provenance or working integration. Local native loading,
successful authenticated display launch and ordinary-user onboarding are
separate acceptance steps and must be reported separately.
