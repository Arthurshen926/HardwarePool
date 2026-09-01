# ADR 0047: Use an ADB reverse tunnel for the first live touchpad lab

Status: accepted

## Context

CAPY-PTP-002T defines private Hello/Data/Ack/Close records but explicitly does
not provide authentication or encryption. CAPY-PTP-002U builds the real Android
`MotionEvent`/JNI boundary, but no application or live transport exists.

The first physical Android-to-Windows proof must not normalize a plaintext LAN
socket into a production transport. It also must keep virtual touchpad creation
behind a human-visible, explicit desktop-input gate and retain exact Route,
Session, endpoint, epoch and per-frame acknowledgement validation.

## Decision

For CAPY-PTP-002V only, carry the existing private records through an Android
Debug Bridge `reverse` tunnel paired to the explicitly authorized lab phone.

- The Windows listener binds only `127.0.0.1:61000`.
- The Android lab app connects only `127.0.0.1:61000`; ADB maps the device loopback
  endpoint to the Windows loopback listener.
- Windows requires both `--inject` and `--acknowledge-desktop-input` before it
  accepts a connection or creates `SyntheticTouchpadSession`.
- A complete 160-byte Hello must match the compiled Route, Session, Source,
  Sink and epoch before device creation.
- Every Data record is bounded, decoded through both transport and touchpad
  codecs, submitted once, then acknowledged only after native Sink success.
- Android permits at most 64 queued records and validates exact Ack magic,
  version, kind, flags, epoch and sequence.
- Connect and read/write operations use finite three- or thirty-second lab
  deadlines. Failure closes the connection and the Windows Sink fails closed.

The lab APK declares `android.permission.INTERNET`, contains one exported
launcher Activity and no service, receiver, storage, microphone, accessibility
or foreground-service permission. Its debug signing key is not a production
identity. Installation on the identified `V2419A` requires explicit human
approval and is not a hosted-CI action.

## Security boundary

ADB pairing and the operator-controlled reverse tunnel are the lab transport
boundary. This is not CapyIO peer pairing, production mutual authentication,
authorization issuance, key rotation or a deployable network protocol. The lab
must never change its listener to a non-loopback address or connect directly to
a LAN peer without a new transport/security ADR.

The reverse mapping, Activity, temporary uploaded APK and virtual device are
removed/stopped after the evidence run. The installed lab application may remain
on the authorized phone for later tests and is removable through ordinary Android
package management.

## Consequences

- Real Android pointer identity and timing now reach the actual Windows
  Precision Touchpad API without introducing a plaintext LAN listener.
- The first run exposed and rejected a mixed Android clock-domain bug; using
  uptime nanoseconds now matches `MotionEvent.eventTimeNanos`.
- A sandboxed Windows process was correctly unable to submit desktop input;
  the explicitly approved non-sandbox lab process completed the same path.
- Production authenticated transport, pairing UI, reconnect and background
  lifecycle remain open.

## Alternatives considered

- Plain TCP over Wi-Fi/Tailscale: rejected for this slice because record Hello
  is not authentication and Tailscale identity is not yet bound into CapyIO.
- Skip Hello/Ack in the lab: rejected because that would test a different and
  weaker protocol than the intended data path.
- Accessibility-service gesture injection on Android: rejected because the
  goal is forwarding raw contacts to Windows Precision Touchpad semantics.
- Install a Windows kernel driver now: rejected because the proven user-mode
  factory is sufficient for this gate and driver deployment is a separate risk.
