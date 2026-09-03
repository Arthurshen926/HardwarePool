use std::{
    collections::{BTreeMap, BTreeSet},
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterState, Availability,
    CapabilityClass, CapabilityDescriptor, FormatDescriptor, NodeId, PermissionRequirement,
    PortDescriptor, PortDirection, PortRef, ProfileId, QosMode, RouteBackend, RouteId, RouteState,
};
use capyio_data_plane::{DataEnvelope, ImuSampleV1, parse_imu_fixture_jsonl};
use capyio_dsu_adapter::{
    DSU_PROTOCOL_VERSION, DsuImuWorkerConfig, DsuLoopbackConfig, DsuSubmitOutcome, crc32_ieee,
};
use capyio_testkit::{ANDROID_NODE_ID, DemoLab};
use capyio_windows_input::DsuImuRouteController;

const FIXTURE: &str = include_str!("../../../../fixtures/imu/imu_samples_v1.jsonl");
const MESSAGE_PAD_DATA: u32 = 0x10_0002;
const SOURCE_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d022";
const SOURCE_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d202";
const SOURCE_PORT_ID: &str = "00000000-0000-4000-8000-00000000d212";
const DSU_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d012";

#[test]
fn route_retry_rebinds_new_epoch_and_isolates_existing_imu_route() {
    let (mut lab, source) = lab_with_imu_source();
    let existing_imu_route = lab.routes.phone_imu_to_gamepad;
    lab.set_route_active(existing_imu_route, true, 1).unwrap();
    let route_id = RouteId::new();
    let mut controller = controller(&mut lab, route_id, source, 0);
    let mut envelopes = fixture();

    let first_epoch = controller.begin_start(&mut lab.runtime, 2).unwrap();
    envelopes[0].stream_epoch = first_epoch;
    controller
        .activate(&mut lab.runtime, &envelopes[0])
        .unwrap();
    let first_status = controller.status(&lab.runtime).unwrap();
    assert_eq!(first_status.route_state, RouteState::Active);
    let first_address = first_status.local_address.unwrap();
    let first_client = subscribe(&controller, &lab, 7);
    assert_eq!(
        controller
            .submit(&mut lab.runtime, envelopes[0].clone())
            .unwrap(),
        DsuSubmitOutcome::Submitted
    );
    assert_pad_packet(&first_client, 0);
    wait_for_stats(&controller, &lab, |stats| stats.motion_packets_sent == 1);

    controller
        .report_upstream_offline(&mut lab.runtime, "fixture SensorServer disconnected")
        .unwrap();
    let offline = controller.status(&lab.runtime).unwrap();
    assert_eq!(offline.route_state, RouteState::Offline);
    assert!(offline.route_epoch > first_epoch);
    assert_eq!(offline.local_address, None);
    let rebound = UdpSocket::bind(first_address).expect("Offline cleanup must release DSU port");
    drop(rebound);
    assert_eq!(
        lab.runtime.route(existing_imu_route).unwrap().state,
        RouteState::Active
    );

    let retry_epoch = controller.begin_start(&mut lab.runtime, 3).unwrap();
    assert!(retry_epoch > offline.route_epoch);
    envelopes[1].stream_epoch = retry_epoch;
    controller
        .activate(&mut lab.runtime, &envelopes[1])
        .unwrap();
    let retry_client = subscribe(&controller, &lab, 8);
    assert_eq!(
        controller
            .submit(&mut lab.runtime, envelopes[1].clone())
            .unwrap(),
        DsuSubmitOutcome::Submitted
    );
    assert_pad_packet(&retry_client, 0);
    controller.stop(&mut lab.runtime).unwrap();
    controller.stop(&mut lab.runtime).unwrap();
    assert_eq!(
        controller.status(&lab.runtime).unwrap().route_state,
        RouteState::Stopped
    );
    assert_eq!(
        lab.runtime.route(existing_imu_route).unwrap().state,
        RouteState::Active
    );
    let snapshot = lab.runtime.snapshot();
    assert!(snapshot.problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.DSU_UPSTREAM_OFFLINE"
            && problem.related_route == Some(route_id)
            && problem.related_adapter == Some(parse_id(SOURCE_ADAPTER_ID))
    }));
    assert!(
        snapshot
            .problems
            .iter()
            .all(|problem| problem.related_route != Some(existing_imu_route))
    );
}

#[test]
fn bind_failure_marks_only_dsu_route_offline_and_retains_projection_owner() {
    let reservation = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
    let reserved_port = reservation.local_addr().unwrap().port();
    let (mut lab, source) = lab_with_imu_source();
    let existing_imu_route = lab.routes.phone_imu_to_gamepad;
    lab.set_route_active(existing_imu_route, true, 1).unwrap();
    let route_id = RouteId::new();
    let mut controller = controller(&mut lab, route_id, source, reserved_port);
    let epoch = controller.begin_start(&mut lab.runtime, 2).unwrap();
    let mut anchor = fixture().remove(0);
    anchor.stream_epoch = epoch;

    let error = controller.activate(&mut lab.runtime, &anchor).unwrap_err();
    assert!(error.contains("DSU IMU Worker start failed"));
    assert_eq!(
        controller.status(&lab.runtime).unwrap().route_state,
        RouteState::Offline
    );
    assert_eq!(
        lab.runtime.route(existing_imu_route).unwrap().state,
        RouteState::Active
    );
    assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.DSU_OPEN_FAILED"
            && problem.related_route == Some(route_id)
            && problem.related_adapter == Some(parse_id(DSU_ADAPTER_ID))
    }));
    controller.stop(&mut lab.runtime).unwrap();
}

