# CAPY-AUDIO-001C — Speaker release qualification

Status: active backlog; not part of the completed Speaker functional Gate

Owner: unassigned

Created: 2026-08-28

Depends on: completed `CAPY-AUDIO-001B` functional Gate

## Objective

Turn the proven Windows-to-Android speaker path into a safely distributable,
upgradeable and supportable product without reopening the completed audio-path
architecture.

## Remaining work

1. Build a signed installer that owns the driver, APO, Broker, Windows service
   and desktop binaries under immutable product paths instead of the workspace.
2. Define atomic install/upgrade/rollback/uninstall behavior, retain exact
   package provenance and test cleanup of endpoints, services, processes,
   mappings, TCP/UDP ownership and Driver Store packages.
3. Qualify reboot/autostart, sign-out/sign-in, sleep/resume, Windows Audio
   service restart, endpoint disable/enable, Broker crash and receiver loss.
4. Run bounded long-duration soak, latency, dropout, jitter, CPU, memory and
   audio-quality measurements with reproducible fixtures and human evidence kept
   separate from machine evidence.
5. Replace the single-user-lab named-pipe policy with an explicit multi-user
   authorization decision and test access from interactive, service, remote and
   denied principals.
6. Add CapyIO pairing, peer authorization, authenticated encryption, replay
   protection and downgrade binding; Tailscale and the Audio Share-compatible
   private transport are not sufficient production security.
7. Decide and implement the distributable Android receiver boundary, including
   foreground-service lifecycle, audio focus/routing, lock/background behavior,
   power saving, permissions and signed APK delivery.
8. Produce release SBOM, notices, signing/provenance records, recovery guide and
   support diagnostics. Hosted CI must remain honest about tests that require a
   Windows driver lab or physical Android device.

## Exit criteria

- a clean supported Windows machine can install, use, upgrade and uninstall the
  complete speaker feature from signed packages;
- reboot, sleep, service/endpoint failure and receiver reconnect tests pass
  without destabilizing Windows Audio;
- production peer and local-user authorization are specified and verified;
- performance/quality limits and unsupported cases are documented from retained
  evidence;
- no workspace path, test certificate or lab-only trust assumption remains in
  the product configuration.
