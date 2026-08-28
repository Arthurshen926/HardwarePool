# CapyIO Roadmap

Gates are evidence thresholds, not calendar promises. A Gate closes only when
its acceptance criteria and tests exist. Later Gates do not expand the active
implementation scope automatically.

| Gate | Goal | Non-goal | Acceptance and tests | Primary risk |
|---|---|---|---|---|
| 0 — baseline | Audit and reproduce the bootstrap | new architecture code | baseline report; repository validator, Rust workspace, tests and UI build | hidden environment differences |
| 1 — CapyIO baseline | Name migration, PRD v0.3, architecture, ADRs and provenance skeleton | physical integrations | all active names/docs consistent; validator and full builds pass | large rename obscures behavior changes |
| 2 — typed graph | Symmetric Node, AdapterInstance, Capability/Port/Route/Session/Problem, protocol and Mock UI | real data planes | direction/profile rejection, opposite Routes, independent stop, protocol round trips, four UI Routes | breaking pre-alpha model conversion |
| 3 — Adapter foundation | Manifest/SDK, NDJSON control, Host/Supervisor, mock Source/Sink | real third-party code | schema validation, framing/errors, child lifecycle, crash isolation, smoke test | Windows process/pipe edge cases |
| 4 — product UX | versioned Quick Actions, Workspace pages and built-in Panel registry | dynamic plugin market | accessible Route Builder, task templates, persisted layout tests | exposing internals to ordinary users |
| 5 — standard IMU path | SensorServer Source, `motion.imu-samples/1`, IMU Panel and initial Recorder | system gamepad | vivo manual data evidence, timing metadata, mock/record replay | Android sensor timestamp/permission variance |
| 6 — gamepad | IMU/touch to DSU and VIIPER, haptics feedback | universal USB classes | emulator/game recognition plus reverse feedback; driver work only in approved target | virtual-input distribution and licensing |
| 7A — remote-speaker transport | Windows system mix to Android speaker through Audio Share AdapterManaged Route | production installer/security | real playback, disconnect/background/focus evidence | latency, protected audio and mobile lifecycle |
| 7B — Windows virtual speaker | independent `CapyIO Speaker` render endpoint feeding the 7A transport | uncontrolled host deployment or kernel networking | unmodified SysVAD enumeration baseline plus bounded render-APO/Broker real-PCM bridge in an isolated target or ADR 0029 local lab, exclusive-route playback, restart/uninstall and failure evidence | WDK/APO packaging, signing, real-time safety and distribution |
| 8 — remote microphone | MicYou engine to Windows system microphone Quick Action | new audio stack from scratch | ordinary-app recording, permission/revoke/lock tests | virtual audio packaging/signing |
| 9 — camera | VCamdroid to preview and Windows virtual camera | rewritten codec/FFmpeg stack | resolution/rotation/disconnect and ordinary-app enumeration evidence | upstream C++/FFmpeg/Softcam maintenance |
| 10 — input and mirror | scrcpy composite, keyboard/pointer Adapters, screen mirror | extended display | independent input/display Routes and platform fallback tests | platform injection restrictions |
| 11 — extended display | Windows IDD experiment to mobile Display Sink | conflate mirror with display extension | isolated driver target enumerates/stabilizes virtual display | WDK/IDD stability and signing |
| 12 — research ecosystem | production MCAP recording/playback, ROS 2 and Foxglove export | ROS types in Core | multi-stream timestamp/provenance/replay tests | clock calibration and dependency weight |
| 13 — multi-node/overlay | multi-node catalogs, fan-out and external EasyTier process integration | embed overlay into Core | mDNS/manual IP/EasyTier-address scenarios and independent Routes | topology/security complexity |
| 14 — secure public alpha | pairing, capability authorization, installers, signing, SBOM, log collection | broad enterprise multi-tenancy | three Capability Classes, both directions, system Projection, Panel/Recorder, security evidence | signing, support and supply chain |
| 15 — consolidate | selectively turn valuable AdapterManaged Routes into StandardPorts and decide mobile vendoring | rewrite stable integrations for uniformity | measured interoperability and migration decisions per Adapter | abstraction-driven regressions |

## Current status

- Gates 0–3: complete locally; evidence is in `docs/GATE_0_3_REPORT.md` and the
  completed foundation plans. PR #10 also passed hosted Windows/Linux/macOS,
  Adapter, UI and Windows Tauri checks before merge commit `5f5b81f`.
- Gate 5: complete locally. `CAPY-IMU-001A` proves deterministic bounded replay;
  `CAPY-IMU-001B0..B2` prove the mapped SensorServer contract, bounded WebSocket
  path and physical payload/Panel/Recorder/disconnect behavior; `B3A/B3B` bind
  the physical path to the Tauri UI and the desktop Node's single Runtime with
  explicit Problem, retry epoch and stop evidence. Hosted exact-head evidence
  remains a merge prerequisite rather than a missing Gate 5 behavior.
- Gate 7A initial vertical slice is complete locally: the pinned Audio Share
  process is supervised and projected as one Runtime-owned AdapterManaged
  Quick Action; physical transport, Android PCM submission, disconnect, retry
  epoch, stop and one user-confirmed audible case pass. The Quick Action also
  supports safe session-local playback-endpoint reselection. Gate 7A remains
  open for longer background/audio-focus, latency and soak evidence. Gates 4,
  6 and 8–15
  remain roadmap only.
- Gate 7B Speaker functionality is complete on the controlled lab. The independent
  `CapyIO Speaker` endpoint, post-mix APO, bounded render ring and service-owned
  Broker have physically delivered application playback to Android; endpoint
  volume/mute, Broker restart, port cleanup and ordinary-user desktop control
  are proven. Release qualification is deliberately not folded into the
  functional claim; signed installation, upgrade/uninstall, soak/reboot and
  production security continue under `CAPY-AUDIO-001C` before public alpha.

## Public-alpha proof bar

One product must manage at least three Capability Classes, Android→Windows and
Windows→Android directions, one system Projection, one Panel/API/Recorder path,
and independent Route failure after one node pairing flow.
