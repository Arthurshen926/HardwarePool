use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    time::Duration,
};

use capyio_audio::{
    AudioEncodingSpec, AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec,
    AudioTransportInteroperability, AudioTransportMediaAccess,
};
use capyio_core::{RouteId, SessionId, StreamId};
use capyio_native_audio_lan::{
    MAX_NATIVE_LAN_DATAGRAM_BYTES, MAX_NATIVE_LAN_PACKET_PAYLOAD_BYTES, NativeLanEndpointConfig,
    NativeLanError, NativeLanReassembler, NativeLanReassemblyOutcome, NativeLanReceiveOutcome,
    NativeLanUdpEndpoint, decode_native_lan_fragment, encode_native_lan_fragment,
    native_lan_backend_contract, native_lan_fragment_count,
};

const GOLDEN_NATIVE_LAN_V1: &str =
    include_str!("../../../fixtures/audio/native_lan_v1_opus_single.hex");

fn decode_hex_fixture(value: &str) -> Vec<u8> {
    value
        .split_ascii_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).expect("valid fixture hex"))
        .collect()
}

fn fixed_binding(spec: AudioStreamSpec) -> AudioMediaStreamBinding {
    spec.validate().expect("spec");
    AudioMediaStreamBinding {
        session_id: "11111111-2222-4333-8444-555555555555"
            .parse::<SessionId>()
            .expect("session"),
        route_id: "66666666-7777-4888-8999-aaaaaaaaaaaa"
            .parse::<RouteId>()
            .expect("route"),
        stream_id: "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff"
            .parse::<StreamId>()
            .expect("stream"),
        stream_epoch: 7,
        selected_spec: spec,
    }
}

fn pcm_packet(binding: &AudioMediaStreamBinding, sequence: u64) -> AudioMediaPacket {
    let sample_count = binding.samples_per_packet().expect("samples");
    let payload_bytes = usize::try_from(sample_count).expect("samples fit")
        * usize::from(binding.selected_spec.format.channels)
        * usize::from(
            binding
                .selected_spec
                .format
                .sample_format
                .bytes_per_sample(),
        );
    AudioMediaPacket {
        stream_id: binding.stream_id,
        stream_epoch: binding.stream_epoch,
        sequence,
        source_timestamp_micros: sequence.saturating_mul(10_000),
        first_sample_index: sequence.saturating_mul(u64::from(sample_count)),
        sample_count,
        discontinuity: sequence == 9,
        payload: (0..payload_bytes)
            .map(|index| u8::try_from(index % 251).expect("bounded byte"))
            .collect(),
    }
}

fn opus_packet(binding: &AudioMediaStreamBinding) -> AudioMediaPacket {
    AudioMediaPacket {
        stream_id: binding.stream_id,
        stream_epoch: binding.stream_epoch,
        sequence: 0x0102_0304_0506_0708,
        source_timestamp_micros: 0x1112_1314_1516_1718,
        first_sample_index: 0x2122_2324_2526_2728,
        sample_count: binding.samples_per_packet().expect("samples"),
        discontinuity: true,
        payload: (0_u8..16).collect(),
    }
}

#[test]
fn backend_is_exact_direction_neutral_and_explicitly_insecure() {
    let contract = native_lan_backend_contract().validate().expect("contract");
    assert_eq!(
        contract.interoperability,
        AudioTransportInteroperability::AdapterManaged
    );
    assert_eq!(contract.media_access, AudioTransportMediaAccess::FullPacket);
    assert!(contract.metadata.is_exact());
    assert!(contract.encodings.pcm && contract.encodings.opus);
    assert!(!contract.security.meets_production_baseline());
}

#[test]
fn single_fragment_round_trip_preserves_every_wire_field() {
    let mut spec = AudioStreamSpec::voice_interactive();
    spec.encoding = AudioEncodingSpec::opus(64_000);
    let binding = fixed_binding(spec);
    let packet = opus_packet(&binding);
    let mut datagram = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
    let bytes = encode_native_lan_fragment(&binding, &packet, 0, &mut datagram).expect("encode");
    assert_eq!(&datagram[..bytes], decode_hex_fixture(GOLDEN_NATIVE_LAN_V1));
    let fragment = decode_native_lan_fragment(&datagram[..bytes]).expect("decode");

    assert!(fragment.matches_binding(&binding));
    assert_eq!(fragment.sequence, packet.sequence);
    assert_eq!(
        fragment.source_timestamp_micros,
        packet.source_timestamp_micros
    );
    assert_eq!(fragment.first_sample_index, packet.first_sample_index);
    assert_eq!(fragment.sample_count, packet.sample_count);
    assert_eq!(fragment.discontinuity, packet.discontinuity);
    assert_eq!(fragment.fragment_index, 0);
    assert_eq!(fragment.fragment_count, 1);
    assert_eq!(fragment.payload, packet.payload);
}

