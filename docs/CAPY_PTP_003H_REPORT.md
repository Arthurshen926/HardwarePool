# CAPY-PTP-003H Report — corrected VHF local-lab package

Date: 2026-08-31

Status: package ready; deployment requires exact-package approval

## Outcome

The VHF lower-filter correction builds as a Universal KMDF x64 driver with WDK
code analysis enabled, zero warnings and zero errors. ApiValidator, InfVerif
`/w`, the 214-byte descriptor validator and Inf2Cat all pass. No driver or
certificate from this package has been installed and Windows was not restarted.

Package directory:
`target/lab-packages/CapyIOVhfTouchpad-0.0.2.0-x64`.

## Exact package manifest

Signer: `CN=CapyIO Local Lab Driver Test 003H`

Thumbprint: `D6EF151680FAC70FDF34623DEC23041D76372D7E`

| File | Bytes | SHA-256 |
|---|---:|---|
| `CapyIOVhfTouchpad-Test.cer` | 1068 | `2FC60146855B8831FD567E9821B741BC66931AE761B25BD41BC293048B6F9DE7` |
| `capyiovhftouchpad.cat` | 2895 | `8A539E9619ED9C4036BFAD11778CD8B78B77D54567415AA1C080CDF34DB13C7D` |
| `CapyIOVhfTouchpad.inf` | 1492 | `F6886A1C6535B91D4D886B17C2B2A7245006BCBCBA9F359C9731AAC45CA9E02F` |
| `CapyIOVhfTouchpad.sys` | 25904 | `A1380349DCC42FDB654D5A9B3A212A72D61A105443FDF1AE7AE7917F39096F10` |

The unsigned build inputs were hash-pinned. The temporary non-exportable
CurrentUser private key was deleted after export; the public certificate has
not been imported into LocalMachine Root or TrustedPublisher.

## Deployment boundary

`scripts/install_windows_touchpad_003h_test_driver.ps1` verifies every package
hash, certificate thumbprint, current `testsigning Yes` state and disabled
Secure Boot. It refuses to change BCD, refuses a requested restart, requires the
device to reach PnP status OK, and invokes the exact 003H rollback on failure.

`scripts/remove_windows_touchpad_003h_test_driver.ps1` is scoped to the CapyIO
hardware ID, provider, original INF, version 0.0.2.0 and certificate thumbprint.
The scripts were syntax-checked but neither deployment script has been run.
