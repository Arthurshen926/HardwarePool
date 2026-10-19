use capyio_protocol::{negotiate_display_settings, v1::*};
use prost::Message;

fn caps() -> DisplayStreamCapabilities {
    DisplayStreamCapabilities {
        modes: vec![DisplayStreamMode {
            width: 1920,
            height: 1080,
            refresh_millihertz: 60_000,
        }],
        minimum_bitrate_kbps: 2_000,
        maximum_bitrate_kbps: 40_000,
        h264_sdr: true,
    }
}
fn selection() -> DisplayStreamSelection {
    DisplayStreamSelection {
        mode: Some(caps().modes[0]),
        bitrate_kbps: 15_000,
    }
}

#[test]
fn selection_is_exact_symmetric_and_serializable() {
    let a = caps();
    let mut b = caps();
    b.maximum_bitrate_kbps = 20_000;
    let selected = negotiate_display_settings(&a, &b, &selection()).unwrap();
    assert_eq!(
        selected,
        negotiate_display_settings(&b, &a, &selection()).unwrap()
    );
    assert_eq!(
        selected,
        DisplayStreamSelection::decode(selected.encode_to_vec().as_slice()).unwrap()
    );
}

#[test]
fn unsupported_request_is_not_silently_changed() {
    let a = caps();
    let mut request = selection();
    request.bitrate_kbps = 40_001;
    assert!(negotiate_display_settings(&a, &a, &request).is_err());
    request = selection();
    request.mode.as_mut().unwrap().refresh_millihertz = 120_000;
    assert!(negotiate_display_settings(&a, &a, &request).is_err());
    request.mode = None;
    assert!(negotiate_display_settings(&a, &a, &request).is_err());
}

#[test]
fn unsupported_or_ambiguous_capabilities_are_rejected() {
    let a = caps();
    for b in [
        DisplayStreamCapabilities::default(),
        DisplayStreamCapabilities {
            h264_sdr: false,
            ..caps()
        },
        DisplayStreamCapabilities {
            modes: vec![a.modes[0]; 65],
            ..caps()
        },
        DisplayStreamCapabilities {
            minimum_bitrate_kbps: 50_000,
            ..caps()
        },
    ] {
        assert!(negotiate_display_settings(&a, &b, &selection()).is_err());
    }
}
