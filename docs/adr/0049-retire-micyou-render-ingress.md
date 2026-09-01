# ADR 0049: Retire the MicYou render ingress from the native audio package

Status: accepted

Date: 2026-09-01

Supersedes: ADR 0037 for the default CapyIO-native runtime

## Context

ADR 0037 added `CapyIO Microphone Ingress` as a private Windows render
endpoint because the external MicYou process could only deliver decoded PCM to
a render device. The render APO copied that PCM into the capture ring consumed
by `CapyIO Microphone`.

The native audio subsystem no longer uses that path. The Windows native
microphone Broker receives the common CapyIO packet directly and writes the
same bounded capture ring. Keeping the ingress now exposes two render devices
under the extension parent's identical friendly name.

The shared APO also classified its role by scanning the parent device
collection and accepting the first endpoint association it recognized. With
both ingress and capture associations present, enumeration order could classify
a capture graph as the retired ingress producer. The graph then entered the
render-format lock path and WASAPI initialization failed with `E_INVALIDARG`
(`0x80070057`). The failure was observed after earlier successful capture,
which is consistent with an order-dependent selection rather than a fixed
format declaration.

## Decision

The 21.83 audio package exposes exactly one render endpoint, `CapyIO Speaker`,
and one capture endpoint, `CapyIO Microphone`.

- Remove `WaveMicIngress` and `TopologyMicIngress` from the base INF and the
  render miniport array.
- Remove the ingress FX association from the extension INF.
- Remove the APO's ingress-producer role and capture-ring producer. The APO may
  consume the capture ring for the microphone or produce the render ring for
  the speaker; network ingress remains in the user-mode Broker.
- Make repository validation reject a reintroduced ingress interface, category
  or APO producer role.
- Keep the MicYou integration only as historical compatibility evidence. It is
  not part of the default native package.

## Consequences

- Windows should present one unambiguous CapyIO virtual speaker and one CapyIO
  virtual microphone after a full signed driver upgrade and endpoint rebuild.
- Microphone transport no longer depends on an externally selected render
  device or endpoint enumeration order.
- Installing 21.83 changes the base kernel driver as well as APO/Extension and
  therefore requires the controlled-driver-lab approval, rollback record and
  post-install speaker/microphone regression evidence from ADR 0029.
- Source, x64 Release compilation, INF verification and package signability pass
  with WDK/SDK 10.0.26100.0 and MSVC 14.44.35207. Signing, installation and
  physical acceptance remain separate high-risk actions.

## Controlled-host result

The signed 21.83 package was installed on `DESKTOP-AT8EVE9` and exposes one
active CapyIO render endpoint and one active CapyIO capture endpoint. A
subsequent 21.84 float-pin experiment was rejected and rolled back to 21.83
without reboot. Interactive-session probes opened the restored microphone for
99 callbacks / 47,520 samples; active Android media produced RMS `0.00795198`
and peak `0.13494873`. This accepts the 21.83 endpoint-convergence decision.
