# ADR 1423: Configure trusted-lab audio endpoints at runtime

Status: accepted for the existing trusted-network lab only

Date: 2026-09-17

The new tablet exposed that audio peers were only provided at APK build time.
Add a narrow shared-UI command that selects microphone or speaker, a canonical
private/overlay IPv4 literal, two nonzero u16 ports and explicit acknowledgement
of the insecure lab transport. No DNS, shell, file path, arbitrary command or
public-internet endpoint is accepted. New production dependencies are absent.

The Node service saves each direction privately and applies it without a
rebuild. Existing lab-build metadata remains the fallback when no saved record
exists. Configuration and start/stop admission are serialized; only Stopped or
Failed state with no remaining Adapter worker may change. Persistence failure
leaves the running configuration unchanged. The other direction is not stopped
or reconfigured, and conflicting local ports are rejected.

The fixed lab session/route/stream bindings are retained for compatibility with
existing native-audio helpers. This is explicitly NOT authenticated pairing,
discovery, a peer catalog, production session authority or replay-safe reconnect.
The common UI requires explicit acknowledgement and presents this limitation.
Security requirements for a public deliverable remain unchanged.

Alternatives: keep rebuilding APKs per device (unusable first-run flow), or
silently infer network peers (unacceptable authority). Authenticated negotiated
configuration remains a later integration gate; this slice makes the current
lab workflow usable and independently testable without pretending to close it.

Verification: 29 pure Java address/port cases, shared Vue type checking/build,
Android lint and APK assembly; designated-tablet testing must cover independent
save, active-state rejection, persistence across process recreation and real
media on the configured endpoints.
