use capyio_android_display::{
    AccessUnitOutcome, AndroidDisplaySink, AndroidDisplaySinkError, AndroidDisplaySinkState,
    SurfaceDecoder,
};
use capyio_core::StreamId;
use capyio_display_transport::{
    AvcDecoderConfig, EncodedAccessUnitDescriptor, EncodedAccessUnitFlags,
    EncodedDisplayStreamSpec, OwnedEncodedAccessUnit,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum DecoderEvent {
    Configured(u64),
    FlushedAndResubmitted(u64),
    Queued { sequence: u64, bytes: usize },
    Stopped,
    Cleared,
}

#[derive(Default)]
struct MockDecoder {
    events: Vec<DecoderEvent>,
    fail_queue: bool,
}

impl SurfaceDecoder for MockDecoder {
    type Error = &'static str;

    fn configure(&mut self, config: &AvcDecoderConfig) -> Result<(), Self::Error> {
        self.events
            .push(DecoderEvent::Configured(config.stream_epoch));
        Ok(())
    }

    fn flush_and_resubmit_config(&mut self, config: &AvcDecoderConfig) -> Result<(), Self::Error> {
        self.events
            .push(DecoderEvent::FlushedAndResubmitted(config.stream_epoch));
        Ok(())
    }

    fn queue_access_unit(
        &mut self,
        descriptor: &EncodedAccessUnitDescriptor,
        payload: &[u8],
    ) -> Result<(), Self::Error> {
        if self.fail_queue {
            return Err("fixture queue failure");
        }
        self.events.push(DecoderEvent::Queued {
            sequence: descriptor.sequence,
            bytes: payload.len(),
        });
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        self.events.push(DecoderEvent::Stopped);
        Ok(())
    }

    fn clear_surface(&mut self) -> Result<(), Self::Error> {
        self.events.push(DecoderEvent::Cleared);
        Ok(())
    }
}

fn config(stream_id: StreamId, epoch: u64) -> AvcDecoderConfig {
    AvcDecoderConfig {
        stream_id,
        stream_epoch: epoch,
        selected: EncodedDisplayStreamSpec::fixture_720p30_avc(),
        sequence_parameter_set: vec![0, 0, 0, 1, 0x67, 0x64, 0, 31],
        picture_parameter_set: vec![0, 0, 0, 1, 0x68, 0xeb, 0xec, 0xb2],
    }
}

fn unit(stream_id: StreamId, epoch: u64, sequence: u64, key_frame: bool) -> OwnedEncodedAccessUnit {
    OwnedEncodedAccessUnit {
        descriptor: EncodedAccessUnitDescriptor {
            stream_id,
            stream_epoch: epoch,
            sequence,
            presentation_time_micros: 1_000_000 + sequence * 33_333,
            duration_nanos: 33_333_333,
            payload_bytes: 5,
            flags: EncodedAccessUnitFlags {
                key_frame,
                ..EncodedAccessUnitFlags::default()
            },
        },
        payload: vec![0, 0, 0, 1, sequence as u8],
    }
}

#[test]
fn sink_waits_for_key_frame_and_never_retains_payload() {
    let stream = StreamId::new();
    let mut sink = AndroidDisplaySink::new(MockDecoder::default());
    sink.configure(config(stream, 1)).unwrap();
    assert_eq!(sink.state(), AndroidDisplaySinkState::AwaitingKeyFrame);

    assert_eq!(
        sink.accept(unit(stream, 1, 0, false)).unwrap(),
        AccessUnitOutcome::DroppedAwaitingKeyFrame
    );
    assert_eq!(
        sink.accept(unit(stream, 1, 1, true)).unwrap(),
        AccessUnitOutcome::Submitted {
            resynchronized: false
        }
    );
    assert_eq!(sink.state(), AndroidDisplaySinkState::Rendering);
    assert_eq!(sink.metrics().submitted_access_units, 1);
    assert_eq!(sink.metrics().dropped_awaiting_key_frame, 1);
    assert_eq!(sink.metrics().retained_payload_bytes, 0);

    let decoder = sink.into_decoder();
    assert_eq!(
        decoder.events,
        vec![
            DecoderEvent::Configured(1),
            DecoderEvent::Queued {
                sequence: 1,
                bytes: 5
            }
        ]
    );
}

#[test]
fn sequence_gap_flushes_resubmits_config_and_requires_another_key_frame() {
    let stream = StreamId::new();
    let mut sink = AndroidDisplaySink::new(MockDecoder::default());
    sink.configure(config(stream, 1)).unwrap();
    sink.accept(unit(stream, 1, 0, true)).unwrap();

    assert_eq!(
        sink.accept(unit(stream, 1, 2, false)).unwrap(),
        AccessUnitOutcome::DroppedAwaitingKeyFrame
    );
    assert_eq!(
        sink.accept(unit(stream, 1, 3, true)).unwrap(),
        AccessUnitOutcome::Submitted {
            resynchronized: true
        }
    );
    assert_eq!(sink.metrics().sequence_gaps, 1);
    assert_eq!(sink.metrics().decoder_flushes, 1);
    assert_eq!(sink.metrics().retained_payload_bytes, 0);

    let decoder = sink.into_decoder();
    assert!(
        decoder
            .events
            .contains(&DecoderEvent::FlushedAndResubmitted(1))
    );
}

#[test]
fn sink_rejects_wrong_identity_epoch_sequence_and_timestamp() {
    let stream = StreamId::new();
    let mut sink = AndroidDisplaySink::new(MockDecoder::default());
    sink.configure(config(stream, 2)).unwrap();
    sink.accept(unit(stream, 2, 0, true)).unwrap();

    assert_eq!(
        sink.accept(unit(StreamId::new(), 2, 1, false)),
        Err(AndroidDisplaySinkError::WrongStream)
    );
    assert_eq!(
        sink.accept(unit(stream, 3, 1, false)),
        Err(AndroidDisplaySinkError::WrongEpoch)
    );
    assert_eq!(
        sink.accept(unit(stream, 2, 0, true)),
        Err(AndroidDisplaySinkError::DuplicateOrLateSequence)
    );

    let mut stale_timestamp = unit(stream, 2, 1, false);
    stale_timestamp.descriptor.presentation_time_micros = 1_000_000;
    assert_eq!(
        sink.accept(stale_timestamp),
        Err(AndroidDisplaySinkError::NonAdvancingPresentationTime)
    );
    assert_eq!(sink.metrics().submitted_access_units, 1);
    assert_eq!(sink.metrics().retained_payload_bytes, 0);
}

#[test]
fn format_change_requires_stop_and_a_new_epoch() {
    let stream = StreamId::new();
    let mut sink = AndroidDisplaySink::new(MockDecoder::default());
    sink.configure(config(stream, 4)).unwrap();
    assert!(matches!(
        sink.configure(config(stream, 5)),
        Err(AndroidDisplaySinkError::InvalidState(_))
    ));
    sink.stop().unwrap();
    assert_eq!(sink.state(), AndroidDisplaySinkState::Stopped);
    assert_eq!(sink.metrics().retained_payload_bytes, 0);
    assert_eq!(
        sink.configure(config(stream, 4)),
        Err(AndroidDisplaySinkError::NonAdvancingEpoch)
    );
    sink.configure(config(stream, 5)).unwrap();
}

#[test]
fn end_of_stream_drains_and_explicit_stop_clears_surface() {
    let stream = StreamId::new();
    let mut sink = AndroidDisplaySink::new(MockDecoder::default());
    sink.configure(config(stream, 1)).unwrap();
    sink.accept(unit(stream, 1, 0, true)).unwrap();
    let terminal = OwnedEncodedAccessUnit {
        descriptor: EncodedAccessUnitDescriptor {
            stream_id: stream,
            stream_epoch: 1,
            sequence: 1,
            presentation_time_micros: 1_033_333,
            duration_nanos: 33_333_333,
            payload_bytes: 0,
            flags: EncodedAccessUnitFlags {
                end_of_stream: true,
                ..EncodedAccessUnitFlags::default()
            },
        },
        payload: Vec::new(),
    };
    assert_eq!(
        sink.accept(terminal).unwrap(),
        AccessUnitOutcome::EndOfStreamQueued
    );
    assert_eq!(sink.state(), AndroidDisplaySinkState::Draining);
    assert!(matches!(
        sink.accept(unit(stream, 1, 2, true)),
        Err(AndroidDisplaySinkError::InvalidState(_))
    ));
    sink.stop().unwrap();
    sink.stop().unwrap();
    let decoder = sink.into_decoder();
    assert!(
        decoder
            .events
            .ends_with(&[DecoderEvent::Stopped, DecoderEvent::Cleared])
    );
}

#[test]
fn decoder_failure_fails_closed_and_clears_surface() {
    let stream = StreamId::new();
    let mut sink = AndroidDisplaySink::new(MockDecoder {
        fail_queue: true,
        ..MockDecoder::default()
    });
    sink.configure(config(stream, 1)).unwrap();
    assert!(matches!(
        sink.accept(unit(stream, 1, 0, true)),
        Err(AndroidDisplaySinkError::Decoder { .. })
    ));
    assert_eq!(sink.state(), AndroidDisplaySinkState::Failed);
    assert_eq!(sink.metrics().retained_payload_bytes, 0);
    let decoder = sink.into_decoder();
    assert!(
        decoder
            .events
            .ends_with(&[DecoderEvent::Stopped, DecoderEvent::Cleared])
    );
}
