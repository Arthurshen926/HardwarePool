# Windows Audio Driver Agent Rules

These rules override less specific repository instructions for this directory.

## Absolute boundaries

- Never install or remove a driver unless the human has approved the exact isolated target.
- Never run `bcdedit`, `verifier`, `pnputil`, `devcon`, `signtool`, or alter Secure Boot/BitLocker without explicit approval.
- The daily-development Windows installation is not a test target.
- Do not copy third-party sample code until its license and resulting distribution obligations are recorded.
- Driver code must not link network, TLS, Protobuf, JSON, codec, UI, database, or auto-update libraries.
- All externally supplied sizes, offsets, versions and operation codes must be validated before use.
- Prefer fixed-size structures and bounded queues. Never trust user-mode pointers or lengths.
- Broker absence, network loss and phone disconnect must degrade to silence/drop behavior; they must not hang the Windows audio service.

## Required evidence for every driver change

- target Windows build and architecture;
- WDK/SDK/MSVC versions;
- build configuration and command;
- install/upgrade/uninstall result;
- endpoint enumeration result;
- playback/capture smoke result;
- Driver Verifier scope and result, when applicable;
- crash dump or Event Log evidence for failures.
