//! Private touchpad data channel, separate from control and other Profiles.
use crate::route_tls::route_tls;
route_tls!(
    TouchpadTlsBinding,
    TouchpadTlsListener,
    TouchpadTlsChannel,
    TouchpadTlsStopHandle,
    TOUCHPAD_TLS_ALPN,
    MAX_TOUCHPAD_TLS_RECORD_BYTES,
    b"capyio-touchpad/1",
    b"CPYTPD01",
    "capyio.input.touchpad-frames"
);
