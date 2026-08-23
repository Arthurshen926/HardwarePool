# Windows Broker Agent Rules

- Keep all network, protocol, codec, discovery and reconnect logic in user mode.
- Do not make the GUI process the only owner of active streams; design for a background runtime/service.
- Validate driver IPC version, generation and bounds before accessing buffers.
- No command may install a driver or modify boot configuration without explicit approval.
- Keep platform code outside `capyio-core` and behind an explicit Adapter/port boundary.
