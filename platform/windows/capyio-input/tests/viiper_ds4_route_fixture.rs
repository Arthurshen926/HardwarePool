use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    str::FromStr,
    thread::{self, JoinHandle},
    time::Duration,
};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterState, Availability,
    CapabilityClass, CapabilityDescriptor, FormatDescriptor, PermissionRequirement, PortDescriptor,
    PortDirection, ProfileId, QosMode, RouteBackend,
};
use capyio_core::{NodeId, PortRef, RouteId, RouteState, StreamId};
use capyio_data_plane::parse_imu_fixture_jsonl;
use capyio_input::{
    GamepadButton, GamepadButtons, GamepadControls, GamepadState, InputFrameHeader,
};
use capyio_testkit::{ANDROID_NODE_ID, DemoLab};
use capyio_viiper_adapter::{
    ViiperAutoAttachDisabled, ViiperDs4ControlsMapping, ViiperDs4MotionMapping,
    ViiperLoopbackClient, ViiperLoopbackConfig,
};
use capyio_windows_input::ViiperDs4RouteController;

const TEST_TIMEOUT: Duration = Duration::from_secs(2);
const IMU_FIXTURE: &str = include_str!("../../../../fixtures/imu/imu_samples_v1.jsonl");
const SOURCE_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d031";
const PHONE_IMU_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d301";
const PHONE_GAMEPAD_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d302";
const PHONE_IMU_PORT_ID: &str = "00000000-0000-4000-8000-00000000d311";
const PHONE_GAMEPAD_PORT_ID: &str = "00000000-0000-4000-8000-00000000d312";

#[test]
fn paired_routes_activate_project_retry_and_stop_without_touching_unrelated_route() {
    let fixture = Ds4Fixture::start(&[(31, "7"), (32, "8")]);
    let mut lab = DemoLab::new().unwrap();
    let unrelated = lab.routes.phone_camera_to_panel;
    lab.set_route_active(unrelated, true, 1).unwrap();
    let controls_route_id = RouteId::new();
    let motion_route_id = RouteId::new();
    let mut controller = controller(
        &mut lab,
        controls_route_id,
        motion_route_id,
        fixture.address,
    );
    let controls_stream = StreamId::new();
    let mut samples = parse_imu_fixture_jsonl(IMU_FIXTURE, 64).unwrap();

    let epochs = controller.begin_start(&mut lab.runtime, 2).unwrap();
    samples[0].stream_epoch = epochs.motion;
    controller
        .activate(
            &mut lab.runtime,
            state(
                controls_stream,
                epochs.controls,
                0,
                GamepadControls::neutral(),
            ),
            &samples[0],
        )
        .unwrap();
    let active = controller.status(&lab.runtime).unwrap();
    assert_eq!(active.controls_state, RouteState::Active);
    assert_eq!(active.motion_state, RouteState::Active);
    assert_eq!(active.bus_id, Some(31));

    samples[1].stream_epoch = epochs.motion;
    controller
        .submit(
            &mut lab.runtime,
            state(
                controls_stream,
                epochs.controls,
                1,
                GamepadControls {
                    buttons: GamepadButtons::empty().with(GamepadButton::South),
                    ..GamepadControls::neutral()
                },
            ),
            &samples[1],
        )
        .unwrap();
    controller
        .report_motion_offline(&mut lab.runtime, "fixture IMU disconnected")
        .unwrap();
    let offline = controller.status(&lab.runtime).unwrap();
    assert_eq!(offline.controls_state, RouteState::Offline);
    assert_eq!(offline.motion_state, RouteState::Offline);
    assert_eq!(
        lab.runtime.route(unrelated).unwrap().state,
        RouteState::Active
    );

    let retry = controller.begin_start(&mut lab.runtime, 3).unwrap();
    assert!(retry.controls > epochs.controls);
    assert!(retry.motion > epochs.motion);
    samples[0].stream_epoch = retry.motion;
    samples[0].sequence = 0;
    controller
        .activate(
            &mut lab.runtime,
            state(
                controls_stream,
                retry.controls,
                0,
                GamepadControls::neutral(),
            ),
            &samples[0],
        )
        .unwrap();
    controller.stop(&mut lab.runtime).unwrap();
    controller.stop(&mut lab.runtime).unwrap();
    let stopped = controller.status(&lab.runtime).unwrap();
    assert_eq!(stopped.controls_state, RouteState::Stopped);
    assert_eq!(stopped.motion_state, RouteState::Stopped);
    assert_eq!(
        lab.runtime.route(unrelated).unwrap().state,
        RouteState::Active
    );

    let observed = fixture.finish();
    assert_eq!(
        observed.management,
        vec![
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            b"bus/31/add {\"type\":\"dualshock4\"}\0".to_vec(),
            b"bus/remove 31\0".to_vec(),
            b"ping\0".to_vec(),
            b"bus/create\0".to_vec(),
            b"bus/32/add {\"type\":\"dualshock4\"}\0".to_vec(),
            b"bus/remove 32\0".to_vec(),
        ]
    );
    assert_eq!(observed.streams.len(), 2);
    assert_stream(&observed.streams[0], 31, "7", 4);
    assert_stream(&observed.streams[1], 32, "8", 2);
    assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.DS4_UPSTREAM_OFFLINE"
            && problem.related_route == Some(motion_route_id)
    }));
    assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
        problem.code == "CAPY.GAMEPAD.DS4_PAIRED_SOURCE_OFFLINE"
            && problem.related_route == Some(controls_route_id)
    }));
}

