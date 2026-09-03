# CAPY-GAMEPAD-003C2 report

Date: 2026-08-29

Branch: `codex/capyio-gamepad`

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

## Result

The VIIPER Adapter now owns a fixed Xbox 360 session as one lifecycle. A caller
must explicitly assert that the separately supplied loopback server has local
auto-attach disabled. Open then validates its anchor without emitting it,
re-probes exact v0.7.0 identity, creates one bus, adds only the default Xbox 360
type, validates all returned identity fields, immediately opens the device
stream and sends an initial neutral 20-byte frame before returning `Running`.

The Worker applies the shared stream/epoch/sequence guard transactionally.
Unsupported controls or rejected headers do not consume sequence. Gaps send
neutral before recovery; explicit epoch advance sends neutral before accepting
the new epoch; sequence exhaustion sends a final neutral and latches. Raw
rumble polling returns only exact two-byte intensities, treats zero-byte timeout
as no feedback and treats partial/closed streams as terminal.

Explicit stop is idempotent and orders neutral, stream shutdown and removal of
the owned bus. Open and stop retain cleanup errors rather than replacing their
primary error. Drop only shuts down the socket and does not pretend that
blocking management cleanup is guaranteed.

## Safety decision

ADR 0042 records why create, add and stream cannot be exposed as independent
CRUD. The upstream documentation/source auto-attach mismatch is addressed by a
required caller assertion and owned rollback. The assertion is not proof of
external process configuration; real integration must retain separate evidence
that `--api.auto-attach-local-client=false` was applied.

If `bus/create` succeeds but returns an invalid response without a safely
validated bus ID, the client cannot remove a guessed or enumerated bus without
risking another owner. That upstream-protocol failure remains explicit.

## Automated evidence

- `cargo test -p capyio-viiper-adapter`: passed (22 tests: 7 owned
  session, 6 bounded probe and 9 Xbox codec tests).
- `cargo clippy -p capyio-viiper-adapter --all-targets -- -D warnings`:
  passed.
- `cargo xtask validate-docs`: passed (84 unique Requirement IDs).
- `cargo xtask validate-manifests`: passed (2 manifests).
- `cargo xtask ci`: passed, including workspace format/check/Clippy/tests,
  deterministic demo, repository validation, Adapter smoke and desktop
  typecheck/build.

Fixture coverage includes exact provisioning and cleanup request order,
independently hard-coded neutral/button frames, codec-before-sequence behavior,
gap and epoch neutral ordering, exhaustion latching, exact/consecutive rumble,
no-feedback timeout, truncated/closed feedback, add rollback and combined
cleanup error retention.

## Explicit exclusions

- no real VIIPER process connection, start, download or binary import;
- no USB/IP attach, driver/certificate/security-policy or system-device change;
- no Runtime/UI/Android integration and no real game/emulator claim;
- no haptics Route/duration mapping;
- no commit, push, pull request or release operation.
