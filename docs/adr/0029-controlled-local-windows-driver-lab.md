# ADR 0029: Permit one controlled local Windows driver lab

Status: accepted

Amends: the isolated-target-only safety boundary in ADRs 0027/0028.

## Context

The `CapyIO-DriverLab` Hyper-V guest reached Windows OOBE but did not become a
stable recoverable test target. The development host now has WDK 26100.6584,
Windows SDK 10.0.26100.0 and Visual Studio Build Tools 17.14. A pinned SysVAD
kernel library and the SwapAPO baseline have compiled locally without deploying
a driver. The human explicitly authorized controlled local deployment testing.

The identified host is `DESKTOP-AT8EVE9`, AMD64, Windows build `26200.9168`.
It is normally reached through Remote Desktop, so a driver failure can remove
the only management path. Authorization therefore does not by itself make the
host deployment-ready.

## Decision

Gate 7B may use `DESKTOP-AT8EVE9` for a bounded local driver lab only when all
of the following are retained before each first install or materially changed
package:

1. elevated WinRE, BitLocker and Secure Boot inventory plus a usable recovery
   path independent of the driver under test;
2. current audio endpoint/driver inventory and a repository commit/build
   manifest;
3. exact INF/CAT/SYS/DLL hashes, signing identity and install command;
4. an exact uninstall/rollback command and evidence that the package targets
   only CapyIO/SysVAD test identifiers;
5. explicit human approval for that target and package.

The first local action is install/enumerate/play/uninstall of an unchanged
pinned SysVAD-derived test package. Test signing, Secure Boot, BitLocker,
`bcdedit`, Driver Verifier and reboot remain separate high-risk operations and
are never implied by this ADR. Driver Verifier still defaults to the isolated
VM. No production certificate or release signing key is used.

If recovery preflight cannot be verified, deployment returns to the VM or a
physically accessible dedicated installation. Compile-only, static validation,
APO/Broker unit tests and user-mode transport tests remain safe on the host.

## Consequences

- The local host can shorten Gate 7B endpoint/APO iteration once recovery is
  proven.
- Remote-only access is an explicit risk and may block deployment even though
  the target is authorized.
- A successful local test is functional evidence, not HLK, production signing,
  upgrade or broad Windows-compatibility evidence.
- Every install must be reversible and scoped; unrelated audio devices and
  drivers are never removed or replaced.
