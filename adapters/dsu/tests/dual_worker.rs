use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use capyio_data_plane::{DataEnvelope, ImuSampleV1, parse_imu_fixture_jsonl};
use capyio_dsu_adapter::{
    DSU_PROTOCOL_VERSION, DsuControlsMapping, DsuImuWorker, DsuImuWorkerConfig, DsuImuWorkerStats,
    DsuLoopbackConfig, DsuMotionMapping, DsuNeutralOutcome, DsuSubmitOutcome, DsuWorkerError,
    MAX_DSU_WORKER_QUEUE_CAPACITY, crc32_ieee, encode_pad_data, project_imu_envelope,
};
use capyio_input::{
    DpadState, GamepadButton, GamepadButtons, GamepadControlUpdate, GamepadControls, GamepadState,
    GamepadStateComposer, GamepadStick, GamepadTrigger, InputFrameHeader, SignedAxis, StickState,
    TriggerValue,
};

const MESSAGE_PAD_DATA: u32 = 0x10_0002;
const SERVER_ID: u32 = 0x0102_0304;
const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn pad_request(client_id: u32) -> Vec<u8> {
    let mut packet = vec![0_u8; 28];
    packet[..4].copy_from_slice(b"DSUC");
    write_u16(&mut packet, 4, DSU_PROTOCOL_VERSION);
    write_u16(&mut packet, 6, 12);
    write_u32(&mut packet, 12, client_id);
    write_u32(&mut packet, 16, MESSAGE_PAD_DATA);
    packet[20] = 1;
    packet[21] = 0;
    let checksum = crc32_ieee(&packet);
    write_u32(&mut packet, 8, checksum);
    packet
}

fn test_config() -> DsuImuWorkerConfig {
    DsuImuWorkerConfig {
        queue_capacity: 16,
        poll_interval: Duration::from_millis(1),
        motion_mapping: DsuMotionMapping::identity(),
        controls_mapping: DsuControlsMapping::identity(),
        ..DsuImuWorkerConfig::new(DsuLoopbackConfig::local_lab(0, SERVER_ID))
    }
}

fn controls_anchor() -> GamepadState {
    state(0, GamepadControls::neutral())
}

fn state(sequence: u64, controls: GamepadControls) -> GamepadState {
    GamepadState {
        header: InputFrameHeader {
            stream_id: "00000000-0000-4000-8000-00000000c002".parse().unwrap(),
            stream_epoch: 7,
            sequence,
            source_timestamp_nanos: 3_000_000_000_u64.saturating_add(sequence),
        },
        controls,
    }
}

fn pressed(button: GamepadButton) -> GamepadControls {
    GamepadControls {
        buttons: GamepadButtons::empty().with(button),
        dpad: DpadState { x: 0, y: 0 },
        left_stick: StickState {
            x: SignedAxis::new(12_345).unwrap(),
            y: SignedAxis::new(-23_456).unwrap(),
        },
        right_stick: StickState::default(),
        left_trigger: TriggerValue::new(0),
        right_trigger: TriggerValue::new(40_000),
    }
}

fn wait_for_stats(
    worker: &DsuImuWorker,
    predicate: impl Fn(DsuImuWorkerStats) -> bool,
    message: &str,
) -> DsuImuWorkerStats {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let stats = worker.stats();
        if predicate(stats) {
            return stats;
        }
        assert!(Instant::now() < deadline, "{message}: {stats:?}");
        thread::yield_now();
    }
}

fn subscribe(worker: &DsuImuWorker) -> UdpSocket {
    let client = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .send_to(&pad_request(7), worker.local_address())
        .unwrap();
    wait_for_stats(
        worker,
        |stats| stats.subscriptions_added == 1,
        "worker did not register subscriber",
    );
    client
}

fn receive_packet(client: &UdpSocket) -> [u8; 100] {
    let mut packet = [0_u8; 100];
    let (received, source) = client.recv_from(&mut packet).unwrap();
    assert_eq!(received, packet.len());
    assert!(source.ip().is_loopback());
    packet
}

fn assert_no_packet(client: &UdpSocket) {
    client.set_nonblocking(true).unwrap();
    let mut packet = [0_u8; 100];
    let error = client.recv_from(&mut packet).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    client.set_nonblocking(false).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
}

fn motion_fixture() -> Vec<DataEnvelope<ImuSampleV1>> {
    parse_imu_fixture_jsonl(FIXTURE, 6).unwrap()
}

