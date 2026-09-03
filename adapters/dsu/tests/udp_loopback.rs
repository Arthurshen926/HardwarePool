use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use capyio_data_plane::parse_imu_fixture_jsonl;
use capyio_dsu_adapter::{
    DSU_PROTOCOL_VERSION, DsuControlsMapping, DsuLoopbackConfig, DsuLoopbackServer,
    DsuMotionMapping, DsuTransportError, MAX_DSU_DATAGRAMS_PER_POLL, MAX_DSU_SUBSCRIBERS,
    MAX_DSU_SUBSCRIPTION_TTL_MILLIS, MIN_DSU_SUBSCRIPTION_TTL_MILLIS, crc32_ieee,
    project_imu_envelope,
};
use capyio_input::{DpadState, GamepadButton, GamepadButtons, GamepadControls, TriggerValue};

const MESSAGE_VERSION: u32 = 0x10_0000;
const MESSAGE_PORT_INFO: u32 = 0x10_0001;
const MESSAGE_PAD_DATA: u32 = 0x10_0002;
const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn client_request(client_id: u32, message_type: u32, payload: &[u8]) -> Vec<u8> {
    let mut packet = vec![0_u8; 20 + payload.len()];
    let declared_length = u16::try_from(packet.len() - 16).unwrap();
    packet[..4].copy_from_slice(b"DSUC");
    write_u16(&mut packet, 4, DSU_PROTOCOL_VERSION);
    write_u16(&mut packet, 6, declared_length);
    write_u32(&mut packet, 12, client_id);
    write_u32(&mut packet, 16, message_type);
    packet[20..].copy_from_slice(payload);
    let checksum = crc32_ieee(&packet);
    write_u32(&mut packet, 8, checksum);
    packet
}

fn pad_request(client_id: u32, flags: u8, slot: u8, mac: [u8; 6]) -> Vec<u8> {
    let mut payload = [0_u8; 8];
    payload[0] = flags;
    payload[1] = slot;
    payload[2..].copy_from_slice(&mac);
    client_request(client_id, MESSAGE_PAD_DATA, &payload)
}

fn bind_server(config: DsuLoopbackConfig) -> DsuLoopbackServer {
    DsuLoopbackServer::bind(config).expect("bind ephemeral DSU loopback server")
}

fn bind_client() -> UdpSocket {
    let client = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    client
}

fn poll_until_received(
    server: &mut DsuLoopbackServer,
    now_millis: u64,
) -> capyio_dsu_adapter::DsuPollStats {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let stats = server.poll(now_millis).unwrap();
        if stats.datagrams_received != 0 {
            return stats;
        }
        assert!(Instant::now() < deadline, "DSU loopback datagram timed out");
        thread::yield_now();
    }
}

fn receive(client: &UdpSocket) -> Vec<u8> {
    let mut packet = [0_u8; 128];
    let (received, source) = client.recv_from(&mut packet).unwrap();
    assert!(source.ip().is_loopback());
    packet[..received].to_vec()
}

#[test]
fn loopback_inventory_subscription_renewal_and_expiry() {
    let config = DsuLoopbackConfig {
        port: 0,
        server_id: 0x0102_0304,
        subscriber_capacity: 2,
        subscription_ttl_millis: 1_000,
        datagrams_per_poll: 8,
    };
    let mut server = bind_server(config);
    assert_ne!(server.local_address().port(), 0);
    let client = bind_client();

    client
        .send_to(
            &client_request(7, MESSAGE_VERSION, &[]),
            server.local_address(),
        )
        .unwrap();
    let stats = poll_until_received(&mut server, 0);
    assert_eq!(stats.responses_sent, 1);
    let version = receive(&client);
    assert_eq!(version.len(), 22);
    assert_eq!(&version[..4], b"DSUS");
    assert_eq!(read_u16(&version, 20), DSU_PROTOCOL_VERSION);

    let mut inventory = [0_u8; 6];
    write_u32(&mut inventory, 0, 2);
    inventory[4] = 0;
    inventory[5] = 1;
    client
        .send_to(
            &client_request(7, MESSAGE_PORT_INFO, &inventory),
            server.local_address(),
        )
        .unwrap();
    let stats = poll_until_received(&mut server, 0);
    assert_eq!(stats.responses_sent, 2);
    let mut responses = [receive(&client), receive(&client)];
    responses.sort_by_key(|packet| packet[20]);
    assert_eq!(responses[0][20], 0);
    assert_eq!(responses[0][21], 2);
    assert_eq!(responses[1][20], 1);
    assert_eq!(responses[1][21], 0);

    client
        .send_to(&pad_request(7, 1, 0, [0; 6]), server.local_address())
        .unwrap();
    let stats = poll_until_received(&mut server, 100);
    assert_eq!(stats.subscriptions_added, 1);
    assert_eq!(server.subscriber_count(), 1);

    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let motion = project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap();
    let stats = server.publish_motion(100, motion).unwrap();
    assert_eq!(stats.active_subscribers, 1);
    assert_eq!(stats.packets_sent, 1);
    let first = receive(&client);
    assert_eq!(first.len(), 100);
    assert_eq!(read_u32(&first, 16), MESSAGE_PAD_DATA);
    assert_eq!(read_u32(&first, 32), 0);

    client
        .send_to(&pad_request(7, 1, 0, [0; 6]), server.local_address())
        .unwrap();
    let stats = poll_until_received(&mut server, 500);
    assert_eq!(stats.subscriptions_renewed, 1);
    let stats = server.publish_motion(500, motion).unwrap();
    assert_eq!(stats.packets_sent, 1);
    let second = receive(&client);
    assert_eq!(read_u32(&second, 32), 1);

    let stats = server.publish_motion(1_500, motion).unwrap();
    assert_eq!(stats.subscriptions_expired, 1);
    assert_eq!(stats.active_subscribers, 0);
    assert_eq!(stats.packets_sent, 0);
    assert_eq!(server.subscriber_count(), 0);
}

