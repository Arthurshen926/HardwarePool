use capyio_windows_camera::{
    MAX_PENDING_SAMPLE_REQUESTS, MF_CAMERA_STREAM_ID, MfMediaSourceCore, MfMediaSourceCoreError,
    MfMediaSourceEvent, MfMediaSourceOperation, MfMediaSourceState, MfMediaStreamState,
    MfPresentationSelection,
};

#[test]
fn first_start_emits_new_stream_then_stream_and_source_started() {
    let mut source = MfMediaSourceCore::default();
    let events = source
        .start(MfPresentationSelection::canonical(), 123_000_000)
        .unwrap();

    assert_eq!(
        events,
        [
            MfMediaSourceEvent::NewStream {
                stream_id: MF_CAMERA_STREAM_ID,
                stream_generation: 1,
            },
            MfMediaSourceEvent::StreamStarted {
                stream_id: MF_CAMERA_STREAM_ID,
                stream_generation: 1,
                start_time_100ns: 123_000_000,
            },
            MfMediaSourceEvent::SourceStarted {
                stream_generation: 1,
                start_time_100ns: 123_000_000,
            },
        ]
    );
    assert_eq!(source.source_state(), MfMediaSourceState::Started);
    assert_eq!(source.stream_state(), MfMediaStreamState::Started);
}

#[test]
fn repeated_start_keeps_generation_requests_and_timeline_state() {
    let mut source = MfMediaSourceCore::default();
    source
        .start(MfPresentationSelection::canonical(), 10)
        .unwrap();
    let pending = source.request_sample().unwrap();

    let events = source
        .start(MfPresentationSelection::canonical(), 20)
        .unwrap();
    assert_eq!(
        events,
        [
            MfMediaSourceEvent::UpdatedStream {
                stream_id: MF_CAMERA_STREAM_ID,
                stream_generation: 1,
            },
            MfMediaSourceEvent::StreamStarted {
                stream_id: MF_CAMERA_STREAM_ID,
                stream_generation: 1,
                start_time_100ns: 20,
            },
            MfMediaSourceEvent::SourceStarted {
                stream_generation: 1,
                start_time_100ns: 20,
            },
        ]
    );
    assert_eq!(source.stream_generation(), 1);
    assert_eq!(source.pending_sample_requests(), 1);
    source.complete_sample(pending, 0).unwrap();
}

#[test]
fn stop_cancels_requests_and_restart_announces_updated_stream() {
    let mut source = MfMediaSourceCore::default();
    source
        .start(MfPresentationSelection::canonical(), 10)
        .unwrap();
    source.request_sample().unwrap();
    source.request_sample().unwrap();

    let stopped = source.stop().unwrap();
    assert_eq!(stopped.cancelled_sample_requests, 2);
    assert_eq!(
        stopped.events,
        [
            MfMediaSourceEvent::StreamStopped {
                stream_id: MF_CAMERA_STREAM_ID,
                stream_generation: 1,
            },
            MfMediaSourceEvent::SourceStopped {
                stream_generation: 1,
            },
        ]
    );
    assert_eq!(source.pending_sample_requests(), 0);

    let restarted = source
        .start(MfPresentationSelection::canonical(), 20)
        .unwrap();
    assert_eq!(
        restarted[0],
        MfMediaSourceEvent::UpdatedStream {
            stream_id: MF_CAMERA_STREAM_ID,
            stream_generation: 2,
        }
    );
}

#[test]
fn invalid_selection_and_start_time_fail_without_changing_state() {
    let mut source = MfMediaSourceCore::default();
    assert!(matches!(
        source.start(MfPresentationSelection::new(2, Some(0)), 0),
        Err(MfMediaSourceCoreError::InvalidPresentationSelection { .. })
    ));
    assert_eq!(source.stream_generation(), 0);
    assert_eq!(source.source_state(), MfMediaSourceState::Stopped);

    assert_eq!(
        source.start(MfPresentationSelection::canonical(), -1),
        Err(MfMediaSourceCoreError::InvalidStartTime(-1))
    );
    assert_eq!(source.stream_generation(), 0);
}

