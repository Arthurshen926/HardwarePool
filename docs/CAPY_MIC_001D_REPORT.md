# CAPY-MIC-001D — Windows capture baseline and local ring closure

Date: 2026-08-29

Status: Android physical path and disconnect-to-silence proven; release qualification pending

## Outcome

The approved `DESKTOP-AT8EVE9` lab now has a usable application-facing
`CapyIO Microphone` endpoint and a proven local data path from the dedicated
render ingress through `Global\\CapyIO.CaptureRing.v1` into an ordinary CPAL
capture client.

The approved Android device also completed the physical path. The pinned
MicYou v2.0.1 Debug APK captured its microphone, sent the private MicYou stream
over Tailscale, and the separately built patched MicYou CLI decoded it into the
CapyIO ingress endpoint. An ordinary Windows capture client then received
non-zero phone audio. Disconnecting the phone and opening a fresh capture
session produced exact digital silence rather than replaying retained speech.

The investigation also corrected a false negative in the earlier evidence.
Capture clients launched by the Codex filesystem sandbox run as a separate
medium-integrity account and returned `0x80070057` for every tested capture
device, including a physical USB microphone. The same probes launched in the
interactive high-integrity `arthu` session opened both the physical microphone
and `CapyIO Microphone` successfully. Driver acceptance therefore uses the
interactive-session result and treats sandbox capture failures as a harness
permission limitation, not a device result.

## Driver/APO corrections

- `21.79.0.1` restored the pinned Microsoft SysVAD microphone processing-mode
  table and `KSDATARANGE_ATTRIBUTES` contract before the valid user-session
  comparison was run.
- `21.80.0.1` re-enabled the bounded capture APO on the proven SysVAD baseline.
- The first `21.80` probe reached the capture APO format callback but exposed a
  COM contract error: the exact-format `S_OK` branch did not return an
  `IAudioMediaType` through `ppSupportedOutputFormat`.
- `21.81.0.1` returns and references the accepted requested format. The APO and
  Extension were updated with the audio services stopped and restarted; no
  system reboot was requested or performed.
- `21.82.0.1` changes capture-consumer attachment from backlog consumption to
  live-view semantics. After validating the mapping, the non-real-time attach
  path atomically advances the read sequence to the current write sequence.
  This discards only frames produced before the new application opened; the
  real-time callback remains bounded copy/zero-fill plus atomics.
- The exact signed `21.82.0.1` APO and Extension packages were hot-deployed as
  `oem169.inf` and `oem170.inf`. Only those two INF files were installed. The
  base kernel driver was not replaced, Windows audio services were stopped and
  restored, and no reboot was requested or performed.

## Retained local evidence

The high-integrity comparison opened the physical USB capture endpoint for 98
callbacks / 94,080 samples and `CapyIO Microphone` for 99 callbacks / 47,520
mono samples.

With no ingress producer, the `21.81` capture probe received 47,520 samples
with RMS `0.00000000` and peak `0.00000000`. The capture ring reported a
successful consumer attach, proving that the SysVAD synthetic input was
replaced by bounded silence rather than exposed to the application.

The deterministic local closure selected the second freshly enumerated CapyIO
render endpoint as Microphone Ingress, submitted a 997 Hz stereo float signal,
and recorded:

- 216,000 capture samples;
- 191,519 non-zero samples;
- mean absolute amplitude `0.141116`;
- peak `0.250000`;
- one successful producer attach and four cumulative successful consumer
  attaches;
- process exit `0` and `bridge_signal=PASS`.

Both CapyIO render endpoints still share the same localized Windows friendly
name. The initial physical test therefore used a freshly enumerated one-based
device index plus the expected bounded name. CAPY-MIC-001E subsequently
replaced the persisted index with a stable Windows endpoint ID and retains the
index only as a freshly resolved, double-validated launch coordinate.

## Physical Android evidence

The authorized Android target remained paired at
`100.66.157.119:36087`. Package `com.lanrhyme.micyou` version `2.0.1`
(`versionCode 25`) was installed from the locally built pinned source. Its APK
SHA-256 is
`3E9491CA3F0AF88B8D26F30E01577F6123EB2EABDE77BBA7361D1A434BE2950F`.
The user granted microphone and notification permissions through Android.

