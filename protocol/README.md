# Protocol sources

`proto/capyio/v1/` is the canonical Protobuf control-plane definition.

Compatibility rules:

- never reuse a field number;
- append optional fields within major v1;
- reserve deleted fields;
- keep public wire messages separate from internal Rust memory layout;
- validate semantic required fields after decoding;
- keep real-time audio payloads out of control envelopes.

The Rust crate uses `protoc-bin-vendored`, so contributors do not need a separately installed `protoc` for normal Cargo builds.
