use std::{
    error::Error,
    io::{self, ErrorKind},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

use capyio_vcamdroid_adapter::{AvcRecord, AvcRecordGuard, read_avc_record};
#[cfg(windows)]
use capyio_vcamdroid_adapter::{DecodedNv12Frame, MfAvcDecoder, StageLatencyStats};
#[cfg(windows)]
use {
    capyio_core::StreamId,
    capyio_video::{VideoFrameDescriptor, VideoFrameFlags},
    capyio_windows_camera::{GeneratedVideoFrame, fixture_stream_spec},
    capyio_windows_camera_host::CameraProducerHost,
    uuid::Uuid,
};

const DEFAULT_PORT: u16 = 38_173;
const DEFAULT_MAX_ACCESS_UNITS: u64 = 90;
const MAX_ACCESS_UNITS: u64 = 7_200;
const MAX_RECONNECT_GRACE_MILLIS: u64 = 60_000;

fn main() {
    if let Err(error) = run() {
        eprintln!("CAPYIO_AVC_LAB_ERROR {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let options = Options::parse()?;
    let listener = TcpListener::bind(SocketAddrV4::new(options.bind_ip, options.port))?;
    println!(
        "CAPYIO_AVC_LAB_READY transport={} port={} max_access_units={} decode_nv12={} publish_scope={}",
        options.transport_mode.as_str(),
        options.port,
        options.max_access_units,
        options.decode_nv12,
        options.publish_scope.as_str()
    );

    let mut stream_id = None;
    let mut epoch = 0;
    let mut width = 0;
    let mut height = 0;
    let mut access_units = 0_u64;
    let mut key_frames = 0_u64;
    let mut discontinuities = 0_u64;
    let mut payload_bytes = 0_u64;
    let mut last_sequence = 0_u64;
    let mut decoded_frames = 0_u64;
    let mut decoded_bytes = 0_u64;
    let mut first_decoded_checksum = None;
    let mut last_decoded_checksum = None;
    let mut last_decoded_sequence = 0_u64;
    let mut decoder_low_latency = false;
    let mut max_decoder_pending_samples = 0_usize;
    #[cfg(windows)]
    let mut decoder_latency = StageLatencyStats::default();
    #[cfg(windows)]
    let mut publisher: Option<SharedFramePublisher> = None;
    let mut next_connection = Some(accept_authorized(&listener, options.allowed_peer_ip)?);
    let mut connections = 0_u64;
    'connections: loop {
        let (mut connection, peer) = next_connection
            .take()
            .ok_or("receiver connection state was empty")?;
        if !peer_is_authorized(peer, options.allowed_peer_ip) {
            return Err("lab receiver accepted a peer outside its exact allowlist".into());
        }
        connection.set_read_timeout(Some(Duration::from_secs(15)))?;
        connections += 1;
        let mut guard = None;
        #[cfg(windows)]
        let mut decoder = None;

        while let Some(record) = read_avc_record(&mut connection)? {
            match &record {
                AvcRecord::Config { stream, config } => {
                    if guard.is_some() {
                        return Err("received more than one config record".into());
                    }
                    let mut created = AvcRecordGuard::new(*stream)?;
                    created.accept(&record)?;
                    stream_id = Some(stream.stream_id);
                    epoch = stream.epoch;
                    width = config.width;
                    height = config.height;
                    if options.decode_nv12 {
                        #[cfg(windows)]
                        {
                            let created_decoder = MfAvcDecoder::new(config)?;
                            decoder_low_latency = created_decoder.low_latency_enabled();
                            decoder = Some(created_decoder);
                            if options.publish_scope != PublishScope::None {
                                if let Some(active_publisher) = publisher.as_mut() {
                                    active_publisher.begin_session(
                                        config.width,
                                        config.height,
                                        config.frames_per_second,
                                        config.clockwise_rotation_degrees,
                                    )?;
                                } else {
                                    publisher = Some(SharedFramePublisher::start(
                                        stream.stream_id,
                                        stream.epoch,
                                        config.width,
                                        config.height,
                                        config.frames_per_second,
                                        config.clockwise_rotation_degrees,
                                        options.publish_scope,
                                    )?);
                                }
                            }
                        }
                        #[cfg(not(windows))]
                        {
                            return Err("--decode-nv12 is available on Windows only".into());
                        }
                    }
                    guard = Some(created);
                    println!(
                        "CAPYIO_AVC_LAB_CONFIG connection={} stream={} epoch={} size={}x{} fps={} bitrate={} rotation={} access_layout={:?} config_layout={:?} decoder_low_latency={}",
                        connections,
                        hex_stream(stream.stream_id),
                        stream.epoch,
                        config.width,
                        config.height,
                        config.frames_per_second,
                        config.bitrate_bits_per_second,
                        config.clockwise_rotation_degrees,
                        config.access_unit_layout,
                        config.codec_specific_layout,
                        decoder_low_latency
                    );
                }
                AvcRecord::AccessUnit(unit) => {
                    let active_guard = guard
                        .as_mut()
                        .ok_or("received an access unit before config")?;
                    active_guard.accept(&record)?;
                    if unit.end_of_stream {
                        break;
                    }
                    access_units += 1;
                    key_frames += u64::from(unit.key_frame);
                    discontinuities += u64::from(unit.discontinuity);
                    payload_bytes = payload_bytes
                        .checked_add(unit.payload.len() as u64)
                        .ok_or("payload byte counter overflowed")?;
                    last_sequence = unit.sequence;
                    if options.decode_nv12 {
                        #[cfg(windows)]
                        {
                            let active_decoder = decoder
                                .as_mut()
                                .ok_or("decoder was not initialized by the config record")?;
                            let frames = active_decoder.decode(unit)?;
                            max_decoder_pending_samples =
                                max_decoder_pending_samples.max(active_decoder.pending_samples());
                            for frame in frames {
                                observe_decoded_frame(
                                    frame,
                                    &mut decoded_frames,
                                    &mut decoded_bytes,
                                    &mut first_decoded_checksum,
                                    &mut last_decoded_checksum,
                                    &mut last_decoded_sequence,
                                    publisher.as_mut(),
                                )?;
                            }
                        }
                    }
                    if access_units >= options.max_access_units {
                        break;
                    }
                }
            }
        }

        if options.decode_nv12 {
            #[cfg(windows)]
            {
                let active_decoder = decoder
                    .as_mut()
                    .ok_or("connection ended before decoder configuration")?;
                for frame in active_decoder.finish()? {
                    observe_decoded_frame(
                        frame,
                        &mut decoded_frames,
                        &mut decoded_bytes,
                        &mut first_decoded_checksum,
                        &mut last_decoded_checksum,
                        &mut last_decoded_sequence,
                        publisher.as_mut(),
                    )?;
                }
                merge_latency_stats(&mut decoder_latency, active_decoder.latency_stats());
            }
        }

        if access_units >= options.max_access_units {
            break 'connections;
        }
        let Some(grace) = options.reconnect_grace else {
            break 'connections;
        };
        println!(
            "CAPYIO_AVC_LAB_RECONNECT_WAIT completed_connections={} grace_millis={}",
            connections,
            grace.as_millis()
        );
        let Some(accepted) = accept_with_grace(&listener, grace, options.allowed_peer_ip)? else {
            break 'connections;
        };
        next_connection = Some(accepted);
    }

    let stream_id = stream_id.ok_or("connection ended before config")?;
    if access_units == 0 || key_frames == 0 {
        return Err("connection did not contain a key-framed AVC access unit".into());
    }
    println!(
        "CAPYIO_AVC_LAB_OK stream={} epoch={} size={}x{} access_units={} key_frames={} discontinuities={} payload_bytes={} last_sequence={}",
        hex_stream(stream_id),
        epoch,
        width,
        height,
        access_units,
        key_frames,
        discontinuities,
        payload_bytes,
        last_sequence
    );
    if options.decode_nv12 {
        if decoded_frames == 0 {
            return Err("Media Foundation did not produce an NV12 frame".into());
        }
        let (decoder_latency_samples, decoder_latency_average_us, decoder_latency_max_us) = {
            #[cfg(windows)]
            {
                let stats = decoder_latency;
                (stats.samples, stats.average_micros(), stats.max_micros)
            }
            #[cfg(not(windows))]
            {
                (0, 0, 0)
            }
        };
        let (publish_latency_samples, publish_latency_average_us, publish_latency_max_us) = {
            #[cfg(windows)]
            {
                let stats = publisher
                    .as_ref()
                    .map(SharedFramePublisher::latency_stats)
                    .unwrap_or_default();
                (stats.samples, stats.average_micros(), stats.max_micros)
            }
            #[cfg(not(windows))]
            {
                (0, 0, 0)
            }
        };
        println!(
            "CAPYIO_AVC_LAB_DECODE_OK decoded_frames={} decoded_bytes={} first_checksum={:016x} last_checksum={:016x} last_source_sequence={} decoder_low_latency={} max_pending_samples={} decoder_latency_samples={} decoder_latency_average_us={} decoder_latency_max_us={} publish_latency_samples={} publish_latency_average_us={} publish_latency_max_us={}",
            decoded_frames,
            decoded_bytes,
            first_decoded_checksum.ok_or("missing first decoded checksum")?,
            last_decoded_checksum.ok_or("missing last decoded checksum")?,
            last_decoded_sequence,
            decoder_low_latency,
            max_decoder_pending_samples,
            decoder_latency_samples,
            decoder_latency_average_us,
            decoder_latency_max_us,
            publish_latency_samples,
            publish_latency_average_us,
            publish_latency_max_us
        );
    }
    if options.publish_scope != PublishScope::None {
        #[cfg(windows)]
        {
            let active_publisher = publisher
                .as_ref()
                .ok_or("shared publisher was not initialized by the config record")?;
            if active_publisher.published_frames() != decoded_frames {
                return Err("shared publication count did not match decoded frames".into());
            }
            println!(
                "CAPYIO_AVC_LAB_PUBLISH_OK scope={} published_frames={} last_source_sequence={}",
                options.publish_scope.as_str(),
                active_publisher.published_frames(),
                last_decoded_sequence
            );
        }
    }
    Ok(())
}

#[cfg(windows)]
fn observe_decoded_frame(
    frame: DecodedNv12Frame,
    decoded_frames: &mut u64,
    decoded_bytes: &mut u64,
    first_checksum: &mut Option<u64>,
    last_checksum: &mut Option<u64>,
    last_sequence: &mut u64,
    publisher: Option<&mut SharedFramePublisher>,
) -> Result<(), Box<dyn Error>> {
    let expected_bytes = usize::try_from(frame.width)?
        .checked_mul(usize::try_from(frame.height)?)
        .and_then(|pixels| pixels.checked_mul(3))
        .and_then(|bytes| bytes.checked_div(2))
        .ok_or("decoded NV12 size overflowed")?;
    if frame.payload.len() != expected_bytes {
        return Err(format!(
            "decoded NV12 payload has {} bytes; expected {expected_bytes}",
            frame.payload.len()
        )
        .into());
    }
    let checksum = fnv1a64(&frame.payload);
    *decoded_frames = decoded_frames
        .checked_add(1)
        .ok_or("decoded frame counter overflowed")?;
    *decoded_bytes = decoded_bytes
        .checked_add(frame.payload.len() as u64)
        .ok_or("decoded byte counter overflowed")?;
    first_checksum.get_or_insert(checksum);
    *last_checksum = Some(checksum);
    *last_sequence = frame.source_sequence;
    if let Some(publisher) = publisher {
        publisher.publish(frame)?;
    }
    Ok(())
}

fn accept_authorized(
    listener: &TcpListener,
    allowed_peer_ip: Ipv4Addr,
) -> io::Result<(TcpStream, SocketAddr)> {
    loop {
        let (stream, peer) = listener.accept()?;
        if peer_is_authorized(peer, allowed_peer_ip) {
            return Ok((stream, peer));
        }
        drop(stream);
        thread::sleep(Duration::from_millis(10));
    }
}

fn accept_with_grace(
    listener: &TcpListener,
    grace: Duration,
    allowed_peer_ip: Ipv4Addr,
) -> io::Result<Option<(TcpStream, SocketAddr)>> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + grace;
    loop {
        match listener.accept() {
            Ok((stream, peer)) => {
                if !peer_is_authorized(peer, allowed_peer_ip) {
                    drop(stream);
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                // Windows may inherit the listener's nonblocking mode on the
                // accepted socket. Record decoding is deliberately blocking
                // with a bounded read timeout, so restore that contract before
                // handing the reconnect to the stream loop.
                stream.set_nonblocking(false)?;
                return Ok(Some((stream, peer)));
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Ok(None);
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

fn peer_is_authorized(peer: SocketAddr, allowed_peer_ip: Ipv4Addr) -> bool {
    matches!(peer, SocketAddr::V4(peer) if *peer.ip() == allowed_peer_ip)
}

#[cfg(windows)]
fn merge_latency_stats(total: &mut StageLatencyStats, next: StageLatencyStats) {
    total.samples = total.samples.saturating_add(next.samples);
    total.total_micros = total.total_micros.saturating_add(next.total_micros);
    total.max_micros = total.max_micros.max(next.max_micros);
}

#[cfg(windows)]
struct SharedFramePublisher {
    host: CameraProducerHost,
    stream_id: StreamId,
    stream_epoch: u64,
    duration_nanos: u64,
    latency_stats: StageLatencyStats,
    force_discontinuity: bool,
    clockwise_rotation_degrees: u16,
}

#[cfg(windows)]
impl SharedFramePublisher {
    fn start(
        stream_bytes: [u8; 16],
        stream_epoch: u64,
        width: u16,
        height: u16,
        frames_per_second: u16,
        clockwise_rotation_degrees: u16,
        publish_scope: PublishScope,
    ) -> Result<Self, Box<dyn Error>> {
        let selected = fixture_stream_spec();
        if u32::from(width) != selected.width
            || u32::from(height) != selected.height
            || u32::from(frames_per_second) != selected.frame_rate.numerator()
            || selected.frame_rate.denominator() != 1
        {
            return Err(
                "shared camera ingress accepts canonical 1280x720 NV12 at 30 fps only".into(),
            );
        }
        let stream_id = StreamId::from_uuid(Uuid::from_bytes(stream_bytes));
        let mut host = CameraProducerHost::new(stream_id, stream_epoch)?;
        match publish_scope {
            PublishScope::None => return Err("shared publisher scope is missing".into()),
            PublishScope::Global => host.start()?,
            PublishScope::LocalLab => host.start_local_lab()?,
        }
        Ok(Self {
            host,
            stream_id,
            stream_epoch,
            duration_nanos: 1_000_000_000_u64 / u64::from(frames_per_second),
            latency_stats: StageLatencyStats::default(),
            force_discontinuity: true,
            clockwise_rotation_degrees,
        })
    }

    fn begin_session(
        &mut self,
        width: u16,
        height: u16,
        frames_per_second: u16,
        clockwise_rotation_degrees: u16,
    ) -> Result<(), Box<dyn Error>> {
        let selected = fixture_stream_spec();
        if u32::from(width) != selected.width
            || u32::from(height) != selected.height
            || u32::from(frames_per_second) != selected.frame_rate.numerator()
            || selected.frame_rate.denominator() != 1
        {
            return Err(
                "reconnected shared camera ingress must remain canonical 1280x720 NV12 at 30 fps"
                    .into(),
            );
        }
        self.force_discontinuity = true;
        self.clockwise_rotation_degrees = clockwise_rotation_degrees;
        Ok(())
    }

    fn publish(&mut self, mut frame: DecodedNv12Frame) -> Result<(), Box<dyn Error>> {
        let publish_started = Instant::now();
        frame = orient_nv12_for_landscape(frame, self.clockwise_rotation_degrees)?;
        let (sequence, presentation_time_us) =
            next_publication_timing(self.host.published_frames(), self.duration_nanos)?;
        frame.source_sequence = sequence;
        frame.presentation_time_us = presentation_time_us;
        frame.discontinuity |= self.force_discontinuity;
        self.force_discontinuity = false;
        let generated = decoded_to_generated_frame(
            frame,
            self.stream_id,
            self.stream_epoch,
            self.duration_nanos,
        )?;
        self.host.publish(generated)?;
        self.latency_stats.observe(publish_started.elapsed());
        Ok(())
    }

    fn published_frames(&self) -> u64 {
        self.host.published_frames()
    }

    fn latency_stats(&self) -> StageLatencyStats {
        self.latency_stats
    }
}

#[cfg(windows)]
fn orient_nv12_for_landscape(
    frame: DecodedNv12Frame,
    clockwise_rotation_degrees: u16,
) -> Result<DecodedNv12Frame, Box<dyn Error>> {
    if clockwise_rotation_degrees == 0 {
        return Ok(frame);
    }
    let width = usize::try_from(frame.width)?;
    let height = usize::try_from(frame.height)?;
    if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
        return Err("NV12 orientation requires positive even dimensions".into());
    }
    let expected = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(3))
        .and_then(|bytes| bytes.checked_div(2))
        .ok_or("NV12 orientation size overflowed")?;
    if frame.payload.len() != expected {
        return Err("NV12 orientation payload length is invalid".into());
    }
    let mut output = vec![16_u8; expected];
    output[width * height..].fill(128);
    match clockwise_rotation_degrees {
        180 => rotate_nv12_180(&frame.payload, &mut output, width, height),
        90 | 270 => rotate_nv12_portrait_pillarbox(
            &frame.payload,
            &mut output,
            width,
            height,
            clockwise_rotation_degrees,
        )?,
        _ => return Err("NV12 orientation is not 0/90/180/270 degrees".into()),
    }
    Ok(DecodedNv12Frame {
        payload: output,
        ..frame
    })
}

#[cfg(windows)]
fn rotate_nv12_180(input: &[u8], output: &mut [u8], width: usize, height: usize) {
    for y in 0..height {
        for x in 0..width {
            output[y * width + x] = input[(height - 1 - y) * width + (width - 1 - x)];
        }
    }
    let y_bytes = width * height;
    let chroma_width = width / 2;
    let chroma_height = height / 2;
    for y in 0..chroma_height {
        for x in 0..chroma_width {
            let source = y_bytes + (chroma_height - 1 - y) * width + (chroma_width - 1 - x) * 2;
            let target = y_bytes + y * width + x * 2;
            output[target..target + 2].copy_from_slice(&input[source..source + 2]);
        }
    }
}

#[cfg(windows)]
fn rotate_nv12_portrait_pillarbox(
    input: &[u8],
    output: &mut [u8],
    width: usize,
    height: usize,
    rotation: u16,
) -> Result<(), Box<dyn Error>> {
    let raw_width = height
        .checked_mul(height)
        .and_then(|value| value.checked_div(width))
        .ok_or("NV12 portrait fit overflowed")?;
    let content_width = raw_width.max(4).min(width) & !3;
    if content_width == 0 {
        return Err("NV12 portrait fit is empty".into());
    }
    let x_offset = (width - content_width) / 2;
    for y in 0..height {
        let rotated_y = y * width / height;
        for x in 0..content_width {
            let rotated_x = x * height / content_width;
            let (source_x, source_y) = if rotation == 90 {
                (rotated_y, height - 1 - rotated_x)
            } else {
                (width - 1 - rotated_y, rotated_x)
            };
            output[y * width + x_offset + x] = input[source_y * width + source_x];
        }
    }

    let y_bytes = width * height;
    let chroma_width = width / 2;
    let chroma_height = height / 2;
    let content_chroma_width = content_width / 2;
    let x_offset_chroma = x_offset / 2;
    for y in 0..chroma_height {
        let rotated_y = y * chroma_width / chroma_height;
        for x in 0..content_chroma_width {
            let rotated_x = x * chroma_height / content_chroma_width;
            let (source_x, source_y) = if rotation == 90 {
                (rotated_y, chroma_height - 1 - rotated_x)
            } else {
                (chroma_width - 1 - rotated_y, rotated_x)
            };
            let source = y_bytes + source_y * width + source_x * 2;
            let target = y_bytes + y * width + (x_offset_chroma + x) * 2;
            output[target..target + 2].copy_from_slice(&input[source..source + 2]);
        }
    }
    Ok(())
}

fn next_publication_timing(
    published_frames: u64,
    duration_nanos: u64,
) -> Result<(u64, u64), &'static str> {
    let sequence = published_frames
        .checked_add(1)
        .ok_or("shared publication sequence overflowed")?;
    let presentation_time_us = published_frames
        .checked_mul(duration_nanos / 1_000)
        .ok_or("shared publication timestamp overflowed")?;
    Ok((sequence, presentation_time_us))
}

#[cfg(windows)]
fn decoded_to_generated_frame(
    frame: DecodedNv12Frame,
    stream_id: StreamId,
    stream_epoch: u64,
    duration_nanos: u64,
) -> Result<GeneratedVideoFrame, Box<dyn Error>> {
    let source_timestamp_nanos = frame
        .presentation_time_us
        .checked_mul(1_000)
        .ok_or("decoded source timestamp overflowed nanoseconds")?;
    let payload_bytes = u64::try_from(frame.payload.len())?;
    Ok(GeneratedVideoFrame {
        descriptor: VideoFrameDescriptor {
            stream_id,
            stream_epoch,
            sequence: frame.source_sequence,
            source_timestamp_nanos,
            duration_nanos,
            payload_bytes,
            flags: VideoFrameFlags {
                discontinuity: frame.discontinuity,
                end_of_stream: false,
            },
        },
        payload: frame.payload,
    })
}

#[cfg(windows)]
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn hex_stream(stream_id: [u8; 16]) -> String {
    stream_id
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect()
}

struct Options {
    port: u16,
    bind_ip: Ipv4Addr,
    allowed_peer_ip: Ipv4Addr,
    transport_mode: TransportMode,
    max_access_units: u64,
    decode_nv12: bool,
    publish_scope: PublishScope,
    reconnect_grace: Option<Duration>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransportMode {
    AdbReverse,
    TrustedLanLab,
}

impl TransportMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AdbReverse => "adb-reverse-lab",
            Self::TrustedLanLab => "trusted-lan-lab",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublishScope {
    None,
    Global,
    LocalLab,
}

impl PublishScope {
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Global => "global",
            Self::LocalLab => "local-lab",
        }
    }
}

impl Options {
    fn parse() -> Result<Self, Box<dyn Error>> {
        Self::parse_from(std::env::args().skip(1))
    }

    fn parse_from(mut arguments: impl Iterator<Item = String>) -> Result<Self, Box<dyn Error>> {
        let mut port = DEFAULT_PORT;
        let mut max_access_units = DEFAULT_MAX_ACCESS_UNITS;
        let mut decode_nv12 = false;
        let mut publish_scope = PublishScope::None;
        let mut reconnect_grace = None;
        let mut trusted_lan_bind = None;
        let mut trusted_lan_peer = None;
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--port" => {
                    port = arguments.next().ok_or("--port requires a value")?.parse()?;
                    if port == 0 {
                        return Err("--port must be positive".into());
                    }
                }
                "--max-access-units" => {
                    max_access_units = arguments
                        .next()
                        .ok_or("--max-access-units requires a value")?
                        .parse()?;
                    if max_access_units == 0 || max_access_units > MAX_ACCESS_UNITS {
                        return Err("--max-access-units is outside 1..=7200".into());
                    }
                }
                "--decode-nv12" => decode_nv12 = true,
                "--reconnect-grace-millis" => {
                    let millis: u64 = arguments
                        .next()
                        .ok_or("--reconnect-grace-millis requires a value")?
                        .parse()?;
                    if millis == 0 || millis > MAX_RECONNECT_GRACE_MILLIS {
                        return Err("--reconnect-grace-millis is outside 1..=60000".into());
                    }
                    reconnect_grace = Some(Duration::from_millis(millis));
                }
                "--trusted-lan-bind" => {
                    if trusted_lan_bind.is_some() {
                        return Err("--trusted-lan-bind was provided more than once".into());
                    }
                    trusted_lan_bind = Some(parse_trusted_lan_ipv4(
                        &arguments
                            .next()
                            .ok_or("--trusted-lan-bind requires an IPv4 literal")?,
                        "--trusted-lan-bind",
                    )?);
                }
                "--trusted-lan-peer" => {
                    if trusted_lan_peer.is_some() {
                        return Err("--trusted-lan-peer was provided more than once".into());
                    }
                    trusted_lan_peer = Some(parse_trusted_lan_ipv4(
                        &arguments
                            .next()
                            .ok_or("--trusted-lan-peer requires an IPv4 literal")?,
                        "--trusted-lan-peer",
                    )?);
                }
                "--publish-shared" => {
                    if publish_scope != PublishScope::None {
                        return Err("shared publication scope was selected more than once".into());
                    }
                    decode_nv12 = true;
                    publish_scope = PublishScope::Global;
                }
                "--publish-shared-local-lab" => {
                    if publish_scope != PublishScope::None {
                        return Err("shared publication scope was selected more than once".into());
                    }
                    decode_nv12 = true;
                    publish_scope = PublishScope::LocalLab;
                }
                _ => return Err(format!("unknown argument: {argument}").into()),
            }
        }
        let (bind_ip, allowed_peer_ip, transport_mode) = match (trusted_lan_bind, trusted_lan_peer)
        {
            (None, None) => (
                Ipv4Addr::LOCALHOST,
                Ipv4Addr::LOCALHOST,
                TransportMode::AdbReverse,
            ),
            (Some(bind_ip), Some(peer_ip)) => {
                if bind_ip == peer_ip {
                    return Err("trusted LAN bind and peer IPv4 literals must be different".into());
                }
                if port != DEFAULT_PORT {
                    return Err("trusted LAN camera lab is fixed to TCP port 38173".into());
                }
                (bind_ip, peer_ip, TransportMode::TrustedLanLab)
            }
            _ => {
                return Err(
                    "trusted LAN mode requires both --trusted-lan-bind and --trusted-lan-peer"
                        .into(),
                );
            }
        };
        Ok(Self {
            port,
            bind_ip,
            allowed_peer_ip,
            transport_mode,
            max_access_units,
            decode_nv12,
            publish_scope,
            reconnect_grace,
        })
    }
}

