use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    rc::Rc,
};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId,
    AdapterState, AuthorizationState, Availability, CapabilityClass, CapabilityDescriptor,
    CapabilityId, InteroperabilityMode, PermissionRequirement, PortDescriptor, PortDirection,
    PortId, PortRef, QosMode, RouteBackend, RouteId,
};
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType,
    TouchpadContact, TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
    TouchpadPosition, touchpad_frame_format, touchpad_frames_profile,
};
use capyio_remote_touchpad_adapter::{
    MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS, PrivateTouchpadAdmittedChannel,
    PrivateTouchpadChannelAdmissionError, PrivateTouchpadChannelSendOutcome,
    PrivateTouchpadClockSample, PrivateTouchpadHostChannelReceiveOutcome,
    PrivateTouchpadIngressLimits, PrivateTouchpadMonotonicClock, PrivateTouchpadPacketCodecV1,
    PrivateTouchpadPacketV1, PrivateTouchpadReceiverLimits, PrivateTouchpadRouteBinding,
    PrivateTouchpadRouteProvider, PrivateTouchpadRouteSessionState,
    PrivateTouchpadRuntimeDeliveryBuildError, PrivateTouchpadRuntimeDeliveryError,
    PrivateTouchpadRuntimeDeliveryWorker, PrivateTouchpadRuntimeWorker,
    PrivateTouchpadRuntimeWorkerError, PrivateTouchpadRuntimeWorkerFactoryBuildError,
    PrivateTouchpadSink, PrivateTouchpadSinkFactory, private_touchpad_host_channel,
};
use capyio_runtime::{NodeRuntime, RuntimeError};
use capyio_testkit::{android_node, windows_node};
#[cfg(windows)]
use capyio_windows_input::{SyntheticTouchpadGesture, build_touchpad_injection_fixture};
use capyio_windows_input::{WindowsTouchpadProjectionDisposition, WindowsTouchpadProjector};

const SOURCE_ADAPTER: &str = "00000000-0000-4000-8000-00000000e001";
const SOURCE_CAPABILITY: &str = "00000000-0000-4000-8000-00000000e002";
const SOURCE_PORT: &str = "00000000-0000-4000-8000-00000000e003";
const SINK_ADAPTER: &str = "00000000-0000-4000-8000-00000000e011";
const SINK_CAPABILITY: &str = "00000000-0000-4000-8000-00000000e012";
const SINK_PORT: &str = "00000000-0000-4000-8000-00000000e013";

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: fmt::Debug,
{
    value.parse().expect("valid fixture ID")
}

fn adapter(id_value: &str, adapter_type: &str) -> AdapterInstanceDescriptor {
    AdapterInstanceDescriptor {
        id: id(id_value),
        adapter_type: adapter_type.to_owned(),
        display_name: adapter_type.to_owned(),
        deployment_mode: AdapterDeploymentMode::Sidecar,
        version: "0.1.0-test".to_owned(),
        state: AdapterState::Ready,
        health: AdapterHealth::Healthy,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
    }
}

fn touchpad_capability(
    adapter_id: AdapterInstanceId,
    capability_id: CapabilityId,
    port_id: PortId,
    direction: PortDirection,
) -> CapabilityDescriptor {
    let port = PortDescriptor {
        id: port_id,
        capability_id,
        display_name: "Private Touchpad Port".to_owned(),
        direction,
        profile: touchpad_frames_profile(),
        schema_id: None,
        formats: vec![touchpad_frame_format()],
        qos_modes: BTreeSet::from([QosMode::Interactive]),
        clock_domain: Some("android.uptime_nanos".to_owned()),
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::UserConfirmation,
        interoperability_mode: InteroperabilityMode::AdapterManaged,
    };
    CapabilityDescriptor {
        id: capability_id,
        adapter_instance_id: adapter_id,
        display_name: "Private Touchpad".to_owned(),
        class: CapabilityClass::Touchpad,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::UserConfirmation,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(port_id, port)]),
    }
}

struct RuntimeFixture {
    runtime: Rc<RefCell<NodeRuntime>>,
    route_id: RouteId,
    sink: PortRef,
}

fn runtime_fixture() -> RuntimeFixture {
    let windows = windows_node();
    let android = android_node();
    let windows_id = windows.id;
    let android_id = android.id;
    let source_adapter = id(SOURCE_ADAPTER);
    let source_capability = id(SOURCE_CAPABILITY);
    let source_port = id(SOURCE_PORT);
    let sink_adapter = id(SINK_ADAPTER);
    let sink_capability = id(SINK_CAPABILITY);
    let sink_port = id(SINK_PORT);
    let source = PortRef {
        node_id: android_id,
        capability_id: source_capability,
        port_id: source_port,
    };
    let sink = PortRef {
        node_id: windows_id,
        capability_id: sink_capability,
        port_id: sink_port,
    };

    let mut runtime = NodeRuntime::new(windows).expect("Windows Runtime");
    runtime.register_peer(android, true).expect("Android peer");
    runtime
        .register_adapter_catalog(
            android_id,
            adapter(SOURCE_ADAPTER, "capyio.android.private-touchpad"),
            vec![touchpad_capability(
                source_adapter,
                source_capability,
                source_port,
                PortDirection::Source,
            )],
        )
        .expect("source catalog");
    runtime
        .register_adapter_catalog(
            windows_id,
            adapter(SINK_ADAPTER, "capyio.windows.private-touchpad"),
            vec![touchpad_capability(
                sink_adapter,
                sink_capability,
                sink_port,
                PortDirection::Sink,
            )],
        )
        .expect("sink catalog");
    let session_id = runtime.open_session(android_id).expect("Session");
    let route_id = runtime
        .create_route(session_id, source, sink, RouteBackend::AdapterManaged)
        .expect("Route");
    runtime
        .authorize_route(route_id, Some(10_000))
        .expect("authorization");
    runtime
        .prepare_route(
            route_id,
            Some(touchpad_frame_format()),
            QosMode::Interactive,
            10,
        )
        .expect("prepare");
    runtime
        .begin_route_start(route_id, 10)
        .expect("begin start");
    RuntimeFixture {
        runtime: Rc::new(RefCell::new(runtime)),
        route_id,
        sink,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderError {
    Runtime,
    Unavailable,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ProviderError {}

#[derive(Clone)]
struct RuntimeRouteProvider {
    runtime: Rc<RefCell<NodeRuntime>>,
    unavailable: Rc<Cell<bool>>,
}

impl PrivateTouchpadRouteProvider for RuntimeRouteProvider {
    type Error = ProviderError;

    fn current_route(&mut self, route_id: RouteId) -> Result<capyio_core::Route, Self::Error> {
        if self.unavailable.get() {
            return Err(ProviderError::Unavailable);
        }
        self.runtime
            .borrow()
            .route(route_id)
            .cloned()
            .map_err(|_error: RuntimeError| ProviderError::Runtime)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ClockError;

impl fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("clock unavailable")
    }
}

impl Error for ClockError {}

#[derive(Clone)]
struct FakeClock {
    sample: Rc<Cell<PrivateTouchpadClockSample>>,
    unavailable: Rc<Cell<bool>>,
}

impl PrivateTouchpadMonotonicClock for FakeClock {
    type Error = ClockError;

    fn sample(&mut self) -> Result<PrivateTouchpadClockSample, Self::Error> {
        if self.unavailable.get() {
            Err(ClockError)
        } else {
            Ok(self.sample.get())
        }
    }
}

#[derive(Default)]
struct SinkState {
    frames: Vec<TouchpadFrame>,
    epochs: Vec<(u64, u64)>,
    closes: usize,
}

#[derive(Clone, Default)]
struct MemorySink(Rc<RefCell<SinkState>>);

impl PrivateTouchpadSink for MemorySink {
    type Error = std::convert::Infallible;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        self.0.borrow_mut().frames.push(frame.clone());
        Ok(())
    }

    fn advance_epoch(&mut self, new_epoch: u64, first_sequence: u64) -> Result<(), Self::Error> {
        self.0.borrow_mut().epochs.push((new_epoch, first_sequence));
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        self.0.borrow_mut().closes += 1;
        Ok(())
    }
}

#[derive(Default)]
struct ProjectorState {
    dispositions: Vec<WindowsTouchpadProjectionDisposition>,
    closes: usize,
}

struct ProjectingSink {
    projector: WindowsTouchpadProjector,
    shared: Rc<RefCell<ProjectorState>>,
}

impl PrivateTouchpadSink for ProjectingSink {
    type Error = capyio_windows_input::WindowsTouchpadProjectionError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        let projection = self.projector.project(frame)?;
        self.shared
            .borrow_mut()
            .dispositions
            .push(projection.disposition);
        Ok(())
    }

    fn advance_epoch(&mut self, new_epoch: u64, first_sequence: u64) -> Result<(), Self::Error> {
        let projection = self.projector.advance_epoch(new_epoch, first_sequence)?;
        self.shared
            .borrow_mut()
            .dispositions
            .push(projection.disposition);
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        let projection = self.projector.cancel_active();
        let mut shared = self.shared.borrow_mut();
        shared.dispositions.push(projection.disposition);
        shared.closes += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FactoryError;

impl fmt::Display for FactoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("memory Sink factory failed")
    }
}

impl Error for FactoryError {}

#[derive(Clone)]
struct MemorySinkFactory {
    opens: Rc<Cell<usize>>,
    sink: MemorySink,
    fail: bool,
}

impl PrivateTouchpadSinkFactory for MemorySinkFactory {
    type Sink = MemorySink;
    type Error = FactoryError;

    fn open(
        &mut self,
        _stream: &InputStreamDescriptor,
        _descriptor: TouchpadDescriptor,
        _first_sequence: u64,
    ) -> Result<Self::Sink, Self::Error> {
        self.opens.set(self.opens.get() + 1);
        if self.fail {
            Err(FactoryError)
        } else {
            Ok(self.sink.clone())
        }
    }
}

fn descriptor() -> TouchpadDescriptor {
    TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 10_000,
            height_himetric: 6_000,
        },
        max_contacts: 5,
        button_type: TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: false,
    }
}

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: id("00000000-0000-4000-8000-00000000e021"),
        stream_epoch: epoch,
        clock_domain_id: "android.uptime_nanos".to_owned(),
    }
}

