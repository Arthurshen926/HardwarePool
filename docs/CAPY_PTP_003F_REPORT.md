# CAPY-PTP-003F Report — VHF local-lab test package

Date: 2026-08-31

Status: package and elevated inventory complete; deployment blocked

## Outcome

The package script validated the exact unsigned inputs, applied an embedded
test signature to the staged SYS, ran WDK 26100 Inf2Cat with zero warnings or
errors, signed the catalog and exported the public certificate. The temporary
non-exportable private key was deleted in `finally`. No driver, device, trusted
certificate or boot-policy change occurred.

Package directory:
`target/lab-packages/CapyIOVhfTouchpad-0.0.1.0-x64`.

## Exact package manifest

Signer: `CN=CapyIO Local Lab Driver Test 003F`

Thumbprint: `6D771D01DFED85EE5F4473F7449C093182F0960D`

| File | Bytes | SHA-256 |
|---|---:|---|
| `CapyIOVhfTouchpad-Test.cer` | 1068 | `81F0C6B791AB20CA74B97536206378DE70D6B6918C73E84B721141E009B4A79E` |
| `capyiovhftouchpad.cat` | 2895 | `CED8DFCE05C30E6439937863AE026C8274E4A585E0FCFD06B4F7710592F53DE8` |
| `CapyIOVhfTouchpad.inf` | 1407 | `CC034E0FE8DEA161B47DD8C6E84218419F3F62CB70DD8B5DC18BEC9E3EE47514` |
| `CapyIOVhfTouchpad.sys` | 25904 | `1F1F0BC738DAE54D5B3FE54983524C66554B33AAFEAAC4832E084F2BA8B951BC` |

CAT/SYS contain signatures, but Authenticode reports `UnknownError` because
the self-signed certificate is intentionally untrusted. The subject was absent
afterward from CurrentUser and LocalMachine My, Root and TrustedPublisher
stores. Normal Windows kernel policy therefore cannot load this package yet.

## Build identity

```text
branch: codex/capyio-touchpad
base HEAD: fc3da3636ca6c969667e71a9b596dcc944380146
worktree: dirty (132 porcelain entries at package generation)
hardware ID: Root\CapyIOVhfTouchpad
WDK tools: 10.0.26100.0
```

Exact input/output hashes make the package auditable, but the dirty worktree
means it is not reconstructible from HEAD alone. No commit was created because
committing or publishing was not authorized.

## Recovery and rollback

The first elevation request still ran under a non-administrator token. A later
explicit UAC `RunAs` invocation successfully captured the read-only inventory:

```text
WinRE: Enabled, 10.0.26100.9168
BitLocker C:: fully decrypted, protection off, no key protectors
Secure Boot: False
TPM: present, ready, enabled and activated; RestartPending=True
restore point: sequence 392, "CapyIO USBip preinstall 2026-08-30"
```

The transcript is retained at
`target/lab-evidence/CAPY-PTP-003F-recovery.txt`. This proves local recovery
components exist but cannot prove a human can reach them independently of RDP;
that physical/out-of-band access remains a human confirmation.

`scripts/remove_windows_touchpad_test_driver.ps1` is the rollback command. It
removes only instances beginning `ROOT\CAPYIOVHFTOUCHPAD\`, then removes at
most one package whose provider is exactly `CapyIO` and original INF is exactly
`CapyIOVhfTouchpad.inf`. Ambiguous matches fail without deletion. The script
was syntax-checked but not run because nothing was installed.

## Deployment blockers

The elevated inventory is now retained, but independent recovery access is not
confirmed. Loading this self-signed kernel package still requires explicit
certificate-trust and Windows test-signing decisions plus a reboot. Secure Boot
is already off, but no boot entry was queried or changed. TPM also reports a
pending restart. These remain separate high-risk approvals.
