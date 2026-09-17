# ADR 1424: Shared display receiver entry and prerequisites

Status: accepted for integrated receiver entry; media qualification pending

Date: 2026-09-17

The imported display Activity is already compiled into the unified Android APK
but was unreachable from its shared UI. Add two narrow CapyIOApi operations:
read a bounded prerequisite DTO and request the fixed internal display Activity.
Browser mock and Windows return no Android receiver, not simulated readiness.

The status exposes transport selection, bounded private-file presence and
native-library load availability only. It never includes keys, certificates,
file paths or arbitrary launch arguments. No network request is made by status
inspection. The Activity re-reads and validates configuration on opening and
authenticates the remote launch before decoding. File presence and library load
are explicitly not proof of valid credentials, peer trust, codec support,
active Route or presented frames. Missing GameStream code never selects CPD1.

Opening is allowed only when the selected transport's prerequisites are present.
Activity launch is marshalled to the main thread, targets an existing unexported
Activity and adds no permissions, dependencies or exported components. This
does not provision credentials or finish the Windows sender/UI/packaging work.

Fourteen pure Java cases cover transport/config/native combinations, explicit
unavailability and unknown-mode rejection. Full Android and shared UI build
checks cover the platform integration. Physical missing-configuration behavior
is tested separately; successful display media needs a provisioned bundle and
the native engine and is not inferred from the status card.
