use std::collections::BTreeSet;

use capyio_core::StreamId;
use capyio_display::{
    DeterministicDisplayFixture, DisplayContractError, DisplayFrameDescriptor, DisplayFrameFlags,
    DisplaySequenceGap, DisplaySinkCapabilities, DisplaySourceCapabilities,
    DisplayStreamCapabilities, DisplayStreamSpec, DisplayWorkflow, LatestDisplayFrameSlot,
    LatestFramePushOutcome, OwnedDisplayFrame, display_frames_profile, negotiate_display_stream,
};
use capyio_video::{FrameRate, VideoColorimetry, VideoPixelFormat};

fn small_fixture_spec() -> DisplayStreamSpec {
    DisplayStreamSpec {
        width: 64,
        height: 36,
        ..DisplayStreamSpec::fixture_720p30_bgra()
    }
}

fn source(
    workflows: impl IntoIterator<Item = DisplayWorkflow>,
    candidates: Vec<DisplayStreamSpec>,
) -> DisplaySourceCapabilities {
    DisplaySourceCapabilities {
        workflows: workflows.into_iter().collect::<BTreeSet<_>>(),
        streams: DisplayStreamCapabilities::new(candidates).expect("source streams"),
    }
}

fn sink(candidates: Vec<DisplayStreamSpec>) -> DisplaySinkCapabilities {
    DisplaySinkCapabilities {
        streams: DisplayStreamCapabilities::new(candidates).expect("sink streams"),
    }
}

#[test]
fn virtual_monitor_and_capture_fallback_share_one_exact_sink_contract() {
    assert_eq!(display_frames_profile().name, "capyio.display.frames");
    assert_eq!(display_frames_profile().major, 1);

    let selected = small_fixture_spec();
    selected.validate().expect("display stream");
    let format = selected.format_descriptor();
    format.validate().expect("Core descriptor");
    assert_eq!(format.id, "packed-raw-display-v1");
    assert_eq!(format.parameters["cursor_mode"], "composited");

    let source = source(
        [
            DisplayWorkflow::VirtualMonitor,
            DisplayWorkflow::ExistingDisplayCapture,
        ],
        vec![selected.clone()],
    );
    let sink = sink(vec![selected.clone()]);
    assert_eq!(
        negotiate_display_stream(&source, &sink, DisplayWorkflow::VirtualMonitor)
            .expect("virtual monitor"),
        selected
    );
    assert_eq!(
        negotiate_display_stream(&source, &sink, DisplayWorkflow::ExistingDisplayCapture)
            .expect("capture fallback"),
        selected
    );
}

#[test]
fn negotiation_rejects_unsupported_workflow_and_implicit_conversion() {
    let selected = small_fixture_spec();
    let virtual_monitor_only = source([DisplayWorkflow::VirtualMonitor], vec![selected.clone()]);
    let exact_sink = sink(vec![selected.clone()]);
    assert_eq!(
        negotiate_display_stream(
            &virtual_monitor_only,
            &exact_sink,
            DisplayWorkflow::ExistingDisplayCapture
        ),
        Err(DisplayContractError::UnsupportedWorkflow)
    );

    let mut resized = selected;
    resized.width = 128;
    let resized_sink = sink(vec![resized]);
    assert_eq!(
        negotiate_display_stream(
            &virtual_monitor_only,
            &resized_sink,
            DisplayWorkflow::VirtualMonitor
        ),
        Err(DisplayContractError::NoCompatibleDisplayStream)
    );
}

