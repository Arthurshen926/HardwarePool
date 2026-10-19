# ADR 1511: Service allocation for ordinary-user camera ingress

Status: accepted for implementation under the user's explicit gap-closing request,
2026-10-03. Physical acceptance remains pending.

ADR 1509 kept elevated direct projection as an Alpha exception. This slice replaces
that requirement on a configured installation: the SCM LocalSystem Broker creates
only the fixed Global camera mapping; an ordinary-user decoder opens and owns it.
The service never receives video, network endpoints, executable paths or keys.
The existing CurrentUser/Session Media Foundation camera remains user-owned.

An administrator configures --camera-ingress-user-sid explicitly. A separate fixed,
exclusive, remote-rejecting pipe admits that SID, administrators and SYSTEM. The
client checks its server PID against SCM's CapyIOBroker PID. Version 1 requests carry only stream identity and epoch, with no paths or sizes. The mapping has its existing fixed size/name, SYSTEM
access, LocalService read-only access and write access for the exact selected SID;
medium integrity permits the intended ordinary-user writer. No Everyone/Users ACL.

The service holds a fresh exclusive mapping only until a bounded acknowledgement.
The service initializes the fixed empty layout before replying; the client opens it
without rewriting visible headers before acknowledging. The header producer PID records
the allocator process for this path; it is diagnostic metadata, not ownership authority. Existing mappings are never
adopted or overwritten by another allocation. Receiver exit closes its handle;
normal Route stop/revoke still owns receiver and camera cleanup. Same-user arbitrary
code is outside isolation, as for the existing local Node API. No automatic UAC,
driver deployment or boot-policy change is introduced. Elevated creation remains
compatible; only access-denied/privilege-not-held creation errors try the service.

First-party dependency note: camera-host now uses windows-service's bounded native
handoff; windows-service uses camera-share allocation. Existing workspace Apache-2.0
code and pinned windows-sys are reused. No external production dependency is added.
A duplicated platform IPC implementation was rejected. Core/domain/wire unchanged.

Deployment requires the updated service, decoder and Desktop together. Old service
installations do not silently gain the new permission. Record exact service image,
configured SID and rollback before deployment. This ADR does not claim clean-machine
installation, ordinary-user delivered frames or complete media acceptance.


The controlled-lab service deployment entry now accepts explicit fresh installation
(-InstallIfMissing plus empty expected prior ImagePath), without mandatory display
configuration for Core. It records whether the service previously existed; failure
restores the old service state or deletes only the service it just created. A matching
restore entry checks the exact currently approved ImagePath before restoring/removing
the registration. It never removes pairing or application data. Driver installation,
COM deployment and production-protected packaging remain separate gates; this is not
yet a public unified installer. New-machine execution is not proven on an existing host.