fn frame(epoch: u64, sequence: u64) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(epoch).stream_id,
            stream_epoch: epoch,
            sequence,
            source_timestamp_nanos: sequence.saturating_add(1) * 1_000,
        },
        kind: TouchpadFrameKind::Update,
        button: TouchpadButtonState::Released,
        contacts: vec![TouchpadContact {
            contact_id: 1,
            position: TouchpadPosition {
                x_himetric: 2_000,
                y_himetric: 3_000,
            },
            confidence: true,
            size: None,
            pressure: None,
        }],
    }
}

fn limits() -> PrivateTouchpadIngressLimits {
    PrivateTouchpadIngressLimits {
        queue_packets: 4,
        receiver: PrivateTouchpadReceiverLimits {
            max_packets_per_second: 100,
            active_idle_timeout_nanos: MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
        },
    }
}

struct WorkerFixture {
    runtime: Rc<RefCell<NodeRuntime>>,
    route_id: RouteId,
    sink_state: Rc<RefCell<SinkState>>,
    clock_sample: Rc<Cell<PrivateTouchpadClockSample>>,
    clock_unavailable: Rc<Cell<bool>>,
    provider_unavailable: Rc<Cell<bool>>,
    worker: PrivateTouchpadRuntimeWorker<RuntimeRouteProvider, FakeClock, MemorySink>,
}

fn worker_fixture() -> WorkerFixture {
    let fixture = runtime_fixture();
    let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let clock_unavailable = Rc::new(Cell::new(false));
    let provider_unavailable = Rc::new(Cell::new(false));
    let sink_impl = MemorySink::default();
    let sink_state = Rc::clone(&sink_impl.0);
    let worker = PrivateTouchpadRuntimeWorker::new(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: Rc::clone(&provider_unavailable),
        },
        FakeClock {
            sample: Rc::clone(&clock_sample),
            unavailable: Rc::clone(&clock_unavailable),
        },
        sink_impl,
    )
    .expect("worker");
    WorkerFixture {
        runtime: fixture.runtime,
        route_id: fixture.route_id,
        sink_state,
        clock_sample,
        clock_unavailable,
        provider_unavailable,
        worker,
    }
}

fn set_clock(fixture: &WorkerFixture, now_ms: u64, now_nanos: u64) {
    fixture
        .clock_sample
        .set(PrivateTouchpadClockSample { now_ms, now_nanos });
}

#[test]
fn worker_drives_real_runtime_start_packet_tick_and_stop_without_manual_route_snapshots() {
    let mut fixture = worker_fixture();
    assert_eq!(
        fixture.worker.state(),
        PrivateTouchpadRouteSessionState::Starting
    );

    fixture
        .runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Runtime Route");
    set_clock(&fixture, 11, 200);
    fixture.worker.activate().expect("activate worker");

    let packet = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor())
        .expect("codec")
        .encode(&frame(1, 0))
        .expect("packet");
    set_clock(&fixture, 12, 300);
    fixture.worker.enqueue(packet.as_bytes()).expect("enqueue");
    set_clock(&fixture, 13, 400);
    let tick = fixture.worker.tick().expect("tick");
    assert_eq!(tick.packets_processed, 1);
    assert_eq!(fixture.sink_state.borrow().frames.len(), 1);

    fixture.worker.stop().expect("stop worker");
    fixture
        .runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin Runtime stop");
    fixture
        .runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("complete Runtime stop");
    assert_eq!(
        fixture.worker.state(),
        PrivateTouchpadRouteSessionState::Closed
    );
    assert_eq!(fixture.sink_state.borrow().closes, 1);
    assert_eq!(fixture.worker.metrics().packets_processed, 1);
}

#[test]
fn worker_advances_to_new_runtime_epoch_and_discards_old_queue() {
    let mut fixture = worker_fixture();
    fixture
        .runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Route");
    set_clock(&fixture, 11, 200);
    fixture.worker.activate().expect("activate worker");
    let mut codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    set_clock(&fixture, 12, 300);
    fixture
        .worker
        .enqueue(codec.encode(&frame(1, 0)).expect("old packet").as_bytes())
        .expect("enqueue old epoch");

    let mut runtime = fixture.runtime.borrow_mut();
    runtime
        .begin_route_stop(fixture.route_id)
        .expect("begin stop");
    runtime.stop_route(fixture.route_id).expect("stop");
    runtime
        .prepare_route(
            fixture.route_id,
            Some(touchpad_frame_format()),
            QosMode::Interactive,
            20,
        )
        .expect("re-prepare");
    runtime
        .begin_route_start(fixture.route_id, 20)
        .expect("restart");
    drop(runtime);

    set_clock(&fixture, 20, 400);
    let advanced = fixture.worker.advance_epoch(10).expect("advance epoch");
    assert_eq!(advanced.previous_epoch, 1);
    assert_eq!(advanced.new_epoch, 2);
    assert_eq!(advanced.discarded_packets, 1);
    assert_eq!(fixture.worker.queued_packets(), 0);
    assert_eq!(fixture.sink_state.borrow().epochs, vec![(2, 10)]);

    fixture
        .runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate epoch two");
    set_clock(&fixture, 21, 500);
    fixture.worker.activate().expect("activate epoch two");
    codec.advance_epoch(2).expect("codec epoch two");
    set_clock(&fixture, 22, 600);
    fixture
        .worker
        .enqueue(codec.encode(&frame(2, 10)).expect("new packet").as_bytes())
        .expect("enqueue epoch two");
    set_clock(&fixture, 23, 700);
    assert_eq!(fixture.worker.tick().expect("tick").packets_processed, 1);
    assert_eq!(fixture.worker.metrics().discarded_packets, 1);
}

#[test]
fn wall_clock_regression_fails_closed_even_when_nanoseconds_advance() {
    let mut fixture = worker_fixture();
    fixture
        .runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Route");
    set_clock(&fixture, 11, 200);
    fixture.worker.activate().expect("activate worker");
    set_clock(&fixture, 10, 300);

    assert!(matches!(
        fixture.worker.tick(),
        Err(PrivateTouchpadRuntimeWorkerError::ClockRegression { .. })
    ));
    assert_eq!(
        fixture.worker.state(),
        PrivateTouchpadRouteSessionState::Failed
    );
    assert_eq!(fixture.sink_state.borrow().closes, 1);
}

#[test]
fn provider_or_clock_failure_fails_closed_without_opening_platform_devices() {
    let mut provider_failure = worker_fixture();
    provider_failure
        .runtime
        .borrow_mut()
        .activate_route(provider_failure.route_id)
        .expect("activate Route");
    set_clock(&provider_failure, 11, 200);
    provider_failure.worker.activate().expect("activate worker");
    provider_failure.provider_unavailable.set(true);
    set_clock(&provider_failure, 12, 300);
    assert!(matches!(
        provider_failure.worker.tick(),
        Err(PrivateTouchpadRuntimeWorkerError::RouteProvider { .. })
    ));
    assert_eq!(
        provider_failure.worker.state(),
        PrivateTouchpadRouteSessionState::Failed
    );

    let mut clock_failure = worker_fixture();
    clock_failure
        .runtime
        .borrow_mut()
        .activate_route(clock_failure.route_id)
        .expect("activate Route");
    set_clock(&clock_failure, 11, 200);
    clock_failure.worker.activate().expect("activate worker");
    clock_failure.clock_unavailable.set(true);
    assert!(matches!(
        clock_failure.worker.tick(),
        Err(PrivateTouchpadRuntimeWorkerError::Clock { .. })
    ));
    assert_eq!(
        clock_failure.worker.state(),
        PrivateTouchpadRouteSessionState::Failed
    );
    assert_eq!(clock_failure.sink_state.borrow().closes, 1);
}