fn parse_trusted_lan_ipv4(value: &str, option: &str) -> Result<Ipv4Addr, Box<dyn Error>> {
    if value.len() > 15 {
        return Err(format!("{option} IPv4 literal is too long").into());
    }
    let address: Ipv4Addr = value
        .parse()
        .map_err(|_| format!("{option} requires a canonical IPv4 literal"))?;
    if address.to_string() != value {
        return Err(format!("{option} requires a canonical IPv4 literal").into());
    }
    if !is_trusted_lan_ipv4(address) {
        return Err(
            format!("{option} must be inside RFC1918, link-local, or 100.64.0.0/10").into(),
        );
    }
    Ok(address)
}

fn is_trusted_lan_ipv4(address: Ipv4Addr) -> bool {
    let [first, second, _, _] = address.octets();
    first == 10
        || (first == 172 && (16..=31).contains(&second))
        || (first == 192 && second == 168)
        || (first == 100 && (64..=127).contains(&second))
        || (first == 169 && second == 254)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    #[test]
    fn clockwise_sensor_rotation_produces_upright_pillarboxed_nv12() {
        let width = 8_u32;
        let height = 4_u32;
        let mut payload = vec![16_u8; 48];
        for (index, value) in payload[..32].iter_mut().enumerate() {
            *value = u8::try_from(index + 32).unwrap();
        }
        payload[32..].fill(192);
        let frame = DecodedNv12Frame {
            source_sequence: 1,
            presentation_time_us: 0,
            discontinuity: false,
            width,
            height,
            payload,
        };
        let rotated = orient_nv12_for_landscape(frame, 90).unwrap();
        assert_eq!(rotated.width, width);
        assert_eq!(rotated.height, height);
        assert_eq!(rotated.payload.len(), 48);
        assert!(rotated.payload[..32].chunks_exact(8).all(|row| {
            row[..2].iter().all(|value| *value == 16)
                && row[2..6].iter().any(|value| *value != 16)
                && row[6..].iter().all(|value| *value == 16)
        }));
        assert!(rotated.payload[32..].chunks_exact(8).all(|row| {
            row[..2].iter().all(|value| *value == 128)
                && row[2..6].iter().all(|value| *value == 192)
                && row[6..].iter().all(|value| *value == 128)
        }));
    }

    #[test]
    fn invalid_orientation_fails_closed() {
        let frame = DecodedNv12Frame {
            source_sequence: 1,
            presentation_time_us: 0,
            discontinuity: false,
            width: 8,
            height: 4,
            payload: vec![16; 48],
        };
        assert!(orient_nv12_for_landscape(frame, 45).is_err());
    }

    #[test]
    fn decoded_frame_maps_exact_identity_timing_and_discontinuity() {
        let stream_id = StreamId::from_uuid(Uuid::from_bytes([7; 16]));
        let frame = DecodedNv12Frame {
            source_sequence: 41,
            presentation_time_us: 9_876_543,
            discontinuity: true,
            width: 1280,
            height: 720,
            payload: vec![23; 1_382_400],
        };
        let generated = decoded_to_generated_frame(frame, stream_id, 17, 33_333_333).unwrap();
        assert_eq!(generated.descriptor.stream_id, stream_id);
        assert_eq!(generated.descriptor.stream_epoch, 17);
        assert_eq!(generated.descriptor.sequence, 41);
        assert_eq!(generated.descriptor.source_timestamp_nanos, 9_876_543_000);
        assert_eq!(generated.descriptor.duration_nanos, 33_333_333);
        assert_eq!(generated.descriptor.payload_bytes, 1_382_400);
        assert!(generated.descriptor.flags.discontinuity);
        assert!(!generated.descriptor.flags.end_of_stream);
        generated.validate(&fixture_stream_spec()).unwrap();
    }

    #[test]
    fn reconnect_accepts_a_loopback_peer_inside_the_fixed_grace() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let connector = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            let mut stream = TcpStream::connect(address).unwrap();
            thread::sleep(Duration::from_millis(50));
            stream.write_all(&[0x5a]).unwrap();
            stream
        });

        let (mut accepted, peer) =
            accept_with_grace(&listener, Duration::from_secs(1), Ipv4Addr::LOCALHOST)
                .unwrap()
                .expect("reconnect");
        assert!(peer.ip().is_loopback());
        let mut byte = [0_u8; 1];
        accepted.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [0x5a]);
        connector.join().unwrap();
    }

    #[test]
    fn reconnect_grace_accepts_the_fixed_live_hold_bound_only() {
        let options = Options::parse_from(
            ["--reconnect-grace-millis", "60000"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(options.reconnect_grace, Some(Duration::from_secs(60)));
        assert!(
            Options::parse_from(
                ["--reconnect-grace-millis", "60001"]
                    .into_iter()
                    .map(str::to_owned)
            )
            .is_err()
        );
    }

    #[test]
    fn trusted_lan_options_require_exact_private_bind_and_peer() {
        let options = Options::parse_from(
            [
                "--trusted-lan-bind",
                "100.70.0.1",
                "--trusted-lan-peer",
                "100.70.0.2",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(options.bind_ip, Ipv4Addr::new(100, 70, 0, 1));
        assert_eq!(options.allowed_peer_ip, Ipv4Addr::new(100, 70, 0, 2));
        assert_eq!(options.transport_mode, TransportMode::TrustedLanLab);

        for arguments in [
            vec!["--trusted-lan-bind", "192.168.1.10"],
            vec![
                "--trusted-lan-bind",
                "0.0.0.0",
                "--trusted-lan-peer",
                "192.168.1.20",
            ],
            vec![
                "--trusted-lan-bind",
                "192.168.1.10",
                "--trusted-lan-peer",
                "8.8.8.8",
            ],
            vec![
                "--trusted-lan-bind",
                "192.168.1.10",
                "--trusted-lan-peer",
                "192.168.1.10",
            ],
            vec![
                "--port",
                "38174",
                "--trusted-lan-bind",
                "192.168.1.10",
                "--trusted-lan-peer",
                "192.168.1.20",
            ],
        ] {
            assert!(
                Options::parse_from(arguments.into_iter().map(str::to_owned)).is_err(),
                "unexpectedly accepted closed trusted-LAN arguments"
            );
        }
    }

    #[test]
    fn peer_allowlist_is_exact_and_ipv4_only() {
        let allowed = Ipv4Addr::new(100, 66, 157, 119);
        assert!(peer_is_authorized(
            "100.66.157.119:50000".parse().unwrap(),
            allowed
        ));
        assert!(!peer_is_authorized(
            "100.66.157.120:50000".parse().unwrap(),
            allowed
        ));
        assert!(!peer_is_authorized("[::1]:50000".parse().unwrap(), allowed));
    }

    #[test]
    fn reconnected_publication_timing_remains_monotonic() {
        let duration_nanos = 33_333_333;
        assert_eq!(next_publication_timing(0, duration_nanos).unwrap(), (1, 0));
        assert_eq!(
            next_publication_timing(816, duration_nanos).unwrap(),
            (817, 27_199_728)
        );
    }
}
