use std::{
    env,
    f32::consts::TAU,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    thread,
    time::{Duration, Instant},
};

use capyio_audio::AudioMediaPacket;
use capyio_native_audio_lan::{
    MAX_NATIVE_LAN_INFLIGHT_PACKETS, NativeLanEndpointConfig, NativeLanUdpEndpoint,
    speaker_lab_binding,
};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const SAMPLES_PER_PACKET: u32 = 480;
const PACKET_DURATION: Duration = Duration::from_millis(10);
const TONE_HZ: f32 = 440.0;
const AMPLITUDE: f32 = 0.12;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args();
    let executable = args
        .next()
        .unwrap_or_else(|| "capyio-native-audio-tone".to_owned());
    let usage = || format!("usage: {executable} <local-ipv4:port> <phone-ipv4:port> [seconds]");
    let local = args
        .next()
        .ok_or_else(&usage)
        .and_then(|value| parse_explicit_ipv4(&value, "local"))?;
    let peer = args
        .next()
        .ok_or_else(&usage)
        .and_then(|value| parse_explicit_ipv4(&value, "peer"))?;
    let duration = args
        .next()
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|seconds| (1..=60).contains(seconds))
                .map(Duration::from_secs)
                .ok_or_else(|| "seconds must be between 1 and 60".to_owned())
        })
        .transpose()?
        .unwrap_or(Duration::from_secs(10));
    if args.next().is_some() {
        return Err(usage());
    }

    let binding = speaker_lab_binding();
    let mut endpoint = NativeLanUdpEndpoint::bind(
        local,
        NativeLanEndpointConfig {
            peer,
            read_timeout: Duration::from_millis(50),
            inflight_packet_capacity: MAX_NATIVE_LAN_INFLIGHT_PACKETS,
        },
        binding.clone(),
    )
    .map_err(|error| error.to_string())?;
    println!(
        "native_speaker_lab=true local={} peer={} format=s16le/48000/stereo tone_hz={TONE_HZ}",
        endpoint.local_addr().map_err(|error| error.to_string())?,
        peer
    );

    let started = Instant::now();
    let mut next_deadline = started;
    let mut sequence = 0_u64;
    let mut first_sample_index = 0_u64;
    while started.elapsed() < duration {
        let payload = tone_packet(first_sample_index);
        endpoint
            .send_packet(&AudioMediaPacket {
                stream_id: binding.stream_id,
                stream_epoch: binding.stream_epoch,
                sequence,
                source_timestamp_micros: first_sample_index * 1_000_000 / u64::from(SAMPLE_RATE),
                first_sample_index,
                sample_count: SAMPLES_PER_PACKET,
                discontinuity: sequence == 0,
                payload,
            })
            .map_err(|error| error.to_string())?;
        sequence = sequence.wrapping_add(1);
        first_sample_index = first_sample_index.wrapping_add(u64::from(SAMPLES_PER_PACKET));
        next_deadline += PACKET_DURATION;
        if let Some(remaining) = next_deadline.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
    }

    let stats = endpoint.metrics();
    println!(
        "tone_complete=true packets_sent={} datagrams_sent={} bytes_sent={}",
        stats.packets_sent, stats.datagrams_sent, stats.bytes_sent
    );
    Ok(())
}

fn parse_explicit_ipv4(value: &str, name: &str) -> Result<SocketAddr, String> {
    let address = value
        .parse::<SocketAddr>()
        .map_err(|_| format!("{name} must be an explicit IPv4 socket address"))?;
    let IpAddr::V4(ip) = address.ip() else {
        return Err(format!("{name} must use IPv4"));
    };
    if address.port() == 0 || ip.is_unspecified() || ip.is_multicast() || ip == Ipv4Addr::BROADCAST
    {
        return Err(format!(
            "{name} must be a concrete unicast IPv4 and non-zero port"
        ));
    }
    Ok(address)
}

fn tone_packet(first_sample_index: u64) -> Vec<u8> {
    let mut payload = Vec::with_capacity(SAMPLES_PER_PACKET as usize * CHANNELS * 2);
    for frame in 0..SAMPLES_PER_PACKET {
        let sample_index = first_sample_index.wrapping_add(u64::from(frame));
        let phase =
            (sample_index % u64::from(SAMPLE_RATE)) as f32 * TONE_HZ * TAU / SAMPLE_RATE as f32;
        let sample = (phase.sin() * AMPLITUDE * f32::from(i16::MAX)).round() as i16;
        for _ in 0..CHANNELS {
            payload.extend_from_slice(&sample.to_le_bytes());
        }
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_packet_is_exact_stereo_pcm_and_non_silent() {
        let payload = tone_packet(0);
        assert_eq!(payload.len(), SAMPLES_PER_PACKET as usize * CHANNELS * 2);
        assert!(payload.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn endpoint_arguments_are_explicit_ipv4_only() {
        assert!(parse_explicit_ipv4("100.66.231.100:46001", "local").is_ok());
        assert!(parse_explicit_ipv4("0.0.0.0:46001", "local").is_err());
        assert!(parse_explicit_ipv4("[::1]:46001", "local").is_err());
        assert!(parse_explicit_ipv4("example.invalid:46001", "local").is_err());
    }
}
