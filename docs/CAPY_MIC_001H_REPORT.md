# CAPY-MIC-001H — Microphone Quick Action physical acceptance report

Date: 2026-08-29

Status: local functional acceptance complete; merge and release qualification pending

## Outcome

The normal trusted-configured desktop Quick Action now completes the physical
Android-to-Windows virtual-microphone lifecycle. It starts the pinned MicYou
receiver, waits for stable process-owned phone TCP presence, reaches `Active`,
delivers non-zero phone PCM to an ordinary Windows capture client, reports
phone loss as `Offline`, retries into a new Route epoch with restored PCM and
stops terminally.

Active phone loss now stops and reaps the receiver before the Route is reported
offline. This cuts off further decoded frames while preserving the capture-ring
contract: already committed frames may drain for a bounded interval, after
which the Windows endpoint returns exact digital zero.

## Pinned CLI recovery and host configuration

The official MicYou v2.0.1 source archive was reverified against the recorded
SHA-256
`606A5C6FB717F2BBAC4CD0571AD4E484E5602B20DCC8A5F76B55575EEBC13F87`.
The controlled-lab source remains outside this repository and carries the
reviewed stable-endpoint, exact-silence and per-session UDP-metrics patches.

This run also corrected MicYou's retained Windows mode lock. Upstream stores a
PID and timestamp but checks only whether that PID is live; fast Windows PID
reuse can therefore make an unrelated process block a subsequent receiver
start. The local patch verifies that the current process creation time matches
the recorded timestamp before accepting the lock. The rebuilt local-only CLI
SHA-256 is
`04C66B11ADD0EF962CB0155FBC26B75286B85F21EEADE1239D7C65CC7FC394F3`.

`capyio-micyou-config validate` passed against MicYou 2.0.1 and six current
Windows audio endpoints. The fixed `%LOCALAPPDATA%\CapyIO\host\micyou-v1.json`
configuration was used directly by the Quick Action; no environment override
was needed and no executable path or raw endpoint ID entered the WebView DTO.

## Network boundary

The two automatic broad Windows Firewall rules associated with this exact
local CLI were disabled. Two inbound allow rules remain, both limited to the
exact executable and Tailscale IPv4 range `100.64.0.0/10`:

- TCP local port 8554 for MicYou control;
- UDP local port 8555 for MicYou audio.

No other port or remote network range was added by this slice. These are
controlled-lab rules, not a production pairing or authentication design.

## Physical evidence

The approved Android package was already installed with microphone and
foreground-service permission. The host used its Tailscale address and the
phone connected over the existing authorized wireless-ADB/Tailscale setup.

The full ignored physical Quick Action test passed in 191.61 seconds through:

1. `Start` and stable `Active`;
2. phone force-stop and microphone-only `Offline`;
3. explicit `Retry` and renewed `Active`;
4. terminal `Stop`.

During the first active epoch, an ordinary CPAL capture client read 48,480
mono float32 samples at 48 kHz with RMS `0.00190148` and peak `0.00831604`.
After retry it read another 48,480 samples with RMS `0.00182441` and peak
`0.00773621`. TCP state therefore was not used as a substitute for PCM proof.

A later complete-recording acceptance used the same ordinary Windows capture
API to write an eight-second WAV from the active Quick Action. The file was a
valid RIFF/WAVE, 48 kHz mono 16-bit PCM stream containing exactly 384,000
samples. Of those, 363,621 were non-zero; float capture RMS was `0.00148812`
and peak was `0.08110046`. Independent PCM16 inspection found non-zero samples
in every one-second interval, ruling out a single startup spike as the only
signal. The local artifact SHA-256 was
`828D4FB36839A1E8CAE0DED5FD05E6A1AF72F4AB1EC6F3F4EBB94A6C0CF0AF7A`.
Because it contains raw microphone audio, the WAV remains only in the ignored
local evidence directory and must not be committed or attached to CI logs.
The project owner listened to that recording on 2026-08-29 and confirmed that
the phone microphone audio was audible. This is subjective functional evidence,
not a calibrated fidelity, latency, noise-floor or intelligibility score.
The surrounding physical lifecycle again passed `Active`, `Offline`, retried
`Active` and `Stopped` in 68.60 seconds.

An immediate one-second capture after `Offline` contained a short already
committed tail (RMS `0.00166580`, peak `0.04450989`); the next one-second
capture was exact zero. A separate continuous 25-second trace crossing the
planned disconnect recorded non-zero samples in its first four 100 ms windows
and exact-zero RMS/peak from the 400 ms window through the remainder. This is
consistent with the documented 16,384-frame/341.33 ms capture-ring capacity.
The trace is a coarse bounded-drain check, not a synchronized end-to-end
disconnect-latency benchmark.

After the mode-lock correction, two further physical lifecycle runs passed in
39.80 and 44.48 seconds, including immediate explicit retry. The final run
again reached `Active`, `Offline`, retried `Active` and `Stopped` without the
intermittent “receiver exited before listener ready” failure.

## Automated evidence

The following passed during this slice:

- five targeted `micyou_runtime` unit tests, including receiver stop on active
  phone loss, unrelated IMU Route isolation and fresh-process explicit retry;
- trusted host configuration validation against the rebuilt CLI;
- the ignored physical Quick Action lifecycle test three times after the
  fail-closed Route change, including the two runs after the mode-lock patch;
- ordinary-client active, offline and retried-active PCM probes;
- one valid eight-second ordinary-client WAV whose every one-second interval
  contained non-zero phone-microphone samples;
- project-owner listening confirmation that the retained WAV contains audible
  phone microphone audio;
- desktop all-target check, warnings-denied Clippy and 31 library tests with
  five separately authorized physical tests ignored by default;
- final `cargo xtask ci`, including workspace format/check/Clippy/tests,
  documentation/manifests, Adapter smoke, repository validation and frontend
  typecheck/production build.

The local MicYou release build completed. Its Tauri library test executable did
not terminate when invoked for the narrow mode-lock unit filter and had to be
stopped; the updated code nevertheless compiled into the release CLI and the
two subsequent end-to-end restart/retry runs passed. This harness behavior is
retained rather than reported as a passing unit test.

## System impact

- No driver or APK was installed or updated.
- No Android permission, Windows audio service, boot policy or signing policy
  changed, and no reboot occurred.
- The scoped firewall rules above remain enabled; the two automatic broad rules
  remain disabled.
- The local trusted host configuration remains provisioned.
- The physical test leaves the Android MicYou package and supervised receiver
  stopped.
- No commit, push or pull request was created.

## Remaining work

The Gate 8 functional Quick Action path is now physically accepted on the
controlled lab, but public/release qualification remains open:

1. Android screen-lock/background and audio-focus behavior;
2. microphone permission revocation and recovery;
3. normal reboot and desktop/Broker startup survival;
4. measured latency, jitter/loss/glitch behavior and longer soak;
5. production installer, firewall, authentication/encryption and headless
   Runtime ownership;
6. legal/distribution decision for the GPL source patch and executable.
