# CapyIO Windows Camera Host

This crate is the headless lifecycle owner for the decoded-camera producer
mapping. It can start one production mapping, publish already-decoded and
validated canonical frames, report bounded state and release the mapping
deterministically.

It intentionally contains no Android Camera2 code, codec, network listener,
COM class factory, virtual-camera registration or UI lifecycle. A later camera
Adapter/decoder can call this owner after Route authorization without making
the portable Runtime or the audio-specific `CapyIOBroker` depend on camera
platform code.

With compile-time `lab-support`, the same owner may create exactly
`Local\CapyIO.CameraIngress.v1.lab` for current-session integration evidence.
No caller-controlled mapping name is exposed. The registered COM source never
opens that lab name; it accepts only the protected Global production mapping.
