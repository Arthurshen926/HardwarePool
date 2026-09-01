# CAPY-CAMERA-001C6 camera-host composition report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Base commit: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Status: implementation and live-device Local cross-process roundtrip complete;
  controlled Global/registered roundtrip pending

## Implemented boundary

The CAVC receiver's explicit publication modes convert every decoded frame into
the existing canonical `GeneratedVideoFrame`. Stream ID/epoch, source sequence,
microsecond timestamp converted to nanoseconds, frame duration and
discontinuity are retained. `CameraProducerHost` validates and publishes the
owned packed NV12 payload.

`--publish-shared` selects only the fixed production Global mapping.
`--publish-shared-local-lab` is compiled through `lab-support` and selects only
`Local\CapyIO.CameraIngress.v1.lab`; neither mode accepts a mapping name. The
independent camera-host probe opens the corresponding read-only mapping and
requires 30 advancing frames before success.

Registered COM activation now selects a validated production Global consumer
when present. It falls back to the fixture only for `OpenFileMappingW` error 2
and fails every other mapping error. Direct in-process constructors remain
deterministic. Registered activation never opens the Local lab mapping.

## Automated evidence

- decoded-to-generated mapping preserves identity, epoch, sequence, timestamp,
  duration, discontinuity and exact 1,382,400-byte payload;
- adapter tests: nine library tests plus one receiver mapping test passed;
- camera-host lifecycle/duplicate-owner tests passed;
- camera-share cross-process tests and Media Foundation shared-sample tests
  passed in the workspace suite;
- the registered selection fallback test covers file-not-found, access-denied
  and invalid-layout classification;
- `cargo xtask ci` passed after the complete C6 publisher, Local lab feature,
  independent probe and registered-selection changes.

## Mapping preflight evidence

A CAVC golden config was sent over Windows loopback without a key frame. In
Local lab mode the receiver created the mapping and the independent probe
reported:

```text
CAPYIO_CAMERA_SHARED_PROBE_OPEN scope=local-lab
stream=00010203-0405-0607-0809-0a0b0c0d0e0f epoch=2
```

The probe then correctly timed out with zero frames, and the receiver rejected
the session because it contained no key frame. Both processes exited and their
mapping handles were released.

The equivalent production preflight failed with:

```text
Windows CreateFileMappingW failed with error 5
```

The Global probe observed file-not-found. This confirms the ordinary Adapter
must not own the cross-session production mapping; a privileged camera-host or
service must own it under a separate authorization and recovery plan.

## Live physical Local evidence

The explicitly selected wireless endpoint `100.66.157.119:36275` reported the
same authorized `V2419A` / `PD2419` device. The installed lab application retained
its CAMERA and INTERNET grants; no APK installation or permission mutation was
performed. An exact `adb reverse tcp:38173 tcp:38173` carried the Camera2 /
MediaCodec Annex-B stream into the release receiver.

In one continuous run, the receiver accepted 628 access units, including 23 key
frames and four marked discontinuities, through source sequence 671. Media
Foundation decoded 517 changing 1280x720 frames (714,700,800 packed NV12 bytes)
and the camera host published all 517 to the fixed Local lab mapping. The first
and last receiver checksums were `09b7a2f31f928251` and `46872baad9f14a2d`.

While the producer remained live, the separately launched read-only process
reported:

```text
CAPYIO_CAMERA_SHARED_PROBE_OPEN scope=local-lab stream=303db070-b6c3-223a-14cb-c9a26b1f78dd epoch=8095040389746
CAPYIO_CAMERA_SHARED_PROBE_OK scope=local-lab stream=303db070-b6c3-223a-14cb-c9a26b1f78dd epoch=8095040389746 observed_frames=30 discontinuities=4 first_sequence=28 last_sequence=112 first_checksum=e6cbbef51d37c0a9 last_checksum=a36484d4d874d600
```

This proves phone AVC capture reaches Windows, is decoded to NV12, is published
through the camera-host mapping, and is consumed as advancing image buffers by
an independent Windows process. It does not prove semantic image content or
Windows registered-camera visibility. Cleanup force-stopped the app, removed
the reverse mapping, left Camera Service with `Active Camera Clients: []`, and
left no receiver or probe process. The Global registered-camera roundtrip
remains a separate privileged system operation requiring exact approval and
rollback.
