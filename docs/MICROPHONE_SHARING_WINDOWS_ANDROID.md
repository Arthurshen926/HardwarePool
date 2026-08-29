# Android microphone sharing on Windows

Status: controlled-lab functional workflow; not a public installation guide

This guide describes the microphone path that has been physically accepted on
the identified CapyIO lab host. It assumes the CapyIO audio package, the
approved MicYou Android package, the separately built pinned MicYou CLI and the
trusted host configuration already exist. It does not authorize or automate a
driver/APK install, permission change or firewall change.

## What the path does

Android microphone audio crosses MicYou's private TCP/UDP media path into the
private Windows `CapyIO Microphone Ingress`. The bounded CapyIO audio bridge
then exposes it through an ordinary Windows capture endpoint. Recording and
meeting applications consume that endpoint like another microphone.

The phone and Windows node remain peers: this Quick Action creates one typed
Android microphone Source to Windows microphone Sink Route. It does not start,
stop or authorize the Speaker or IMU Routes.

## One-time lab prerequisites

- The signed CapyIO controlled-lab audio package is already installed and its
  endpoints are healthy.
- The approved MicYou Android package has visible microphone permission and is
  allowed to run its foreground service.
- The pinned, locally patched MicYou v2.0.1 CLI exists outside this repository.
  CapyIO does not distribute that GPL-3.0-only executable.
- `%LOCALAPPDATA%\CapyIO\host\micyou-v1.json` has been provisioned with the
  phone-reachable Windows address and exact stable private-ingress endpoint ID.
- The lab firewall admits only the reviewed program/network/port scope. The
  current evidence uses Tailscale `100.64.0.0/10`, TCP 8554 and UDP 8555.

Validate the current executable and endpoint before normal use:

```powershell
cargo run -p capyio-micyou-host-config --bin capyio-micyou-config -- validate
```

Validation is read-only. If it fails, do not select a visually similar endpoint
by guesswork: Windows endpoint names can be localized or duplicated.

## Start sharing

1. Put the phone and PC on a mutually reachable trusted network and unlock the
   phone.
2. From the repository root, start the desktop host:

   ```powershell
   corepack pnpm tauri dev
   ```

3. In **Quick Actions**, find **将手机麦克风用作电脑麦克风** and press **启动**.
4. In Android MicYou, connect to the IP address and port shown after **手机端**
   on the card. The controlled-lab default transport port is TCP 8554; audio
   uses UDP 8555.
5. Wait until the card reports `Active`. This state requires stable
   process-owned phone TCP presence; it is not itself an audio-level meter.
6. In Windows Sound Recorder, a meeting application or another ordinary
   capture client, select the CapyIO microphone input and begin recording.

Windows may display the current componentized lab package as localized
`Microphone (CapyIO Speaker with Render Bridge)` / `麦克风 (CapyIO Speaker with
Render Bridge)` instead of the shorter product pin name `CapyIO Microphone`.
This known device-container naming limitation does not merge the capture and
Speaker audio paths. The Quick Action's trusted configuration selects the
private ingress by stable endpoint ID, not by this display string.

## Stop and recover

- Press **停止** in the CapyIO Quick Action when finished. CapyIO stops and
  reaps the supervised receiver and makes the Route terminally stopped.
- If the phone disappears, the card changes to `Offline`, the receiver is
  stopped and the endpoint drains at most its bounded committed tail before
  returning exact digital silence.
- Restore phone reachability, then press **重试**. A successful retry starts a
  new receiver process and advances the Route epoch.
- Closing the Tauri host invokes the same microphone shutdown path; do not use
  force termination as the normal stop operation.

## Troubleshooting

### The card is `blocked`

Run the host-config `validate` command above. The file may be missing, the
external CLI may have changed, or the configured stable ingress endpoint may
no longer exist. Private executable paths and endpoint IDs intentionally do
not appear in the WebView.

### The card remains `starting`

Check that Android MicYou is connected to the exact address shown on the card,
that the phone can reach the PC, and that the foreground service still owns
microphone permission. `Active` is deliberately withheld until the phone TCP
connection is stable.

### The card is `active`, but an application records silence

Confirm that the application selected the CapyIO capture endpoint rather than
the physical/RDP microphone. Windows privacy settings must allow that
application to use microphones. Reproduce first with Windows Sound Recorder;
application-specific DSP, exclusive mode or conferencing policy can otherwise
obscure the base path.

### Disconnect leaves a brief tail

This is expected only within the fixed 16,384-frame capture ring, approximately
341.33 ms at 48 kHz. New capture clients synchronize to the current producer
position and must not replay an old disconnected backlog.

## Current acceptance and limits

The controlled lab has passed Quick Action start, ordinary-client non-zero PCM,
an eight-second audible WAV, phone-loss `Offline`, bounded return to exact
silence, explicit retry with a new process/epoch and terminal stop. The project
owner listened to the retained local recording on 2026-08-29 and confirmed that
phone microphone audio was audible.

This is functional acceptance, not release qualification. Android lock and
long-background behavior, permission revocation, reboot/autostart, latency and
soak, production pairing/encryption, signed installation and the MicYou GPL
distribution decision remain open in the microphone release plan.
