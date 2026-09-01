# CapyIO Repository Map

- `crates/capyio-core` — pure Node/Adapter/Capability/Port/Route/Session/Problem domain.
- `crates/capyio-audio` — bounded audio frame, reorder and drift primitives.
- `crates/capyio-protocol` — `capyio.v1` Protobuf generation/conversion.
- `crates/capyio-runtime` — catalogs, Routes, operations, events and snapshots.
- `crates/capyio-adapter-sdk` — manifest and Adapter control DTO/codec.
- `crates/capyio-adapter-host` — Sidecar process supervision.
- `crates/capyio-testkit` — deterministic Windows/Android fixtures.
- `apps/capyio-node` — headless demo and mock-sidecar binaries.
- `apps/desktop` — Vue/Tauri Quick Actions and Workspace UI.
- `adapters/native-audio-lan` — bounded direction-neutral UDP audio lab reference.
- remaining `adapters` — executable mocks and compatibility/integration boundaries.
- `protocol/proto/capyio/v1` — canonical node control schema.
- `protocol/schemas` — Adapter manifest JSON Schema.
- `third_party` / `LICENSES` — provenance and license tracking.
- `platform/android` — CapyIO Android audio Node/service shell plus native-LAN Java contract.
- `platform/windows` / `drivers` — Windows platform and controlled-lab driver boundaries.
- `platform/windows/render-ring` — bounded user-mode owner of the render APO
  shared-memory ring, reused by compatibility and native speaker Brokers.
- `panels` — built-in Panel policy.
- `xtask` / `scripts` — safe unified checks and repository validation.
- `docs` — normative requirements, architecture, ADRs, plans and evidence.
