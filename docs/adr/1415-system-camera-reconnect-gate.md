# ADR 1415: System-camera reconnect gate

Status: accepted

Date: 2026-09-16

## Decision

Keep the desktop-owned AVC receiver and system-camera session alive during the
existing fixed 60-second receiver reconnect grace. A phone capture Stop is a
temporary absence of frames, not an instruction to remove the Windows camera.
The next Android Start creates a new wire connection; the shared publisher
retains its Windows mapping identity, continues its monotonic publication
sequence and marks the first recovered frame discontinuous.

The ignored physical camera gate optionally enables this case only with
`CAPYIO_LIVE_CAMERA_RECONNECT=1`. It requires an initial decoded frame, a
verified 1.5-second no-new-frame gap after the phone stops, a later higher
sequence after restart, an independent Media Foundation consumer pass, and
normal composed Stop/cleanup. Each start/restart wait is bounded to 5–60
seconds. Markers are test orchestration only and are not product control input.

`AndroidReceiverActions` also owns cleanup in `Drop`, in addition to the
application's explicit shutdown path. This prevents a direct owner or a
panicking ignored test from silently abandoning its camera/receiver children;
normal product shutdown remains explicit.

## Evidence

On the approved elevated Windows/Android 0.5.6 lab, the first live phone frame
was publication sequence 1. Android Stop produced the required quiet interval
without ending the Windows test. Android Start then recovered live delivery at
sequence 1027 with timestamp 34,199,658,000 ns, 1,382,400 NV12 bytes and checksum
112,599,881. The separate Media Foundation consumer passed after recovery.
Final Stop left no Android active camera client, no ADB reverse and no existing
current-user camera registration. No pixels or recordings were retained.