#[test]
fn mismatched_stream_epoch_fails_before_socket_bind_and_is_source_owned() {
    let (mut lab, source) = lab_with_imu_source();
    let route_id = RouteId::new();
    let mut controller = controller(&mut lab, route_id, source, 0);
    let epoch = controller.begin_start(&mut lab.runtime, 1).unwrap();
    let mut anchor = fixture().remove(0);
    anchor.stream_epoch = epoch + 1;

    let error = controller.activate(&mut lab.runtime, &anchor).unwrap_err();
    assert!(error.contains("does not match Runtime Route epoch"));
    let status = controller.status(&lab.runtime).unwrap();
    assert_eq!(status.route_state, RouteState::Offline);
    assert_eq!(status.local_address, None);
    assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.DSU_STREAM_ANCHOR_INVALID"
            && problem.related_route == Some(route_id)
            && problem.related_adapter == Some(parse_id(SOURCE_ADAPTER_ID))
    }));
}

fn controller(
    lab: &mut DemoLab,
    route_id: RouteId,
    source: PortRef,
    port: u16,
) -> DsuImuRouteController {
    let config = DsuImuWorkerConfig {
        queue_capacity: 16,
        poll_interval: Duration::from_millis(1),
        ..DsuImuWorkerConfig::new(DsuLoopbackConfig::local_lab(port, 0x4341_5059))
    };
    DsuImuRouteController::install(&mut lab.runtime, lab.session_id, route_id, source, config)
        .unwrap()
}

fn subscribe(controller: &DsuImuRouteController, lab: &DemoLab, client_id: u32) -> UdpSocket {
    let client = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let address = controller
        .status(&lab.runtime)
        .unwrap()
        .local_address
        .unwrap();
    client.send_to(&pad_request(client_id), address).unwrap();
    wait_for_stats(controller, lab, |stats| stats.subscriptions_added == 1);
    client
}

fn wait_for_stats(
    controller: &DsuImuRouteController,
    lab: &DemoLab,
    predicate: impl Fn(capyio_dsu_adapter::DsuImuWorkerStats) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let stats = controller
            .status(&lab.runtime)
            .unwrap()
            .worker_stats
            .unwrap();
        if predicate(stats) {
            return;
        }
        assert!(Instant::now() < deadline, "DSU stats timed out: {stats:?}");
        thread::yield_now();
    }
}

fn assert_pad_packet(client: &UdpSocket, packet_number: u32) {
    let mut packet = [0_u8; 100];
    let (received, source) = client.recv_from(&mut packet).unwrap();
    assert_eq!(received, packet.len());
    assert!(source.ip().is_loopback());
    assert_eq!(&packet[..4], b"DSUS");
    assert_eq!(read_u32(&packet, 16), MESSAGE_PAD_DATA);
    assert_eq!(read_u32(&packet, 32), packet_number);
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

fn fixture() -> Vec<DataEnvelope<ImuSampleV1>> {
    parse_imu_fixture_jsonl(FIXTURE, 6).unwrap()
}

fn lab_with_imu_source() -> (DemoLab, PortRef) {
    let mut lab = DemoLab::new().unwrap();
    let node_id: NodeId = parse_id(ANDROID_NODE_ID);
    let adapter_id = parse_id(SOURCE_ADAPTER_ID);
    let capability_id = parse_id(SOURCE_CAPABILITY_ID);
    let port_id = parse_id(SOURCE_PORT_ID);
    lab.runtime
        .register_adapter_catalog(
            node_id,
            AdapterInstanceDescriptor {
                id: adapter_id,
                adapter_type: "capyio.fixture.imu-source".to_owned(),
                display_name: "Fixture IMU Source".to_owned(),
                deployment_mode: AdapterDeploymentMode::ExternalService,
                version: env!("CARGO_PKG_VERSION").to_owned(),
                state: AdapterState::Ready,
                health: AdapterHealth::Healthy,
                owned_capabilities: BTreeSet::new(),
                supported_route_modes: BTreeSet::from([RouteBackend::ExternalProtocol]),
            },
            vec![CapabilityDescriptor {
                id: capability_id,
                adapter_instance_id: adapter_id,
                display_name: "Fixture Phone IMU".to_owned(),
                class: CapabilityClass::Imu,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::None,
                metadata: BTreeMap::new(),
                ports: BTreeMap::from([(
                    port_id,
                    PortDescriptor {
                        id: port_id,
                        capability_id,
                        display_name: "Fixture IMU Sample Source".to_owned(),
                        direction: PortDirection::Source,
                        profile: ProfileId::imu_samples_v1(),
                        schema_id: None,
                        formats: vec![FormatDescriptor::new("imu-si-f32-le")],
                        qos_modes: BTreeSet::from([QosMode::Measurement]),
                        clock_domain: Some("fixture.monotonic".to_owned()),
                        availability: Availability::Available,
                        permission_requirement: PermissionRequirement::None,
                        interoperability_mode: capyio_core::InteroperabilityMode::StandardPort,
                    },
                )]),
            }],
        )
        .unwrap();
    (
        lab,
        PortRef {
            node_id,
            capability_id,
            port_id,
        },
    )
}

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

fn parse_id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().unwrap()
}
