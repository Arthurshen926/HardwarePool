//! Explicit synthetic private-graph acceptance fixture, never a product launcher.
use capyio_core::{InteroperabilityMode, Platform, QosMode, RouteBackend, StreamId};
use capyio_linux_avc::RuntimeAvcCamera;
use capyio_linux_camera::{CameraBinding, FRAME_DURATION_NS};
use capyio_runtime::{NodeRuntime, PortLeasePolicy};
use capyio_vcamdroid_adapter::{
    AvcAccessUnit, AvcRecord, AvcStreamKey, decode_record, encode_access_unit, encode_config,
};
use capyio_video::VideoStreamSpec;
use std::{
    io::{self, BufRead, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn fixture(now: u64) -> (NodeRuntime, CameraBinding) {
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let original = lab
        .runtime
        .route(lab.routes.phone_camera_to_panel)
        .unwrap()
        .clone();
    let snapshot = lab.runtime.snapshot();
    let mut local = snapshot.local_node;
    local.platform = Platform::Linux;
    let mut peer = snapshot.peers[0].clone();
    for node in [&mut local, &mut peer] {
        for adapter in node.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        for cap in node.capabilities.values_mut() {
            cap.availability = capyio_core::Availability::Available;
            for port in cap.ports.values_mut() {
                port.availability = capyio_core::Availability::Available;
                port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                if port.profile == capyio_video::video_frames_profile() {
                    port.formats = vec![VideoStreamSpec::camera_720p30_nv12().format_descriptor()];
                    port.qos_modes.insert(QosMode::Interactive);
                }
            }
        }
    }
    let peer_id = peer.id;
    let mut runtime = NodeRuntime::new(local).unwrap();
    runtime.register_peer(peer, true).unwrap();
    let session_id = runtime.open_session(peer_id).unwrap();
    let route_id = runtime
        .create_route(
            session_id,
            original.source,
            original.sink,
            RouteBackend::AdapterManaged,
        )
        .unwrap();
    runtime
        .set_local_port_lease_policy(original.sink, PortLeasePolicy::Exclusive)
        .unwrap();
    runtime
        .authorize_route(route_id, Some(now + 20_000))
        .unwrap();
    runtime
        .acquire_route_port_lease(route_id, original.sink, now + 15_000, now)
        .unwrap();
    runtime
        .prepare_route(
            route_id,
            Some(VideoStreamSpec::camera_720p30_nv12().format_descriptor()),
            QosMode::Interactive,
            now,
        )
        .unwrap();
    runtime.begin_route_start(route_id, now).unwrap();
    (
        runtime,
        CameraBinding {
            session_id,
            route_id,
            stream_id: StreamId::new(),
            stream_epoch: 1,
            local_sink: original.sink,
        },
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() != Some("--synthetic-private-graph") {
        return Err("explicit synthetic private-graph opt-in required".into());
    }
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    if line.trim() != "approve synthetic camera" {
        return Err("approval missing".into());
    }
    let scenario = std::env::args().nth(2).unwrap_or_else(|| "pixels".into());
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let (runtime, binding) = fixture(now);
    let mut owner = RuntimeAvcCamera::start(
        runtime,
        binding.clone(),
        if scenario == "expiry" {
            Duration::from_secs(4)
        } else {
            Duration::from_secs(12)
        },
    )?;
    println!(
        "CAMERA_READY {} {}",
        owner.helper_pid().unwrap(),
        owner.node_name().unwrap()
    );
    println!("DECODER_READY {}", owner.decoder_pid().unwrap());
    io::stdout().flush()?;
    line.clear();
    io::stdin().lock().read_line(&mut line)?;
    if line.trim() != "start frames" {
        return Err("frame start missing".into());
    }
    let stream = AvcStreamKey {
        stream_id: *binding.stream_id.as_uuid().as_bytes(),
        epoch: binding.stream_epoch,
    };
    let bytes = include_bytes!("../../../../fixtures/linux-avc/solid-720p30.cavc");
    let mut pos = 0;
    let mut units = Vec::new();
    while pos < bytes.len() {
        let n = 56 + u32::from_be_bytes(bytes[pos + 52..pos + 56].try_into()?) as usize;
        match decode_record(&bytes[pos..pos + n])? {
            AvcRecord::Config { config, .. } => {
                owner.submit_record(&encode_config(stream, &config)?)?;
            }
            AvcRecord::AccessUnit(u) if !u.end_of_stream => units.push(u),
            _ => {}
        }
        pos += n;
    }
    let start = Instant::now();
    for sequence in 1..=90u64 {
        if scenario == "revoke" && sequence == 4 {
            owner.runtime_mut().release_route_port_lease(
                binding.route_id,
                binding.local_sink,
                binding.session_id,
            )?;
        }
        if scenario == "malformed_cavc" && sequence == 4 {
            owner.submit_record(b"CAVC")?;
        }
        let u = AvcAccessUnit {
            stream,
            sequence,
            presentation_time_us: sequence * 33333,
            ..units[(sequence as usize - 1) % 3].clone()
        };
        owner.submit_record(&encode_access_unit(&u)?)?;
        let deadline = start + Duration::from_nanos(sequence * FRAME_DURATION_NS);
        if scenario != "no_consumer"
            && let Some(wait) = deadline.checked_duration_since(Instant::now())
        {
            std::thread::sleep(wait);
        }
    }
    if scenario == "no_consumer" && owner.metrics().dropped == 0 {
        return Err("backpressure not exercised".into());
    }
    if scenario == "expiry" {
        std::thread::sleep(Duration::from_secs(2));
        owner.poll()?;
    }
    let eos = AvcAccessUnit {
        stream,
        sequence: 91,
        presentation_time_us: 91 * 33333,
        key_frame: false,
        end_of_stream: true,
        discontinuity: false,
        payload: vec![],
    };
    owner.submit_record(&encode_access_unit(&eos)?)?;
    println!(
        "CAMERA_STOPPED queued={} dropped={}",
        owner.metrics().queued,
        owner.metrics().dropped
    );
    Ok(())
}
