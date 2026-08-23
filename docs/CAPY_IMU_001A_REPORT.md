# CAPY-IMU-001A validation report

Date: 2026-08-24

Branch: `codex/capyio-imu-standard-path`

Base: Foundation merge commit `5f5b81faa572341881e4e780af7e065643d6d56d`

## Result

The fixture-first StandardPort slice is locally complete. CapyIO now has a
bounded, transport-independent Rust data-plane path that replays a versioned IMU
fixture to independent numeric Panel and JSONL Recorder consumers. The desktop
Browser Mock and Tauri backend expose the same fixture summary. A separately
authorized Android target was inventoried through read-only, explicit-serial
ADB commands.

This report does not claim a live Android sensor stream, SensorServer Adapter,
APK, production transport, physical timing accuracy, system virtual device or
driver.

## Implemented behavior

### Bounded StandardPort data plane

- `DataEnvelope<T>` preserves Profile, StreamId, epoch, sequence, source and
  receive timestamps, clock domain and a typed payload.
- Profile, clock-domain, epoch, payload-validation and serialized-payload bounds
  fail explicitly.
- `BoundedEnvelopeQueue` distinguishes accepted data, sequence gaps,
  duplicates, late data, wrong stream, stale/future epoch and full queue.
- Explicit epoch advancement clears retained data and sequence state.
- `BoundedFanout` gives every consumer an independent fixed-capacity queue,
  lifecycle state and saturating full-rejection counter.
- A slow/full/stopped Recorder neither blocks nor mutates Panel progress.

### IMU Profile and sinks

- `capyio.motion.imu-samples/1` preserves SI acceleration/angular velocity,
  optional microtesla magnetic field, Android device coordinates, accuracy,
  calibration and bounded sensor metadata.
- `fixtures/imu/imu_samples_v1.jsonl` contains six deterministic, continuous
  envelopes and is not captured phone data.
- Fixture parsing imposes record and line bounds and rejects empty, malformed,
  wrong-Profile or oversized evidence.
- `NumericImuPanel` retains the latest sample plus received/missing counts.
- `BoundedJsonlRecorder` imposes record and per-line bounds and emits valid,
  deterministic JSONL without owning arbitrary filesystem access.
- `capyio-node imu-fixture-demo` and `cargo xtask imu-demo` replay the compiled
  fixture through both consumers.

### Desktop projection

- UI snapshot schema v3 adds a fixture-only IMU summary shared by Browser Mock
  and Tauri.
- The UI shows acceleration, angular velocity, Panel/Recorder counters and
  independent sink states.
- Labels say `deterministic fixture` and explicitly deny live phone data.
- No new Tauri command, shell/filesystem permission or remote-content power was
  added.

### Android lab

- `android-doctor --serial` validates official ADB, exact target presence,
  authorization/online state and read-only policy.
- `android-baseline --serial` prints sanitized JSON from allow-listed read-only
  calls.
- `android-collect --serial` writes only to a generated ignored path below
  `test-results/android/` and refuses overwrite.
- Child stdout/stderr reads are independently drained and capped at four
  megabytes.
- The collected baseline omits ADB address/port, serial and build fingerprint.

Observed sanitized target facts:

- vivo V2419A / PD2419;
- Android 16, API 36, arm64-v8a;
- security patch 2026-06-01;
- physical display 1216x2640;
- 63 bounded SensorService inventory rows;
- claims flags: no APK installed by this task, no permission mutation, no live
  CapyIO stream.

## Validation evidence

Passed locally:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                         # 79 tests passed
python scripts/validate_repository.py --self-test
python scripts/validate_repository.py          # 84 Requirement IDs traced
cargo xtask validate-docs
cargo xtask validate-manifests                 # 2 manifests
cargo xtask adapter-smoke
cargo xtask imu-demo                           # Panel 6, missing 0, Recorder 6
cargo xtask ci
pnpm install --frozen-lockfile
pnpm --filter @capyio/desktop typecheck
pnpm --filter @capyio/desktop build             # 26 modules
cargo check -p capyio-desktop
cargo build -p capyio-desktop
cargo xtask android-doctor --serial <explicit-serial>
cargo xtask android-baseline --serial <explicit-serial>
cargo xtask android-collect --serial <explicit-serial>
```

Rust test count by component:

- Adapter Host 9;
- Adapter SDK 12;
- Audio 10;
- Core 12;
- Data Plane/IMU 7;
- Protocol 10;
- Runtime 4;
- Testkit 13;
- xtask Android sanitizer/target logic 2.

MSVC emitted its normal DLL import-library linker message during desktop build;
Clippy with warnings denied passed and no source warning remains.

## Safety and evidence boundary

- No APK was built, installed or removed.
- No Android permission, service or setting was changed.
- No Windows driver, WDK tool, boot setting, Secure Boot or BitLocker operation
  ran.
- No production signing key, release, tag or package was produced.
- The ignored Android evidence is local laboratory output and is not staged for
  Git.
- The untracked user file `CapyIO_Codex_Master_Prompt.md` was preserved.

## Remaining risks and next slice

`CAPY-IMU-001B` remains necessary for live SensorServer data, Android permission
and foreground lifecycle, source clock behavior, disconnect/reconnect gaps,
wireless rate/latency evidence and APK development/install flow. A production
transport still needs framing, authentication, replay defense, MTU/rate bounds
and fuzz/golden-wire tests. None of those claims are implied by this slice.
