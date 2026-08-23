# Security Policy

CapyIO is pre-release software that is intended to access microphones, speakers, system audio endpoints, and eventually other sensitive hardware capabilities. Do not use current builds for secrets, regulated data, or unattended production systems.

## Reporting a vulnerability

Before a public repository is created, report security issues privately to the project owner. After repository creation, configure GitHub private vulnerability reporting and replace this paragraph with the actual contact channel.

A report should include:

- affected commit or version;
- affected platform and OS build;
- threat scenario and required privileges;
- reproducible steps or a minimal proof of concept;
- expected impact;
- any proposed mitigation.

Do not include microphone recordings, tokens, certificates, device identifiers, or personal logs unless they have been sanitized.

## Security boundaries

- The Windows kernel driver is not a network trust boundary and must not parse untrusted wire data.
- Pairing is capability-scoped; granting speaker output does not grant microphone capture.
- Microphone sharing requires visible, revocable user consent on the provider device.
- Production transport must provide mutual authentication, confidentiality, integrity, replay resistance, and downgrade protection.
- Disconnect and malformed input must fail closed without destabilizing OS audio services.

See `docs/SECURITY_MODEL.md` for the detailed model and `AGENTS.md` for protected operations.
