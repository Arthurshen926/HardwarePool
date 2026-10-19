# Android Keystore credential-store device report (ADR 1450)

- Date: 2026-09-18
- Branch: `codex/capyio-integrated-alpha`
- Device: Lenovo TB323FU, Android 16 / API 36
- ADB endpoint used: `<LAB_IPV4_01>:35585`
- Unified debug APK: version `0.5.11-dev` / code 23
- Unified APK SHA-256:
  `E547A942B63C0B7816E0A19CA7D35EEBD0E6B9F75D26DB88D1BC0802FB581419`
- Test APK SHA-256:
  `9A55687CB1ADB45A74426644463FD523AC0354F8DB3B4C9F5617A258DA0CCB92`

## Executed gate

The dependency-free instrumentation entrypoint used only the dedicated handle
`android-keystore:cccc...cccc`. It verified:

1. Android Keystore AES-GCM creation and first protected write;
2. exact plaintext recovery;
3. atomic replacement and recovery of the second generation;
4. rejection after changing one ciphertext byte;
5. rejection of wrong prefixes, short values, uppercase/non-hex suffixes and a
   65,537-byte plaintext;
6. Keystore entry and ciphertext removal.

The device returned:

```text
Android credential store device gate: PASS
```

The dedicated test APK was then uninstalled. The CapyIO main application was
retained, and a direct read of its private `no_backup/pairing/credentials-v1`
directory showed no files after cleanup. No microphone, camera, display,
virtual-device or driver operation was started by this gate.

## Claim boundary

This proves the credential-storage primitive on this identified device. It does
not prove CapyIO enrollment, paired-directory persistence on Android, TLS
authentication, reconnection, catalog exchange or behavior on other devices.