After the Windows audio-service reload, the two indistinguishable CapyIO
render names exchanged enumeration order. The adapter probe detected the live
private ingress at one-based index 5 rather than assuming the earlier index 6.
MicYou connected from `100.66.157.119`, negotiated TCP control on 8554 and UDP
audio on 8555, emitted changing microphone levels, and began each new session
with `0.00%` loss. Observed steady-state loss remained a bounded fraction near
`0.2%` to `0.3%`, rather than the earlier value that had accidentally been
multiplied by 100 twice.

An ordinary high-integrity CPAL client opened the application-facing capture
endpoint at mono float32/48 kHz and recorded 99 callbacks / 47,520 samples with
RMS `0.01841975` and peak `0.10452271`. Capture-ring diagnostics reported four
cumulative successful producer attaches, eleven cumulative successful
consumer attaches and `last_error = 0`.

The phone was then explicitly disconnected. Before the next application
opened, diagnostics proved that the ring still held 16,320 frames
(`write_sequence = 525600`, `read_sequence = 509280`). A new ordinary capture
session on `21.82` recorded 99 callbacks / 47,520 samples with RMS
`0.00000000` and peak `0.00000000`; the consumer attach count advanced to 12
with `last_error = 0`. This is direct evidence that the new attach discarded
the old backlog and exposed only the current disconnected-state silence.

The local MicYou build has two additional reviewed lab-only corrections:

- exact zero-valued float samples remain exact zero during int16 quantization;
  non-zero samples retain the upstream TPDF dither path;
- packet-loss tracking is reset per connection epoch, stored as a fraction in
  `[0, 1]`, uses received-plus-lost as its denominator and does not move the
  sequence watermark backwards for reordered packets.

## Commands and checks

- x64 Release WDK build of `SwapAPO.vcxproj` and
  `TabletAudioSample.vcxproj`: compile/link/API validation passed with `/W4
  /WX`;
- x64 `InfVerif.exe /u` and `/w`: passed;
- the known WDK managed-build `x86\\InfVerif.dll` loader defect remains visible
  and is not the independent INF result;
- MicYou `cargo test -p micyou-audio --lib`: 20 passed;
- MicYou `cargo test -p micyou-protocol`: 3 passed;
- `cargo test -p capyio-windows-service`: 9 passed;
- `cargo xtask ci`: passed, including format, workspace check, Clippy with
  warnings denied, workspace tests, deterministic demo, repository validation,
  Adapter smoke, desktop typecheck and production UI build;
- `cargo xtask validate-docs`: passed;
- `cargo xtask validate-manifests`: passed;
- `Audiosrv`, `AudioEndpointBuilder` and `CapyIOBroker` were running after the
  hot update.

## Package identity and remaining work

The retained signed package is
`artifacts/windows-audio/21.82-microphone-fresh-attach-signed`. Its signing
certificate thumbprint is
`7353E729B92ACD148BB0046FB254E976A4762FEF`; the catalog SHA-256 is
`3218C5210BFA853735B1E09401C248AF06CE0BED97998CBCCAF4F6408B89A827`
and the APO DLL SHA-256 is
`8101F12D084540F24FE1E0EA31AE4DD046E118A423699BC8AB726A791B56F6EA`.
`package-manifest.json`, signing log and hot-install transcript retain the
complete file set and deployment evidence.

The functional Android-to-Windows microphone path is now proven, but the Gate
is not a distributable product release. Remaining qualification work is:

1. verify package/service behavior after a later normal reboot without forcing
   an extra reboot solely for this test;
2. exercise Android screen-off, secure-lock, background, permission-revocation
   and reconnect behavior;
3. integrate the completed stable endpoint identity into product-facing
   Runtime/UI lifecycle control without exposing the raw Windows ID;
4. decide legal/release handling for the reviewed GPL patches and independently
   installed APK/CLI;
5. qualify latency, sustained loss, glitch behavior and DSP policy under
   controlled speech and network conditions.

Stable endpoint selection and its independent physical regression are recorded
in `CAPY_MIC_001E_REPORT.md`.
