use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream},
    str::FromStr,
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterState, Availability,
    CapabilityClass, CapabilityDescriptor, FormatDescriptor, NodeId, PermissionRequirement,
    PortDescriptor, PortDirection, PortRef, ProfileId, QosMode, RouteBackend, RouteId, RouteState,
    StreamId,
};
use capyio_input::{
    GamepadButton, GamepadButtons, GamepadControls, GamepadState, InputFrameHeader,
};
use capyio_testkit::{ANDROID_NODE_ID, DemoLab};
use capyio_viiper_adapter::{
    ViiperAutoAttachDisabled, ViiperLoopbackClient, ViiperLoopbackConfig, ViiperXbox360Mapping,
};
use capyio_windows_input::ViiperGamepadRouteController;

const TEST_TIMEOUT: Duration = Duration::from_secs(2);
const SOURCE_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d021";
const SOURCE_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d201";
const SOURCE_PORT_ID: &str = "00000000-0000-4000-8000-00000000d211";
const NEUTRAL: [u8; 20] = [0; 20];
const SOUTH: [u8; 20] = [
    0x00, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const EAST: [u8; 20] = [
    0x00, 0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[test]
fn route_retry_recreates_worker_on_a_new_epoch_and_isolates_imu() {
    let fixture = SuccessfulFixture::start(&[(7, "3"), (8, "4")]);
    let (mut lab, source) = lab_with_gamepad_source();
    let imu_route = lab.routes.phone_imu_to_gamepad;
    lab.set_route_active(imu_route, true, 1).unwrap();

    let route_id = RouteId::new();
    let mut controller = controller(&mut lab, route_id, source, fixture.address);
    let stream_id = StreamId::new();
    let first_epoch = controller
        .start(&mut lab.runtime, 2, stream_id, 0, 2_000_000)
        .unwrap();
    assert_eq!(first_epoch, 1);
    assert_eq!(
        controller.status(&lab.runtime).unwrap().route_state,
        RouteState::Active
    );

    let rejected = state(
        stream_id,
        first_epoch,
        0,
        controls_with(GamepadButton::Touchpad),
    );
    assert!(
        controller
            .submit(&mut lab.runtime, rejected)
            .unwrap_err()
            .contains("have no Xbox 360 field")
    );
    assert_eq!(
        controller.status(&lab.runtime).unwrap().route_state,
        RouteState::Active
    );
    controller
        .submit(
            &mut lab.runtime,
            state(
                stream_id,
                first_epoch,
                0,
                controls_with(GamepadButton::South),
            ),
        )
        .unwrap();

    controller
        .report_upstream_offline(&mut lab.runtime, "fixture source disconnected")
        .unwrap();
    let offline_status = controller.status(&lab.runtime).unwrap();
    assert_eq!(offline_status.route_state, RouteState::Offline);
    assert!(offline_status.route_epoch > first_epoch);
    assert_eq!(
        lab.runtime.route(imu_route).unwrap().state,
        RouteState::Active
    );

    let retry_epoch = controller
        .start(&mut lab.runtime, 3, stream_id, 0, 3_000_000)
        .unwrap();
    assert!(retry_epoch > offline_status.route_epoch);
    controller
        .submit(
            &mut lab.runtime,
            state(
                stream_id,
                retry_epoch,
                0,
                controls_with(GamepadButton::East),
            ),
        )
        .unwrap();
    controller.stop(&mut lab.runtime).unwrap();
    controller.stop(&mut lab.runtime).unwrap();
    assert_eq!(
        controller.status(&lab.runtime).unwrap().route_state,
        RouteState::Stopped
    );
    assert_eq!(
        lab.runtime.route(imu_route).unwrap().state,
        RouteState::Active
    );

    let observed = fixture.finish();
    assert_eq!(
        observed.management,
        vec![
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            b"bus/7/add {\"type\":\"xbox360\"}\0".to_vec(),
            b"bus/remove 7\0".to_vec(),
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            b"bus/8/add {\"type\":\"xbox360\"}\0".to_vec(),
            b"bus/remove 8\0".to_vec(),
        ]
    );
    assert_eq!(
        observed.streams,
        vec![
            expected_stream(7, "3", &[NEUTRAL, SOUTH, NEUTRAL]),
            expected_stream(8, "4", &[NEUTRAL, EAST, NEUTRAL]),
        ]
    );
    assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.UPSTREAM_OFFLINE"
            && problem.related_route == Some(route_id)
            && problem.related_adapter == Some(parse_id(SOURCE_ADAPTER_ID))
    }));
    assert!(
        lab.runtime
            .snapshot()
            .problems
            .iter()
            .all(|problem| problem.related_route != Some(imu_route))
    );
}

