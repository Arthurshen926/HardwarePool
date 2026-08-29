# CAPY-MIC-001H — Microphone Quick Action physical acceptance

Status: completed (controlled-lab functional Gate); release qualification moved to plan 0021

Owner: Codex and project owner

Created: 2026-08-29

Completed: 2026-08-29 in PR #14, merge commit `2145f9f`

Depends on: `CAPY-MIC-001F`, `CAPY-MIC-001G`

## Objective

Exercise the trusted-configured desktop microphone Quick Action through the
approved Android phone and installed CapyIO Windows capture endpoint. Prove
that Route state corresponds to ordinary-client PCM, that active phone loss
returns the capture path to bounded exact silence, and that explicit retry
creates a clean new receiver lifetime.

## In scope

1. Recover, rebuild and revalidate the pinned local MicYou v2.0.1 CLI patches.
2. Provision and validate the fixed trusted host configuration.
3. Restrict local-lab inbound access to the exact CLI, Tailscale IPv4 range and
   TCP 8554/UDP 8555.
4. Drive Quick Action start, stable phone presence, disconnect, retry and stop
   against the approved physical phone.
5. Observe non-zero PCM and disconnect silence through an ordinary Windows
   capture client.
6. Fail closed by stopping the receiver on active phone loss and correct the
   pinned CLI's retained-lock/PID-reuse false-positive behavior.

## Out of scope

- driver or APK installation/update, Android permission changes, Windows audio
  service changes, boot-policy changes or reboot;
- claiming Android lock/background, permission revocation, latency, quality,
  long soak or reboot survival;
- distributing the local GPL source patch or executable;
- production pairing, authentication, encryption or installer firewall policy.

## Acceptance criteria

1. Trusted configuration validates the pinned version, required capability and
   current stable endpoint without exposing private identity to the WebView.
2. The Quick Action reaches `Active` only after stable process-owned phone TCP
   presence and an ordinary capture client observes non-zero PCM.
3. Active phone loss stops and reaps the receiver, changes only the microphone
   Route to `Offline` and produces exact zero after the bounded committed-frame
   drain.
4. Explicit retry advances the Route epoch, launches a fresh receiver, reaches
   `Active`, restores non-zero PCM and then stops terminally.
5. A retained MicYou mode lock cannot confuse a newly reused Windows PID with
   the earlier receiver process.
6. Targeted tests, repository validation and full local CI pass.

## Safety

The physical run uses the already installed Android package and signed CapyIO
audio components. It may start/force-stop that package and the user-mode
receiver only. The separately approved firewall change disables two automatic
broad rules for the current CLI and adds exact-program inbound allows limited
to `100.64.0.0/10`, TCP 8554 and UDP 8555. It does not authorize any driver/APK
deployment, permission/service/boot mutation, reboot, commit, push or PR.

## Completion evidence

See `docs/CAPY_MIC_001H_REPORT.md` for retained command, PCM, lifecycle,
firewall and unresolved-risk evidence. The final ignored eight-second WAV had
non-zero samples in every one-second interval; the project owner listened to it
on 2026-08-29 and confirmed audible phone microphone audio. Exact-head hosted
Linux, macOS, Windows, repository, UI and Windows Tauri checks then passed on
PR #14 before merge commit `2145f9f`.