#[test]
fn runtime_stopping_transition_is_observed_on_the_next_worker_tick() {
    let mut fixture = worker_fixture();
    fixture
        .runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Route");
    set_clock(&fixture, 11, 200);
    fixture.worker.activate().expect("activate worker");
    fixture
        .runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin stop");
    set_clock(&fixture, 12, 300);

    assert!(matches!(
        fixture.worker.tick(),
        Err(PrivateTouchpadRuntimeWorkerError::Session(_))
    ));
    assert_eq!(
        fixture.worker.state(),
        PrivateTouchpadRouteSessionState::Failed
    );
    assert_eq!(fixture.sink_state.borrow().closes, 1);
}

#[test]
fn sink_factory_opens_only_after_route_and_contract_preflight() {
    let fixture = runtime_fixture();
    fixture
        .runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin stop");
    fixture
        .runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("stop");
    let opens = Rc::new(Cell::new(0));
    let provider_unavailable = Rc::new(Cell::new(false));
    let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let clock_unavailable = Rc::new(Cell::new(false));
    let result = PrivateTouchpadRuntimeWorker::<RuntimeRouteProvider, FakeClock, MemorySink>::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: provider_unavailable,
        },
        FakeClock {
            sample: clock_sample,
            unavailable: clock_unavailable,
        },
        MemorySinkFactory {
            opens: Rc::clone(&opens),
            sink: MemorySink::default(),
            fail: false,
        },
    );
    assert!(matches!(
        result,
        Err(PrivateTouchpadRuntimeWorkerFactoryBuildError::Preflight(_))
    ));
    assert_eq!(opens.get(), 0);

    let fixture = runtime_fixture();
    let mut invalid_descriptor = descriptor();
    invalid_descriptor.max_contacts = 2;
    let result = PrivateTouchpadRuntimeWorker::<RuntimeRouteProvider, FakeClock, MemorySink>::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        invalid_descriptor,
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: fixture.runtime,
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::new(Cell::new(PrivateTouchpadClockSample {
                now_ms: 10,
                now_nanos: 100,
            })),
            unavailable: Rc::new(Cell::new(false)),
        },
        MemorySinkFactory {
            opens: Rc::clone(&opens),
            sink: MemorySink::default(),
            fail: false,
        },
    );
    assert!(matches!(
        result,
        Err(PrivateTouchpadRuntimeWorkerFactoryBuildError::Preflight(_))
    ));
    assert_eq!(opens.get(), 0);
}

#[test]
fn valid_preflight_opens_exactly_one_sink_and_factory_failure_is_typed() {
    let fixture = runtime_fixture();
    let opens = Rc::new(Cell::new(0));
    let sink = MemorySink::default();
    let sink_state = Rc::clone(&sink.0);
    let mut worker = PrivateTouchpadRuntimeWorker::<
        RuntimeRouteProvider,
        FakeClock,
        MemorySink,
    >::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::new(Cell::new(PrivateTouchpadClockSample {
                now_ms: 10,
                now_nanos: 100,
            })),
            unavailable: Rc::new(Cell::new(false)),
        },
        MemorySinkFactory {
            opens: Rc::clone(&opens),
            sink,
            fail: false,
        },
    )
    .expect("factory worker");
    assert_eq!(opens.get(), 1);
    assert_eq!(worker.state(), PrivateTouchpadRouteSessionState::Starting);
    worker.stop().expect("close factory Sink");
    assert_eq!(sink_state.borrow().closes, 1);

    let fixture = runtime_fixture();
    let result = PrivateTouchpadRuntimeWorker::<RuntimeRouteProvider, FakeClock, MemorySink>::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: fixture.runtime,
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::new(Cell::new(PrivateTouchpadClockSample {
                now_ms: 10,
                now_nanos: 100,
            })),
            unavailable: Rc::new(Cell::new(false)),
        },
        MemorySinkFactory {
            opens: Rc::clone(&opens),
            sink: MemorySink::default(),
            fail: true,
        },
    );
    assert!(matches!(
        result,
        Err(PrivateTouchpadRuntimeWorkerFactoryBuildError::SinkFactory(
            FactoryError
        ))
    ));
    assert_eq!(opens.get(), 2);
}

#[cfg(windows)]
#[test]
fn windows_factories_satisfy_sink_factory_without_opening_a_device() {
    fn assert_factory<Factory: PrivateTouchpadSinkFactory>() {}
    assert_factory::<capyio_remote_touchpad_adapter::WindowsSyntheticTouchpadSinkFactory>();
    assert_factory::<capyio_remote_touchpad_adapter::WindowsVhfTouchpadSinkFactory>();
}

#[cfg(windows)]
#[test]
fn invalid_route_preflight_never_opens_the_vhf_interface() {
    let fixture = runtime_fixture();
    fixture
        .runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin stop");
    fixture
        .runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("stop");

    let result = PrivateTouchpadRuntimeWorker::<
        RuntimeRouteProvider,
        FakeClock,
        capyio_windows_input::VhfTouchpadSession<capyio_windows_input::VhfWin32Transport>,
    >::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: fixture.runtime,
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::new(Cell::new(PrivateTouchpadClockSample {
                now_ms: 10,
                now_nanos: 100,
            })),
            unavailable: Rc::new(Cell::new(false)),
        },
        capyio_remote_touchpad_adapter::WindowsVhfTouchpadSinkFactory,
    );
    assert!(matches!(
        result,
        Err(PrivateTouchpadRuntimeWorkerFactoryBuildError::Preflight(_))
    ));
}

#[cfg(windows)]
#[test]
#[ignore = "opens the protected VHF driver interface and completes Hello/Close; requires installed exact driver package and explicit human approval"]
fn authorized_vhf_factory_opens_and_closes_real_driver_interface_without_frames() {
    let fixture = runtime_fixture();
    let runtime = Rc::clone(&fixture.runtime);
    let mut worker = PrivateTouchpadRuntimeWorker::<
        RuntimeRouteProvider,
        FakeClock,
        capyio_windows_input::VhfTouchpadSession<capyio_windows_input::VhfWin32Transport>,
    >::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime,
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::new(Cell::new(PrivateTouchpadClockSample {
                now_ms: 10,
                now_nanos: 100,
            })),
            unavailable: Rc::new(Cell::new(false)),
        },
        capyio_remote_touchpad_adapter::WindowsVhfTouchpadSinkFactory,
    )
    .expect("authorized VHF factory must open the installed protected interface");
    worker.stop().expect("close VHF Sink");
}

#[cfg(windows)]
#[test]
#[ignore = "moves the real Windows desktop pointer through the installed VHF Precision Touchpad; requires explicit human approval"]
fn authorized_vhf_worker_submits_one_finger_motion_then_releases_and_closes() {
    let fixture = runtime_fixture();
    let runtime = Rc::clone(&fixture.runtime);
    let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let mut worker = PrivateTouchpadRuntimeWorker::<
        RuntimeRouteProvider,
        FakeClock,
        capyio_windows_input::VhfTouchpadSession<capyio_windows_input::VhfWin32Transport>,
    >::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::clone(&clock_sample),
            unavailable: Rc::new(Cell::new(false)),
        },
        capyio_remote_touchpad_adapter::WindowsVhfTouchpadSinkFactory,
    )
    .expect("authorized VHF factory must open the installed protected interface");
    runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Runtime Route");
    clock_sample.set(PrivateTouchpadClockSample {
        now_ms: 11,
        now_nanos: 200,
    });
    worker.activate().expect("activate VHF worker");

    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let mut cancel = frame(1, 0);
    cancel.kind = TouchpadFrameKind::CancelAll;
    cancel.contacts.clear();
    let mut down = frame(1, 1);
    down.contacts[0].position = TouchpadPosition {
        x_himetric: 2_000,
        y_himetric: 3_000,
    };
    let mut moved = frame(1, 2);
    moved.contacts[0].position = TouchpadPosition {
        x_himetric: 6_500,
        y_himetric: 3_000,
    };
    let mut released = frame(1, 3);
    released.contacts.clear();

    for (index, touchpad_frame) in [&mut cancel, &mut down, &mut moved, &mut released]
        .into_iter()
        .enumerate()
    {
        touchpad_frame.header.source_timestamp_nanos = 1_000_000 + (index as u64 * 12_000_000);
    }

    for (index, touchpad_frame) in [cancel, down, moved, released].iter().enumerate() {
        let base_nanos = 1_000_000 + (index as u64 * 1_000_000);
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: base_nanos,
        });
        let packet = codec.encode(touchpad_frame).expect("encode fixture frame");
        worker
            .enqueue(packet.as_bytes())
            .expect("enqueue fixture frame");
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: base_nanos + 100_000,
        });
        assert_eq!(
            worker.tick().expect("pump fixture frame").packets_processed,
            1
        );
        std::thread::sleep(std::time::Duration::from_millis(12));
    }

    assert_eq!(worker.metrics().packets_enqueued, 4);
    assert_eq!(worker.metrics().packets_processed, 4);
    worker.stop().expect("close released VHF touchpad");
    assert_eq!(worker.state(), PrivateTouchpadRouteSessionState::Closed);
    drop(worker);
    runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin Runtime cleanup");
    runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("complete Runtime cleanup");
}