#[test]
fn open_failure_rolls_back_before_only_the_gamepad_route_becomes_offline() {
    let fixture = AddFailureFixture::start(9);
    let (mut lab, source) = lab_with_gamepad_source();
    let imu_route = lab.routes.phone_imu_to_gamepad;
    lab.set_route_active(imu_route, true, 1).unwrap();
    let route_id = RouteId::new();
    let mut controller = controller(&mut lab, route_id, source, fixture.address);

    let error = controller
        .start(&mut lab.runtime, 2, StreamId::new(), 0, 2_000_000)
        .unwrap_err();
    assert!(error.contains("VIIPER gamepad open failed"));
    assert_eq!(
        controller.status(&lab.runtime).unwrap().route_state,
        RouteState::Offline
    );
    assert_eq!(
        lab.runtime.route(imu_route).unwrap().state,
        RouteState::Active
    );
    assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.VIIPER_OPEN_FAILED" && problem.related_route == Some(route_id)
    }));

    controller.stop(&mut lab.runtime).unwrap();
    assert_eq!(
        fixture.finish(),
        vec![
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            b"bus/9/add {\"type\":\"xbox360\"}\0".to_vec(),
            b"bus/remove 9\0".to_vec(),
        ]
    );
}

#[test]
fn exhausted_sequence_neutralizes_cleans_up_and_requires_route_retry() {
    let fixture = SuccessfulFixture::start(&[(11, "5")]);
    let (mut lab, source) = lab_with_gamepad_source();
    let route_id = RouteId::new();
    let mut controller = controller(&mut lab, route_id, source, fixture.address);
    let stream_id = StreamId::new();
    let epoch = controller
        .start(&mut lab.runtime, 1, stream_id, u64::MAX, 1_000_000)
        .unwrap();

    let outcome = controller
        .submit(
            &mut lab.runtime,
            state(
                stream_id,
                epoch,
                u64::MAX,
                controls_with(GamepadButton::South),
            ),
        )
        .unwrap();
    assert!(outcome.exhausted());
    assert_eq!(
        controller.status(&lab.runtime).unwrap().route_state,
        RouteState::Offline
    );
    assert!(
        controller
            .submit(
                &mut lab.runtime,
                state(
                    stream_id,
                    epoch,
                    u64::MAX,
                    controls_with(GamepadButton::East),
                ),
            )
            .unwrap_err()
            .contains("not Active")
    );
    assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.SEQUENCE_EXHAUSTED" && problem.related_route == Some(route_id)
    }));

    let observed = fixture.finish();
    assert_eq!(
        observed.streams,
        vec![expected_stream(
            11,
            "5",
            &[NEUTRAL, SOUTH, NEUTRAL, NEUTRAL]
        )]
    );
    assert_eq!(
        observed.management.last(),
        Some(&b"bus/remove 11\0".to_vec())
    );
}

