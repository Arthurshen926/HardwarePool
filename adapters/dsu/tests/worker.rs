use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use capyio_data_plane::{DataPlaneError, parse_imu_fixture_jsonl};
use capyio_dsu_adapter::{
    DSU_PROTOCOL_VERSION, DsuImuWorker, DsuImuWorkerConfig, DsuImuWorkerStats, DsuLoopbackConfig,
    DsuSubmitOutcome, DsuWorkerError, MAX_DSU_WORKER_POLL_INTERVAL, MAX_DSU_WORKER_QUEUE_CAPACITY,
    crc32_ieee,
};

const MESSAGE_PAD_DATA: u32 = 0x10_0002;
const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
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
        ..DsuImuWorkerConfig::new(DsuLoopbackConfig::local_lab(0, 0x0102_0304))
    }
}

fn wait_for_stats(
    worker: &DsuImuWorker,
    predicate: impl Fn(DsuImuWorkerStats) -> bool,
    message: &str,
) -> DsuImuWorkerStats {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let stats = worker.stats();
        if predicate(stats) {
            return stats;
        }
        assert!(Instant::now() < deadline, "{message}: {stats:?}");
        thread::yield_now();
    }
}

#[test]
fn fixture_crosses_bounded_worker_and_reaches_loopback_subscriber() {
    let envelopes = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap();
    let mut worker = DsuImuWorker::start(test_config(), &envelopes[0]).unwrap();
    let sender = worker.sender();
    let address = worker.local_address();
    let client = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();

    client.send_to(&pad_request(7), address).unwrap();
    wait_for_stats(
        &worker,
        |stats| stats.subscriptions_added == 1,
        "worker did not add DSU subscriber",
    );

    for envelope in envelopes {
        assert_eq!(sender.try_submit(envelope), DsuSubmitOutcome::Submitted);
    }

    for packet_number in 0..6_u32 {
        let mut packet = [0_u8; 100];
        let (received, source) = client.recv_from(&mut packet).unwrap();
        assert_eq!(received, packet.len());
        assert!(source.ip().is_loopback());
        assert_eq!(&packet[..4], b"DSUS");
        assert_eq!(read_u32(&packet, 16), MESSAGE_PAD_DATA);
        assert_eq!(read_u32(&packet, 32), packet_number);
    }

    let stats = wait_for_stats(
        &worker,
        |stats| stats.samples_accepted == 6 && stats.motion_packets_sent == 6,
        "worker did not publish every IMU sample",
    );
    assert_eq!(stats.samples_submitted, 6);
    assert_eq!(stats.queue_full, 0);
    assert_eq!(stats.projection_errors, 0);
    assert_eq!(stats.transport_failures, 0);

    worker.stop().unwrap();
    worker.stop().unwrap();
    assert!(worker.stats().stopped);
    assert_eq!(
        sender.try_submit(parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0)),
        DsuSubmitOutcome::Stopped
    );
    let rebound = UdpSocket::bind(address).expect("stopped worker must release its UDP port");
    drop(rebound);
}

#[test]
fn stream_epoch_sequence_and_profile_failures_are_isolated_and_counted() {
    let mut envelopes = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap();
    let mut anchor = envelopes.remove(0);
    anchor.stream_epoch = 2;
    anchor.sequence = 0;
    let worker = DsuImuWorker::start(test_config(), &anchor).unwrap();
    let sender = worker.sender();

    assert_eq!(
        sender.try_submit(anchor.clone()),
        DsuSubmitOutcome::Submitted
    );

    let mut gap = anchor.clone();
    gap.sequence = 2;
    assert_eq!(sender.try_submit(gap.clone()), DsuSubmitOutcome::Submitted);
    assert_eq!(sender.try_submit(gap), DsuSubmitOutcome::Submitted);

    let mut wrong_stream = anchor.clone();
    wrong_stream.stream_id = "00000000-0000-4000-8000-00000000beef".parse().unwrap();
    wrong_stream.sequence = 3;
    assert_eq!(sender.try_submit(wrong_stream), DsuSubmitOutcome::Submitted);

    let mut stale_epoch = anchor.clone();
    stale_epoch.stream_epoch = 1;
    stale_epoch.sequence = 3;
    assert_eq!(sender.try_submit(stale_epoch), DsuSubmitOutcome::Submitted);

    let mut future_epoch = anchor.clone();
    future_epoch.stream_epoch = 3;
    future_epoch.sequence = 3;
    assert_eq!(sender.try_submit(future_epoch), DsuSubmitOutcome::Submitted);

    let mut invalid_profile = anchor;
    invalid_profile.profile.name = "capyio.invalid.imu".to_owned();
    invalid_profile.sequence = 3;
    assert_eq!(
        sender.try_submit(invalid_profile),
        DsuSubmitOutcome::Submitted
    );

    let stats = wait_for_stats(
        &worker,
        |stats| {
            stats.samples_accepted == 2
                && stats.late_samples == 1
                && stats.wrong_stream_samples == 1
                && stats.stale_epoch_samples == 1
                && stats.future_epoch_samples == 1
                && stats.invalid_envelopes == 1
        },
        "worker did not classify every rejected envelope",
    );
    assert_eq!(stats.samples_submitted, 7);
    assert_eq!(stats.input_gaps, 1);
    assert_eq!(stats.missing_sequences, 1);
    assert_eq!(stats.transport_failures, 0);
    assert!(!stats.stopped);
}

#[test]
fn invalid_anchor_and_worker_bounds_fail_before_thread_start() {
    let mut anchor = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    anchor.stream_epoch = 0;
    assert!(matches!(
        DsuImuWorker::start(test_config(), &anchor),
        Err(DsuWorkerError::InputQueue(DataPlaneError::InvalidEpoch))
    ));

    anchor.stream_epoch = 1;
    for config in [
        DsuImuWorkerConfig {
            queue_capacity: 0,
            ..test_config()
        },
        DsuImuWorkerConfig {
            queue_capacity: MAX_DSU_WORKER_QUEUE_CAPACITY + 1,
            ..test_config()
        },
    ] {
        assert!(matches!(
            DsuImuWorker::start(config, &anchor),
            Err(DsuWorkerError::InvalidQueueCapacity { .. })
        ));
    }

    for controls_queue_capacity in [0, MAX_DSU_WORKER_QUEUE_CAPACITY + 1] {
        let worker = DsuImuWorker::start(
            DsuImuWorkerConfig {
                controls_queue_capacity,
                ..test_config()
            },
            &anchor,
        )
        .expect("IMU-only mode must not validate an unused controls queue");
        drop(worker);
    }

    for poll_interval in [
        Duration::ZERO,
        MAX_DSU_WORKER_POLL_INTERVAL + Duration::from_nanos(1),
    ] {
        assert!(matches!(
            DsuImuWorker::start(
                DsuImuWorkerConfig {
                    poll_interval,
                    ..test_config()
                },
                &anchor,
            ),
            Err(DsuWorkerError::InvalidPollInterval { .. })
        ));
    }
}
