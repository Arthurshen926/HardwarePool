# CapyIO Windows Camera Share

This crate owns the Windows-only decoded-frame shared-memory ABI between a
background camera producer and the Media Foundation consumer DLL. It has no COM
class factory, virtual-camera registration, codec, network or Android API.

The production object is fixed to `Global\\CapyIO.CameraIngress.v1`. One
producer owns a versioned 4,147,648-byte triple-slot latest-frame mapping and
consumers open read-only views. Stream identity, epoch, generation, canonical
720p30 NV12 metadata, publication sequence and per-slot commit markers are
validated before a frame is returned.

The optional `test-support` feature exposes only bounded `Local\\...test...`
names for cross-crate process tests. It cannot redirect production APIs to an
arbitrary global object.