#[test]
fn closed_device_stream_cleans_up_and_only_marks_gamepad_offline() {
    let fixture = PeerCloseFixture::start(12, "6");
    let (mut lab, source) = lab_with_gamepad_source();
    let imu_route = lab.routes.phone_imu_to_gamepad;
    lab.set_route_active(imu_route, true, 1).unwrap();
    let route_id = RouteId::new();
    let mut controller = controller(&mut lab, route_id, source, fixture.address);
    controller
        .start(&mut lab.runtime, 2, StreamId::new(), 0, 2_000_000)
        .unwrap();
    fixture.wait_for_close();

    let error = controller.poll_rumble(&mut lab.runtime).unwrap_err();
    assert!(error.contains("VIIPER gamepad feedback failed"));
    assert_eq!(
        controller.status(&lab.runtime).unwrap().route_state,
        RouteState::Offline
    );
    assert_eq!(
        lab.runtime.route(imu_route).unwrap().state,
        RouteState::Active
    );
    assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.VIIPER_STREAM_FAILED"
            && problem.related_route == Some(route_id)
    }));
    assert_eq!(
        fixture.finish(),
        vec![
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            b"bus/12/add {\"type\":\"xbox360\"}\0".to_vec(),
            b"bus/remove 12\0".to_vec(),
        ]
    );
}

fn controller(
    lab: &mut DemoLab,
    route_id: RouteId,
    source: PortRef,
    address: SocketAddr,
) -> ViiperGamepadRouteController {
    let config = ViiperLoopbackConfig::new(address, TEST_TIMEOUT, TEST_TIMEOUT, 512).unwrap();
    ViiperGamepadRouteController::install(
        &mut lab.runtime,
        lab.session_id,
        route_id,
        source,
        ViiperLoopbackClient::new(config),
        ViiperAutoAttachDisabled::confirmed_by_caller(),
        ViiperXbox360Mapping::preserve(),
    )
    .unwrap()
}

