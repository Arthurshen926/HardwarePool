# HardwarePool Testing Strategy

## 1. Principles

- Tests are evidence for explicit requirements, not an afterthought.
- Pure Core and Protocol tests must run without hardware.
- Platform and driver tests run only in identified target environments.
- Every audio failure should be explainable through counters, logs and retained test artifacts.
- Subjective listening complements but does not replace automated signal analysis.

## 2. Unified commands

```bash
cargo xtask doctor
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo xtask ci
cargo xtask demo
```

Frontend:

```bash
pnpm typecheck
pnpm build
```

The `xtask` command intentionally excludes the Tauri shell from default Core checks because Linux Tauri builds require platform packages. CI has a separate UI workflow.

## 3. Test layers

### Unit tests

Targets:

- ID parsing and type safety;
- audio format validation;
- capability role consistency;
- session and binding state transitions;
- independent microphone/speaker behavior;
- Runtime event ordering;
- Protobuf conversion and codec.

Run on every PR on multiple desktop operating systems.

### Deterministic integration tests

Use `hardwarepool-testkit` fixtures and no real sockets:

- register Android peer;
- open session;
- request both capabilities;
- activate one or both;
- stop one without changing the other;
- simulate disconnect;
- snapshot state and events.

### Transport simulation tests

Future in-memory and network-emulation layers inject:

- latency and jitter;
- packet loss;
- duplicates and reordering;
- abrupt disconnect;
- malformed frame headers;
- two deliberately different audio clock rates.

### Android component tests

Required on real phone:

- local render test tone;
- microphone record to WAV;
- actual stream parameters;
- screen lock and background;
- foreground-service stop;
- permission revoke;
- Audio Focus contention;
- route changes involving Bluetooth/USB when available;
- power-saving mode.

### Windows user-mode tests

- enumerate endpoints;
- WASAPI playback/capture test programs;
- Broker process start/stop/reconnect;
- bounded IPC queues;
- service restart and sleep/resume.

### Windows driver tests

Only on an isolated test VM or dedicated test installation:

- install, enumerate, enable, disable, upgrade, uninstall;
- render/capture stream open/close;
- Broker absent and crashes mid-stream;
- Windows Audio Service restart;
- reboot;
- Driver Verifier for the project driver only;
- static analysis and relevant HLK tests before release.

### End-to-end tests

- Windows test tone → virtual speaker → Android speaker;
- Android microphone → virtual microphone → Windows recorder/browser;
- independent start/stop;
- full duplex;
- network interruption and reconnection;
- Android lock screen and Windows sleep/resume.

## 4. Audio signal tests

### Playback latency

```text
known Windows chirp
 -> HardwarePool path
 -> Android speaker
 -> physical laptop microphone recording
```

Use cross-correlation against the source signal. Retain input/output WAV and report median, p95 and outliers.

### Capture latency

```text
physical laptop speaker chirp
 -> Android microphone
 -> HardwarePool path
 -> Windows virtual microphone recorder
```

### Quality checks

Automated checks should measure:

- expected sample rate and duration;
- clipping rate;
- silent gaps;
- dominant tone/chirp alignment;
- discontinuities;
- lost/repeated sample indicators;
- output RMS relative to baseline.

### Drift test

Do not use acoustic latency for drift. Record source samples, sink samples, queue water level and dynamic resampling ratio over a long run. The water level must remain bounded.

## 5. Suggested initial acceptance thresholds

These are engineering starting points, not public guarantees:

| Metric | Bootstrap target |
|---|---|
| Core/Protocol tests | 100% pass |
| Independent binding state | deterministic under repeated tests |
| Single-direction soak | 2 hours without crash or unbounded memory |
| Duplex soak | 2 hours without uncontrolled feedback |
| Normal LAN median one-way latency | approximately <= 150 ms initially |
| Disconnect behavior | endpoints remain stable; capture supplies silence |
| Reconnect | clean new epoch, no old audio replay |
| Driver Verifier | no project-driver violation |

## 6. Test evidence format

```text
test-results/<run-id>/
  manifest.json
  summary.md
  windows-version.txt
  android-version.txt
  config.json
  broker.log
  android-logcat.txt
  driver-events.evtx
  metrics.jsonl
  input.wav
  output.wav
  latency-report.json
```

`manifest.json` records Git commit, tool versions, protocol/Profile versions, OS builds, device model, driver version, network mode, test case and timestamps.

## 7. CI policy

Required before merge:

- Rust formatting;
- Clippy with warnings denied;
- Rust tests;
- Protobuf build;
- frontend type check and build;
- license/dependency review when dependencies change.

Hardware tests can be manually triggered initially but must attach artifacts and a signed-off result. Claims about tested platforms must match actual evidence.
