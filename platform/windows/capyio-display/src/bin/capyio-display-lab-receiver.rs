use std::{
    env, fs,
    net::Ipv4Addr,
    path::PathBuf,
    time::{Duration, Instant},
};

use capyio_core::{RouteId, SessionId, StreamId};
use capyio_display::{DisplayCursorMode, DisplayStreamSpec};
use capyio_display_tls::{
    AuthenticatedDisplayItem, AuthenticatedDisplayReceiver, DisplayTlsClientIdentity,
    DisplayTlsServerEndpoint,
};
use capyio_display_transport::{
    AvcLevel, AvcProfile, DisplayTransportBinding, EncodedDisplayCodec, EncodedDisplayStreamSpec,
    PrivateDisplayRecord, decode_private_display_record,
};
use capyio_video::{FrameRate, VideoColorimetry, VideoPixelFormat};

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let address: Ipv4Addr = number("CAPYIO_DISPLAY_ADDRESS")?;
    let port = number("CAPYIO_DISPLAY_PORT")?;
    let width = number("CAPYIO_DISPLAY_WIDTH")?;
    let height = number("CAPYIO_DISPLAY_HEIGHT")?;
    let frame_rate = FrameRate::new(
        number("CAPYIO_DISPLAY_FRAME_RATE_NUMERATOR")?,
        number("CAPYIO_DISPLAY_FRAME_RATE_DENOMINATOR")?,
    )
    .ok_or("invalid display frame rate")?;
    let duration_seconds: u64 = number("CAPYIO_DISPLAY_DURATION_SECONDS")?;
    if !(1..=300).contains(&duration_seconds) {
        return Err("CAPYIO_DISPLAY_DURATION_SECONDS must be inside 1..=300".into());
    }
    let decoded = DisplayStreamSpec {
        width,
        height,
        frame_rate,
        pixel_format: VideoPixelFormat::Bgra8,
        colorimetry: VideoColorimetry::SrgbFull,
        cursor_mode: DisplayCursorMode::Composited,
        target_latency_millis: 50,
        maximum_latency_millis: 150,
        allow_frame_drop: true,
    };
    let selected = EncodedDisplayStreamSpec {
        avc_level: AvcLevel::minimum_for(&decoded)
            .ok_or("no supported AVC level covers the selected display mode")?,
        decoded,
        codec: EncodedDisplayCodec::AvcAnnexB,
        avc_profile: AvcProfile::Main,
        maximum_access_unit_bytes: number("CAPYIO_DISPLAY_MAXIMUM_ACCESS_UNIT_BYTES")?,
    };
    let binding = DisplayTransportBinding::new(
        variable("CAPYIO_DISPLAY_SESSION_ID")?.parse::<SessionId>()?,
        variable("CAPYIO_DISPLAY_ROUTE_ID")?.parse::<RouteId>()?,
        variable("CAPYIO_DISPLAY_STREAM_ID")?.parse::<StreamId>()?,
        number("CAPYIO_DISPLAY_STREAM_EPOCH")?,
        selected,
    )?;
    let identity = DisplayTlsClientIdentity {
        certificate_chain_der: vec![
            credential("CAPYIO_DISPLAY_CLIENT_CERT")?,
            credential("CAPYIO_DISPLAY_CA_CERT")?,
        ],
        private_key_der: credential("CAPYIO_DISPLAY_CLIENT_KEY")?,
        server_ca_der: credential("CAPYIO_DISPLAY_CA_CERT")?,
    };
    let endpoint = DisplayTlsServerEndpoint {
        address,
        port,
        io_timeout: Duration::from_secs(5),
    };
    let mut receiver = AuthenticatedDisplayReceiver::connect(
        endpoint,
        &address.to_string(),
        identity,
        binding.clone(),
    )?;
    println!("TLS_AUTHENTICATED address={address}:{port}");

    let deadline = Instant::now() + Duration::from_secs(duration_seconds);
    let mut configurations = 0_u64;
    let mut access_units = 0_u64;
    let mut heartbeats = 0_u64;
    let mut payload_bytes = 0_u64;
    while Instant::now() < deadline {
        match receiver.receive()? {
            AuthenticatedDisplayItem::Heartbeat => {
                heartbeats = heartbeats
                    .checked_add(1)
                    .ok_or("heartbeat count exhausted")?;
                println!("HEARTBEAT count={heartbeats}");
            }
            AuthenticatedDisplayItem::Record(record) => {
                payload_bytes = payload_bytes
                    .checked_add(u64::try_from(record.len())?)
                    .ok_or("received byte count exhausted")?;
                match decode_private_display_record(&record, binding.selected())? {
                    PrivateDisplayRecord::DecoderConfig(_) => {
                        configurations = configurations
                            .checked_add(1)
                            .ok_or("configuration count exhausted")?;
                        println!("DECODER_CONFIG count={configurations}");
                    }
                    PrivateDisplayRecord::AccessUnit(_) => {
                        access_units = access_units
                            .checked_add(1)
                            .ok_or("access-unit count exhausted")?;
                        if access_units == 1 || access_units.is_multiple_of(30) {
                            println!(
                                "ACCESS_UNIT count={access_units} received_bytes={payload_bytes}"
                            );
                        }
                    }
                }
            }
        }
    }
    if configurations == 0 || access_units == 0 || heartbeats == 0 {
        return Err(format!(
            "incomplete display evidence: configurations={configurations} access_units={access_units} heartbeats={heartbeats}"
        )
        .into());
    }
    println!(
        "COMPLETE configurations={configurations} access_units={access_units} heartbeats={heartbeats} received_bytes={payload_bytes}"
    );
    Ok(())
}

fn variable(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = env::var(name).map_err(|_| format!("missing environment variable {name}"))?;
    if value.is_empty() || value.len() > 1024 {
        return Err(format!("environment variable {name} is outside its bound").into());
    }
    Ok(value)
}

fn number<T: std::str::FromStr>(name: &str) -> Result<T, Box<dyn std::error::Error>>
where
    T::Err: std::error::Error + 'static,
{
    Ok(variable(name)?.parse()?)
}

fn credential(name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = PathBuf::from(variable(name)?);
    let metadata = fs::metadata(&path)?;
    if metadata.len() == 0 || metadata.len() > 64 * 1024 {
        return Err(format!("credential {name} is outside its bound").into());
    }
    Ok(fs::read(path)?)
}