fn lab_with_gamepad_source() -> (DemoLab, PortRef) {
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
                adapter_type: "capyio.fixture.gamepad-source".to_owned(),
                display_name: "Fixture Gamepad Source".to_owned(),
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
                display_name: "Fixture Gamepad".to_owned(),
                class: CapabilityClass::Gamepad,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::None,
                metadata: BTreeMap::new(),
                ports: BTreeMap::from([(
                    port_id,
                    PortDescriptor {
                        id: port_id,
                        capability_id,
                        display_name: "Fixture Gamepad State Source".to_owned(),
                        direction: PortDirection::Source,
                        profile: ProfileId::gamepad_state_v1(),
                        schema_id: None,
                        formats: vec![FormatDescriptor::new("gamepad-state-v1")],
                        qos_modes: BTreeSet::from([QosMode::Interactive]),
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

fn state(
    stream_id: StreamId,
    stream_epoch: u64,
    sequence: u64,
    controls: GamepadControls,
) -> GamepadState {
    GamepadState {
        header: InputFrameHeader {
            stream_id,
            stream_epoch,
            sequence,
            source_timestamp_nanos: sequence,
        },
        controls,
    }
}

fn controls_with(button: GamepadButton) -> GamepadControls {
    GamepadControls {
        buttons: GamepadButtons::empty().with(button),
        ..GamepadControls::neutral()
    }
}

fn expected_stream(bus_id: u32, device_id: &str, reports: &[[u8; 20]]) -> Vec<u8> {
    let mut expected = format!("bus/{bus_id}/{device_id}\0").into_bytes();
    for report in reports {
        expected.extend_from_slice(report);
    }
    expected
}

fn parse_id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().unwrap()
}

#[derive(Debug, Eq, PartialEq)]
struct SuccessfulObservation {
    management: Vec<Vec<u8>>,
    streams: Vec<Vec<u8>>,
}

struct SuccessfulFixture {
    address: SocketAddr,
    handle: JoinHandle<SuccessfulObservation>,
}

impl SuccessfulFixture {
    fn start(sessions: &[(u32, &str)]) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let sessions = sessions
            .iter()
            .map(|(bus_id, device_id)| (*bus_id, (*device_id).to_owned()))
            .collect::<Vec<_>>();
        let handle = thread::spawn(move || {
            let mut management = Vec::new();
            let mut streams = Vec::new();
            for (bus_id, device_id) in sessions {
                serve_management(&listener, compatible_response(), &mut management);
                serve_management(
                    &listener,
                    format!("{{\"busId\":{bus_id}}}").as_bytes(),
                    &mut management,
                );
                serve_management(
                    &listener,
                    format!(
                        "{{\"busId\":{bus_id},\"devId\":\"{device_id}\",\"vid\":\"0x045e\",\"pid\":\"0x028e\",\"type\":\"xbox360\",\"deviceSpecific\":{{\"subType\":1}}}}"
                    )
                    .as_bytes(),
                    &mut management,
                );
                let (mut stream, _) = listener.accept().unwrap();
                configure(&stream);
                let mut observed = Vec::new();
                stream.read_to_end(&mut observed).unwrap();
                streams.push(observed);
                serve_management(
                    &listener,
                    format!("{{\"busId\":{bus_id}}}").as_bytes(),
                    &mut management,
                );
            }
            SuccessfulObservation {
                management,
                streams,
            }
        });
        Self { address, handle }
    }

    fn finish(self) -> SuccessfulObservation {
        self.handle.join().unwrap()
    }
}

struct AddFailureFixture {
    address: SocketAddr,
    handle: JoinHandle<Vec<Vec<u8>>>,
}

struct PeerCloseFixture {
    address: SocketAddr,
    closed: Receiver<()>,
    handle: JoinHandle<Vec<Vec<u8>>>,
}

impl PeerCloseFixture {
    fn start(bus_id: u32, device_id: &str) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let device_id = device_id.to_owned();
        let (closed_sender, closed) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || {
            let mut management = Vec::new();
            serve_management(&listener, compatible_response(), &mut management);
            serve_management(
                &listener,
                format!("{{\"busId\":{bus_id}}}").as_bytes(),
                &mut management,
            );
            serve_management(
                &listener,
                format!(
                    "{{\"busId\":{bus_id},\"devId\":\"{device_id}\",\"vid\":\"0x045e\",\"pid\":\"0x028e\",\"type\":\"xbox360\",\"deviceSpecific\":{{\"subType\":1}}}}"
                )
                .as_bytes(),
                &mut management,
            );
            let (mut stream, _) = listener.accept().unwrap();
            configure(&stream);
            let expected_prefix = expected_stream(bus_id, &device_id, &[NEUTRAL]);
            let mut observed = vec![0; expected_prefix.len()];
            stream.read_exact(&mut observed).unwrap();
            assert_eq!(observed, expected_prefix);
            stream.shutdown(Shutdown::Both).unwrap();
            drop(stream);
            closed_sender.send(()).unwrap();
            serve_management(
                &listener,
                format!("{{\"busId\":{bus_id}}}").as_bytes(),
                &mut management,
            );
            management
        });
        Self {
            address,
            closed,
            handle,
        }
    }

    fn wait_for_close(&self) {
        self.closed.recv_timeout(TEST_TIMEOUT).unwrap();
    }

    fn finish(self) -> Vec<Vec<u8>> {
        self.handle.join().unwrap()
    }
}

impl AddFailureFixture {
    fn start(bus_id: u32) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut management = Vec::new();
            serve_management(&listener, compatible_response(), &mut management);
            serve_management(
                &listener,
                format!("{{\"busId\":{bus_id}}}").as_bytes(),
                &mut management,
            );
            serve_management(
                &listener,
                br#"{"status":409,"title":"Conflict","detail":"fixture add failure"}"#,
                &mut management,
            );
            serve_management(
                &listener,
                format!("{{\"busId\":{bus_id}}}").as_bytes(),
                &mut management,
            );
            management
        });
        Self { address, handle }
    }

    fn finish(self) -> Vec<Vec<u8>> {
        self.handle.join().unwrap()
    }
}

fn serve_management(listener: &TcpListener, response: &[u8], observed: &mut Vec<Vec<u8>>) {
    let (mut stream, _) = listener.accept().unwrap();
    configure(&stream);
    let mut request = Vec::new();
    stream.read_to_end(&mut request).unwrap();
    observed.push(request);
    stream.write_all(response).unwrap();
}

fn configure(stream: &TcpStream) {
    stream.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TEST_TIMEOUT)).unwrap();
}

fn compatible_response() -> &'static [u8] {
    br#"{"server":"VIIPER","version":"0.7.0"}"#
}