#[test]
fn unsigned_wire_counters_preserve_all_bits() {
    let mut spec = AudioStreamSpec::voice_interactive();
    spec.encoding = AudioEncodingSpec::opus(64_000);
    let binding = fixed_binding(spec);
    let packet = AudioMediaPacket {
        stream_id: binding.stream_id,
        stream_epoch: binding.stream_epoch,
        sequence: u64::MAX,
        source_timestamp_micros: 1_u64 << 63,
        first_sample_index: 0xfedc_ba98_7654_3210,
        sample_count: binding.samples_per_packet().expect("samples"),
        discontinuity: false,
        payload: vec![42],
    };
    let mut datagram = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
    let bytes = encode_native_lan_fragment(&binding, &packet, 0, &mut datagram).unwrap();
    let fragment = decode_native_lan_fragment(&datagram[..bytes]).unwrap();

    assert_eq!(fragment.sequence, u64::MAX);
    assert_eq!(fragment.source_timestamp_micros, 1_u64 << 63);
    assert_eq!(fragment.first_sample_index, 0xfedc_ba98_7654_3210);
}

#[test]
fn stereo_pcm_reassembles_in_reverse_fragment_order() {
    let binding = fixed_binding(AudioStreamSpec::media_balanced());
    let packet = pcm_packet(&binding, 9);
    assert_eq!(packet.payload.len(), 1_920);
    assert_eq!(native_lan_fragment_count(packet.payload.len()).unwrap(), 2);

    let mut datagrams = Vec::new();
    for index in 0..2 {
        let mut datagram = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
        let bytes =
            encode_native_lan_fragment(&binding, &packet, index, &mut datagram).expect("encode");
        datagrams.push(datagram[..bytes].to_vec());
    }
    let mut reassembler = NativeLanReassembler::new(binding, 2).expect("reassembler");
    assert_eq!(
        reassembler.push_datagram(&datagrams[1]).unwrap(),
        NativeLanReassemblyOutcome::Pending
    );
    assert_eq!(
        reassembler.push_datagram(&datagrams[0]).unwrap(),
        NativeLanReassemblyOutcome::Complete(packet)
    );
    assert_eq!(reassembler.stats().accepted_fragments, 2);
    assert_eq!(reassembler.stats().completed_packets, 1);
}

#[test]
fn malformed_and_conflicting_fragments_fail_closed() {
    let mut spec = AudioStreamSpec::voice_interactive();
    spec.encoding = AudioEncodingSpec::opus(64_000);
    let binding = fixed_binding(spec);
    let packet = opus_packet(&binding);
    let mut datagram = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
    let bytes = encode_native_lan_fragment(&binding, &packet, 0, &mut datagram).unwrap();

    let mut malformed = datagram[..bytes].to_vec();
    malformed[4] = 2;
    assert!(matches!(
        decode_native_lan_fragment(&malformed),
        Err(NativeLanError::InvalidDatagram(_))
    ));

    let mut malformed = datagram[..bytes].to_vec();
    malformed[5] = 0x80;
    assert!(decode_native_lan_fragment(&malformed).is_err());

    let mut malformed = datagram[..bytes].to_vec();
    malformed[102] = 1;
    assert!(decode_native_lan_fragment(&malformed).is_err());

    let mut reassembler = NativeLanReassembler::new(binding.clone(), 1).unwrap();
    let mut different_binding = binding;
    different_binding.route_id = RouteId::new();
    let different_packet = AudioMediaPacket {
        stream_id: different_binding.stream_id,
        stream_epoch: different_binding.stream_epoch,
        ..packet
    };
    let mut foreign = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
    let foreign_bytes =
        encode_native_lan_fragment(&different_binding, &different_packet, 0, &mut foreign).unwrap();
    assert_eq!(
        reassembler
            .push_datagram(&foreign[..foreign_bytes])
            .unwrap(),
        NativeLanReassemblyOutcome::WrongBinding
    );
}