#[test]
fn controls_cache_until_motion_and_either_stream_update_publishes_latest_pair() {
    let motion = motion_fixture();
    let anchor = controls_anchor();
    let mut worker = DsuImuWorker::start_with_controls(test_config(), &motion[0], &anchor).unwrap();
    let motion_sender = worker.sender();
    let controls_sender = worker.controls_sender().unwrap();
    let client = subscribe(&worker);
    let south = pressed(GamepadButton::South);
    let east = pressed(GamepadButton::East);

    assert_eq!(
        controls_sender.try_submit(state(0, south)),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.controls_cached_without_motion == 1,
        "controls were not cached",
    );
    assert_no_packet(&client);

    assert_eq!(
        motion_sender.try_submit(motion[0].clone()),
        DsuSubmitOutcome::Submitted
    );
    let projected_0 = project_imu_envelope(&motion[0], DsuMotionMapping::identity()).unwrap();
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            0,
            projected_0,
            south,
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );

    assert_eq!(
        controls_sender.try_submit(state(1, east)),
        DsuSubmitOutcome::Submitted
    );
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            1,
            projected_0,
            east,
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );

    assert_eq!(
        motion_sender.try_submit(motion[1].clone()),
        DsuSubmitOutcome::Submitted
    );
    let projected_1 = project_imu_envelope(&motion[1], DsuMotionMapping::identity()).unwrap();
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            2,
            projected_1,
            east,
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );

    let stats = wait_for_stats(
        &worker,
        |stats| stats.samples_accepted == 2 && stats.controls_accepted == 2,
        "both streams were not accepted",
    );
    assert_eq!(stats.motion_packets_sent, 3);
    assert_eq!(stats.controls_gaps, 0);
    worker.stop().unwrap();
    let neutral = receive_packet(&client);
    assert_eq!(
        neutral,
        encode_pad_data(
            SERVER_ID,
            0,
            3,
            projected_1,
            GamepadControls::neutral(),
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );
}

#[test]
fn controls_gap_publishes_neutral_before_accepting_later_snapshot() {
    let motion = motion_fixture();
    let anchor = controls_anchor();
    let mut worker = DsuImuWorker::start_with_controls(test_config(), &motion[0], &anchor).unwrap();
    let client = subscribe(&worker);
    let motion_sender = worker.sender();
    let controls_sender = worker.controls_sender().unwrap();
    let south = pressed(GamepadButton::South);
    let east = pressed(GamepadButton::East);

    assert_eq!(
        controls_sender.try_submit(state(0, south)),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.controls_cached_without_motion == 1,
        "controls were not cached",
    );
    assert_eq!(
        motion_sender.try_submit(motion[0].clone()),
        DsuSubmitOutcome::Submitted
    );
    let _baseline = receive_packet(&client);

    assert_eq!(
        controls_sender.try_submit(state(2, east)),
        DsuSubmitOutcome::Submitted
    );
    let projected = project_imu_envelope(&motion[0], DsuMotionMapping::identity()).unwrap();
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            1,
            projected,
            GamepadControls::neutral(),
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            2,
            projected,
            east,
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );

    let stats = wait_for_stats(
        &worker,
        |stats| stats.controls_accepted == 2,
        "gapped controls were not accepted after reset",
    );
    assert_eq!(stats.controls_gaps, 1);
    assert_eq!(stats.controls_missing_sequences, 1);
    assert_eq!(stats.controls_neutral_resets, 1);
    assert_eq!(stats.controls_neutral_packets_sent, 1);
    worker.stop().unwrap();
    let _stop_neutral = receive_packet(&client);
}

#[test]
fn explicit_neutral_request_discards_old_generation_and_new_snapshot_recovers() {
    let motion = motion_fixture();
    let anchor = controls_anchor();
    let mut worker = DsuImuWorker::start_with_controls(test_config(), &motion[0], &anchor).unwrap();
    let client = subscribe(&worker);
    let motion_sender = worker.sender();
    let controls_sender = worker.controls_sender().unwrap();

    assert_eq!(
        controls_sender.try_submit(state(0, pressed(GamepadButton::South))),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.controls_cached_without_motion == 1,
        "controls were not cached",
    );
    assert_eq!(
        motion_sender.try_submit(motion[0].clone()),
        DsuSubmitOutcome::Submitted
    );
    let _baseline = receive_packet(&client);

    assert_eq!(
        controls_sender.request_neutral(),
        DsuNeutralOutcome::Requested
    );
    let projected = project_imu_envelope(&motion[0], DsuMotionMapping::identity()).unwrap();
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            1,
            projected,
            GamepadControls::neutral(),
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );

    assert_eq!(
        controls_sender.try_submit(state(1, pressed(GamepadButton::East))),
        DsuSubmitOutcome::Submitted
    );
    let recovered = receive_packet(&client);
    assert_eq!(recovered[37] & 0x40, 0x40);
    assert_eq!(recovered[32..36], 2_u32.to_le_bytes());
    worker.stop().unwrap();
    let _stop_neutral = receive_packet(&client);
    assert_eq!(
        controls_sender.request_neutral(),
        DsuNeutralOutcome::Stopped
    );
}

