# ADR 1413: Hash-scoped camera lab deployment

Status: accepted

Date: 2026-09-16

The integrated system-camera gate needs a current COM source instead of the
historical standalone binary. The existing fixed-target lab deployment script
now accepts an explicit 64-hex SHA-256 (defaulting to its historical candidate
for backward compatibility). A read-only Preflight checks the release file and
any existing deployment without requiring elevation. It does not claim that
camera activation, privileges or media delivery work.

Mutation still requires an administrator token. Both cleanup modes verify the
fixed COM server path and deployed DLL hash before deleting anything or
restarting FrameServer. All modes reject reparse points in the fixed deployment
tree and require a 64-bit process. No alternate DLL, registry key or deployment
directory is accepted. Existing deployment overwrite remains prohibited.

The 2026-09-16 candidate was built offline from the integrated branch using
`cargo build --offline --release -p capyio-windows-camera-mf --lib`.
SHA-256: `E03BAA67CC9E5D080C282AE9B54E1F54D73E8B70993800EDFCEA4B9643D5A46D`.
Target: approved local lab host `LAB-WINDOWS-HOST`, fixed
`C:\ProgramData\CapyIO\Lab\Camera\capyio_windows_camera_mf.dll`, CLSID
`{35754be3-54b6-4133-a1c7-1716395c6f1c}`. Preflight found no existing file or
registration. No driver or boot policy is part of this deployment.

Rollback, after closing the owned camera session, is a 64-bit elevated
PowerShell invocation of `scripts/capyio-camera-virtual-lab-admin.ps1 -Action
Remove -ExpectedSha256 E03BAA67CC9E5D080C282AE9B54E1F54D73E8B70993800EDFCEA4B9643D5A46D`.
If Windows retains the DLL, report the lock instead of automatically invoking
the separately disruptive FrameServer-restart mode.
