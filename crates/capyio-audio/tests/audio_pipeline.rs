use capyio_audio::{
    AudioFormat, AudioFrame, AudioMediaPacket, AudioMediaStreamBinding, AudioStreamCapabilities,
    AudioStreamSpec, AudioUseCase, BoundedAudioPacketQueue, ClockDriftEstimator, InsertOutcome,
    PacketQueuePushOutcome, ReorderBuffer, negotiate_audio_stream,
};
use capyio_core::{RouteId, SessionId, StreamId};

#[test]
fn decoded_frames_can_be_validated_reordered_and_measured() {
    let format = AudioFormat::microphone_baseline();
    let stream_id = StreamId::new();
    let make_frame = |sequence: u64| AudioFrame {
        stream_id,
        stream_epoch: 1,
        sequence,
        source_timestamp_micros: sequence * 10_000,
        first_sample_index: sequence * 480,
        sample_count: 480,
        discontinuity: false,
        payload: vec![0; 960],
    };

    let later = make_frame(1);
    let first = make_frame(0);
    later.validate(&format).expect("later frame");
    first.validate(&format).expect("first frame");

    let mut buffer = ReorderBuffer::new(stream_id, 1, 0, 8).expect("buffer");
    assert_eq!(buffer.insert(later), InsertOutcome::Accepted);
    assert_eq!(buffer.insert(first), InsertOutcome::Accepted);
    assert_eq!(buffer.pop_next().expect("sequence 0").sequence, 0);
    assert_eq!(buffer.pop_next().expect("sequence 1").sequence, 1);

    let mut drift = ClockDriftEstimator::new(format.sample_rate_hz);
    assert!(drift.observe(0, 0).is_none());
    let estimate = drift.observe(48_000, 1_000_000).expect("estimate");
    assert!((estimate.rate_ratio - 1.0).abs() < f64::EPSILON);
}

#[test]
fn microphone_and_speaker_use_one_contract_without_merging_specs() {
    let voice = AudioStreamSpec::voice_interactive();
    let speaker = AudioStreamSpec::media_balanced();
    let both = AudioStreamCapabilities::new(vec![voice.clone(), speaker.clone()])
        .expect("direction-neutral candidates");

    assert_eq!(
        negotiate_audio_stream(&both, &both, AudioUseCase::VoiceInteractive).expect("voice"),
        voice
    );
    assert_eq!(
        negotiate_audio_stream(&both, &both, AudioUseCase::MediaBalanced).expect("speaker"),
        speaker
    );
}

#[test]
fn opposite_audio_routes_share_a_session_without_sharing_media_state() {
    let session_id = SessionId::new();
    let microphone = AudioMediaStreamBinding {
        session_id,
        route_id: RouteId::new(),
        stream_id: StreamId::new(),
        stream_epoch: 3,
        selected_spec: AudioStreamSpec::voice_interactive(),
    };
    let speaker = AudioMediaStreamBinding {
        session_id,
        route_id: RouteId::new(),
        stream_id: StreamId::new(),
        stream_epoch: 8,
        selected_spec: AudioStreamSpec::media_balanced(),
    };

    let mut microphone_queue =
        BoundedAudioPacketQueue::new(microphone.clone(), 1, 960).expect("microphone queue");
    let mut speaker_queue =
        BoundedAudioPacketQueue::new(speaker.clone(), 2, 3_840).expect("speaker queue");

    let packet =
        |binding: &AudioMediaStreamBinding, sequence: u64, payload_bytes: usize| AudioMediaPacket {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence,
            source_timestamp_micros: sequence * 10_000,
            first_sample_index: sequence * 480,
            sample_count: 480,
            discontinuity: false,
            payload: vec![0; payload_bytes],
        };

    assert_eq!(
        microphone_queue
            .try_push(packet(&microphone, 0, 960))
            .expect("microphone packet"),
        PacketQueuePushOutcome::Accepted
    );
    assert_eq!(
        microphone_queue
            .try_push(packet(&microphone, 1, 960))
            .expect("bounded microphone queue"),
        PacketQueuePushOutcome::PacketCapacityReached
    );
    assert_eq!(
        speaker_queue
            .try_push(packet(&speaker, 0, 1_920))
            .expect("speaker packet"),
        PacketQueuePushOutcome::Accepted
    );

    assert_eq!(microphone.session_id, speaker.session_id);
    assert_ne!(microphone.route_id, speaker.route_id);
    assert_ne!(microphone.stream_id, speaker.stream_id);
    assert_eq!(
        speaker_queue.pop().expect("speaker stays usable").sequence,
        0
    );
    assert_eq!(speaker_queue.stats().packet_capacity_drops, 0);
    assert_eq!(microphone_queue.stats().packet_capacity_drops, 1);
}