#[cfg(windows)]
#[test]
#[ignore = "may click the active Windows desktop surface through the installed VHF Precision Touchpad; requires an isolated click target and explicit human approval"]
fn authorized_vhf_worker_submits_one_finger_tap_then_closes() {
    let fixture = runtime_fixture();
    let runtime = Rc::clone(&fixture.runtime);
    let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let mut worker = PrivateTouchpadRuntimeWorker::<
        RuntimeRouteProvider,
        FakeClock,
        capyio_windows_input::VhfTouchpadSession<capyio_windows_input::VhfWin32Transport>,
    >::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::clone(&clock_sample),
            unavailable: Rc::new(Cell::new(false)),
        },
        capyio_remote_touchpad_adapter::WindowsVhfTouchpadSinkFactory,
    )
    .expect("authorized VHF factory must open the installed protected interface");
    runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Runtime Route");
    clock_sample.set(PrivateTouchpadClockSample {
        now_ms: 11,
        now_nanos: 200,
    });
    worker.activate().expect("activate VHF worker");

    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let mut cancel = frame(1, 0);
    cancel.kind = TouchpadFrameKind::CancelAll;
    cancel.contacts.clear();
    cancel.header.source_timestamp_nanos = 1_000_000;
    let mut down = frame(1, 1);
    down.contacts[0].position = TouchpadPosition {
        x_himetric: 5_000,
        y_himetric: 3_000,
    };
    down.header.source_timestamp_nanos = 13_000_000;
    let mut held = frame(1, 2);
    held.contacts[0].position = TouchpadPosition {
        x_himetric: 5_000,
        y_himetric: 3_000,
    };
    held.header.source_timestamp_nanos = 53_000_000;
    let mut released = frame(1, 3);
    released.contacts.clear();
    released.header.source_timestamp_nanos = 93_000_000;

    for (index, touchpad_frame) in [cancel, down, held, released].iter().enumerate() {
        let base_nanos = 1_000_000 + (index as u64 * 100_000_000);
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: base_nanos,
        });
        let packet = codec.encode(touchpad_frame).expect("encode fixture frame");
        worker
            .enqueue(packet.as_bytes())
            .expect("enqueue fixture frame");
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: base_nanos + 100_000,
        });
        assert_eq!(
            worker.tick().expect("pump fixture frame").packets_processed,
            1
        );
        std::thread::sleep(std::time::Duration::from_millis(
            if index == 1 || index == 2 { 40 } else { 12 },
        ));
    }

    assert_eq!(worker.metrics().packets_enqueued, 4);
    assert_eq!(worker.metrics().packets_processed, 4);
    worker.stop().expect("close released VHF touchpad");
    assert_eq!(worker.state(), PrivateTouchpadRouteSessionState::Closed);
    drop(worker);
    runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin Runtime cleanup");
    runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("complete Runtime cleanup");
}

#[cfg(windows)]
#[test]
#[ignore = "scrolls an isolated Windows target through the installed VHF Precision Touchpad; requires explicit human approval"]
fn authorized_vhf_worker_submits_two_finger_pan_then_releases_and_closes() {
    let fixture = runtime_fixture();
    let runtime = Rc::clone(&fixture.runtime);
    let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let mut worker = PrivateTouchpadRuntimeWorker::<
        RuntimeRouteProvider,
        FakeClock,
        capyio_windows_input::VhfTouchpadSession<capyio_windows_input::VhfWin32Transport>,
    >::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::clone(&clock_sample),
            unavailable: Rc::new(Cell::new(false)),
        },
        capyio_remote_touchpad_adapter::WindowsVhfTouchpadSinkFactory,
    )
    .expect("authorized VHF factory must open the installed protected interface");
    runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Runtime Route");
    clock_sample.set(PrivateTouchpadClockSample {
        now_ms: 11,
        now_nanos: 200,
    });
    worker.activate().expect("activate VHF worker");

    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let mut cancel = frame(1, 0);
    cancel.kind = TouchpadFrameKind::CancelAll;
    cancel.contacts.clear();
    cancel.header.source_timestamp_nanos = 1_000_000;
    let two_contacts = |sequence: u64, y_himetric: u32| {
        let mut touchpad_frame = frame(1, sequence);
        touchpad_frame.header.source_timestamp_nanos = 13_000_000 + ((sequence - 1) * 16_000_000);
        touchpad_frame.contacts[0].position = TouchpadPosition {
            x_himetric: 3_500,
            y_himetric,
        };
        touchpad_frame.contacts.push(TouchpadContact {
            contact_id: 2,
            position: TouchpadPosition {
                x_himetric: 6_500,
                y_himetric,
            },
            confidence: true,
            size: None,
            pressure: None,
        });
        touchpad_frame
    };
    let down = two_contacts(1, 2_000);
    let moved_1 = two_contacts(2, 2_600);
    let moved_2 = two_contacts(3, 3_200);
    let moved_3 = two_contacts(4, 3_800);
    let moved_4 = two_contacts(5, 4_500);
    let mut released = frame(1, 6);
    released.contacts.clear();
    released.header.source_timestamp_nanos = 93_000_000;
    let frames = [cancel, down, moved_1, moved_2, moved_3, moved_4, released];

    for (index, touchpad_frame) in frames.iter().enumerate() {
        let base_nanos = 1_000_000 + (index as u64 * 20_000_000);
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: base_nanos,
        });
        let packet = codec.encode(touchpad_frame).expect("encode fixture frame");
        worker
            .enqueue(packet.as_bytes())
            .expect("enqueue fixture frame");
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: base_nanos + 100_000,
        });
        assert_eq!(
            worker.tick().expect("pump fixture frame").packets_processed,
            1
        );
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    assert_eq!(worker.metrics().packets_enqueued, frames.len() as u64);
    assert_eq!(worker.metrics().packets_processed, frames.len() as u64);
    worker.stop().expect("close released VHF touchpad");
    assert_eq!(worker.state(), PrivateTouchpadRouteSessionState::Closed);
    drop(worker);
    runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin Runtime cleanup");
    runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("complete Runtime cleanup");
}

#[cfg(windows)]
type RealVhfWorker = PrivateTouchpadRuntimeWorker<
    RuntimeRouteProvider,
    FakeClock,
    capyio_windows_input::VhfTouchpadSession<capyio_windows_input::VhfWin32Transport>,
>;

#[cfg(windows)]
struct ActivatedVhfWorkerFixture {
    runtime: Rc<RefCell<NodeRuntime>>,
    route_id: RouteId,
    clock_sample: Rc<Cell<PrivateTouchpadClockSample>>,
    worker: RealVhfWorker,
}

#[cfg(windows)]
impl ActivatedVhfWorkerFixture {
    fn new() -> Self {
        let fixture = runtime_fixture();
        let runtime = Rc::clone(&fixture.runtime);
        let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
            now_ms: 10,
            now_nanos: 100,
        }));
        let mut worker = RealVhfWorker::new_with_sink_factory(
            fixture.route_id,
            fixture.sink,
            stream(1),
            descriptor(),
            0,
            limits(),
            RuntimeRouteProvider {
                runtime: Rc::clone(&runtime),
                unavailable: Rc::new(Cell::new(false)),
            },
            FakeClock {
                sample: Rc::clone(&clock_sample),
                unavailable: Rc::new(Cell::new(false)),
            },
            capyio_remote_touchpad_adapter::WindowsVhfTouchpadSinkFactory,
        )
        .expect("authorized VHF factory must open the installed protected interface");
        runtime
            .borrow_mut()
            .activate_route(fixture.route_id)
            .expect("activate Runtime Route");
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 11,
            now_nanos: 200,
        });
        worker.activate().expect("activate VHF worker");
        Self {
            runtime,
            route_id: fixture.route_id,
            clock_sample,
            worker,
        }
    }

    fn submit_frames_with_interval(&mut self, frames: &[TouchpadFrame], interval_millis: u64) {
        let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
        for (index, touchpad_frame) in frames.iter().enumerate() {
            let base_nanos = 1_000_000 + (index as u64 * 20_000_000);
            self.clock_sample.set(PrivateTouchpadClockSample {
                now_ms: 12 + index as u64,
                now_nanos: base_nanos,
            });
            let packet = codec.encode(touchpad_frame).expect("encode fixture frame");
            self.worker
                .enqueue(packet.as_bytes())
                .expect("enqueue fixture frame");
            self.clock_sample.set(PrivateTouchpadClockSample {
                now_ms: 12 + index as u64,
                now_nanos: base_nanos + 100_000,
            });
            assert_eq!(
                self.worker
                    .tick()
                    .expect("pump fixture frame")
                    .packets_processed,
                1
            );
            std::thread::sleep(std::time::Duration::from_millis(interval_millis));
        }
    }

    fn close(mut self) {
        self.worker.stop().expect("close released VHF touchpad");
        assert_eq!(
            self.worker.state(),
            PrivateTouchpadRouteSessionState::Closed
        );
        drop(self.worker);
        self.runtime
            .borrow_mut()
            .begin_route_stop(self.route_id)
            .expect("begin Runtime cleanup");
        self.runtime
            .borrow_mut()
            .stop_route(self.route_id)
            .expect("complete Runtime cleanup");
    }
}

