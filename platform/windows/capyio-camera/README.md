# CapyIO Windows Camera

This crate currently provides the hardware-free CAPY-CAMERA-001A fixture:

- deterministic 1280x720, 30 fps, packed NV12 color bars;
- a moving marker and binary frame-sequence clock;
- rational, system-clock-independent timestamps;
- checked resumption at an explicit existing sequence/timestamp for continuous
  placeholder fallback;
- a validated queue bounded to twelve owned frames / 16,588,800 payload bytes;
- explicit reject-newest or drop-oldest overflow behavior.

CAPY-CAMERA-001B0 adds a pure Windows Media Foundation projection seam:

- a session/current-user-only virtual-camera plan;
- a closed start/stop/shutdown lifecycle model;
- QPC-correlated 100 ns sample timing without cumulative frame-rate drift;
- checked row-wise copy from packed NV12 into a positive-stride 2D buffer;
- an opt-in read-only SoftwareCameraSource support probe.

CAPY-CAMERA-001B1A adds the non-COM protocol and execution cores needed before
Frame Server integration:

- exact first-start and restart event ordering for one canonical stream;
- a fixed four-entry FIFO of `RequestSample` tickets with no runtime growth;
- transactional sample completion with stream-generation and sequence checks;
- stop/shutdown cancellation of all outstanding sample requests;
- a backend-neutral registrar that makes prepare/start/stop/shutdown explicit,
  attempts terminal cleanup after failures and exposes cleanup-required state.

CAPY-CAMERA-001B1E adds a process-neutral decoded-frame ingress contract:

- one fixed stream ID and positive epoch;
- canonical 1280x720 30 fps packed NV12 owned payloads only;
- strictly advancing sequence and source timestamps;
- a caller-selected capacity inside the existing twelve-frame maximum;
- drop-oldest overflow with an explicit discontinuity on the retained stream.

The library itself does not call Media Foundation. On Windows, the probe can be
built and run with:

~~~text
cargo run -p capyio-windows-camera --features windows-mf-probe --bin capyio-camera-mf-probe
~~~

That binary calls only `MFIsVirtualCameraTypeSupported`; it never creates,
starts, stops, registers, enumerates or removes a camera. The sibling
`capyio-windows-camera-mf` crate supplies the Windows-only COM projection,
class factory and closed session/current-user registrar introduced by 001B1B
and 001B1C. CAPY-CAMERA-001B1D adds only a bounded cross-process validation
harness. The sibling's non-registered 001B1E constructor can consume the
external ingress, but registered COM activation remains on the fixture until a
separate process-boundary slice. The library remains independent of the Windows
SDK and there is still no driver, codec, network path or physical camera-device
read in this crate.