#[test]
fn mismatched_anchor_epochs_fail_closed_before_opening_viiper() {
    let mut lab = DemoLab::new().unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut controller = controller(
        &mut lab,
        RouteId::new(),
        RouteId::new(),
        listener.local_addr().unwrap(),
    );
    let epochs = controller.begin_start(&mut lab.runtime, 1).unwrap();
    let mut sample = parse_imu_fixture_jsonl(IMU_FIXTURE, 64).unwrap().remove(0);
    sample.stream_epoch = epochs.motion + 1;
    let error = controller
        .activate(
            &mut lab.runtime,
            state(
                StreamId::new(),
                epochs.controls,
                0,
                GamepadControls::neutral(),
            ),
            &sample,
        )
        .unwrap_err();
    assert!(error.contains("do not match Runtime epochs"));
    let status = controller.status(&lab.runtime).unwrap();
    assert_eq!(status.controls_state, RouteState::Offline);
    assert_eq!(status.motion_state, RouteState::Offline);
    drop(listener);
}

fn controller(
    lab: &mut DemoLab,
    controls_route_id: RouteId,
    motion_route_id: RouteId,
    address: SocketAddr,
) -> ViiperDs4RouteController {
    let node_id: NodeId = parse_id(ANDROID_NODE_ID);
    register_sources(lab, node_id);
    let controls_source = PortRef {
        node_id,
        capability_id: parse_id(PHONE_GAMEPAD_CAPABILITY_ID),
        port_id: parse_id(PHONE_GAMEPAD_PORT_ID),
    };
    let motion_source = PortRef {
        node_id,
        capability_id: parse_id(PHONE_IMU_CAPABILITY_ID),
        port_id: parse_id(PHONE_IMU_PORT_ID),
    };
    let config = ViiperLoopbackConfig::new(address, TEST_TIMEOUT, TEST_TIMEOUT, 512).unwrap();
    ViiperDs4RouteController::install(
        &mut lab.runtime,
        lab.session_id,
        controls_route_id,
        controls_source,
        motion_route_id,
        motion_source,
        ViiperLoopbackClient::new(config),
        ViiperAutoAttachDisabled::confirmed_by_caller(),
        ViiperDs4ControlsMapping::preserve(),
        ViiperDs4MotionMapping::identity(),
    )
    .unwrap()
}