#[cfg(windows)]
#[test]
#[ignore = "may invoke the configured Windows three-finger Shell action through the installed VHF Precision Touchpad; requires explicit human approval"]
fn authorized_vhf_worker_submits_three_finger_swipe_then_releases_and_closes() {
    let mut worker_fixture = ActivatedVhfWorkerFixture::new();
    let gesture_fixture =
        build_touchpad_injection_fixture(SyntheticTouchpadGesture::ThreeFingerSwipe, stream(1));
    assert_eq!(gesture_fixture.gesture.contact_count(), 3);
    worker_fixture
        .submit_frames_with_interval(&gesture_fixture.frames, gesture_fixture.interval_millis);
    assert_eq!(
        worker_fixture.worker.metrics().packets_enqueued,
        gesture_fixture.frames.len() as u64
    );
    assert_eq!(
        worker_fixture.worker.metrics().packets_processed,
        gesture_fixture.frames.len() as u64
    );
    worker_fixture.close();
}

#[cfg(windows)]
#[test]
#[ignore = "may invoke the configured Windows four-finger Shell action through the installed VHF Precision Touchpad; requires explicit human approval"]
fn authorized_vhf_worker_submits_four_finger_swipe_then_releases_and_closes() {
    let mut worker_fixture = ActivatedVhfWorkerFixture::new();
    let gesture_fixture =
        build_touchpad_injection_fixture(SyntheticTouchpadGesture::FourFingerSwipe, stream(1));
    assert_eq!(gesture_fixture.gesture.contact_count(), 4);
    worker_fixture
        .submit_frames_with_interval(&gesture_fixture.frames, gesture_fixture.interval_millis);
    assert_eq!(
        worker_fixture.worker.metrics().packets_enqueued,
        gesture_fixture.frames.len() as u64
    );
    assert_eq!(
        worker_fixture.worker.metrics().packets_processed,
        gesture_fixture.frames.len() as u64
    );
    worker_fixture.close();
}

#[cfg(windows)]
#[test]
#[ignore = "may start a held primary-button drag on the active Windows desktop through the installed VHF Precision Touchpad; requires an isolated drag target and explicit human approval"]
fn authorized_vhf_worker_submits_double_tap_drag_then_releases_and_closes() {
    let mut worker_fixture = ActivatedVhfWorkerFixture::new();
    let gesture_fixture = build_touchpad_injection_fixture(
        SyntheticTouchpadGesture::OneFingerDoubleTapDrag,
        stream(1),
    );
    assert_eq!(gesture_fixture.gesture.contact_count(), 1);
    worker_fixture
        .submit_frames_with_interval(&gesture_fixture.frames, gesture_fixture.interval_millis);
    assert_eq!(
        worker_fixture.worker.metrics().packets_enqueued,
        gesture_fixture.frames.len() as u64
    );
    assert_eq!(
        worker_fixture.worker.metrics().packets_processed,
        gesture_fixture.frames.len() as u64
    );
    worker_fixture.close();
}

#[cfg(windows)]
#[test]
#[ignore = "may start a held primary-button drag on the active Windows desktop through the installed VHF Precision Touchpad; diagnoses immediate contact-ID reuse and requires an isolated drag target plus explicit human approval"]
fn authorized_vhf_worker_submits_double_tap_drag_with_reused_contact_id() {
    let mut worker_fixture = ActivatedVhfWorkerFixture::new();
    let mut gesture_fixture = build_touchpad_injection_fixture(
        SyntheticTouchpadGesture::OneFingerDoubleTapDrag,
        stream(1),
    );
    let mut remapped_contacts = 0_u64;
    for frame in &mut gesture_fixture.frames {
        for contact in &mut frame.contacts {
            if contact.contact_id == 2 {
                contact.contact_id = 1;
                remapped_contacts += 1;
            }
        }
    }
    assert!(remapped_contacts > 0);
    worker_fixture
        .submit_frames_with_interval(&gesture_fixture.frames, gesture_fixture.interval_millis);
    assert_eq!(
        worker_fixture.worker.metrics().packets_processed,
        gesture_fixture.frames.len() as u64
    );
    worker_fixture.close();
}

#[cfg(windows)]
#[test]
#[ignore = "may start a held primary-button drag on the active Windows desktop through the installed VHF Precision Touchpad; mirrors measured Android cadence and requires an isolated drag target plus explicit human approval"]
fn authorized_vhf_worker_submits_android_cadence_double_tap_drag() {
    const INTERVAL_MILLIS: u64 = 16;
    let stream = stream(1);
    let make_frame = |sequence: u64, contacts: Vec<TouchpadContact>| TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream.stream_id,
            stream_epoch: stream.stream_epoch,
            sequence,
            source_timestamp_nanos: 1_000_000_000 + sequence * INTERVAL_MILLIS * 1_000_000,
        },
        kind: if sequence == 0 || sequence == 39 {
            TouchpadFrameKind::CancelAll
        } else {
            TouchpadFrameKind::Update
        },
        button: TouchpadButtonState::Released,
        contacts,
    };
    let contact = |contact_id, x_himetric| TouchpadContact {
        contact_id,
        position: TouchpadPosition {
            x_himetric,
            y_himetric: 3_000,
        },
        confidence: true,
        size: None,
        pressure: None,
    };

    let mut frames = Vec::with_capacity(40);
    frames.push(make_frame(0, Vec::new()));
    for sequence in 1_u64..=6 {
        frames.push(make_frame(sequence, vec![contact(1, 5_000)]));
    }
    frames.push(make_frame(7, Vec::new()));
    frames.push(make_frame(8, Vec::new()));
    for sequence in 9_u64..=37 {
        let motion_step = sequence.saturating_sub(10);
        let x_himetric = 5_000 + ((motion_step * 3_000) / 27) as u32;
        frames.push(make_frame(sequence, vec![contact(2, x_himetric)]));
    }
    frames.push(make_frame(38, Vec::new()));
    frames.push(make_frame(39, Vec::new()));
    assert_eq!(frames.len(), 40);

    let mut worker_fixture = ActivatedVhfWorkerFixture::new();
    worker_fixture.submit_frames_with_interval(&frames, INTERVAL_MILLIS);
    assert_eq!(worker_fixture.worker.metrics().packets_processed, 40);
    worker_fixture.close();
}

#[cfg(windows)]
#[test]
#[ignore = "creates and immediately destroys a real Windows synthetic touchpad; requires explicit human approval"]
fn authorized_windows_factory_opens_and_closes_real_synthetic_touchpad_without_frames() {
    let fixture = runtime_fixture();
    let runtime = Rc::clone(&fixture.runtime);
    let mut worker = PrivateTouchpadRuntimeWorker::<
        RuntimeRouteProvider,
        FakeClock,
        capyio_windows_input::SyntheticTouchpadSession,
    >::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::new(Cell::new(PrivateTouchpadClockSample {
                now_ms: 10,
                now_nanos: 100,
            })),
            unavailable: Rc::new(Cell::new(false)),
        },
        capyio_remote_touchpad_adapter::WindowsSyntheticTouchpadSinkFactory,
    )
    .expect("authorized Windows factory must create the synthetic touchpad");

    assert_eq!(worker.state(), PrivateTouchpadRouteSessionState::Starting);
    assert_eq!(worker.metrics().packets_enqueued, 0);
    assert_eq!(worker.metrics().packets_processed, 0);
    worker
        .stop()
        .expect("synthetic touchpad must close without submitting frames");
    assert_eq!(worker.state(), PrivateTouchpadRouteSessionState::Closed);
    drop(worker);

    runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin Runtime cleanup");
    runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("complete Runtime cleanup");
}

