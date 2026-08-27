# ADR 0028: Use a bounded render APO bridge for CapyIO Speaker

Status: accepted

Supersedes: the Gate 7B data-path decision in ADR 0027. ADR 0027 still owns
the dedicated-endpoint product decision. ADR 0029 amends its target-safety
boundary with one controlled local-lab exception.

## Context

ADR 0027 assumed that a minimal SysVAD-derived render endpoint could expose
the application's real render PCM through standard WASAPI loopback. Source
review invalidated that assumption: Microsoft's SysVAD README states that the
sample does not implement mixing and simulates capture and loopback with a
synthetic tone. Building or installing the sample can prove toolchain and
endpoint enumeration, but cannot prove the CapyIO media path.

Two third-party candidates were also reviewed at fixed revisions without
importing or executing their source:

- VirtualDrivers/Virtual-Audio-Driver exposes speaker and microphone endpoints,
  but its render stream is discarded by default (or written through a debug
  kernel file path when enabled), while its capture stream is filled with
  silence. It has no supported user-mode PCM boundary. The reviewed signed
  release archive also omits the third-party notices present on its current
  main branch.
- Scream intercepts real render PCM and passes it through bounded staging, but
  then transmits from the kernel through WSK or IVSHMEM. Its PCM tap is useful
  design evidence; its kernel networking conflicts with CapyIO's driver
  boundary and is rejected.

Microsoft's Audio Processing Object (APO) contract provides a user-mode,
endpoint-associated processing point whose real-time `APOProcess` callback
receives audio buffers. A stream effect (SFX) can have one instance per input
stream and therefore cannot safely be the single producer of CapyIO's SPSC
ring. The endpoint effect (EFX) runs after endpoint mixing and exposes exactly
the final stream that the virtual speaker must forward. That is the suitable
boundary, provided the callback obeys strict real-time constraints.

## Decision

Gate 7B uses this first functional path:

```text
Windows application
  -> CapyIO Speaker WaveRT render endpoint
  -> CapyIO post-mix endpoint EFX APO
  -> pre-opened bounded shared-memory/SPSC staging ring
  -> CapyIO Broker (user mode)
  -> existing Audio Share transport
  -> Android speaker
```

The single post-mix EFX callback performs only a bounded, non-blocking copy into preallocated
storage and counter updates. It must not allocate, wait, open files, access the
network, emit ordinary logs or execute protocol/codec/reconnect policy. When
the Broker is absent or the ring is full, the callback drops the block and
increments a bounded diagnostic counter.

The Gate 7B lab ABI is implemented as Broker-owned
`Global\\CapyIO.RenderRing.v1`: 32 fixed slots, a 16 KiB per-block maximum and a
generation-tagged 48 kHz stereo float32 epoch. The Broker polls outside the
real-time path, converts to bounded S16LE and feeds the Adapter-managed Android
transport. Exact layout and memory ordering are recorded in
`drivers/windows-audio/IPC_CONTRACT.md`. This remains a lab ABI rather than a
production-frozen compatibility promise. A small versioned driver IPC is the
fallback only if deployment evidence shows that the APO boundary cannot meet
lifecycle, latency, packaging or Windows certification needs.

The unchanged pinned SysVAD build remains useful for the B1 toolchain and
enumeration baseline. It is not accepted as real-PCM evidence. Third-party
candidate code is research reference only; no source or binary is imported.

## Consequences

- The driver remains free of networking, identity, protocol, codec, UI and
  reconnect logic.
- The Broker continues to own the existing Android transport and failure
  policy.
- Gate 7B needs an APO packaging and real-time safety spike in addition to the
  driver enumeration baseline.
- Passing WASAPI loopback tests against an ordinary physical endpoint does not
  validate the virtual endpoint data path.
- Windows 10 and Windows 11 APO packaging/certification differences must be
  recorded before distribution work.
- Microphone projection may later share the package, but remains Gate 8 scope.

## Reviewed evidence

- Microsoft SysVAD, revision
  `717778a20ba4dd2440fe609f69153a1f8a64f597`, MS-PL.
- VirtualDrivers/Virtual-Audio-Driver current main
  `bb34fba15faf569a6ae9bdea360bc1cf4821354e`; reviewed signed release `25.7.14`
  at `191d307c858cb7c2749bc849060849d2dac18d3b`.
- Scream revision `d789743c248b11d1df7e5ecc546b1bc60b90cd91`, MS-PL.

Exact archive hashes, import state and review findings are recorded in
`third_party/THIRD_PARTY.yml`.

## Sources

- SysVAD README:
  <https://github.com/microsoft/Windows-driver-samples/blob/main/audio/sysvad/README.md>
- Microsoft APO implementation guidance:
  <https://learn.microsoft.com/windows-hardware/drivers/audio/implementing-audio-processing-objects>
- `IAudioProcessingObjectRT::APOProcess`:
  <https://learn.microsoft.com/windows/win32/api/audioenginebaseapo/nf-audioenginebaseapo-iaudioprocessingobjectrt-apoprocess>
- Windows 11 APO requirements:
  <https://learn.microsoft.com/windows-hardware/drivers/audio/windows-11-apis-for-audio-processing-objects>
- APO deployment guidance:
  <https://learn.microsoft.com/windows-hardware/drivers/dashboard/deploying-audio-processing-objects>
- VirtualDrivers/Virtual-Audio-Driver:
  <https://github.com/VirtualDrivers/Virtual-Audio-Driver>
- Scream:
  <https://github.com/duncanthrax/scream>