#[test]
fn pending_sample_requests_are_fixed_bounded_and_monotonic() {
    let mut source = MfMediaSourceCore::default();
    source
        .start(MfPresentationSelection::canonical(), 0)
        .unwrap();

    let mut tickets = Vec::new();
    for expected_id in 1..=MAX_PENDING_SAMPLE_REQUESTS as u64 {
        let ticket = source.request_sample().unwrap();
        assert_eq!(ticket.request_id(), expected_id);
        assert_eq!(ticket.stream_generation(), 1);
        tickets.push(ticket);
    }
    assert_eq!(
        source.pending_sample_requests(),
        MAX_PENDING_SAMPLE_REQUESTS
    );
    assert_eq!(
        source.request_sample(),
        Err(MfMediaSourceCoreError::PendingRequestLimitReached {
            maximum: MAX_PENDING_SAMPLE_REQUESTS,
        })
    );
    assert_eq!(
        source.pending_sample_requests(),
        MAX_PENDING_SAMPLE_REQUESTS
    );

    for (sequence, ticket) in tickets.into_iter().enumerate() {
        assert_eq!(
            source.complete_sample(ticket, sequence as u64).unwrap(),
            MfMediaSourceEvent::StreamSample {
                ticket,
                sequence: sequence as u64,
            }
        );
    }
    assert_eq!(source.pending_sample_requests(), 0);
}

#[test]
fn completion_is_fifo_and_sequence_failure_is_transactional() {
    let mut source = MfMediaSourceCore::default();
    source
        .start(MfPresentationSelection::canonical(), 0)
        .unwrap();
    let first = source.request_sample().unwrap();
    let second = source.request_sample().unwrap();

    assert!(matches!(
        source.complete_sample(second, 10),
        Err(MfMediaSourceCoreError::WrongSampleRequest { .. })
    ));
    assert_eq!(source.pending_sample_requests(), 2);
    source.complete_sample(first, 10).unwrap();
    assert_eq!(
        source.complete_sample(second, 10),
        Err(MfMediaSourceCoreError::NonAdvancingFrameSequence {
            previous: 10,
            actual: 10,
        })
    );
    assert_eq!(source.pending_sample_requests(), 1);
    source.complete_sample(second, 11).unwrap();
}

#[test]
fn cancellation_is_fifo_transactional_and_keeps_source_started() {
    let mut source = MfMediaSourceCore::default();
    source
        .start(MfPresentationSelection::canonical(), 0)
        .unwrap();
    let first = source.request_sample().unwrap();
    let second = source.request_sample().unwrap();

    assert!(matches!(
        source.cancel_sample(second),
        Err(MfMediaSourceCoreError::WrongSampleRequest { .. })
    ));
    assert_eq!(source.pending_sample_requests(), 2);
    source.cancel_sample(first).unwrap();
    assert_eq!(source.pending_sample_requests(), 1);
    assert_eq!(source.source_state(), MfMediaSourceState::Started);
    assert_eq!(source.stream_state(), MfMediaStreamState::Started);
    source.complete_sample(second, 10).unwrap();
}

#[test]
fn stopped_source_rejects_requests_and_duplicate_stop() {
    let mut source = MfMediaSourceCore::default();
    assert_eq!(
        source.request_sample(),
        Err(MfMediaSourceCoreError::InvalidState {
            operation: MfMediaSourceOperation::RequestSample,
            state: MfMediaSourceState::Stopped,
        })
    );
    source
        .start(MfPresentationSelection::canonical(), 0)
        .unwrap();
    source.stop().unwrap();
    assert!(matches!(
        source.stop(),
        Err(MfMediaSourceCoreError::InvalidState {
            operation: MfMediaSourceOperation::Stop,
            state: MfMediaSourceState::Stopped,
        })
    ));
}

#[test]
fn shutdown_is_idempotent_cancels_requests_and_is_terminal() {
    let mut source = MfMediaSourceCore::default();
    source
        .start(MfPresentationSelection::canonical(), 0)
        .unwrap();
    source.request_sample().unwrap();

    let first = source.shutdown();
    assert!(first.was_started);
    assert_eq!(first.cancelled_sample_requests, 1);
    assert_eq!(source.source_state(), MfMediaSourceState::Shutdown);
    assert_eq!(source.stream_state(), MfMediaStreamState::Shutdown);

    let repeated = source.shutdown();
    assert!(!repeated.was_started);
    assert_eq!(repeated.cancelled_sample_requests, 0);
    assert!(matches!(
        source.start(MfPresentationSelection::canonical(), 0),
        Err(MfMediaSourceCoreError::InvalidState {
            operation: MfMediaSourceOperation::Start,
            state: MfMediaSourceState::Shutdown,
        })
    ));
}