#[test]
fn mode_inventory_and_raw_display_bounds_fail_closed() {
    assert_eq!(
        DisplayStreamCapabilities::new(Vec::new()),
        Err(DisplayContractError::EmptyStreamCandidates)
    );
    let selected = small_fixture_spec();
    assert_eq!(
        DisplayStreamCapabilities::new(vec![selected.clone(), selected.clone()]),
        Err(DisplayContractError::DuplicateStreamCandidate)
    );
    assert!(matches!(
        DisplayStreamCapabilities::new(vec![selected; 33]),
        Err(DisplayContractError::TooManyStreamCandidates {
            actual: 33,
            limit: 32
        })
    ));

    let empty_workflows = DisplaySourceCapabilities {
        workflows: BTreeSet::new(),
        streams: DisplayStreamCapabilities::new(vec![small_fixture_spec()]).unwrap(),
    };
    assert_eq!(
        empty_workflows.validate(),
        Err(DisplayContractError::EmptySourceWorkflows)
    );

    let mut odd_nv12 = small_fixture_spec();
    odd_nv12.width = 63;
    odd_nv12.pixel_format = VideoPixelFormat::Nv12;
    odd_nv12.colorimetry = VideoColorimetry::Bt709Limited;
    assert!(matches!(
        odd_nv12.validate(),
        Err(DisplayContractError::InvalidStreamSpec(_))
    ));

    let mut no_drop = small_fixture_spec();
    no_drop.allow_frame_drop = false;
    assert!(matches!(
        no_drop.validate(),
        Err(DisplayContractError::InvalidStreamSpec(_))
    ));

    let mut excessive_rate = small_fixture_spec();
    excessive_rate.frame_rate = FrameRate::new(481, 1).expect("rate");
    assert!(matches!(
        excessive_rate.validate(),
        Err(DisplayContractError::InvalidStreamSpec(_))
    ));
}

#[test]
fn frames_distinguish_payload_clear_and_end_of_stream() {
    let selected = small_fixture_spec();
    let stream_id = StreamId::new();
    let payload_bytes = selected.packed_frame_bytes().expect("frame bytes");
    let normal = DisplayFrameDescriptor {
        stream_id,
        stream_epoch: 1,
        sequence: 0,
        source_timestamp_nanos: 10,
        duration_nanos: 33_333_333,
        payload_bytes,
        flags: DisplayFrameFlags::default(),
    };
    normal.validate(&selected).expect("normal frame");

    let clear = DisplayFrameDescriptor {
        payload_bytes: 0,
        flags: DisplayFrameFlags {
            discontinuity: true,
            end_of_stream: false,
            clear_surface: true,
        },
        ..normal.clone()
    };
    clear.validate(&selected).expect("clear surface");

    let invalid_terminal = DisplayFrameDescriptor {
        flags: DisplayFrameFlags {
            discontinuity: false,
            end_of_stream: true,
            clear_surface: true,
        },
        ..clear
    };
    assert!(matches!(
        invalid_terminal.validate(&selected),
        Err(DisplayContractError::InvalidFrame(_))
    ));

    let wrong_payload = OwnedDisplayFrame {
        descriptor: normal,
        payload: vec![0; usize::try_from(payload_bytes - 1).expect("test size")],
    };
    assert!(matches!(
        wrong_payload.validate(&selected),
        Err(DisplayContractError::InvalidFrame(_))
    ));
}

#[test]
fn latest_slot_replaces_pending_frames_and_tracks_gaps_and_epochs() {
    let selected = small_fixture_spec();
    let stream_id = StreamId::new();
    let mut fixture =
        DeterministicDisplayFixture::new(selected.clone(), stream_id, 1, 1_000).unwrap();
    let first = fixture.next_frame().unwrap();
    let mut second = fixture.next_frame().unwrap();
    let mut slot = LatestDisplayFrameSlot::new(selected.clone(), stream_id, 1).unwrap();

    assert_eq!(
        slot.push(first).unwrap(),
        LatestFramePushOutcome::Stored {
            gap: None,
            replaced_pending: false
        }
    );
    second.descriptor.sequence = 2;
    assert_eq!(
        slot.push(second).unwrap(),
        LatestFramePushOutcome::Stored {
            gap: Some(DisplaySequenceGap {
                first_missing: 1,
                last_missing: 1
            }),
            replaced_pending: true
        }
    );
    assert_eq!(slot.replacements(), 1);
    assert_eq!(slot.take().unwrap().descriptor.sequence, 2);
    assert!(!slot.has_pending());

    let duplicate = fixture.next_frame().unwrap();
    assert_eq!(
        slot.push(duplicate).unwrap(),
        LatestFramePushOutcome::Duplicate { sequence: 2 }
    );
    let mut late = fixture.next_frame().unwrap();
    late.descriptor.sequence = 1;
    assert_eq!(
        slot.push(late).unwrap(),
        LatestFramePushOutcome::Late {
            expected: 3,
            actual: 1
        }
    );

    let mut future = fixture.next_frame().unwrap();
    future.descriptor.stream_epoch = 2;
    assert_eq!(
        slot.push(future).unwrap(),
        LatestFramePushOutcome::FutureEpoch {
            current: 1,
            actual: 2
        }
    );
    assert!(!slot.advance_epoch(2).unwrap());
    assert_eq!(slot.epoch(), 2);
    let mut current = fixture.next_frame().unwrap();
    current.descriptor.stream_epoch = 2;
    assert_eq!(
        slot.push(current).unwrap(),
        LatestFramePushOutcome::Stored {
            gap: None,
            replaced_pending: false
        }
    );
    assert!(slot.advance_epoch(3).unwrap());
    assert!(!slot.has_pending());
}