#[cfg(windows)]
#[test]
#[ignore = "moves the real Windows desktop pointer through an authorized synthetic touchpad; requires explicit human approval"]
fn authorized_worker_submits_one_finger_motion_then_releases_and_closes() {
    let fixture = runtime_fixture();
    let runtime = Rc::clone(&fixture.runtime);
    let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let mut worker = PrivateTouchpadRuntimeWorker::<
        RuntimeRouteProvider,
        FakeClock,
        capyio_windows_input::SyntheticTouchpadSession,
    >::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::clone(&clock_sample),
            unavailable: Rc::new(Cell::new(false)),
        },
        capyio_remote_touchpad_adapter::WindowsSyntheticTouchpadSinkFactory,
    )
    .expect("authorized Windows factory must create the synthetic touchpad");
    runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Runtime Route");
    clock_sample.set(PrivateTouchpadClockSample {
        now_ms: 11,
        now_nanos: 200,
    });
    worker.activate().expect("activate worker");

    let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
    let mut cancel = frame(1, 0);
    cancel.kind = TouchpadFrameKind::CancelAll;
    cancel.contacts.clear();
    let mut down = frame(1, 1);
    down.contacts[0].position = TouchpadPosition {
        x_himetric: 2_000,
        y_himetric: 3_000,
    };
    let mut moved = frame(1, 2);
    moved.contacts[0].position = TouchpadPosition {
        x_himetric: 6_500,
        y_himetric: 3_000,
    };
    let mut released = frame(1, 3);
    released.contacts.clear();

    for (index, touchpad_frame) in [cancel, down, moved, released].iter().enumerate() {
        let base_nanos = 1_000_000 + (index as u64 * 1_000_000);
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: base_nanos,
        });
        let packet = codec.encode(touchpad_frame).expect("encode fixture frame");
        worker
            .enqueue(packet.as_bytes())
            .expect("enqueue fixture frame");
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: base_nanos + 100_000,
        });
        assert_eq!(
            worker.tick().expect("pump fixture frame").packets_processed,
            1
        );
        std::thread::sleep(std::time::Duration::from_millis(12));
    }

    assert_eq!(worker.metrics().packets_enqueued, 4);
    assert_eq!(worker.metrics().packets_processed, 4);
    worker.stop().expect("close released synthetic touchpad");
    assert_eq!(worker.state(), PrivateTouchpadRouteSessionState::Closed);
    drop(worker);
    runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin Runtime cleanup");
    runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("complete Runtime cleanup");
}

#[cfg(windows)]
type RealWindowsWorker = PrivateTouchpadRuntimeWorker<
    RuntimeRouteProvider,
    FakeClock,
    capyio_windows_input::SyntheticTouchpadSession,
>;

#[cfg(windows)]
struct ActivatedWindowsWorkerFixture {
    runtime: Rc<RefCell<NodeRuntime>>,
    route_id: RouteId,
    clock_sample: Rc<Cell<PrivateTouchpadClockSample>>,
    worker: RealWindowsWorker,
}

#[cfg(windows)]
impl ActivatedWindowsWorkerFixture {
    fn new() -> Self {
        let fixture = runtime_fixture();
        let runtime = Rc::clone(&fixture.runtime);
        let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
            now_ms: 10,
            now_nanos: 100,
        }));
        let mut worker = RealWindowsWorker::new_with_sink_factory(
            fixture.route_id,
            fixture.sink,
            stream(1),
            descriptor(),
            0,
            limits(),
            RuntimeRouteProvider {
                runtime: Rc::clone(&runtime),
                unavailable: Rc::new(Cell::new(false)),
            },
            FakeClock {
                sample: Rc::clone(&clock_sample),
                unavailable: Rc::new(Cell::new(false)),
            },
            capyio_remote_touchpad_adapter::WindowsSyntheticTouchpadSinkFactory,
        )
        .expect("authorized Windows factory must create the synthetic touchpad");
        runtime
            .borrow_mut()
            .activate_route(fixture.route_id)
            .expect("activate Runtime Route");
        clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 11,
            now_nanos: 200,
        });
        worker.activate().expect("activate worker");
        Self {
            runtime,
            route_id: fixture.route_id,
            clock_sample,
            worker,
        }
    }

    fn submit_frames(&mut self, frames: &[TouchpadFrame]) {
        self.submit_frames_with_interval(frames, 12);
    }

    fn submit_frames_with_interval(&mut self, frames: &[TouchpadFrame], interval_millis: u64) {
        let codec = PrivateTouchpadPacketCodecV1::new(stream(1), descriptor()).expect("codec");
        for (index, touchpad_frame) in frames.iter().enumerate() {
            let base_nanos = 1_000_000 + (index as u64 * 1_000_000);
            self.clock_sample.set(PrivateTouchpadClockSample {
                now_ms: 12 + index as u64,
                now_nanos: base_nanos,
            });
            let packet = codec.encode(touchpad_frame).expect("encode fixture frame");
            self.worker
                .enqueue(packet.as_bytes())
                .expect("enqueue fixture frame");
            self.clock_sample.set(PrivateTouchpadClockSample {
                now_ms: 12 + index as u64,
                now_nanos: base_nanos + 100_000,
            });
            assert_eq!(
                self.worker
                    .tick()
                    .expect("pump fixture frame")
                    .packets_processed,
                1
            );
            std::thread::sleep(std::time::Duration::from_millis(interval_millis));
        }
    }

    fn close(mut self) {
        self.worker
            .stop()
            .expect("close released synthetic touchpad");
        assert_eq!(
            self.worker.state(),
            PrivateTouchpadRouteSessionState::Closed
        );
        drop(self.worker);
        self.runtime
            .borrow_mut()
            .begin_route_stop(self.route_id)
            .expect("begin Runtime cleanup");
        self.runtime
            .borrow_mut()
            .stop_route(self.route_id)
            .expect("complete Runtime cleanup");
    }
}

#[cfg(windows)]
#[test]
#[ignore = "scrolls the active Windows desktop surface through an authorized two-finger synthetic touchpad pan; requires explicit human approval"]
fn authorized_worker_submits_two_finger_pan_then_releases_and_closes() {
    let mut fixture = ActivatedWindowsWorkerFixture::new();
    let mut cancel = frame(1, 0);
    cancel.kind = TouchpadFrameKind::CancelAll;
    cancel.contacts.clear();

    let mut down = frame(1, 1);
    down.contacts[0].position = TouchpadPosition {
        x_himetric: 3_500,
        y_himetric: 2_000,
    };
    down.contacts.push(TouchpadContact {
        contact_id: 2,
        position: TouchpadPosition {
            x_himetric: 6_500,
            y_himetric: 2_000,
        },
        confidence: true,
        size: None,
        pressure: None,
    });

    let mut moved = frame(1, 2);
    moved.contacts[0].position = TouchpadPosition {
        x_himetric: 3_500,
        y_himetric: 4_500,
    };
    moved.contacts.push(TouchpadContact {
        contact_id: 2,
        position: TouchpadPosition {
            x_himetric: 6_500,
            y_himetric: 4_500,
        },
        confidence: true,
        size: None,
        pressure: None,
    });

    let mut released = frame(1, 3);
    released.contacts.clear();
    fixture.submit_frames(&[cancel, down, moved, released]);
    assert_eq!(fixture.worker.metrics().packets_enqueued, 4);
    assert_eq!(fixture.worker.metrics().packets_processed, 4);
    fixture.close();
}

#[cfg(windows)]
#[test]
#[ignore = "may switch the active Windows virtual desktop or application through an authorized three-finger synthetic touchpad swipe; requires explicit human approval"]
fn authorized_worker_submits_three_finger_swipe_then_releases_and_closes() {
    let mut worker_fixture = ActivatedWindowsWorkerFixture::new();
    let gesture_fixture =
        build_touchpad_injection_fixture(SyntheticTouchpadGesture::ThreeFingerSwipe, stream(1));
    assert_eq!(gesture_fixture.gesture.contact_count(), 3);
    worker_fixture
        .submit_frames_with_interval(&gesture_fixture.frames, gesture_fixture.interval_millis);
    assert_eq!(
        worker_fixture.worker.metrics().packets_enqueued,
        gesture_fixture.frames.len() as u64
    );
    assert_eq!(
        worker_fixture.worker.metrics().packets_processed,
        gesture_fixture.frames.len() as u64
    );
    worker_fixture.close();
}

#[cfg(windows)]
#[test]
#[ignore = "may trigger the configured Windows four-finger system action through an authorized synthetic touchpad swipe; requires explicit human approval"]
fn authorized_worker_submits_four_finger_swipe_then_releases_and_closes() {
    let mut worker_fixture = ActivatedWindowsWorkerFixture::new();
    let gesture_fixture =
        build_touchpad_injection_fixture(SyntheticTouchpadGesture::FourFingerSwipe, stream(1));
    assert_eq!(gesture_fixture.gesture.contact_count(), 4);
    worker_fixture
        .submit_frames_with_interval(&gesture_fixture.frames, gesture_fixture.interval_millis);
    assert_eq!(
        worker_fixture.worker.metrics().packets_enqueued,
        gesture_fixture.frames.len() as u64
    );
    assert_eq!(
        worker_fixture.worker.metrics().packets_processed,
        gesture_fixture.frames.len() as u64
    );
    worker_fixture.close();
}