#[test]
fn stop_sends_exactly_one_neutral_packet_and_invalid_anchor_fails_before_start() {
    let motion = motion_fixture();
    let mut invalid_anchor = controls_anchor();
    invalid_anchor.header.stream_epoch = 0;
    assert!(matches!(
        DsuImuWorker::start_with_controls(test_config(), &motion[0], &invalid_anchor),
        Err(DsuWorkerError::InputContract(_))
    ));
    let unsupported_anchor = state(
        0,
        GamepadControls {
            buttons: GamepadButtons::empty().with(GamepadButton::Paddle1),
            ..GamepadControls::neutral()
        },
    );
    assert!(matches!(
        DsuImuWorker::start_with_controls(test_config(), &motion[0], &unsupported_anchor),
        Err(DsuWorkerError::ControlsProjection(_))
    ));
    for controls_queue_capacity in [0, MAX_DSU_WORKER_QUEUE_CAPACITY + 1] {
        assert!(matches!(
            DsuImuWorker::start_with_controls(
                DsuImuWorkerConfig {
                    controls_queue_capacity,
                    ..test_config()
                },
                &motion[0],
                &controls_anchor(),
            ),
            Err(DsuWorkerError::InvalidControlsQueueCapacity { .. })
        ));
    }

    let anchor = controls_anchor();
    let mut worker = DsuImuWorker::start_with_controls(test_config(), &motion[0], &anchor).unwrap();
    let address = worker.local_address();
    let client = subscribe(&worker);
    let controls_sender = worker.controls_sender().unwrap();
    assert_eq!(
        controls_sender.try_submit(state(0, pressed(GamepadButton::South))),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.controls_cached_without_motion == 1,
        "controls were not cached",
    );
    assert_eq!(
        worker.sender().try_submit(motion[0].clone()),
        DsuSubmitOutcome::Submitted
    );
    let _baseline = receive_packet(&client);

    worker.stop().unwrap();
    let neutral = receive_packet(&client);
    let projected = project_imu_envelope(&motion[0], DsuMotionMapping::identity()).unwrap();
    assert_eq!(
        neutral,
        encode_pad_data(
            SERVER_ID,
            0,
            1,
            projected,
            GamepadControls::neutral(),
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );
    assert_eq!(worker.stats().controls_stop_neutral_packets, 1);
    worker.stop().unwrap();
    assert_no_packet(&client);
    assert_eq!(
        controls_sender.try_submit(state(1, GamepadControls::neutral())),
        DsuSubmitOutcome::Stopped
    );
    drop(client);
    let rebound = UdpSocket::bind(address).expect("stopped worker must release its UDP port");
    drop(rebound);
}

#[test]
fn motion_and_controls_stream_guards_are_independent() {
    let mut motion = motion_fixture();
    motion[0].stream_epoch = 3;
    motion[0].sequence = 100;
    let mut controls_anchor = controls_anchor();
    controls_anchor.header.sequence = 10;
    let worker =
        DsuImuWorker::start_with_controls(test_config(), &motion[0], &controls_anchor).unwrap();
    let motion_sender = worker.sender();
    let controls_sender = worker.controls_sender().unwrap();

    assert_eq!(
        motion_sender.try_submit(motion[0].clone()),
        DsuSubmitOutcome::Submitted
    );
    let mut motion_gap = motion[0].clone();
    motion_gap.sequence = 102;
    assert_eq!(
        motion_sender.try_submit(motion_gap.clone()),
        DsuSubmitOutcome::Submitted
    );
    let mut wrong_motion = motion_gap.clone();
    wrong_motion.stream_id = "00000000-0000-4000-8000-00000000dead".parse().unwrap();
    wrong_motion.sequence = 103;
    let mut stale_motion = motion_gap.clone();
    stale_motion.stream_epoch = 2;
    stale_motion.sequence = 103;
    let mut future_motion = motion_gap.clone();
    future_motion.stream_epoch = 4;
    future_motion.sequence = 103;
    for rejected in [wrong_motion, stale_motion, future_motion, motion_gap] {
        assert_eq!(
            motion_sender.try_submit(rejected),
            DsuSubmitOutcome::Submitted
        );
    }

    let south = pressed(GamepadButton::South);
    assert_eq!(
        controls_sender.try_submit(state(10, south)),
        DsuSubmitOutcome::Submitted
    );
    let mut wrong_controls = state(11, south);
    wrong_controls.header.stream_id = "00000000-0000-4000-8000-00000000beef".parse().unwrap();
    let mut stale_controls = state(11, south);
    stale_controls.header.stream_epoch = 6;
    let mut future_controls = state(11, south);
    future_controls.header.stream_epoch = 8;
    for rejected in [
        wrong_controls,
        stale_controls,
        future_controls,
        state(10, south),
    ] {
        assert_eq!(
            controls_sender.try_submit(rejected),
            DsuSubmitOutcome::Submitted
        );
    }
    assert_eq!(
        controls_sender.try_submit(state(12, pressed(GamepadButton::East))),
        DsuSubmitOutcome::Submitted
    );

    let stats = wait_for_stats(
        &worker,
        |stats| {
            stats.samples_accepted == 2
                && stats.wrong_stream_samples == 1
                && stats.stale_epoch_samples == 1
                && stats.future_epoch_samples == 1
                && stats.late_samples == 1
                && stats.controls_accepted == 2
                && stats.wrong_stream_controls == 1
                && stats.stale_epoch_controls == 1
                && stats.future_epoch_controls == 1
                && stats.late_controls == 1
        },
        "independent stream guards did not classify every input",
    );
    assert_eq!(stats.input_gaps, 1);
    assert_eq!(stats.missing_sequences, 1);
    assert_eq!(stats.controls_gaps, 1);
    assert_eq!(stats.controls_missing_sequences, 1);
    assert_eq!(stats.transport_failures, 0);
}

#[test]
fn invalid_and_unsupported_controls_neutralize_then_valid_sequence_recovers() {
    let motion = motion_fixture();
    let anchor = controls_anchor();
    let mut worker = DsuImuWorker::start_with_controls(test_config(), &motion[0], &anchor).unwrap();
    let client = subscribe(&worker);
    let controls_sender = worker.controls_sender().unwrap();
    let south = pressed(GamepadButton::South);
    assert_eq!(
        controls_sender.try_submit(state(0, south)),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.controls_cached_without_motion == 1,
        "controls were not cached",
    );
    assert_eq!(
        worker.sender().try_submit(motion[0].clone()),
        DsuSubmitOutcome::Submitted
    );
    let _baseline = receive_packet(&client);

    let invalid = GamepadControls {
        dpad: DpadState { x: 2, y: 0 },
        ..south
    };
    let mut foreign_invalid = state(1, invalid);
    foreign_invalid.header.stream_id = "00000000-0000-4000-8000-00000000beef".parse().unwrap();
    assert_eq!(
        controls_sender.try_submit(foreign_invalid),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.wrong_stream_controls == 1,
        "foreign invalid controls were not classified by stream first",
    );
    assert_eq!(worker.stats().invalid_controls, 0);
    assert_no_packet(&client);

    assert_eq!(
        controls_sender.try_submit(state(1, invalid)),
        DsuSubmitOutcome::Submitted
    );
    let projected = project_imu_envelope(&motion[0], DsuMotionMapping::identity()).unwrap();
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            1,
            projected,
            GamepadControls::neutral(),
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );

    let unsupported = GamepadControls {
        buttons: GamepadButtons::empty().with(GamepadButton::Paddle1),
        ..GamepadControls::neutral()
    };
    assert_eq!(
        controls_sender.try_submit(state(1, unsupported)),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.invalid_controls == 1 && stats.unsupported_controls == 1,
        "invalid controls were not classified",
    );
    assert_no_packet(&client);

    let east = pressed(GamepadButton::East);
    assert_eq!(
        controls_sender.try_submit(state(1, east)),
        DsuSubmitOutcome::Submitted
    );
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            2,
            projected,
            east,
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );
    assert_eq!(worker.stats().transport_failures, 0);
    worker.stop().unwrap();
    let _stop_neutral = receive_packet(&client);
}

