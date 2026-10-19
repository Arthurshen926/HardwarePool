# Android paired-directory device report (ADR 1451)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Device: Lenovo TB323FU, Android 16 / API 36
- ADB endpoint used: `<LAB_IPV4_01>:35585`
- Unified debug APK: version `0.5.11-dev` / code 23
- Unified APK SHA-256:
  `17C11DF86109BE1AF149B5BA4730C75B722C42E4FDC3ED80CC00B98BB6D7B401`
- Test APK SHA-256:
  `99C17E3712260D30CB45FC53094688641E5151369DDCC0A81AB396376D2322DC`

## Executed gate

The instrumentation used an isolated no-backup test root, not the production
paired-directory path. It verified:

1. an absent directory returns canonical empty schema without creating a file;
2. a one-peer schema-1 document is validated by shared Rust code, atomically
   committed and recovered byte-for-byte in canonical form;
3. malformed replacement is rejected without changing the committed document;
4. direct stored-file corruption fails closed;
5. the isolated directory is recursively removed afterward.

The same run repeated the ADR 1450 Keystore replacement, tamper rejection,
bounds and cleanup checks and returned
`Android pairing storage device gate: PASS`. The dedicated test APK was then
uninstalled. A direct private-directory listing showed neither the isolated
`pairing-store-test-1451` root nor a credential file; the unified main app was
retained.

## Claim boundary

This proves atomic paired-directory storage and shared-schema validation on the
identified device. The ordinary Service does not load it yet, and no peer was
enrolled, authenticated or connected.
