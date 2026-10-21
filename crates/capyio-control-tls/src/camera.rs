//! Dedicated pinned TLS camera data transport. One outer record contains one
//! complete bounded CAVC record; codec parsing remains adapter-owned. No fallback
//! to the plaintext lab listener is permitted.
use crate::route_tls::route_tls;
route_tls!(
    CameraTlsBinding,
    CameraTlsListener,
    CameraTlsChannel,
    CameraTlsStopHandle,
    CAMERA_TLS_ALPN,
    MAX_CAMERA_TLS_RECORD_BYTES,
    b"capyio-camera/1",
    b"CPYCAM01",
    "capyio.video.frames",
    56,
    4 * 1024 * 1024 + 56,
    u32,
    true
);
