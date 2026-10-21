# ADR 1551: Interactive finite paired-store touchpad

Status: experimental; isolated terminal and software-backend acceptance required.
Extends ADRs 1547–1550 without changing the Node input protocol or lease policy.

## Production scope and consent

The console adds `touchpad --config ABSOLUTE_PATH --peer NODE_UUID`. It invokes
only its trusted fixed sibling `capyio-node linux-touchpad --trusted-lab`, with the
same four explicit paired-store selectors. No arbitrary executable, device path,
backend switch, default keyring discovery, unlock or credential export is added.
The bounded public config and paired-store marker use inherited anonymous pipes;
the controlling terminal and fresh-prompt rules remain unchanged.

The strict nonsecret manifest has version, local Node, exact remote Source Port,
concrete supported local IPv4 listen address and a 3–60-second duration. It is
not an audio/camera resource schema or a grant. The regular owned bounded file
rules still apply; filesystem latency is not a hard deadline. The capability
probe and immutable config precede interactive resource consent.

The current authenticated approval must match the exact Source and local Sink,
request/route/session, `touchpad-frame-v1`, `tls13_mtls_v1`, finite duration and
30-second consent window. The fixed profile is five contacts, 120×70 mm,
non-clickable and pressure-capable. All those fields are displayed and bound.
Only a fresh exact `start REQUEST ROUTE PEER` after prompt flushing authorizes the
one local session. Pretyped, stale, generic or mismatched input cannot do so.
The Node's own request revalidation and the remote Source's independent grant
and finite lease remain mandatory. Pairing alone never authorizes input injection.

## Honest state, not invented telemetry

The existing public Node events expose route state and
`os_gestures_verified=false`, not helper IDs, frame/commit counters or ACK numbers.
This increment preserves that schema. The console validates exact route, one
binding and one activation notification, with monotonic `Starting` → `Active`.
A solicited status may legitimately report `Active` before the separate activation
notification because of the existing Node loop order. Only current Node runtime
state may report it; binding, queue admission or local confirmation never does.
The runtime's existing full-backend-commit/ACK activation rule is unchanged.
No OS gesture or physical-device acceptance is inferred, and the console does
not fabricate missing counters or helper identity.

`Touchpad event:` contains only typed public events. `status`, `stop`, terminal
EOF/interruption, peer loss, Source revocation and finite expiry remain one-shot.
Existing terminal/process bounds apply: 55-second preapproval envelope,
30-second consent, selected duration plus at most three seconds of Stop grace,
bounded framing and owned child reap. These bounds do not extend any resource
grant or lease. An uncertain failure is not relabeled as clean release. No
persistent approval, automatic renewal, reconnect or session restart is added.
Saved protected pairing credentials are retained.

## Software-only acceptance boundary

The opt-in development feature `capyio-linux-console/touchpad-test-fixture`
implies paired-store support and adds an explicitly named `touchpad-fixture`
action. It targets only the same fixed sibling Node's existing
`linux-touchpad-fixture` command. Both build features are required. The mode is
one of normal, fail-create, partial-write, delayed-commit or backpressure; no
path or native backend value is accepted. The ordinary `touchpad` action remains
native even in this test build and is never executed by software acceptance.
Both production staging profiles exclude the fixture feature/action and verify
that its help invocation is rejected.

Only this explicitly compiled fixture action inherits Node stderr directly so
the existing synthetic fake-helper trace reaches the isolated test oracle.
Production operations continue to discard bounded Node stderr and report fixed
errors. This instrumentation is not a production diagnostic/logging mode or a
claim that arbitrary stderr is safe. Tests use only synthetic credentials and
assert no secret appears in terminal, arguments, environment or diagnostics.

Nine mandatory ignored hosted cases drive actual private PTY → console → fixed
Node → isolated fake Secret Service → pinned synthetic peer → software fake-uinput
helper. They reuse existing real commit receipts and ACK checks, rather than
substitute a controller marker for backend completion. They cover five fresh
consent negatives, five-slot tracking/cancel/gap semantics, mismatched hello,
create/partial-write/backpressure failures, delayed commit, Stop/EOF/expiry,
changed pin/offer and partial data handshake cleanup. Source revocation uses the
existing runtime grant/control path; changing a saved pairing record is not
claimed to revoke an already loaded session snapshot.

Exact test discovery, zero skips and one precise console marker per case are
required, alongside independent process ancestry, full commit-before-ACK,
contact-release trace and all-process cleanup. Helper inspection brackets executable
and public-metadata reads with PID, parent and process-start-time identity. A
missing or permission-denied executable/metadata read triggers one bounded
100 ms observation window with 2 ms spacing, not a media or lease extension.
Every retry repeats the complete proof for the same generation. Only a complete
live inspection earns identity/redaction evidence; proven exit or disappearance
ends observation without new evidence. Persistent live denial, PID reuse, wrong
executable and malformed/stat-read errors remain failures. The window is never
reset; it bounds retries, not arbitrary filesystem latency. Existing touchpad software tests,
13 camera console profiles and all 64 audio graph profiles remain mandatory.
No test opens `/dev/uinput`, changes kernel/input permissions or touches a user's
keyring, real terminal or physical device. Production transport, cryptography,
resource authority and native helper semantics are unchanged.