#[derive(Default)]
struct DeliveryChannelState {
    binding: Option<PrivateTouchpadRouteBinding>,
    packets: Vec<Vec<u8>>,
    closes: usize,
}

#[derive(Clone)]
struct RuntimeDeliveryChannel(Rc<RefCell<DeliveryChannelState>>);

impl PrivateTouchpadAdmittedChannel for RuntimeDeliveryChannel {
    fn current_binding(
        &self,
    ) -> Result<PrivateTouchpadRouteBinding, PrivateTouchpadChannelAdmissionError> {
        self.0
            .borrow()
            .binding
            .clone()
            .ok_or(PrivateTouchpadChannelAdmissionError::Unavailable)
    }

    fn send(&mut self, packet: &PrivateTouchpadPacketV1) -> PrivateTouchpadChannelSendOutcome {
        self.0.borrow_mut().packets.push(packet.as_bytes().to_vec());
        PrivateTouchpadChannelSendOutcome::Delivered
    }

    fn close(&mut self) {
        self.0.borrow_mut().closes += 1;
    }
}

fn runtime_binding(runtime: &NodeRuntime, route_id: RouteId) -> PrivateTouchpadRouteBinding {
    let route = runtime.route(route_id).expect("Route");
    let authorization_expires_at_ms = match route.authorization {
        AuthorizationState::Authorized { expires_at_ms } => expires_at_ms,
        actual => panic!("expected authorized Route, got {actual:?}"),
    };
    PrivateTouchpadRouteBinding {
        route_id: route.id,
        session_id: route.session_id,
        source: route.source,
        sink: route.sink,
        route_epoch: route.epoch,
        authorization_expires_at_ms,
    }
}

fn delivery_frame(sequence: u64, kind: TouchpadFrameKind, active: bool) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(1).stream_id,
            stream_epoch: 1,
            sequence,
            source_timestamp_nanos: 1_000 + sequence,
        },
        kind,
        button: TouchpadButtonState::Released,
        contacts: if active {
            frame(1, sequence).contacts
        } else {
            Vec::new()
        },
    }
}

struct DeliveryWorkerFixture {
    runtime: Rc<RefCell<NodeRuntime>>,
    route_id: RouteId,
    clock_sample: Rc<Cell<PrivateTouchpadClockSample>>,
    clock_unavailable: Rc<Cell<bool>>,
    provider_unavailable: Rc<Cell<bool>>,
    channel_state: Rc<RefCell<DeliveryChannelState>>,
    worker: PrivateTouchpadRuntimeDeliveryWorker<
        RuntimeRouteProvider,
        FakeClock,
        RuntimeDeliveryChannel,
    >,
}

fn delivery_worker_fixture() -> DeliveryWorkerFixture {
    let fixture = runtime_fixture();
    fixture
        .runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Runtime Route");
    let binding = runtime_binding(&fixture.runtime.borrow(), fixture.route_id);
    let expected_source = binding.source;
    let clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let clock_unavailable = Rc::new(Cell::new(false));
    let provider_unavailable = Rc::new(Cell::new(false));
    let channel_state = Rc::new(RefCell::new(DeliveryChannelState {
        binding: Some(binding),
        ..DeliveryChannelState::default()
    }));
    let worker = PrivateTouchpadRuntimeDeliveryWorker::new(
        fixture.route_id,
        expected_source,
        stream(1),
        descriptor(),
        0,
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: Rc::clone(&provider_unavailable),
        },
        FakeClock {
            sample: Rc::clone(&clock_sample),
            unavailable: Rc::clone(&clock_unavailable),
        },
        RuntimeDeliveryChannel(Rc::clone(&channel_state)),
    )
    .expect("delivery worker");
    DeliveryWorkerFixture {
        runtime: fixture.runtime,
        route_id: fixture.route_id,
        clock_sample,
        clock_unavailable,
        provider_unavailable,
        channel_state,
        worker,
    }
}

fn set_delivery_clock(fixture: &DeliveryWorkerFixture, now_ms: u64, now_nanos: u64) {
    fixture
        .clock_sample
        .set(PrivateTouchpadClockSample { now_ms, now_nanos });
}

#[test]
fn runtime_delivery_worker_revalidates_real_route_for_each_frame_and_close() {
    let mut fixture = delivery_worker_fixture();
    set_delivery_clock(&fixture, 11, 200);
    fixture
        .worker
        .deliver(&delivery_frame(0, TouchpadFrameKind::CancelAll, false))
        .expect("cancel");
    set_delivery_clock(&fixture, 12, 300);
    fixture
        .worker
        .deliver(&delivery_frame(1, TouchpadFrameKind::Update, true))
        .expect("down");
    set_delivery_clock(&fixture, 13, 400);
    fixture
        .worker
        .deliver(&delivery_frame(2, TouchpadFrameKind::Update, false))
        .expect("release");
    set_delivery_clock(&fixture, 14, 500);
    fixture.worker.close().expect("close");

    assert_eq!(fixture.channel_state.borrow().packets.len(), 3);
    assert_eq!(fixture.channel_state.borrow().closes, 1);
    assert_eq!(fixture.worker.metrics().clock_samples, 5);
    assert_eq!(fixture.worker.metrics().route_snapshots, 5);
    assert_eq!(fixture.worker.metrics().frames_delivered, 3);
    assert_eq!(fixture.worker.metrics().closes, 1);
}

#[test]
fn runtime_delivery_worker_fails_closed_on_authorization_expiry_or_route_stop() {
    let mut expired = delivery_worker_fixture();
    set_delivery_clock(&expired, 10_001, 200);
    assert!(matches!(
        expired
            .worker
            .deliver(&delivery_frame(0, TouchpadFrameKind::CancelAll, false)),
        Err(PrivateTouchpadRuntimeDeliveryError::Binding(_))
    ));
    assert_eq!(expired.channel_state.borrow().closes, 1);
    assert!(expired.channel_state.borrow().packets.is_empty());

    let mut stopped = delivery_worker_fixture();
    stopped
        .runtime
        .borrow_mut()
        .begin_route_stop(stopped.route_id)
        .expect("begin stop");
    set_delivery_clock(&stopped, 11, 200);
    assert!(matches!(
        stopped
            .worker
            .deliver(&delivery_frame(0, TouchpadFrameKind::CancelAll, false)),
        Err(PrivateTouchpadRuntimeDeliveryError::Binding(_))
    ));
    assert_eq!(stopped.channel_state.borrow().closes, 1);
}

#[test]
fn runtime_delivery_worker_fails_closed_on_provider_or_clock_fault() {
    let mut provider = delivery_worker_fixture();
    provider.provider_unavailable.set(true);
    set_delivery_clock(&provider, 11, 200);
    assert!(matches!(
        provider
            .worker
            .deliver(&delivery_frame(0, TouchpadFrameKind::CancelAll, false)),
        Err(PrivateTouchpadRuntimeDeliveryError::RouteProvider(_))
    ));
    assert_eq!(provider.channel_state.borrow().closes, 1);

    let mut clock = delivery_worker_fixture();
    clock.clock_unavailable.set(true);
    assert!(matches!(
        clock
            .worker
            .deliver(&delivery_frame(0, TouchpadFrameKind::CancelAll, false)),
        Err(PrivateTouchpadRuntimeDeliveryError::Clock(_))
    ));
    assert_eq!(clock.channel_state.borrow().closes, 1);

    let mut regression = delivery_worker_fixture();
    set_delivery_clock(&regression, 9, 200);
    assert!(matches!(
        regression
            .worker
            .deliver(&delivery_frame(0, TouchpadFrameKind::CancelAll, false)),
        Err(PrivateTouchpadRuntimeDeliveryError::ClockRegression { .. })
    ));
    assert_eq!(regression.channel_state.borrow().closes, 1);
}

#[test]
fn runtime_delivery_worker_requires_active_route_before_channel_ownership() {
    let fixture = runtime_fixture();
    let route = fixture
        .runtime
        .borrow()
        .route(fixture.route_id)
        .expect("Route")
        .clone();
    let binding = PrivateTouchpadRouteBinding {
        route_id: route.id,
        session_id: route.session_id,
        source: route.source,
        sink: route.sink,
        route_epoch: route.epoch,
        authorization_expires_at_ms: Some(10_000),
    };
    let channel_state = Rc::new(RefCell::new(DeliveryChannelState {
        binding: Some(binding),
        ..DeliveryChannelState::default()
    }));
    let result = PrivateTouchpadRuntimeDeliveryWorker::new(
        fixture.route_id,
        route.source,
        stream(1),
        descriptor(),
        0,
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::new(Cell::new(PrivateTouchpadClockSample {
                now_ms: 10,
                now_nanos: 100,
            })),
            unavailable: Rc::new(Cell::new(false)),
        },
        RuntimeDeliveryChannel(Rc::clone(&channel_state)),
    );
    assert!(matches!(
        result,
        Err(PrivateTouchpadRuntimeDeliveryBuildError::Binding(_))
    ));
    assert_eq!(channel_state.borrow().closes, 1);
}