fn register_sources(lab: &mut DemoLab, node_id: NodeId) {
    let adapter_id = parse_id(SOURCE_ADAPTER_ID);
    let imu_capability_id = parse_id(PHONE_IMU_CAPABILITY_ID);
    let imu_port_id = parse_id(PHONE_IMU_PORT_ID);
    let gamepad_capability_id = parse_id(PHONE_GAMEPAD_CAPABILITY_ID);
    let gamepad_port_id = parse_id(PHONE_GAMEPAD_PORT_ID);
    lab.runtime
        .register_adapter_catalog(
            node_id,
            AdapterInstanceDescriptor {
                id: adapter_id,
                adapter_type: "capyio.fixture.ds4-sources".to_owned(),
                display_name: "Fixture DS4 Sources".to_owned(),
                deployment_mode: AdapterDeploymentMode::ExternalService,
                version: env!("CARGO_PKG_VERSION").to_owned(),
                state: AdapterState::Ready,
                health: AdapterHealth::Healthy,
                owned_capabilities: std::collections::BTreeSet::new(),
                supported_route_modes: std::collections::BTreeSet::from([
                    RouteBackend::ExternalProtocol,
                ]),
            },
            vec![
                source_capability(
                    adapter_id,
                    gamepad_capability_id,
                    gamepad_port_id,
                    "Fixture Gamepad State",
                    CapabilityClass::Gamepad,
                    ProfileId::gamepad_state_v1(),
                    "gamepad-state-v1",
                    QosMode::Interactive,
                ),
                source_capability(
                    adapter_id,
                    imu_capability_id,
                    imu_port_id,
                    "Fixture IMU Samples",
                    CapabilityClass::Imu,
                    ProfileId::imu_samples_v1(),
                    "imu-si-f32-le",
                    QosMode::Measurement,
                ),
            ],
        )
        .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn source_capability(
    adapter_id: capyio_core::AdapterInstanceId,
    capability_id: capyio_core::CapabilityId,
    port_id: capyio_core::PortId,
    name: &str,
    class: CapabilityClass,
    profile: ProfileId,
    format: &str,
    qos: QosMode,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id: capability_id,
        adapter_instance_id: adapter_id,
        display_name: name.to_owned(),
        class,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        metadata: std::collections::BTreeMap::new(),
        ports: std::collections::BTreeMap::from([(
            port_id,
            PortDescriptor {
                id: port_id,
                capability_id,
                display_name: format!("{name} Source"),
                direction: PortDirection::Source,
                profile,
                schema_id: None,
                formats: vec![FormatDescriptor::new(format)],
                qos_modes: std::collections::BTreeSet::from([qos]),
                clock_domain: Some("fixture.monotonic".to_owned()),
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::None,
                interoperability_mode: capyio_core::InteroperabilityMode::StandardPort,
            },
        )]),
    }
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

fn assert_stream(observed: &[u8], bus_id: u32, device_id: &str, report_count: usize) {
    let handshake = format!("bus/{bus_id}/{device_id}\0").into_bytes();
    assert!(observed.starts_with(&handshake));
    assert_eq!(observed.len(), handshake.len() + report_count * 31);
    let first_report = &observed[handshake.len()..handshake.len() + 31];
    assert_eq!(&first_report[..6], &[0, 0, 0, 0, 0, 0]);
}

#[derive(Debug)]
struct Observation {
    management: Vec<Vec<u8>>,
    streams: Vec<Vec<u8>>,
}

struct Ds4Fixture {
    address: SocketAddr,
    handle: JoinHandle<Observation>,
}

impl Ds4Fixture {
    fn start(sessions: &[(u32, &str)]) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let sessions = sessions
            .iter()
            .map(|(bus, device)| (*bus, (*device).to_owned()))
            .collect::<Vec<_>>();
        let handle = thread::spawn(move || {
            let mut management = Vec::new();
            let mut streams = Vec::new();
            for (bus_id, device_id) in sessions {
                serve_management(
                    &listener,
                    br#"{"server":"VIIPER","version":"0.7.0"}"#,
                    &mut management,
                );
                serve_management(
                    &listener,
                    format!("{{\"busId\":{bus_id}}}").as_bytes(),
                    &mut management,
                );
                serve_management(
                    &listener,
                    format!("{{\"busId\":{bus_id},\"devId\":\"{device_id}\",\"vid\":\"0x054c\",\"pid\":\"0x09cc\",\"type\":\"dualshock4\",\"deviceSpecific\":{{\"vendorDefined\":true}}}}").as_bytes(),
                    &mut management,
                );
                let (mut stream, _) = listener.accept().unwrap();
                configure(&stream);
                let mut bytes = Vec::new();
                stream.read_to_end(&mut bytes).unwrap();
                streams.push(bytes);
                serve_management(
                    &listener,
                    format!("{{\"busId\":{bus_id}}}").as_bytes(),
                    &mut management,
                );
            }
            Observation {
                management,
                streams,
            }
        });
        Self { address, handle }
    }

    fn finish(self) -> Observation {
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

fn parse_id<T: FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().unwrap()
}
