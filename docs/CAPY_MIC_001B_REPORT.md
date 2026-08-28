# CAPY-MIC-001B paired projection and capture-ring report

Date: 2026-08-28

Status: source/build complete; not deployed or physically validated

## Outcome

CapyIO now owns the source-level Windows projection needed by the pinned MicYou
process boundary:

- `CapyIO Microphone Ingress` is a separate render miniport that MicYou can
  select without consuming the existing Speaker route;
- `Global\\CapyIO.CaptureRing.v1` is a service-owned, versioned 48 kHz mono
  float32 SPSC frame FIFO;
- the shared MFX class selects ingress-producer or capture-consumer behavior
  from the locked graph channel count;
- the capture path replaces SysVAD input with ring frames and zero-fills every
  underrun, including complete Broker absence;
- the service exposes bounded production, consumption, drop, underrun and APO
  attach diagnostics.

No driver, APK or MicYou executable was installed or started in this slice.
The result is therefore not yet evidence that ordinary applications can record
phone audio.

## Real-time bounds

- capacity: 16,384 mono frames (65,536 payload bytes, 341.33 ms maximum);
- producer: complete-callback commit or complete-callback drop;
- consumer: bounded copy plus zero-fill, with no wait for missing frames;
- callback work: atomics, float copy/downmix and zero-fill only;
- mapping creation/open, ACL work and diagnostics reporting remain outside the
  callback.

## Verification

- `cargo test -p capyio-windows-service`: 9 passed;
- `cargo clippy -p capyio-windows-service --all-targets -- -D warnings`: passed;
- `cargo xtask check`: passed;
- WDK x64 Release `SwapAPO.vcxproj`, `/W4 /WX`, `SignMode=Off`: passed;
- WDK x64 Release package compile/link and Universal API validation: passed;
- package Signability: zero errors and zero warnings;
- native x64 InfVerif `/u`: exit 0 for all three INFs;
- native x64 InfVerif `/w`: exit 0 for all three INFs.

The known WDK managed-task defect still reports three failures loading its
relative `x86\\InfVerif.dll`; the independently installed x64 verifier passes.

## Remaining work

1. Add a deterministic user-mode/WASAPI lab producer and record the result from
   an ordinary application before using a phone.
2. Build or obtain the exact pinned MicYou v2.0.1 CLI and configure its exact
   output device as `CapyIO Microphone Ingress`.
3. Deploy the next exact driver package only after a separate approval and
   recorded rollback target.
4. Install the MicYou APK only for the physical Android-to-Windows lab, then
   retain disconnect-to-silence and latency/drop evidence.
5. Replace channel-count role selection with explicit endpoint initialization
   metadata if future endpoint formats make the roles ambiguous.
