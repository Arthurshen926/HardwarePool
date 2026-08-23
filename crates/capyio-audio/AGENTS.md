# Audio Data-Plane Agent Rules

- This crate is operating-system and transport independent.
- Do not import platform SDKs, sockets, async runtimes, codecs or UI frameworks.
- Every queue and payload size must be explicitly bounded and validated.
- Never interpret old stream epochs as current data.
- Sequence, sample-index and timestamp handling must have tests for gaps, duplicates, late frames and overflow.
- Algorithms used from a real-time callback must not allocate after initialization unless the API explicitly documents otherwise. The bootstrap `ReorderBuffer` is a control/worker-thread component, not an audio-callback ring.
