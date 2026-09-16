use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};

use capyio_audio::AudioMediaStreamBinding;
use capyio_native_audio_lan::{
    MAX_NATIVE_LAN_INFLIGHT_PACKETS, NativeLanEndpointConfig, NativeLanError,
    NativeLanReceiveOutcome, NativeLanUdpEndpoint, microphone_lab_binding, speaker_lab_binding,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Profile {
    Microphone,
    Speaker,
}

impl Profile {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "microphone" => Ok(Self::Microphone),
            "speaker" => Ok(Self::Speaker),
            _ => Err("profile must be microphone or speaker".to_owned()),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::Speaker => "speaker",
        }
    }

    fn binding(self) -> AudioMediaStreamBinding {
        match self {
            Self::Microphone => microphone_lab_binding(),
            Self::Speaker => speaker_lab_binding(),
        }
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("native_audio_probe_error={error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args();
    let executable = arguments
        .next()
        .unwrap_or_else(|| "capyio-native-audio-receive-probe".to_owned());
    let usage = || {
        format!(
            "usage: {executable} <microphone|speaker> <local-ipv4:port> <peer-ipv4:port> [seconds]"
        )
    };
    let profile = arguments
        .next()
        .ok_or_else(&usage)
        .and_then(|value| Profile::parse(&value))?;
    let local = arguments
        .next()
        .ok_or_else(&usage)
        .and_then(|value| parse_explicit_ipv4(&value, "local"))?;
    let peer = arguments
        .next()
        .ok_or_else(&usage)
        .and_then(|value| parse_explicit_ipv4(&value, "peer"))?;
    let seconds = arguments
        .next()
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|seconds| (1..=60).contains(seconds))
                .ok_or_else(|| "seconds must be between 1 and 60".to_owned())
        })
        .transpose()?
        .unwrap_or(10);
    if arguments.next().is_some() {
        return Err(usage());
    }

    let mut endpoint = NativeLanUdpEndpoint::bind(
        local,
        NativeLanEndpointConfig {
            peer,
            read_timeout: Duration::from_millis(50),
            inflight_packet_capacity: MAX_NATIVE_LAN_INFLIGHT_PACKETS,
        },
        profile.binding(),
    )
    .map_err(|error| error.to_string())?;
    println!(
        "native_audio_probe_ready=true profile={} local={} peer={}",
        profile.label(),
        endpoint.local_addr().map_err(|error| error.to_string())?,
        peer
    );

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut packets = 0_u64;
    let mut payload_bytes = 0_u64;
    let mut discontinuities = 0_u64;
    let mut first_sequence = None;
    let mut last_sequence = None;
    while Instant::now() < deadline {
        match endpoint.receive() {
            Ok(NativeLanReceiveOutcome::Packet(packet)) => {
                packets = packets.saturating_add(1);
                payload_bytes = payload_bytes.saturating_add(packet.payload.len() as u64);
                discontinuities = discontinuities.saturating_add(u64::from(packet.discontinuity));
                first_sequence.get_or_insert(packet.sequence);
                last_sequence = Some(packet.sequence);
            }
            Ok(
                NativeLanReceiveOutcome::Pending
                | NativeLanReceiveOutcome::DuplicateFragment
                | NativeLanReceiveOutcome::DroppedWrongPeer
                | NativeLanReceiveOutcome::DroppedMalformed,
            ) => {}
            Err(NativeLanError::ReceiveTimeout) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let metrics = endpoint.metrics();
    println!(
        "native_audio_probe_complete=true profile={} packets={} payload_bytes={} discontinuities={} first_sequence={} last_sequence={} datagrams={} wrong_peer={} malformed={} duplicates={}",
        profile.label(),
        packets,
        payload_bytes,
        discontinuities,
        first_sequence.map_or_else(|| "none".to_owned(), |value| value.to_string()),
        last_sequence.map_or_else(|| "none".to_owned(), |value| value.to_string()),
        metrics.datagrams_received,
        metrics.wrong_peer_datagrams,
        metrics.malformed_datagrams,
        metrics.duplicate_fragments,
    );
    if packets == 0 || payload_bytes == 0 {
        return Err("no complete audio packet was received".to_owned());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_profiles_select_the_exact_lab_bindings() {
        assert_eq!(
            Profile::parse("microphone").unwrap().binding(),
            microphone_lab_binding()
        );
        assert_eq!(
            Profile::parse("speaker").unwrap().binding(),
            speaker_lab_binding()
        );
        assert!(Profile::parse("other").is_err());
    }

    #[test]
    fn endpoint_parser_is_ipv4_only_and_concrete() {
        assert!(parse_explicit_ipv4("100.66.231.100:40011", "local").is_ok());
        assert!(parse_explicit_ipv4("0.0.0.0:40011", "local").is_err());
        assert!(parse_explicit_ipv4("[::1]:40011", "local").is_err());
    }
}