#[test]
fn bounded_host_channel_connects_runtime_sender_to_runtime_receiver() {
    let fixture = runtime_fixture();
    let receiver_clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let projector_state = Rc::new(RefCell::new(ProjectorState::default()));
    let sink = ProjectingSink {
        projector: WindowsTouchpadProjector::new(&stream(1), descriptor(), 0)
            .expect("Windows projector"),
        shared: Rc::clone(&projector_state),
    };
    let mut receiver_worker = PrivateTouchpadRuntimeWorker::new(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::clone(&receiver_clock_sample),
            unavailable: Rc::new(Cell::new(false)),
        },
        sink,
    )
    .expect("receiver worker");

    fixture
        .runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Runtime Route");
    receiver_clock_sample.set(PrivateTouchpadClockSample {
        now_ms: 11,
        now_nanos: 200,
    });
    receiver_worker.activate().expect("activate receiver");

    let binding = runtime_binding(&fixture.runtime.borrow(), fixture.route_id);
    let expected_source = binding.source;
    let (admission, channel, mut channel_receiver) =
        private_touchpad_host_channel(binding, 4).expect("bounded host channel");
    let sender_clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 11,
        now_nanos: 200,
    }));
    let mut sender_worker = PrivateTouchpadRuntimeDeliveryWorker::new(
        fixture.route_id,
        expected_source,
        stream(1),
        descriptor(),
        0,
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::clone(&sender_clock_sample),
            unavailable: Rc::new(Cell::new(false)),
        },
        channel,
    )
    .expect("sender worker");

    for (index, frame) in [
        delivery_frame(0, TouchpadFrameKind::CancelAll, false),
        delivery_frame(1, TouchpadFrameKind::Update, true),
        delivery_frame(2, TouchpadFrameKind::Update, false),
    ]
    .iter()
    .enumerate()
    {
        sender_clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: 300 + index as u64 * 100,
        });
        sender_worker.deliver(frame).expect("deliver frame");
    }
    sender_clock_sample.set(PrivateTouchpadClockSample {
        now_ms: 15,
        now_nanos: 600,
    });
    sender_worker.close().expect("close sender");

    let mut received = 0_u64;
    loop {
        match channel_receiver.receive() {
            PrivateTouchpadHostChannelReceiveOutcome::Packet(packet) => {
                received += 1;
                receiver_clock_sample.set(PrivateTouchpadClockSample {
                    now_ms: 15 + received,
                    now_nanos: 600 + received * 100,
                });
                receiver_worker
                    .enqueue(packet.as_bytes())
                    .expect("enqueue received packet");
                receiver_clock_sample.set(PrivateTouchpadClockSample {
                    now_ms: 15 + received,
                    now_nanos: 650 + received * 100,
                });
                assert_eq!(
                    receiver_worker
                        .tick()
                        .expect("pump received packet")
                        .packets_processed,
                    1
                );
            }
            PrivateTouchpadHostChannelReceiveOutcome::Closed => break,
            PrivateTouchpadHostChannelReceiveOutcome::Empty => {
                panic!("closed sender left an unexpectedly empty open channel")
            }
        }
    }
    receiver_worker.stop().expect("stop receiver");

    assert_eq!(received, 3);
    assert_eq!(
        projector_state.borrow().dispositions,
        vec![
            WindowsTouchpadProjectionDisposition::Cancelled,
            WindowsTouchpadProjectionDisposition::Applied,
            WindowsTouchpadProjectionDisposition::Applied,
            WindowsTouchpadProjectionDisposition::Cancelled,
        ]
    );
    assert_eq!(projector_state.borrow().closes, 1);
    assert_eq!(admission.metrics().packets_enqueued, 3);
    assert_eq!(admission.metrics().packets_received, 3);
    assert_eq!(admission.metrics().sender_closes, 1);
    assert_eq!(admission.metrics().packets_discarded, 0);
}

#[cfg(windows)]
#[test]
#[ignore = "creates a real Windows synthetic touchpad and moves the desktop pointer through the bounded host channel; requires explicit human approval"]
fn authorized_bounded_host_channel_submits_one_finger_motion_to_real_windows_touchpad() {
    let fixture = runtime_fixture();
    let receiver_clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 10,
        now_nanos: 100,
    }));
    let mut receiver_worker = RealWindowsWorker::new_with_sink_factory(
        fixture.route_id,
        fixture.sink,
        stream(1),
        descriptor(),
        0,
        limits(),
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::clone(&receiver_clock_sample),
            unavailable: Rc::new(Cell::new(false)),
        },
        capyio_remote_touchpad_adapter::WindowsSyntheticTouchpadSinkFactory,
    )
    .expect("authorized Windows factory must create the synthetic touchpad");

    fixture
        .runtime
        .borrow_mut()
        .activate_route(fixture.route_id)
        .expect("activate Runtime Route");
    receiver_clock_sample.set(PrivateTouchpadClockSample {
        now_ms: 11,
        now_nanos: 200,
    });
    receiver_worker.activate().expect("activate receiver");

    let binding = runtime_binding(&fixture.runtime.borrow(), fixture.route_id);
    let expected_source = binding.source;
    let (admission, channel, mut channel_receiver) =
        private_touchpad_host_channel(binding, 4).expect("bounded host channel");
    let sender_clock_sample = Rc::new(Cell::new(PrivateTouchpadClockSample {
        now_ms: 11,
        now_nanos: 200,
    }));
    let mut sender_worker = PrivateTouchpadRuntimeDeliveryWorker::new(
        fixture.route_id,
        expected_source,
        stream(1),
        descriptor(),
        0,
        RuntimeRouteProvider {
            runtime: Rc::clone(&fixture.runtime),
            unavailable: Rc::new(Cell::new(false)),
        },
        FakeClock {
            sample: Rc::clone(&sender_clock_sample),
            unavailable: Rc::new(Cell::new(false)),
        },
        channel,
    )
    .expect("sender worker");

    let mut cancel = frame(1, 0);
    cancel.kind = TouchpadFrameKind::CancelAll;
    cancel.contacts.clear();
    let mut down = frame(1, 1);
    down.contacts[0].position = TouchpadPosition {
        x_himetric: 2_000,
        y_himetric: 3_000,
    };
    let mut moved = frame(1, 2);
    moved.contacts[0].position = TouchpadPosition {
        x_himetric: 6_500,
        y_himetric: 3_000,
    };
    let mut released = frame(1, 3);
    released.contacts.clear();

    for (index, touchpad_frame) in [cancel, down, moved, released].iter().enumerate() {
        sender_clock_sample.set(PrivateTouchpadClockSample {
            now_ms: 12 + index as u64,
            now_nanos: 1_000_000 + index as u64 * 1_000_000,
        });
        sender_worker
            .deliver(touchpad_frame)
            .expect("deliver fixture frame to host channel");
    }
    sender_clock_sample.set(PrivateTouchpadClockSample {
        now_ms: 16,
        now_nanos: 5_000_000,
    });
    sender_worker.close().expect("close sender");

    let mut received = 0_u64;
    loop {
        match channel_receiver.receive() {
            PrivateTouchpadHostChannelReceiveOutcome::Packet(packet) => {
                let base_nanos = 6_000_000 + received * 1_000_000;
                receiver_clock_sample.set(PrivateTouchpadClockSample {
                    now_ms: 17 + received,
                    now_nanos: base_nanos,
                });
                receiver_worker
                    .enqueue(packet.as_bytes())
                    .expect("enqueue channel packet");
                receiver_clock_sample.set(PrivateTouchpadClockSample {
                    now_ms: 17 + received,
                    now_nanos: base_nanos + 100_000,
                });
                assert_eq!(
                    receiver_worker
                        .tick()
                        .expect("submit channel packet to Windows")
                        .packets_processed,
                    1
                );
                received += 1;
                std::thread::sleep(std::time::Duration::from_millis(12));
            }
            PrivateTouchpadHostChannelReceiveOutcome::Closed => break,
            PrivateTouchpadHostChannelReceiveOutcome::Empty => {
                panic!("closed sender left an unexpectedly empty open channel")
            }
        }
    }

    assert_eq!(received, 4);
    assert_eq!(receiver_worker.metrics().packets_enqueued, 4);
    assert_eq!(receiver_worker.metrics().packets_processed, 4);
    assert_eq!(admission.metrics().packets_enqueued, 4);
    assert_eq!(admission.metrics().packets_received, 4);
    receiver_worker
        .stop()
        .expect("close released synthetic touchpad");
    drop(receiver_worker);
    fixture
        .runtime
        .borrow_mut()
        .begin_route_stop(fixture.route_id)
        .expect("begin Runtime cleanup");
    fixture
        .runtime
        .borrow_mut()
        .stop_route(fixture.route_id)
        .expect("complete Runtime cleanup");
}