#[test]
fn reassembly_capacity_and_duplicate_fragments_are_observable() {
    let binding = fixed_binding(AudioStreamSpec::media_balanced());
    let first = pcm_packet(&binding, 1);
    let second = pcm_packet(&binding, 2);
    let mut first_fragment = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
    let first_bytes = encode_native_lan_fragment(&binding, &first, 0, &mut first_fragment).unwrap();
    let mut second_fragment = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
    let second_bytes =
        encode_native_lan_fragment(&binding, &second, 0, &mut second_fragment).unwrap();

    let mut reassembler = NativeLanReassembler::new(binding, 1).unwrap();
    assert_eq!(
        reassembler
            .push_datagram(&first_fragment[..first_bytes])
            .unwrap(),
        NativeLanReassemblyOutcome::Pending
    );
    assert_eq!(
        reassembler
            .push_datagram(&first_fragment[..first_bytes])
            .unwrap(),
        NativeLanReassemblyOutcome::DuplicateFragment
    );
    assert_eq!(
        reassembler
            .push_datagram(&second_fragment[..second_bytes])
            .unwrap(),
        NativeLanReassemblyOutcome::Pending
    );
    assert_eq!(reassembler.stats().duplicate_fragments, 1);
    assert_eq!(reassembler.stats().partial_evictions, 1);
}

#[test]
fn udp_loopback_delivers_fragmented_stereo_and_rejects_another_peer() {
    let binding = fixed_binding(AudioStreamSpec::media_balanced());
    let packet = pcm_packet(&binding, 3);
    let sender_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("sender socket");
    let receiver_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("receiver socket");
    let sender_address = sender_socket.local_addr().expect("sender address");
    let receiver_address = receiver_socket.local_addr().expect("receiver address");
    let config = |peer| NativeLanEndpointConfig {
        peer,
        read_timeout: Duration::from_millis(250),
        inflight_packet_capacity: 2,
    };
    let mut sender =
        NativeLanUdpEndpoint::from_socket(sender_socket, config(receiver_address), binding.clone())
            .expect("sender");
    let mut receiver =
        NativeLanUdpEndpoint::from_socket(receiver_socket, config(sender_address), binding.clone())
            .expect("receiver");

    sender.send_packet(&packet).expect("send");
    assert_eq!(
        receiver.receive().unwrap(),
        NativeLanReceiveOutcome::Pending
    );
    assert_eq!(
        receiver.receive().unwrap(),
        NativeLanReceiveOutcome::Packet(packet)
    );
    assert_eq!(sender.metrics().packets_sent, 1);
    assert_eq!(sender.metrics().datagrams_sent, 2);
    assert_eq!(receiver.metrics().packets_received, 1);

    let spoof = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("spoof socket");
    let spoof_packet = pcm_packet(&binding, 4);
    let mut datagram = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
    let bytes = encode_native_lan_fragment(&binding, &spoof_packet, 0, &mut datagram).unwrap();
    spoof
        .send_to(&datagram[..bytes], receiver_address)
        .expect("spoof send");
    assert_eq!(
        receiver.receive().unwrap(),
        NativeLanReceiveOutcome::DroppedWrongPeer
    );
    assert_eq!(receiver.metrics().wrong_peer_datagrams, 1);
}

#[test]
fn endpoint_configuration_is_literal_bounded_and_times_out() {
    let binding = fixed_binding(AudioStreamSpec::voice_interactive());
    let local = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0));
    let invalid = NativeLanEndpointConfig {
        peer: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 9000)),
        read_timeout: Duration::from_millis(20),
        inflight_packet_capacity: 1,
    };
    assert!(invalid.validate().is_err());
    let broadcast = NativeLanEndpointConfig {
        peer: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, 9000)),
        read_timeout: Duration::from_millis(20),
        inflight_packet_capacity: 1,
    };
    assert!(broadcast.validate().is_err());
    assert_eq!(
        native_lan_fragment_count(MAX_NATIVE_LAN_PACKET_PAYLOAD_BYTES).unwrap(),
        64
    );
    assert!(native_lan_fragment_count(MAX_NATIVE_LAN_PACKET_PAYLOAD_BYTES + 1).is_err());

    let peer_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("peer");
    let config = NativeLanEndpointConfig {
        peer: peer_socket.local_addr().unwrap(),
        read_timeout: Duration::from_millis(5),
        inflight_packet_capacity: 1,
    };
    let mut endpoint = NativeLanUdpEndpoint::bind(local, config, binding).expect("endpoint");
    assert!(matches!(
        endpoint.receive(),
        Err(NativeLanError::ReceiveTimeout)
    ));
}
