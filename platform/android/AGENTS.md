# Android Adapter Agent Rules

- Do not add or change Android permissions or foreground-service declarations without explicit human approval.
- Microphone capture must require visible platform consent and an active, user-visible lifecycle.
- Never claim background/lock-screen behavior without a real-device test result.
- Keep Kotlin/Java DTOs narrow and versioned; do not mirror arbitrary Rust memory layouts through JNI.
- Audio callbacks must not block, log, allocate without bound, call UI code, or perform network operations.
- Record actual negotiated device parameters; do not assume a requested low-latency/exclusive mode was granted.
