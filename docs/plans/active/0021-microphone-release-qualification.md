# CAPY-MIC-003 — Microphone release qualification

Status: active backlog; not part of the completed microphone functional slice

Owner: unassigned

Created: 2026-08-29

Depends on: completed `CAPY-MIC-001/002` controlled-lab functional slice

## Objective

Turn the proven Android-to-Windows microphone path into a safely distributable,
upgradeable and supportable product without reopening the accepted typed Route,
bounded capture-ring or stable endpoint-identity architecture.

## Remaining work

1. Build signed Windows installation/upgrade/rollback/uninstall ownership for
   the driver, APO/Extension, desktop/runtime binaries, trusted host
   configuration and narrowly scoped firewall policy.
2. Complete ADR 0040's least-privilege per-user headless owner: signed install,
   single-instance login autostart, crash recovery, hardened configuration ACL
   and physical window-close/sign-out/multi-user acceptance. The privileged
   Broker remains the global capture-ring owner; the WebView must never gain
   executable-path, endpoint-ID or arbitrary process authority.
3. Qualify clean reboot/autostart, sign-out/in, sleep/resume, Windows Audio
   restart, endpoint disable/enable, process crash, phone loss and reconnect.
4. Qualify Android lock, long background, battery optimization, foreground-
   service lifecycle, audio focus/routing and microphone permission
   revocation/regrant with explicit privacy UI.
5. Measure end-to-end latency, jitter/loss, glitches, noise floor, CPU, memory
   and long-duration stability with reproducible fixtures. Keep calibrated
   metrics distinct from subjective listening evidence.
6. Define capture volume/mute, sample-rate conversion and optional voice DSP
   behavior without coupling microphone policy to the Speaker media profile.
7. Replace Tailscale/local-lab trust with CapyIO pairing, peer authorization,
   authenticated encryption, replay protection and downgrade binding.
8. Resolve the MicYou GPL-3.0-only source patch/executable distribution and
   update obligations before any bundled Android or Windows delivery. Produce
   notices, SBOM, provenance and reproducible build records.
9. Improve the componentized Windows device/container presentation so ordinary
   users can identify the CapyIO capture endpoint without relying on the current
   localized `Microphone (CapyIO Speaker with Render Bridge)` label.
10. Run clean supported-machine and multi-user acceptance, including ordinary
    recording/meeting applications, privacy denial and complete uninstall with
    no endpoint, service, process, mapping, firewall or Driver Store residue.

## Exit criteria

- a clean supported Windows machine and supported Android device can install,
  pair, use, upgrade and uninstall the complete microphone feature from signed
  packages;
- privacy, reboot/sleep, failure/reconnect and long-soak tests pass without
  destabilizing Windows Audio or leaking microphone access;
- production peer, local-user and process authority are specified and tested;
- performance and quality limits plus unsupported application modes are backed
  by retained measurements;
- the GPL distribution decision is legally and technically implementable;
- no workspace path, test certificate, raw endpoint guess, Tailscale-only trust
  or manually retained firewall rule remains in the product workflow.
