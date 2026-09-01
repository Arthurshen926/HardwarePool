# ADR 0042: Bound the ADB-free camera lab transport

- Status: Accepted
- Date: 2026-08-31

## Context

The camera path has already proved Camera2 capture, bounded AVC export,
Windows decode, shared-memory publication and Media Foundation virtual-camera
projection through an ADB reverse tunnel. That tunnel is useful for exact-device
debugging but is unsuitable as the ordinary user workflow. The private CAVC
record provides framing and replay checks only within one connection; it does
not authenticate a peer or encrypt video.

Exposing the existing receiver on every interface would turn a local parser lab
into an unauthenticated video listener. Adding production cryptography in the
same slice would require pairing identity, key lifecycle, Route/Session binding,
replay and downgrade decisions that the pre-alpha Runtime does not yet provide.

## Decision

Add a separately named `trusted-lan` lab mode while preserving ADB reverse as
the default and compatibility path.

- Android accepts either blank input for `127.0.0.1:38173`, or one canonical
  RFC1918, link-local or 100.64.0.0/10 IPv4 literal for TCP port 38173.
- Android performs no DNS lookup, discovery, persistence, inbound listen,
  permission change or background capture.
- Windows trusted-LAN mode requires both an exact local bind IPv4 and a
  different exact allowed phone IPv4. It rejects wildcard/public/loopback
  addresses and caller-selected ports.
- The receiver closes connections from every other peer before parsing CAVC.
- `trusted-lan-live-hold` accepts exactly those two IPv4 parameters and otherwise
  reuses the fixed sibling receiver, Session/CurrentUser registration, bounded
  liveness checks and cleanup from `live-hold`.
- No firewall rule is created automatically. A necessary host rule is a
  separately approved system change with an exact rollback.
- The mode is explicitly plaintext and limited to a reviewed trusted lab. It is
  not the production no-ADB transport.

## Consequences

Camera development and controlled device use can proceed without keeping an
ADB session or reverse mapping alive. Accidental exposure is limited by one
interface, private/shared address classes, fixed port and one peer allowlist.
No dependency, Android permission, service, wire record, Core type, Protobuf or
driver changes are introduced.

The allowed phone or an on-path observer can still inspect or inject traffic.
Production completion therefore still requires mutual application
authentication, authenticated encryption, fresh keys, Route/Session/stream/
epoch binding, replay windows and downgrade protection. A Tailscale overlay can
protect transit for a lab but does not satisfy those CapyIO authorization
requirements.

## Alternatives

- wildcard plaintext listener: rejected because it exposes unauthenticated
  camera data and parsing to every reachable peer;
- hard-coded machine address: rejected because it is not portable or auditable;
- DNS/discovery in the lab: rejected because it broadens identity and spoofing
  concerns before pairing exists;
- treat Tailscale identity as CapyIO pairing: rejected because overlay identity
  does not authorize a Capability or bind the CAVC stream to a Route;
- block all ADB-free work until production pairing: rejected because a clearly
  labelled, fail-closed trusted-lab mode advances integration without claiming
  production security.