#[test]
fn controls_sequence_exhaustion_latches_neutral_until_new_worker_epoch() {
    let motion = motion_fixture();
    let anchor = state(u64::MAX, GamepadControls::neutral());
    let worker = DsuImuWorker::start_with_controls(test_config(), &motion[0], &anchor).unwrap();
    let controls_sender = worker.controls_sender().unwrap();
    assert_eq!(
        controls_sender.try_submit(state(u64::MAX, pressed(GamepadButton::South),)),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.controls_accepted == 1,
        "terminal sequence was not accepted",
    );
    assert_eq!(
        controls_sender.try_submit(state(u64::MAX, pressed(GamepadButton::East),)),
        DsuSubmitOutcome::Submitted
    );
    let stats = wait_for_stats(
        &worker,
        |stats| stats.exhausted_controls == 1 && stats.controls_neutral_resets == 1,
        "sequence exhaustion was not classified",
    );
    assert_eq!(stats.controls_neutral_resets, 1);
    assert_eq!(stats.controls_accepted, 1);
}

#[test]
fn semantic_composer_snapshots_join_imu_and_cross_dsu_loopback() {
    let motion = motion_fixture();
    let mut composer = GamepadStateComposer::new(
        "00000000-0000-4000-8000-00000000c002".parse().unwrap(),
        7,
        0,
    )
    .unwrap();
    let anchor = composer.anchor(9).unwrap();
    let mut worker = DsuImuWorker::start_with_controls(test_config(), &motion[0], &anchor).unwrap();
    let client = subscribe(&worker);
    let controls_sender = worker.controls_sender().unwrap();

    let pressed = composer
        .apply(
            GamepadControlUpdate::Button {
                button: GamepadButton::South,
                pressed: true,
            },
            10,
        )
        .unwrap();
    assert_eq!(
        controls_sender.try_submit(pressed),
        DsuSubmitOutcome::Submitted
    );
    let stick_state = StickState {
        x: SignedAxis::new(20_000).unwrap(),
        y: SignedAxis::new(-10_000).unwrap(),
    };
    assert_eq!(
        controls_sender.try_submit(
            composer
                .apply(
                    GamepadControlUpdate::Stick {
                        stick: GamepadStick::Left,
                        state: stick_state,
                    },
                    11,
                )
                .unwrap(),
        ),
        DsuSubmitOutcome::Submitted
    );
    let composed = composer
        .apply(
            GamepadControlUpdate::Trigger {
                trigger: GamepadTrigger::Right,
                value: TriggerValue::new(50_000),
            },
            12,
        )
        .unwrap();
    assert_eq!(
        controls_sender.try_submit(composed),
        DsuSubmitOutcome::Submitted
    );
    wait_for_stats(
        &worker,
        |stats| stats.controls_accepted == 3 && stats.controls_cached_without_motion == 3,
        "composed controls were not cached before motion",
    );
    assert_no_packet(&client);

    let expected_full = GamepadControls {
        buttons: GamepadButtons::empty().with(GamepadButton::South),
        left_stick: stick_state,
        right_trigger: TriggerValue::new(50_000),
        ..GamepadControls::neutral()
    };
    assert_eq!(composed.controls, expected_full);

    assert_eq!(
        worker.sender().try_submit(motion[0].clone()),
        DsuSubmitOutcome::Submitted
    );
    let projected = project_imu_envelope(&motion[0], DsuMotionMapping::identity()).unwrap();
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            0,
            projected,
            expected_full,
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );

    let released = composer
        .apply(
            GamepadControlUpdate::Button {
                button: GamepadButton::South,
                pressed: false,
            },
            13,
        )
        .unwrap();
    assert_eq!(
        controls_sender.try_submit(released),
        DsuSubmitOutcome::Submitted
    );
    let expected_released = GamepadControls {
        buttons: GamepadButtons::empty(),
        ..expected_full
    };
    assert_eq!(released.controls, expected_released);
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            1,
            projected,
            expected_released,
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );

    let reset = composer.apply(GamepadControlUpdate::Reset, 14).unwrap();
    assert_eq!(
        controls_sender.try_submit(reset),
        DsuSubmitOutcome::Submitted
    );
    assert_eq!(
        receive_packet(&client),
        encode_pad_data(
            SERVER_ID,
            0,
            2,
            projected,
            GamepadControls::neutral(),
            DsuControlsMapping::identity(),
        )
        .unwrap()
    );
    worker.stop().unwrap();
    assert_no_packet(&client);
}
