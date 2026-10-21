# Linux owned touchpad

This library adds a real Type-B event projection/backend implementation, not a
relative-cursor approximation. It is opt-in and does not arrange OS access.
It reuses existing pinned touchpad TLS and complete five-contact snapshots.

Software-only checks (never open `/dev/uinput`):

```text
cargo test --locked -p capyio-linux-touchpad
cargo clippy --locked -p capyio-linux-touchpad --all-targets -- -D warnings
python3 platform/linux/capyio-touchpad/native/test_helper.py
```

Optional native compile with official libevdev development headers installed:

```text
cargo build --locked -p capyio-linux-touchpad --features native-uinput
```

Do not run the resulting helper as a diagnostic. Actual creation/injection is a
separate explicitly authorized OS acceptance step. No module loading, permission
changes, default-device settings or fallback injection are part of this feature.

The factory is single-use and the owner requires exact Runtime authority and a
matched authenticated Hello before device creation. Helpers are trusted regular
sibling executables, retain their own finite/idle deadlines and fail closed on
uncertain or partial writes. Stop must interrupt transport before worker join.
Normal cleanup releases slots/keys, synchronizes and destroys. Hard process kill
only guarantees owned descriptor closure, not explicit release events.

See [ADR 1535](../../../docs/adr/1535-linux-owned-type-b-touchpad.md) for the fixed
descriptor, pressure/confidence policy, bounded IPC and acceptance limits.
Physical Android contact capture and Linux desktop recognition/gestures remain
unverified; compilation and fake event traces are not physical acceptance.