#[test]
fn malformed_and_capacity_failures_do_not_evict_an_existing_subscriber() {
    let config = DsuLoopbackConfig {
        port: 0,
        server_id: 9,
        subscriber_capacity: 1,
        subscription_ttl_millis: 1_000,
        datagrams_per_poll: 8,
    };
    let mut server = bind_server(config);
    let first = bind_client();
    let second = bind_client();

    first
        .send_to(
            &pad_request(1, 2, u8::MAX, [1, 2, 3, 4, 5, 6]),
            server.local_address(),
        )
        .unwrap();
    let stats = poll_until_received(&mut server, 0);
    assert_eq!(stats.selectors_without_projected_slot, 1);
    assert_eq!(server.subscriber_count(), 0);

    first
        .send_to(&pad_request(1, 1, 0, [0; 6]), server.local_address())
        .unwrap();
    let stats = poll_until_received(&mut server, 10);
    assert_eq!(stats.subscriptions_added, 1);

    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let motion = project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap();
    assert_eq!(server.publish_motion(10, motion).unwrap().packets_sent, 1);
    assert_eq!(read_u32(&receive(&first), 32), 0);

    second
        .send_to(&pad_request(2, 0, 0, [0; 6]), server.local_address())
        .unwrap();
    let stats = poll_until_received(&mut server, 10);
    assert_eq!(stats.subscriptions_rejected_full, 1);
    assert_eq!(server.subscriber_count(), 1);

    first
        .send_to(&pad_request(3, 1, 0, [0; 6]), server.local_address())
        .unwrap();
    let stats = poll_until_received(&mut server, 10);
    assert_eq!(stats.subscriptions_replaced, 1);
    assert_eq!(server.subscriber_count(), 1);
    assert_eq!(server.publish_motion(10, motion).unwrap().packets_sent, 1);
    assert_eq!(read_u32(&receive(&first), 32), 0);

    let mut corrupt = client_request(1, MESSAGE_VERSION, &[]);
    corrupt[8] ^= 1;
    first.send_to(&corrupt, server.local_address()).unwrap();
    first.send_to(&[0_u8; 129], server.local_address()).unwrap();
    first
        .send_to(
            &client_request(1, MESSAGE_VERSION, &[]),
            server.local_address(),
        )
        .unwrap();
    let mut received = 0;
    let mut malformed = 0;
    let mut responses = 0;
    let deadline = Instant::now() + Duration::from_secs(1);
    while received < 3 {
        let stats = server.poll(10).unwrap();
        received += stats.datagrams_received;
        malformed += stats.malformed_datagrams;
        responses += stats.responses_sent;
        assert!(
            Instant::now() < deadline,
            "DSU loopback datagrams timed out"
        );
        thread::yield_now();
    }
    assert_eq!(malformed, 2);
    assert_eq!(responses, 1);
    assert_eq!(receive(&first).len(), 22);
    assert_eq!(server.subscriber_count(), 1);
}

