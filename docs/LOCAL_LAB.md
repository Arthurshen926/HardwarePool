# CapyIO Local Lab

## Named fixture devices

- HP OmniBook Ultra Flip 14: Windows Node fixture.
- vivo X200 Pro mini: Android Node fixture.

The names in testkit are deterministic sample identities, not proof that either
physical device was connected. Each Node fixture owns both Source and Sink Ports.

## Foundation loop

Gates 0–3 require no hardware. Run Core/UI/Adapter smoke tests on the development
host and retain failures in the active plan.

## Future Windows inventory

Before native/driver work, record Windows edition/build/architecture, SDK/WDK,
CPU/RAM, network and the isolated test target. Do not change BitLocker, Secure
Boot, boot policy or Verifier without explicit approval.

## Android inventory baseline

`CAPY-IMU-001A` uses explicit-serial, read-only ADB commands. The current vivo
fixture was observed as Android 16/API 36, arm64-v8a, 1216x2640, security patch
2026-06-01, with 63 bounded SensorService inventory rows. The ignored evidence
contains no ADB address/port, serial or build fingerprint.

This inventory proves only that the authorized target was online and exposed
sensor metadata. It is not live CapyIO data-plane, SensorServer, permission,
background-lifecycle or APK evidence. Use:

```text
cargo xtask android-doctor --serial <explicit-serial>
cargo xtask android-baseline --serial <explicit-serial>
cargo xtask android-collect --serial <explicit-serial>
```

## Network evidence

Real Adapter tests record interface/link type, addresses, access point/band,
client isolation, baseline latency/jitter, firewall changes and whether an
external overlay is used. Foundation Mock Sidecars use local process pipes only.

## Driver target

Use a Hyper-V Generation 2 VM or dedicated Windows installation with snapshots,
debugging and recovery planning. The daily-development host is never a driver
test target.
