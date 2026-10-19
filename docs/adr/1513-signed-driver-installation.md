# ADR 1513: Optional signed driver payload for new Windows machines

Status: Accepted for implementation; signed inputs and hardware execution pending.
Amends ADR 1512's prerequisite-only driver step at the user's explicit request.

The user has no signed packages or Hardware Dev Center account and requested code
completion plus a signing-material checklist. No live driver deployment is authorized
by this ADR. Existing Gate 7B exact-package and target approval rules still apply.

Add an optional closed audio/VHF payload manifest to the unified installer. Validate
all listed file hashes, catalog signatures and pinned catalog signer certificates
before any installation. These checks do not replace Windows driver policy or prove
WHQL certification. Windows SetupAPI/NewDev retains signature/load enforcement.
Do not import certificates or change boot/security settings. No forced downgrade.

On a new machine, preinstall the audio component/extension/base and VHF packages,
create the two fixed ROOT devices, then bind drivers. APO is an audio child, never
an independently fabricated root. Healthy existing installations are unchanged;
partial/unhealthy installations are rejected for explicit repair. Persist each step,
created instance ID, failure and reboot requirement in a protected journal. If a
step fails, remove only device instances created by this invocation. Retain staged
Driver Store packages and report that fact, because deleting them could affect other
consumers. A reboot-required result is pending, not success; never reboot automatically.

The package build accepts only a complete verified signed input set. Without it,
produce an explicitly prerequisite-only installer and a material checklist; never
mark new-machine installation or clean-machine acceptance complete. NewDev API use
adds no external runtime dependency. The native wrapper is original small user-mode
interop code, not imported DevCon sample code. No kernel code or permissions change.

References:
- https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-policy--windows-vista-and-later-
- https://learn.microsoft.com/en-us/windows/win32/api/newdev/nf-newdev-diinstalldriverw
- https://learn.microsoft.com/en-us/windows/win32/api/newdev/nf-newdev-updatedriverforplugandplaydevicesw