#[test]
fn configuration_poll_budget_and_monotonic_time_are_explicit() {
    for (config, expected) in [
        (
            DsuLoopbackConfig {
                subscriber_capacity: 0,
                ..DsuLoopbackConfig::local_lab(0, 1)
            },
            "capacity",
        ),
        (
            DsuLoopbackConfig {
                subscription_ttl_millis: MIN_DSU_SUBSCRIPTION_TTL_MILLIS - 1,
                ..DsuLoopbackConfig::local_lab(0, 1)
            },
            "TTL",
        ),
        (
            DsuLoopbackConfig {
                datagrams_per_poll: MAX_DSU_DATAGRAMS_PER_POLL + 1,
                ..DsuLoopbackConfig::local_lab(0, 1)
            },
            "budget",
        ),
    ] {
        let error = DsuLoopbackServer::bind(config).err().unwrap();
        assert!(error.to_string().contains(expected));
    }
    assert!(matches!(
        DsuLoopbackServer::bind(DsuLoopbackConfig {
            subscriber_capacity: MAX_DSU_SUBSCRIBERS + 1,
            ..DsuLoopbackConfig::local_lab(0, 1)
        }),
        Err(DsuTransportError::InvalidSubscriberCapacity { .. })
    ));
    assert!(matches!(
        DsuLoopbackServer::bind(DsuLoopbackConfig {
            subscription_ttl_millis: MAX_DSU_SUBSCRIPTION_TTL_MILLIS + 1,
            ..DsuLoopbackConfig::local_lab(0, 1)
        }),
        Err(DsuTransportError::InvalidSubscriptionTtl { .. })
    ));

    let mut server = bind_server(DsuLoopbackConfig {
        datagrams_per_poll: 1,
        ..DsuLoopbackConfig::local_lab(0, 1)
    });
    let client = bind_client();
    let request = client_request(1, MESSAGE_VERSION, &[]);
    client.send_to(&request, server.local_address()).unwrap();
    client.send_to(&request, server.local_address()).unwrap();
    let first = poll_until_received(&mut server, 10);
    assert_eq!(first.datagrams_received, 1);
    assert!(first.poll_budget_exhausted);
    assert_eq!(receive(&client).len(), 22);
    let second = poll_until_received(&mut server, 10);
    assert_eq!(second.datagrams_received, 1);
    assert!(second.poll_budget_exhausted);
    assert_eq!(receive(&client).len(), 22);

    assert!(matches!(
        server.poll(9),
        Err(DsuTransportError::MonotonicTimeRegressed {
            previous_millis: 10,
            actual_millis: 9,
        })
    ));
}

#[test]
fn combined_controls_and_motion_reach_a_loopback_subscriber() {
    let mut server = bind_server(DsuLoopbackConfig::local_lab(0, 0x0102_0304));
    let client = bind_client();
    client
        .send_to(&pad_request(7, 1, 0, [0; 6]), server.local_address())
        .unwrap();
    assert_eq!(poll_until_received(&mut server, 10).subscriptions_added, 1);

    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let motion = project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap();
    let controls = GamepadControls {
        buttons: GamepadButtons::empty()
            .with(GamepadButton::South)
            .with(GamepadButton::Start),
        right_trigger: TriggerValue::new(u16::MAX),
        ..GamepadControls::neutral()
    };
    assert_eq!(
        server
            .publish_state(
                10,
                motion,
                controls,
                DsuControlsMapping::dualshock_physical(),
            )
            .unwrap()
            .packets_sent,
        1
    );
    let packet = receive(&client);
    assert_eq!(packet.len(), 100);
    assert_eq!(packet[36], 1 << 3);
    assert_eq!(packet[37], (1 << 6) | (1 << 1));
    assert_eq!(packet[49], u8::MAX);
    assert_eq!(packet[54], u8::MAX);
    assert_eq!(read_u32(&packet, 32), 0);
}

#[test]
fn control_validation_does_not_depend_on_having_a_subscriber() {
    let mut server = bind_server(DsuLoopbackConfig::local_lab(0, 1));
    assert_eq!(server.subscriber_count(), 0);
    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let motion = project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap();

    let invalid = GamepadControls {
        dpad: DpadState { x: 2, y: 0 },
        ..GamepadControls::neutral()
    };
    assert!(matches!(
        server.publish_state(0, motion, invalid, DsuControlsMapping::identity()),
        Err(DsuTransportError::Packet(
            capyio_dsu_adapter::DsuPacketError::InvalidGamepadControls
        ))
    ));

    let unsupported = GamepadControls {
        buttons: GamepadButtons::empty().with(GamepadButton::Paddle1),
        ..GamepadControls::neutral()
    };
    assert!(matches!(
        server.publish_state(0, motion, unsupported, DsuControlsMapping::identity()),
        Err(DsuTransportError::Packet(
            capyio_dsu_adapter::DsuPacketError::UnsupportedGamepadButtons(_)
        ))
    ));
}
