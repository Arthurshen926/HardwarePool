# ADR 1528: Android audio catalog selection and admission boundary

Status: implemented locally; review and exact-head hosted validation pending.
Date: 2026-10-20

## Decision

Android audio discovery selects actual advertised Ports rather than constructing
Windows UUIDs. A candidate must have the correct Source/Sink direction, audio
Profile major, exact mono (microphone) or stereo (speaker) PCM format descriptor,
Interactive QoS, AdapterManaged mode, usable availability and a Ready owning
Adapter declaring that backend. Local hardware selection additionally requires
the local Microphone or Speaker class. Remote Microphone and Speaker classes
are both eligible: a Linux Speaker Sink can consume Android microphone frames.
Ambiguous Ports are rejected rather than silently selecting the first match.
Start resolves both endpoints and one Ready Session from the current snapshot;
stale catalogs, offline endpoints and ambiguous Sessions cannot create a Draft.
PermissionRequired means only that a consent request may be needed, not that any
permission is granted. Existing independent consent, paired control authentication,
finite grants, leases, exact bindings, Stop and invalidation remain unchanged.

## Compatibility is not admission

An AdapterManaged audio Profile is not an interoperable receiver contract.
The existing Windows native-audio-route receiver admits only its two fixed local
endpoints. A small legacy admission predicate therefore checks that advertised
Adapter type and exact endpoint pair after generic discovery. Those UUIDs are
never used to manufacture missing catalog entries. This is a compatibility seam,
not a new OS-based allowlist or a general contract for future receivers.

Linux currently owns a Linux-led playback proposal (ADR 1524), and does not yet
admit an incoming Android-led Draft. Its random-UUID compatible Speaker Sink is
shown through optional `unavailableDirections` with a reason, never inserted into
the actionable `directions` array. Direct JNI Start also rejects unsupported
admission before creating a Route or recording local intent. Existing consumers
that do not understand the additive field still see no actionable direction.
No Linux Source, reverse-audio or virtual microphone is introduced or advertised.
The existing Linux-led playback entry remains unchanged.

The next increment must implement an independently approved, bounded inbound
receiver and a versioned admission advertisement before enabling Android-led
Linux Start. It must retain exact Session/Route/epoch/endpoint binding, independent
local consent, bounded grant/lease negotiation, Stop and fail-closed invalidation.
Merely setting metadata or removing this admission predicate is not sufficient.

## Evidence scope

Pure selector tests cover randomized Linux identifiers, both legacy Windows
directions, incompatible descriptors, ambiguity, disconnected Sessions and stale
catalogs. Static UI/JNI regression tests check disabled unsupported candidates and
the shared Start-selection seam. These tests do not execute JNI, Android capture,
Windows services, Linux PipeWire or a physical device. Full JNI/Android-target and
frontend build validation remain separate gates; device acceptance is postponed,
not silently waived. No wire schema, credentials, platform permissions, service
declarations or media transport security is changed.