#[test]
fn latest_slot_rejects_wrong_stream_and_stale_epoch() {
    let selected = small_fixture_spec();
    let stream_id = StreamId::new();
    let other_stream = StreamId::new();
    let mut slot = LatestDisplayFrameSlot::new(selected.clone(), stream_id, 2).unwrap();
    let mut fixture = DeterministicDisplayFixture::new(selected, other_stream, 1, 0).unwrap();
    let wrong = fixture.next_frame().unwrap();
    assert_eq!(
        slot.push(wrong).unwrap(),
        LatestFramePushOutcome::WrongStream
    );

    let mut stale = fixture.next_frame().unwrap();
    stale.descriptor.stream_id = stream_id;
    assert_eq!(
        slot.push(stale).unwrap(),
        LatestFramePushOutcome::StaleEpoch {
            current: 2,
            actual: 1
        }
    );
    assert_eq!(
        slot.advance_epoch(2),
        Err(DisplayContractError::NonAdvancingEpoch {
            current_epoch: 2,
            new_epoch: 2
        })
    );
}

#[test]
fn latest_slot_reconfigures_only_with_a_valid_mode_and_newer_epoch() {
    let selected = small_fixture_spec();
    let stream_id = StreamId::new();
    let mut fixture = DeterministicDisplayFixture::new(selected.clone(), stream_id, 1, 0).unwrap();
    let mut slot = LatestDisplayFrameSlot::new(selected.clone(), stream_id, 1).unwrap();
    slot.push(fixture.next_frame().unwrap()).unwrap();

    let mut resized = selected;
    resized.width = 128;
    assert!(slot.reconfigure(resized.clone(), 2).unwrap());
    assert_eq!(slot.epoch(), 2);
    assert!(!slot.has_pending());

    let mut new_fixture = DeterministicDisplayFixture::new(resized, stream_id, 2, 0).unwrap();
    assert_eq!(
        slot.push(new_fixture.next_frame().unwrap()).unwrap(),
        LatestFramePushOutcome::Stored {
            gap: None,
            replaced_pending: false
        }
    );
    assert_eq!(
        slot.reconfigure(small_fixture_spec(), 2),
        Err(DisplayContractError::NonAdvancingEpoch {
            current_epoch: 2,
            new_epoch: 2
        })
    );
}

#[test]
fn deterministic_fixture_is_reproducible_and_contains_no_screen_capture() {
    let selected = small_fixture_spec();
    let stream_id = StreamId::new();
    let mut left = DeterministicDisplayFixture::new(selected.clone(), stream_id, 1, 42).unwrap();
    let mut right = DeterministicDisplayFixture::new(selected, stream_id, 1, 42).unwrap();
    let left_frame = left.next_frame().unwrap();
    let right_frame = right.next_frame().unwrap();
    assert_eq!(left_frame, right_frame);
    assert_eq!(left_frame.descriptor.sequence, 0);
    assert!(
        left_frame
            .payload
            .chunks_exact(4)
            .all(|pixel| pixel[3] == 255)
    );
}
