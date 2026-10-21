//! Explicit synthetic private-graph acceptance fixture, never a product launcher.
use capyio_core::{InteroperabilityMode, Platform, QosMode, RouteBackend, StreamId};
use capyio_linux_camera::{CameraBinding, FRAME_BYTES, FRAME_DURATION_NS, RuntimeCamera};
use capyio_runtime::{NodeRuntime, PortLeasePolicy};
use capyio_video::{VideoFrameDescriptor, VideoFrameFlags, VideoStreamSpec};
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
    // Harness must approve before Runtime grant or helper creation.
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    if line.trim() != "approve synthetic camera" {
        return Err("approval missing".into());
    }
    let scenario = std::env::args().nth(2).unwrap_or_else(|| "pixels".into());
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let (runtime, binding) = fixture(now);
    let mut owner = RuntimeCamera::start(
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
    io::stdout().flush()?;
    // The fixture alone waits for the exact graph links to be installed.
    line.clear();
    io::stdin().lock().read_line(&mut line)?;
    if line.trim() != "start frames" {
        return Err("frame start missing".into());
    }
    let start = Instant::now();
    for sequence in 1..=if scenario == "expiry" { 300u64 } else { 90u64 } {
        let mut payload = vec![0u8; FRAME_BYTES];
        if scenario != "no_consumer" {
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte = if i < 1280 * 720 {
                    (16 + (i % 1280 + 3 * (i / 1280) + sequence as usize % 220) % 220) as u8
                } else {
                    let j = i - 1280 * 720;
                    (16 + ((j / 2) % 1280
                        + if j % 2 == 1 { 97 } else { 23 }
                        + sequence as usize % 225)
                        % 225) as u8
                };
            }
        }
        let frame = VideoFrameDescriptor {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence,
            source_timestamp_nanos: sequence * FRAME_DURATION_NS,
            duration_nanos: FRAME_DURATION_NS,
            payload_bytes: FRAME_BYTES as u64,
            flags: VideoFrameFlags::default(),
        };
        owner.submit(frame, payload)?;
        let deadline = start + Duration::from_nanos(sequence * FRAME_DURATION_NS);
        if let Some(wait) = deadline
            .checked_duration_since(Instant::now())
            .filter(|_| scenario != "no_consumer")
        {
            std::thread::sleep(wait);
        }
    }
    if scenario == "no_consumer" && owner.metrics().dropped == 0 {
        return Err("backpressure was not exercised".into());
    }
    owner.stop()?;
    println!(
        "CAMERA_STOPPED queued={} dropped={}",
        owner.metrics().queued,
        owner.metrics().dropped
    );
    Ok(())
}
