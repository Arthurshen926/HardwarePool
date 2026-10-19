# ADR 1516: Finite Linux mock payload over separate local IPC

Status: experimental implementation, 2026-10-19. IPC execution unverified.
Reconstructed from retained design/implementation context after a workspace reset;
this is not a byte-identical recovery claim or completed acceptance.

## Decision

Preserve ADR 1515 lifecycle-only `--mock-routes` and the empty default. Add
`--mock-payload`, requiring `--mock-routes`, for exactly one finite synthetic
Source-to-Sink token per Route epoch. Known repository subprocesses remain finite
test tools. Unix-domain filesystem IPC is local process communication, not IP
network access; the mock hardware/network prohibition remains in force. No
hardware, IP socket, service, credential, installation or security-policy change
is introduced. No new third-party dependency is added.

Reuse `capyio.test.samples/1`, exact `utf8-test-token` with no parameters, Basic
QoS, the synthetic CapyDataPlane backend, `DataEnvelope`, `DataPayload` and
`BoundedEnvelopeQueue`. A fixture-private JSON datagram wraps version 1, Route
identity and the envelope; it is not JSON-RPC or a public wire. Exact synthetic
clock/timestamps, sequence zero and token are validated. The bounded queue
validates Profile/Stream/epoch. Records are at most 2048 bytes, with a 2049-byte
receive buffer to reject oversized/truncated input. One record is sent/received;
there is no worker loop, retry loop or real-time callback.

The host creates a fresh UUID-named directory under the process temporary root
with mode 0700 and a fixed `sink` pathname. Creation refuses existing paths.
Endpoint addresses are UTF-8 and at most 100 bytes. Cleanup removes only the fixed
socket and empty owned directory, never recursively deleting user data. Sink
Prepare binds before Source Start sends. Socket send/receive use 500 ms deadlines,
below the existing five-second Sidecar response wait. Bind/connect do not gain a
universal execution deadline. Unsupported platform/endpoint/format/QoS fails closed.

Control carries endpoint setup and receipt metadata only; payload mode omits the
legacy independent `test_sample`. Sink Start returns a receipt only after actual
receive, validation, enqueue and consumption. Host checks endpoint, Route, Stream,
epoch, count, byte length and a consumed-token FNV-1a-64 checksum before Active.
The Sink also validates the exact expected token bytes; this checksum is only
diagnostic fixture integrity, not authentication or a cryptographic hash. This is trusted-fixture evidence, not
cryptographic proof against a malicious Adapter. Synthetic Session/consent and
CapyDataPlane backend labels do not claim real peer trust or public interoperability.

Repeated host Start revalidates lifecycle without resending. Stop clears receipt
and endpoint. Restart uses a fresh Stream/socket and advancing Route epoch. Any
prepare/start/receipt/status failure rolls back and reaps children before unlinking
owned IPC. EOF, output failure, oversized input and shutdown retain owner cleanup.
Forced host termination can leave a stale directory; no broad janitor is added.

## Trust and limits

Both repository binaries, the caller and temporary-directory ancestry are trusted.
Directory privacy does not authenticate same-user processes or sandbox malicious
Adapters. This is not cross-device transport, continuous media, hardware support,
production security or public wire interoperability. IPC tests remain required.

## Validation and reconstruction evidence

The earlier volatile draft's three socket tests failed at Unix bind with
`Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }`.
Its real process transfer failed closed before Active. An escalated executor failed
in bwrap setup on a read-only filesystem before tests ran. That draft disappeared
on environment remount; published lifecycle commit df65fca remained intact.

This reconstruction uses df65fca and preserves those restrictions. No socket or
escalated test is retried locally. Parser/receipt/endpoint tests and fake-owner
rollback can run without sockets. Full socket transfer, timeout, oversize datagram
and real subprocess receipt/cleanup tests remain ordinary required tests, not
ignored tests. The separate experimental CI branch may run these on its authorized
Linux runner. Until passing results exist, IPC execution remains unverified;
lifecycle results cannot substitute for payload delivery acceptance.

Reconstruction checks on 2026-10-19 passed: targeted check, Clippy all-targets with
warnings denied, formatting/diff whitespace, documentation/manifest validation,
Adapter smoke/crash isolation, four socket-free mock tests, 13 host unit tests,
nine lifecycle/opt-out process tests and seven generic data-plane tests. The host
fault matrix covers both sides' prepare/start/status/stop failure and verifies
owned-directory removal without opening sockets. The checksum tests reject a
wrong receipt checksum and same-length corrupted token; stale previous-epoch
records are rejected. An independent review found no blocking implementation
issue for an experimental CI candidate. These results do not establish IPC
execution; the three socket unit tests and real subprocess payload test remain
required and were not retried in the restricted local environment.
