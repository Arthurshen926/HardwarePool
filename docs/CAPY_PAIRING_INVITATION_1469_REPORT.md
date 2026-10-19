# Target-bound pairing invitation report (ADR 1469)

The integrated alpha now provides a Windows invitation generator and Android
system-file-picker importer. The container codec is bounded and target-bound;
Windows persists its identity with DPAPI, Android persists its identity with
Android Keystore, and both paired directories retain only non-secret metadata.

Automated evidence:

- `cargo test -p capyio-pairing`
- `cargo check -p capyio-desktop -p capyio-android-jni`
- `pnpm typecheck`
- `cargo xtask android-check` (91 Android tasks, successful)

Physical-device evidence on TB323FU / Android 16 confirms the updated debug APK
installs, launches, retains notification/camera/microphone grants and retains its
stable Node ID. A full invitation import and authenticated reconnect remains a
separate physical acceptance case; no such pass is inferred from installation.

A subsequent physical pass imported the target-bound invitation through the
Android system picker. The app retained one encrypted credential, one canonical
peer directory entry, reported `pairedPeerCount=1`, and began discovery
announcements. Multicast did not traverse the active overlay-network path, which
motivated the still-mTLS-authenticated manual endpoint fallback in ADR 1470.
