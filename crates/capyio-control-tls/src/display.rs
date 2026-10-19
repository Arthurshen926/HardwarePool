//! Paired-node TLS launch records for an independently authorized display Route.
//! Carries one bounded launch record and acknowledgement, never video frames.
use crate::route_tls::route_tls;
route_tls!(
    DisplayLaunchTlsBinding,
    DisplayLaunchTlsListener,
    DisplayLaunchTlsChannel,
    DisplayLaunchTlsStopHandle,
    DISPLAY_LAUNCH_TLS_ALPN,
    MAX_DISPLAY_LAUNCH_TLS_RECORD_BYTES,
    b"capyio-display-launch/1",
    b"CPYDSP01",
    "capyio.video.frames"
);
