# Linux touchpad software acceptance — 2026-10-21

## Scope

Accepted source-only implementation
`fda6080eb9e58844ebe52d6d8c957c22fd6de9da`, based on Android bridge
`54c026456375ed343767d6b465880db6106b0086`, adds the Runtime-owned
Type-B five-contact Sink and optional libevdev helper described by ADR 1535.
This acceptance covers the library/backend, not an executable Linux touchpad
launcher. Launcher composition remains the next separate software slice.
No input device was opened or created and no cursor/gesture was injected.

## Passed local checks

- `cargo test --locked -p capyio-linux-touchpad`: 25 tests (3 session, 12
  projector, 10 authenticated owner). Projector coverage includes 4,096
  deterministic randomized snapshots. Owner tests use real loopback pinned
  mTLS, actual Android capture/motion policy and the production owner/projector
  with fake native event recording. The owner suite also passed repeated runs.
- `cargo test --locked -p capyio-remote-touchpad-adapter --features tls`:
  all 68 existing regression tests passed after the fresh pre-ACK admission check.
- Default touchpad and remote-touchpad strict all-target clippy passed.
- `cargo build --locked -p capyio-linux-touchpad --features native-uinput` and
  native all-target strict clippy passed. C uses `-Wall -Wextra -Werror`.
  Official Debian libevdev-dev 1.13.4+dfsg-1 headers were downloaded and extracted
  privately; no packages or system settings were installed/changed. The compiled
  real helper was not executed. Dependency URL/hash are in THIRD_PARTY_NOTICES.
- `python3 platform/linux/capyio-touchpad/native/test_helper.py` passed the same
  production parsing/deadline/rate/cleanup loop with fake backend operations.
  Shared descriptor tables, fixed path/open flags, malformed/partial IPC,
  missing/denied create, write fault/no premature ACK, EOF, SIGTERM and idle/
  finite deadlines were exercised without `/dev/uinput`.
- Workspace formatting, documentation traceability and Adapter manifests passed.
- `cargo check --workspace --exclude capyio-desktop --all-targets` passed.

## Aggregate check distinction

`cargo xtask check` first exposed three existing non-Windows compilation errors:
AVC lab checksum Option inference (`!: LowerHex`), a non-Windows Windows-service
branch referencing a Windows-only capture-ring dependency, and a non-Windows
display-probe branch calling a Windows-only DPI function. The candidate contains
small type/cfg fixes with unchanged Windows behavior and no dependency additions.
The AVC probe also compiled independently on the Linux host.

The aggregate then passed its workspace compile phase but failed its strict
workspace clippy phase on existing `ENTROPY_CONTEXT` dead code in
`platform/windows/capyio-credential-protection/src/lib.rs:11`. Other existing
platform-unused warnings remain. Therefore the full `cargo xtask check` is NOT
reported green. Unrelated lint cleanup was deliberately not folded into this
feature. Full workspace tests and Windows execution were not rerun here.

## Hosted gate and deferred acceptance

All seven jobs across four workflows passed for
`fda6080eb9e58844ebe52d6d8c957c22fd6de9da`. The dedicated
[touchpad run 39573626828](https://github.com/Arthurshen926/HardwarePool/actions/runs/39573626828)
([job 119133632399](https://github.com/Arthurshen926/HardwarePool/actions/runs/39573626828/job/119133632399))
confirmed all 25 Rust tests, including the ten named real-mTLS owner cases,
native compilation against official runner libevdev packages, strict focused
clippy and the production-loop fake-backend success marker. No kernel input
device was accessed. Required test-name checks prevent ignored/cfg-omitted
acceptance cases from silently passing. These hosted gates do not supersede the
separately recorded aggregate workspace-clippy failure above.

Actual uinput device creation, OS permission availability, desktop/libinput
recognition, cursor movement, scrolling, gestures, physical Android input and
user-session interoperability remain unverified. No module loading, udev rules,
permission changes, driver installation or security changes were attempted.
Normal lifecycle cleanup is tested separately from hard-kill behavior: SIGKILL
can close the owned descriptor but cannot guarantee explicit release events.
