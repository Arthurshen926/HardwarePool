# ADR 1501: Native audio Route service control

Status: Accepted (explicit user approval, 2026-09-29); service and desktop handoff implemented, device validation pending

Date: 2026-09-29

## Observed normative conflict

The product needs a selected authenticated peer and a newly authorized Route
to configure microphone and speaker workers without two manual starts.
`NativeLanRouteBinding` and the microphone supervisor's `start_route` already
preserve dynamic identities. They do not authorize changing privileged service
configuration.

`SECURITY_MODEL.md`, in its ADR 0034 section, expressly prohibits requests on
the all-interactive-user Broker pipe from changing bind addresses or ports.
Its ADR 0047 section keeps native microphone local/peer UDP endpoints in
administrator-set launch configuration and excludes them from the pipe and
WebView. `ARCHITECTURE.md` repeats the administrator-controlled network
configuration boundary. ADRs 1491 and 1494 accept groundwork/control metadata
only and do not supersede those requirements.

Appending dynamic StartRoute parameters to that existing pipe would contradict
these documents and expand what any interactive local process can request of
the privileged Broker. A supplied Route or Session JSON object is not proof of
remote authorization. That change was paused until acceptance of this separate
principal boundary; broadening the original pipe remains prohibited.

## Accepted boundary

Keep the existing Broker pipe and its fixed-configuration operations unchanged.
Prefer a separate, opt-in native-audio Route pipe whose administrator-controlled
launch configuration identifies its allowed Windows SID, following the existing
display control principal boundary. Do not add the interactive-users group or
accept a SID, executable, path, device identity or arbitrary argument from a
request. Remote pipe clients remain rejected. The administrator retains the
existing pinned helper paths and capture/render mapping choices.

The allowed principal's Node Runtime is the local authorization authority. It
must have a Ready authenticated Session, user-approved resource scope, matching
catalog endpoints and formats, finite grants and current leases before calling
the Broker. The Broker independently validates closed/bounded request structure,
direction, preset, identities, epoch and deadline, but these checks do not turn
caller-supplied metadata into cryptographic authorization evidence. This trust
delegation was explicitly accepted by the user. A stronger
alternative is to move the authenticated control owner into the Broker; that is
larger work and is not silently included here.

The narrow operations are StartRoute, StatusRoute and StopRoute, bound to
Session/Route/Stream/epoch and the local caller principal. Only explicit literal
IPv4 endpoints and supported PCM presets are admitted. Microphone and speaker
have separate owners; a request cannot replace an active different Route or
stop the opposite direction. Repeated matching start/stop is idempotent.
Workers stop on finite deadline and explicit Stop. The configured Runtime must
stop on session loss/revocation and renew ownership with an identical Start at
least every 15 seconds; missing renewal stops the child independently of UI
activity. A reconnect creates a fresh epoch, never silently resumes old authority.
No service configuration, credential or pairing record is rewritten.

## Implementation after acceptance

1. Add the opt-in administrator SID/configuration and isolated pipe; retain the
   existing pipe's schema and capabilities.
2. Connect the existing microphone `NativeLanRouteBinding`/`start_route` path.
3. Add the symmetric stereo speaker worker arguments and finite lifetime.
   The current native audio control offer recognizes mono only; stereo requires
   an explicit compatible preset extension, not pretending stereo is mono.
4. Return readiness separately from media counters. Child liveness alone never
   reports that Android audio reached the Windows endpoint or vice versa.
5. Connect desktop/Android orchestration, then validate physical audio and
   independent/concurrent Stop behavior before advertising one-click use.

No driver, service deployment, APK, permission, signing or real-machine setting
change is authorized by this proposal. Native UDP remains the explicitly
insecure controlled-lab transport: control TLS does not encrypt its payloads or
provide its integrity/replay protection. No production security or end-to-end
completion is claimed.

## Implemented service slice and remaining integration

The SCM-only `--audio-route-control-user-sid` opt-in opens a first-instance,
remote-client-rejecting `CapyIO.Audio.Route.Control.v1` pipe. It requires native
speaker plus microphone trusted launch configuration. The legacy IU pipe is
unchanged; legacy operations cannot mutate an owned Route worker. Dynamic
endpoints affect only that child launch, not legacy service configuration.
Microphone and speaker use separate ownership slots, fixed mono/stereo presets,
the same bounded 4 KiB framing, finite worker grants and monotonic owner leases.
The service main loop enforces expiry even without pipe traffic. Stop affects
only the matching direction and Session/Route/Stream/epoch. Readiness is not
media receipt or rendering evidence.

The only dependency added is the existing workspace `capyio-audio` crate for
its typed media binding; no external production dependency is introduced.
Executable paths, driver mappings, pairing and credentials remain unchanged.
The desktop now owns a per-Route audio map under the authenticated control owner,
uses real AdapterManaged microphone Sink / speaker Source ports, exchanges
reciprocal offers, and obtains local and remote leases before service startup.
It reuses resource consent, finite pair-generation-scoped remembered approvals,
and five-second service owner renewal; stop/disconnect cleans up the owned child.
Native UDP policy remains restricted to the explicitly approved Windows
<LAB_IPV4_04> / Y700 <LAB_IPV4_01> lab pair. The new internal capyio-audio desktop
dependency adds no external package. Concurrent control exchanges retain audio
offers for the owning coordinator instead of dropping them.

Readiness alone does not make the desktop Route Active: speaker requires Android
received-media Active, and microphone additionally requires capture-ring produced
frame growth. This is not proof of human-audible playback or every application's
capture behavior. Service deployment, paired Android implementation and physical
concurrent-media validation must be completed before delivery claims. Existing
native UDP remains controlled-lab only and is not production encrypted transport.

Android now owns independent microphone/speaker Route jobs in AudioNodeService.
It reserves an actual UDP socket, returns a matching opposite-endpoint offer,
and starts AudioRecord/AudioTrack only after the authenticated Route Start.
Finite grants, local leases and Ready Session remain prerequisites. Runtime
Active waits for processed media frames; this is not an audible-output claim.
Stop, lost authority and Service destruction revoke the guard and close that
job's socket. The bridge supports initiation from either node; system microphone
permission remains a prerequisite and is never silently granted. ARM64 JNI and
Java compilation plus full APK/native-display packaging verification passed on
2026-09-29; simultaneous physical audio validation is still pending.
