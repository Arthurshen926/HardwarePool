# Windows virtual display boundary

- Keep network, codecs, authentication, pairing, reconnect and Android policy
  outside this UMDF/IddCx adapter.
- Swapchain callbacks may only perform bounded acquisition and local handoff.
- Validate every version, size, stride, format, generation and capacity before
  accessing a shared frame buffer.
- Do not add driver install, removal, signing or boot-policy commands here.
- Treat successful compilation as package evidence only, not enumeration or
  device evidence.
