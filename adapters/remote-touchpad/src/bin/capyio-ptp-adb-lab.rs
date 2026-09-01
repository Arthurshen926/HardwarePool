#[cfg(windows)]
mod windows_lab {
    use std::{
        cell::RefCell,
        env,
        error::Error,
        fmt,
        io::{Read, Write},
        net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
        process::ExitCode,
        rc::Rc,
        thread,
        time::{Duration, Instant},
    };

    use capyio_core::{PortRef, RouteId, SessionId};
    use capyio_input::{
        InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType, TouchpadDescriptor,
        TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
    };
    use capyio_remote_touchpad_adapter::{
        PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES, PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES,
        PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES, PRIVATE_TOUCHPAD_TRANSPORT_HELLO_BYTES,
        PrivateTouchpadReceiverLimits, PrivateTouchpadRouteBinding, PrivateTouchpadSink,
        PrivateTouchpadSinkFactory, PrivateTouchpadTransportReceiver,
        PrivateTouchpadTransportReceiverState,
    };
    use capyio_windows_input::{
        SyntheticTouchpadSession, SyntheticTouchpadSessionError, VhfTouchpadSession,
        VhfTouchpadSessionError, VhfWin32Transport,
    };

    const LAB_PORT: u16 = 61000;
    const ROUTE_EPOCH: u64 = 1;
    const MAX_CONTACTS: u8 = 5;
    const IO_TIMEOUT: Duration = Duration::from_secs(30);
    const CURSOR_OBSERVATION_IO_TIMEOUT: Duration = Duration::from_secs(180);
    const TAP_DRAG_TRACE_IO_TIMEOUT: Duration = Duration::from_secs(180);
    const CURSOR_SOURCE_MOTION_MIN_HIMETRIC: i64 = 100;
    const TAP_DRAG_FIRST_TAP_MIN_DURATION_NANOS: u64 = 20_000_000;
    const TAP_DRAG_FIRST_TAP_MAX_DURATION_NANOS: u64 = 300_000_000;
    const TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC: i64 = 150;
    const TAP_DRAG_MAX_INTER_CONTACT_GAP_NANOS: u64 = 500_000_000;
    const TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC: i64 = 500;
    const TAP_DRAG_SECOND_CONTACT_MIN_MOTION_HIMETRIC: i64 = 100;
    const MANUAL_IO_TIMEOUT: Duration = Duration::from_secs(600);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Options {
        inject: bool,
        acknowledged: bool,
        anchor_and_observe_cursor: bool,
        trace_tap_drag: bool,
        exit_after_release_exact_contacts: Option<u8>,
        exit_after_release_min_contacts: Option<u8>,
        manual_session: bool,
        vhf: bool,
    }

    pub fn main() -> ExitCode {
        let options = match parse_args(env::args().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                eprintln!("{error}");
                eprintln!("{}", usage());
                return ExitCode::from(64);
            }
        };
        match run(options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("lab_status=failed");
                eprintln!("lab_detail={error}");
                ExitCode::FAILURE
            }
        }
    }

    fn usage() -> &'static str {
        "Usage: capyio-ptp-adb-lab --inject --acknowledge-desktop-input [--vhf] [--exit-after-release | --exit-after-release-at-least=1..5 | --exit-after-release-exactly=1..5] [--anchor-and-observe-cursor | --trace-tap-drag] [--manual-session]\nListens only on 127.0.0.1:61000 for an authenticated adb reverse lab connection. --vhf uses the already installed protected VHF device; the default remains the user-mode synthetic projection. --exit-after-release-at-least ignores completed gestures below its contact threshold; --exit-after-release-exactly accepts only a gesture with the requested peak contact count. --anchor-and-observe-cursor is a VHF-only exact-one-contact acceptance that anchors the Windows cursor before listening and reports its final delta. --trace-tap-drag is a VHF-only bounded diagnostic that exits only after a qualified short stationary tap followed within 500 ms by a moving one-contact gesture, and reports their timing and motion. --manual-session raises the bounded idle timeout from 30 seconds to 10 minutes."
    }

    fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
        let mut options = Options {
            inject: false,
            acknowledged: false,
            anchor_and_observe_cursor: false,
            trace_tap_drag: false,
            exit_after_release_exact_contacts: None,
            exit_after_release_min_contacts: None,
            manual_session: false,
            vhf: false,
        };
        for argument in arguments {
            match argument.as_str() {
                "--inject" if !options.inject => options.inject = true,
                "--acknowledge-desktop-input" if !options.acknowledged => {
                    options.acknowledged = true;
                }
                "--anchor-and-observe-cursor" if !options.anchor_and_observe_cursor => {
                    options.anchor_and_observe_cursor = true;
                }
                "--trace-tap-drag" if !options.trace_tap_drag => {
                    options.trace_tap_drag = true;
                }
                "--exit-after-release"
                    if options.exit_after_release_min_contacts.is_none()
                        && options.exit_after_release_exact_contacts.is_none() =>
                {
                    options.exit_after_release_min_contacts = Some(1);
                }
                _ if argument.starts_with("--exit-after-release-at-least=")
                    && options.exit_after_release_min_contacts.is_none()
                    && options.exit_after_release_exact_contacts.is_none() =>
                {
                    let value = argument
                        .strip_prefix("--exit-after-release-at-least=")
                        .expect("guard checked prefix")
                        .parse::<u8>()
                        .map_err(|_| {
                            format!(
                                "invalid contact threshold in argument: {argument}; expected 1..={MAX_CONTACTS}"
                            )
                        })?;
                    if !(1..=MAX_CONTACTS).contains(&value) {
                        return Err(format!(
                            "invalid contact threshold in argument: {argument}; expected 1..={MAX_CONTACTS}"
                        ));
                    }
                    options.exit_after_release_min_contacts = Some(value);
                }
                _ if argument.starts_with("--exit-after-release-exactly=")
                    && options.exit_after_release_min_contacts.is_none()
                    && options.exit_after_release_exact_contacts.is_none() =>
                {
                    let value = argument
                        .strip_prefix("--exit-after-release-exactly=")
                        .expect("guard checked prefix")
                        .parse::<u8>()
                        .map_err(|_| {
                            format!(
                                "invalid exact contact count in argument: {argument}; expected 1..={MAX_CONTACTS}"
                            )
                        })?;
                    if !(1..=MAX_CONTACTS).contains(&value) {
                        return Err(format!(
                            "invalid exact contact count in argument: {argument}; expected 1..={MAX_CONTACTS}"
                        ));
                    }
                    options.exit_after_release_exact_contacts = Some(value);
                }
                "--manual-session" if !options.manual_session => {
                    options.manual_session = true;
                }
                "--vhf" if !options.vhf => options.vhf = true,
                "--help" | "-h" => return Err(usage().to_owned()),
                _ => return Err(format!("unsupported or duplicate argument: {argument}")),
            }
        }
        if !options.inject || !options.acknowledged {
            return Err(
                "real device creation requires --inject and --acknowledge-desktop-input".to_owned(),
            );
        }
        if options.anchor_and_observe_cursor
            && (!options.vhf
                || options.exit_after_release_exact_contacts != Some(1)
                || options.manual_session)
        {
            return Err(
                "--anchor-and-observe-cursor requires --vhf and --exit-after-release-exactly=1"
                    .to_owned(),
            );
        }
        if options.trace_tap_drag
            && (!options.vhf
                || options.anchor_and_observe_cursor
                || options.exit_after_release_exact_contacts.is_some()
                || options.exit_after_release_min_contacts.is_some()
                || options.manual_session)
        {
            return Err(
                "--trace-tap-drag requires --vhf and cannot be combined with release-exit, cursor-anchor or manual-session modes"
                    .to_owned(),
            );
        }
        Ok(options)
    }

    fn run(options: Options) -> Result<(), Box<dyn Error>> {
        let stream_descriptor = stream();
        let descriptor = descriptor();
        let metrics = Rc::new(RefCell::new(LabMetrics::default()));
        let factory = LabSinkFactory {
            vhf: options.vhf,
            metrics: Rc::clone(&metrics),
        };
        let mut transport = PrivateTouchpadTransportReceiver::new(
            binding(),
            stream_descriptor,
            descriptor,
            0,
            PrivateTouchpadReceiverLimits::default(),
            factory,
        )?;
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, LAB_PORT))?;
        let mut cursor_before = if options.anchor_and_observe_cursor {
            Some(anchor_cursor_to_virtual_desktop_center()?)
        } else {
            None
        };
        println!("lab_status=listening");
        println!("listen_address=127.0.0.1:{LAB_PORT}");
        println!("desktop_input_acknowledged={}", options.acknowledged);

        let (mut connection, peer) = listener.accept()?;
        if !peer.ip().is_loopback() {
            return Err("non-loopback peer rejected".into());
        }
        let io_timeout = if options.trace_tap_drag {
            TAP_DRAG_TRACE_IO_TIMEOUT
        } else if options.manual_session {
            MANUAL_IO_TIMEOUT
        } else if options.anchor_and_observe_cursor {
            CURSOR_OBSERVATION_IO_TIMEOUT
        } else {
            IO_TIMEOUT
        };
        configure(&connection, io_timeout)?;
        let mut hello = [0_u8; PRIVATE_TOUCHPAD_TRANSPORT_HELLO_BYTES];
        connection.read_exact(&mut hello)?;
        transport.accept_hello(&hello)?;
        println!("hello_binding=accepted");
        println!("device_creation=created");
        println!(
            "projection={}",
            if options.vhf { "vhf" } else { "synthetic" }
        );
        println!("idle_timeout_seconds={}", io_timeout.as_secs());
        if let Some(min_contacts) = options.exit_after_release_min_contacts {
            println!("exit_after_release_min_contacts={min_contacts}");
        }
        if let Some(exact_contacts) = options.exit_after_release_exact_contacts {
            println!("exit_after_release_exact_contacts={exact_contacts}");
        }
        if options.trace_tap_drag {
            println!("tap_drag_trace_gestures_required=2");
            println!(
                "tap_drag_trace_qualification=first_duration_nanos:{}..={},first_max_motion_himetric:{},max_gap_nanos:{},max_start_delta_himetric:{},second_min_motion_himetric:{}",
                TAP_DRAG_FIRST_TAP_MIN_DURATION_NANOS,
                TAP_DRAG_FIRST_TAP_MAX_DURATION_NANOS,
                TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC,
                TAP_DRAG_MAX_INTER_CONTACT_GAP_NANOS,
                TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC,
                TAP_DRAG_SECOND_CONTACT_MIN_MOTION_HIMETRIC,
            );
        }
        let arrival_epoch = Instant::now();
        let mut frames = 0_u64;
        let mut gesture_peak_contacts = 0_usize;
        let mut accepted_exit_gesture_peak_contacts = None;
        let mut last_contact_count = None;
        let mut max_contacts_observed = 0_usize;
        loop {
            let mut header = [0_u8; PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES];
            connection.read_exact(&mut header)?;
            match header[5] {
                2 => {
                    let record = read_data_record(&mut connection, header)?;
                    let sequence = u64::from_le_bytes(header[16..24].try_into()?);
                    let outcome =
                        match transport.receive_data(&record, elapsed_nanos(arrival_epoch)) {
                            Ok(outcome) => outcome,
                            Err(error) => {
                                eprintln!("submission_failure_sequence={sequence}");
                                return Err(Box::new(error));
                            }
                        };
                    let current_contact_count = usize::from(outcome.receive.active_contacts);
                    max_contacts_observed = max_contacts_observed.max(current_contact_count);
                    let gesture_started = current_contact_count > 0 && gesture_peak_contacts == 0;
                    if gesture_started && options.anchor_and_observe_cursor {
                        let baseline = get_cursor_position()?;
                        println!("cursor_gesture_baseline={},{}", baseline.x, baseline.y);
                        cursor_before = Some(baseline);
                    }
                    if last_contact_count != Some(current_contact_count) {
                        println!(
                            "contact_count_transition={}->{} sequence={sequence}",
                            last_contact_count.unwrap_or(0),
                            current_contact_count,
                        );
                        last_contact_count = Some(current_contact_count);
                    }
                    let released = current_contact_count == 0 && gesture_peak_contacts > 0;
                    gesture_peak_contacts = gesture_peak_contacts.max(current_contact_count);
                    frames += 1;
                    connection.write_all(outcome.ack.as_bytes())?;
                    connection.flush()?;
                    if options.trace_tap_drag && metrics.borrow().tap_drag_gesture_count >= 2 {
                        break;
                    }
                    if released {
                        let reached_exit_threshold = options
                            .exit_after_release_min_contacts
                            .is_some_and(|minimum| gesture_peak_contacts >= usize::from(minimum));
                        let reached_exact_contact_count = options
                            .exit_after_release_exact_contacts
                            .is_some_and(|exact| gesture_peak_contacts == usize::from(exact));
                        let cursor_motion_qualified = !options.anchor_and_observe_cursor
                            || metrics
                                .borrow()
                                .single_contact_motion_exceeds(CURSOR_SOURCE_MOTION_MIN_HIMETRIC);
                        if reached_exit_threshold
                            || (reached_exact_contact_count && cursor_motion_qualified)
                        {
                            accepted_exit_gesture_peak_contacts = Some(gesture_peak_contacts);
                            break;
                        }
                        if let Some(exact) = options.exit_after_release_exact_contacts {
                            if reached_exact_contact_count {
                                println!(
                                    "ignored_released_gesture_peak_contacts={gesture_peak_contacts} reason=source_motion_below_threshold min_himetric={CURSOR_SOURCE_MOTION_MIN_HIMETRIC}"
                                );
                            } else {
                                println!(
                                    "ignored_released_gesture_peak_contacts={gesture_peak_contacts} expected_exact_contacts={exact}"
                                );
                            }
                            if options.anchor_and_observe_cursor {
                                metrics.borrow_mut().reset_single_contact_observation();
                                println!("cursor_observation_reset_after_ignored_gesture=true");
                            }
                        }
                        gesture_peak_contacts = 0;
                    }
                }
                4 => {
                    transport.accept_close(&header)?;
                    break;
                }
                actual => return Err(format!("unexpected transport record kind {actual}").into()),
            }
        }
        if transport.state() == PrivateTouchpadTransportReceiverState::Active {
            transport.disconnect()?;
        }
        let metrics = *metrics.borrow();
        println!("lab_status=complete");
        println!("frames_processed={frames}");
        println!("batches_submitted={}", metrics.batches_submitted);
        println!(
            "contact_records_submitted={}",
            metrics.contact_records_submitted
        );
        println!("vhf_frames_submitted={}", metrics.vhf_frames_submitted);
        println!(
            "tap_drag_button_latches={}",
            metrics.tap_drag_button_latches
        );
        println!("max_contacts_observed={max_contacts_observed}");
        if let Some(accepted_peak) = accepted_exit_gesture_peak_contacts {
            println!("accepted_exit_gesture_peak_contacts={accepted_peak}");
        }
        println!("device_cleanup=closed");
        println!(
            "single_contact_frames_observed={}",
            metrics.single_contact_frames_observed
        );
        if let (Some(first), Some(last)) =
            (metrics.first_single_contact, metrics.last_single_contact)
        {
            println!(
                "single_contact_first=timestamp_nanos:{},x_himetric:{},y_himetric:{}",
                first.timestamp_nanos, first.x_himetric, first.y_himetric
            );
            println!(
                "single_contact_last=timestamp_nanos:{},x_himetric:{},y_himetric:{}",
                last.timestamp_nanos, last.x_himetric, last.y_himetric
            );
            println!(
                "single_contact_source_delta=timestamp_nanos:{},x_himetric:{},y_himetric:{}",
                last.timestamp_nanos.saturating_sub(first.timestamp_nanos),
                i64::from(last.x_himetric) - i64::from(first.x_himetric),
                i64::from(last.y_himetric) - i64::from(first.y_himetric),
            );
        }
        if options.trace_tap_drag {
            print_tap_drag_trace(&metrics)?;
        }
        if let Some(before) = cursor_before {
            thread::sleep(Duration::from_millis(250));
            let after = get_cursor_position()?;
            let delta_x = after.x - before.x;
            let delta_y = after.y - before.y;
            println!("cursor_before={},{}", before.x, before.y);
            println!("cursor_after={},{}", after.x, after.y);
            println!("cursor_delta={delta_x},{delta_y}");
            println!("cursor_moved={}", delta_x != 0 || delta_y != 0);
            if delta_x == 0 && delta_y == 0 {
                return Err("VHF frames were accepted but the Windows cursor did not move".into());
            }
        }
        Ok(())
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct SingleContactSample {
        timestamp_nanos: u64,
        x_himetric: u32,
        y_himetric: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct OneContactGestureAccumulator {
        first: SingleContactSample,
        last: SingleContactSample,
        first_arrival_nanos: u64,
        last_arrival_nanos: u64,
        frames: u64,
        first_motion_at_nanos: Option<u64>,
        first_motion_arrival_nanos: Option<u64>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct OneContactGestureTrace {
        first: SingleContactSample,
        last: SingleContactSample,
        released_at_nanos: u64,
        first_arrival_nanos: u64,
        last_arrival_nanos: u64,
        released_arrival_nanos: u64,
        frames: u64,
        first_motion_at_nanos: Option<u64>,
        first_motion_arrival_nanos: Option<u64>,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct LabMetrics {
        batches_submitted: u64,
        contact_records_submitted: u64,
        vhf_frames_submitted: u64,
        single_contact_frames_observed: u64,
        first_single_contact: Option<SingleContactSample>,
        last_single_contact: Option<SingleContactSample>,
        current_one_contact_gesture: Option<OneContactGestureAccumulator>,
        tap_drag_gestures: [Option<OneContactGestureTrace>; 2],
        tap_drag_gesture_count: usize,
        tap_drag_completed_one_contact_gestures: u64,
        tap_drag_rejected_candidates: u64,
        tap_drag_first_tap_candidate: Option<OneContactGestureTrace>,
        tap_drag_previous_contact_count: usize,
        tap_drag_button_latches: u64,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct TapCandidate {
        released_at_nanos: u64,
        x_himetric: u32,
        y_himetric: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct ActiveTap {
        started_at_nanos: u64,
        x_himetric: u32,
        y_himetric: u32,
        max_delta_x_himetric: i64,
        max_delta_y_himetric: i64,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct TapDragButtonLatch {
        active_tap: Option<ActiveTap>,
        candidate: Option<TapCandidate>,
        button_held: bool,
        previous_contact_count: usize,
    }

    impl TapDragButtonLatch {
        fn project(&mut self, frame: &TouchpadFrame) -> (TouchpadFrame, bool) {
            let mut output = frame.clone();
            output.button = TouchpadButtonState::Released;
            if frame.kind == TouchpadFrameKind::CancelAll {
                *self = Self::default();
                return (output, false);
            }

            let contact_count = frame.contacts.len();
            let mut latch_started = false;
            match frame.contacts.as_slice() {
                [contact] => {
                    if self.button_held {
                        output.button = TouchpadButtonState::Pressed;
                    } else if self.previous_contact_count == 0 {
                        let qualifies = self.candidate.is_some_and(|candidate| {
                            frame
                                .header
                                .source_timestamp_nanos
                                .checked_sub(candidate.released_at_nanos)
                                .is_some_and(|gap| gap <= TAP_DRAG_MAX_INTER_CONTACT_GAP_NANOS)
                                && (i64::from(contact.position.x_himetric)
                                    - i64::from(candidate.x_himetric))
                                .abs()
                                    <= TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC
                                && (i64::from(contact.position.y_himetric)
                                    - i64::from(candidate.y_himetric))
                                .abs()
                                    <= TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC
                        });
                        self.candidate = None;
                        if qualifies {
                            self.button_held = true;
                            output.button = TouchpadButtonState::Pressed;
                            latch_started = true;
                            self.active_tap = None;
                        } else {
                            self.active_tap = Some(ActiveTap {
                                started_at_nanos: frame.header.source_timestamp_nanos,
                                x_himetric: contact.position.x_himetric,
                                y_himetric: contact.position.y_himetric,
                                max_delta_x_himetric: 0,
                                max_delta_y_himetric: 0,
                            });
                        }
                    } else if let Some(active) = self.active_tap.as_mut() {
                        active.max_delta_x_himetric = active.max_delta_x_himetric.max(
                            (i64::from(contact.position.x_himetric) - i64::from(active.x_himetric))
                                .abs(),
                        );
                        active.max_delta_y_himetric = active.max_delta_y_himetric.max(
                            (i64::from(contact.position.y_himetric) - i64::from(active.y_himetric))
                                .abs(),
                        );
                    }
                }
                [] => {
                    if self.button_held {
                        self.button_held = false;
                        self.active_tap = None;
                        self.candidate = None;
                    } else if self.previous_contact_count == 1
                        && let Some(active) = self.active_tap.take()
                    {
                        let duration = frame
                            .header
                            .source_timestamp_nanos
                            .saturating_sub(active.started_at_nanos);
                        if (TAP_DRAG_FIRST_TAP_MIN_DURATION_NANOS
                            ..=TAP_DRAG_FIRST_TAP_MAX_DURATION_NANOS)
                            .contains(&duration)
                            && active.max_delta_x_himetric <= TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC
                            && active.max_delta_y_himetric <= TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC
                        {
                            self.candidate = Some(TapCandidate {
                                released_at_nanos: frame.header.source_timestamp_nanos,
                                x_himetric: active.x_himetric,
                                y_himetric: active.y_himetric,
                            });
                        } else {
                            self.candidate = None;
                        }
                    }
                }
                _ => {
                    self.active_tap = None;
                    self.candidate = None;
                    self.button_held = false;
                }
            }
            self.previous_contact_count = contact_count;
            (output, latch_started)
        }

        fn reset(&mut self) {
            *self = Self::default();
        }
    }

    impl LabMetrics {
        fn single_contact_motion_exceeds(&self, minimum_himetric: i64) -> bool {
            let (Some(first), Some(last)) = (self.first_single_contact, self.last_single_contact)
            else {
                return false;
            };
            let delta_x = i64::from(last.x_himetric) - i64::from(first.x_himetric);
            let delta_y = i64::from(last.y_himetric) - i64::from(first.y_himetric);
            delta_x.abs() >= minimum_himetric || delta_y.abs() >= minimum_himetric
        }

        fn reset_single_contact_observation(&mut self) {
            self.single_contact_frames_observed = 0;
            self.first_single_contact = None;
            self.last_single_contact = None;
        }

        #[cfg(test)]
        fn record_frame(&mut self, frame: &TouchpadFrame) {
            self.record_frame_at(frame, frame.header.source_timestamp_nanos);
        }

        fn record_frame_at(&mut self, frame: &TouchpadFrame, arrival_nanos: u64) {
            let contact_count = frame.contacts.len();
            match frame.contacts.as_slice() {
                [contact] => {
                    let sample = SingleContactSample {
                        timestamp_nanos: frame.header.source_timestamp_nanos,
                        x_himetric: contact.position.x_himetric,
                        y_himetric: contact.position.y_himetric,
                    };
                    self.single_contact_frames_observed += 1;
                    self.first_single_contact.get_or_insert(sample);
                    self.last_single_contact = Some(sample);

                    if let Some(current) = self.current_one_contact_gesture.as_mut() {
                        current.frames += 1;
                        if current.first_motion_at_nanos.is_none()
                            && (sample.x_himetric != current.first.x_himetric
                                || sample.y_himetric != current.first.y_himetric)
                        {
                            current.first_motion_at_nanos = Some(sample.timestamp_nanos);
                            current.first_motion_arrival_nanos = Some(arrival_nanos);
                        }
                        current.last = sample;
                        current.last_arrival_nanos = arrival_nanos;
                    } else if self.tap_drag_previous_contact_count == 0 {
                        self.current_one_contact_gesture = Some(OneContactGestureAccumulator {
                            first: sample,
                            last: sample,
                            first_arrival_nanos: arrival_nanos,
                            last_arrival_nanos: arrival_nanos,
                            frames: 1,
                            first_motion_at_nanos: None,
                            first_motion_arrival_nanos: None,
                        });
                    }
                }
                [] => self
                    .finish_one_contact_gesture(frame.header.source_timestamp_nanos, arrival_nanos),
                _ => {
                    self.current_one_contact_gesture = None;
                }
            }
            self.tap_drag_previous_contact_count = contact_count;
        }

        fn finish_one_contact_gesture(
            &mut self,
            released_at_nanos: u64,
            released_arrival_nanos: u64,
        ) {
            let Some(current) = self.current_one_contact_gesture.take() else {
                return;
            };
            if self.tap_drag_gesture_count >= self.tap_drag_gestures.len() {
                return;
            }
            let completed = OneContactGestureTrace {
                first: current.first,
                last: current.last,
                released_at_nanos,
                first_arrival_nanos: current.first_arrival_nanos,
                last_arrival_nanos: current.last_arrival_nanos,
                released_arrival_nanos,
                frames: current.frames,
                first_motion_at_nanos: current.first_motion_at_nanos,
                first_motion_arrival_nanos: current.first_motion_arrival_nanos,
            };
            self.tap_drag_completed_one_contact_gestures += 1;

            if let Some(first) = self.tap_drag_first_tap_candidate.take() {
                if tap_drag_pair_qualifies(first, completed) {
                    self.tap_drag_gestures = [Some(first), Some(completed)];
                    self.tap_drag_gesture_count = self.tap_drag_gestures.len();
                    return;
                }
                self.tap_drag_rejected_candidates += 1;
            }

            if first_tap_qualifies(completed) {
                self.tap_drag_first_tap_candidate = Some(completed);
            }
        }
    }

    fn gesture_duration_nanos(gesture: OneContactGestureTrace) -> Option<u64> {
        gesture
            .released_at_nanos
            .checked_sub(gesture.first.timestamp_nanos)
    }

    fn gesture_motion_himetric(gesture: OneContactGestureTrace) -> (i64, i64) {
        (
            i64::from(gesture.last.x_himetric) - i64::from(gesture.first.x_himetric),
            i64::from(gesture.last.y_himetric) - i64::from(gesture.first.y_himetric),
        )
    }

    fn first_tap_qualifies(gesture: OneContactGestureTrace) -> bool {
        let Some(duration) = gesture_duration_nanos(gesture) else {
            return false;
        };
        let (delta_x, delta_y) = gesture_motion_himetric(gesture);
        (TAP_DRAG_FIRST_TAP_MIN_DURATION_NANOS..=TAP_DRAG_FIRST_TAP_MAX_DURATION_NANOS)
            .contains(&duration)
            && delta_x.abs() <= TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC
            && delta_y.abs() <= TAP_DRAG_FIRST_TAP_MAX_MOTION_HIMETRIC
    }

    fn tap_drag_pair_qualifies(
        first: OneContactGestureTrace,
        second: OneContactGestureTrace,
    ) -> bool {
        if !first_tap_qualifies(first) {
            return false;
        }
        let Some(gap) = second
            .first
            .timestamp_nanos
            .checked_sub(first.released_at_nanos)
        else {
            return false;
        };
        let (delta_x, delta_y) = gesture_motion_himetric(second);
        let start_delta_x = i64::from(second.first.x_himetric) - i64::from(first.first.x_himetric);
        let start_delta_y = i64::from(second.first.y_himetric) - i64::from(first.first.y_himetric);
        gap <= TAP_DRAG_MAX_INTER_CONTACT_GAP_NANOS
            && start_delta_x.abs() <= TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC
            && start_delta_y.abs() <= TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC
            && (delta_x.abs() >= TAP_DRAG_SECOND_CONTACT_MIN_MOTION_HIMETRIC
                || delta_y.abs() >= TAP_DRAG_SECOND_CONTACT_MIN_MOTION_HIMETRIC)
    }

    fn print_tap_drag_trace(metrics: &LabMetrics) -> Result<(), Box<dyn Error>> {
        if metrics.tap_drag_gesture_count != metrics.tap_drag_gestures.len() {
            return Err(format!(
                "tap-and-drag trace ended after {} of {} required one-contact gestures",
                metrics.tap_drag_gesture_count,
                metrics.tap_drag_gestures.len()
            )
            .into());
        }
        let first = metrics.tap_drag_gestures[0].ok_or("missing first tap trace")?;
        let second = metrics.tap_drag_gestures[1].ok_or("missing second drag trace")?;
        println!(
            "tap_drag_completed_one_contact_gestures={}",
            metrics.tap_drag_completed_one_contact_gestures
        );
        println!(
            "tap_drag_rejected_candidates={}",
            metrics.tap_drag_rejected_candidates
        );
        for (index, gesture) in [first, second].iter().enumerate() {
            let number = index + 1;
            let duration = gesture
                .released_at_nanos
                .checked_sub(gesture.first.timestamp_nanos)
                .ok_or("gesture release timestamp regressed")?;
            let delta_x = i64::from(gesture.last.x_himetric) - i64::from(gesture.first.x_himetric);
            let delta_y = i64::from(gesture.last.y_himetric) - i64::from(gesture.first.y_himetric);
            println!("tap_drag_gesture_{number}_frames={}", gesture.frames);
            println!("tap_drag_gesture_{number}_duration_nanos={duration}");
            println!(
                "tap_drag_gesture_{number}_arrival_duration_nanos={}",
                gesture
                    .released_arrival_nanos
                    .saturating_sub(gesture.first_arrival_nanos)
            );
            println!("tap_drag_gesture_{number}_delta_himetric={delta_x},{delta_y}");
            match gesture.first_motion_at_nanos {
                Some(first_motion) => println!(
                    "tap_drag_gesture_{number}_first_motion_after_nanos={}",
                    first_motion.saturating_sub(gesture.first.timestamp_nanos)
                ),
                None => println!("tap_drag_gesture_{number}_first_motion_after_nanos=none"),
            }
            match gesture.first_motion_arrival_nanos {
                Some(first_motion) => println!(
                    "tap_drag_gesture_{number}_first_motion_after_arrival_nanos={}",
                    first_motion.saturating_sub(gesture.first_arrival_nanos)
                ),
                None => println!("tap_drag_gesture_{number}_first_motion_after_arrival_nanos=none"),
            }
            println!(
                "tap_drag_gesture_{number}_last_arrival_after_nanos={}",
                gesture
                    .last_arrival_nanos
                    .saturating_sub(gesture.first_arrival_nanos)
            );
        }
        let gap = second
            .first
            .timestamp_nanos
            .checked_sub(first.released_at_nanos)
            .ok_or("second contact began before first contact release")?;
        println!("tap_drag_inter_contact_gap_nanos={gap}");
        println!(
            "tap_drag_inter_contact_arrival_gap_nanos={}",
            second
                .first_arrival_nanos
                .saturating_sub(first.released_arrival_nanos)
        );
        println!(
            "tap_drag_start_position_delta_himetric={},{}",
            i64::from(second.first.x_himetric) - i64::from(first.first.x_himetric),
            i64::from(second.first.y_himetric) - i64::from(first.first.y_himetric),
        );
        println!("tap_drag_trace_complete=true");
        Ok(())
    }

    enum LabProjection {
        Synthetic(SyntheticTouchpadSession),
        Vhf(VhfTouchpadSession<VhfWin32Transport>),
    }

    struct LabSink {
        projection: LabProjection,
        metrics: Rc<RefCell<LabMetrics>>,
        arrival_epoch: Instant,
        tap_drag_button_latch: Option<TapDragButtonLatch>,
    }

    #[derive(Debug)]
    enum LabSinkError {
        Synthetic(SyntheticTouchpadSessionError),
        Vhf(VhfTouchpadSessionError),
    }

    impl fmt::Display for LabSinkError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Synthetic(error) => error.fmt(formatter),
                Self::Vhf(error) => error.fmt(formatter),
            }
        }
    }

    impl Error for LabSinkError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                Self::Synthetic(error) => Some(error),
                Self::Vhf(error) => Some(error),
            }
        }
    }

    impl From<SyntheticTouchpadSessionError> for LabSinkError {
        fn from(error: SyntheticTouchpadSessionError) -> Self {
            Self::Synthetic(error)
        }
    }

    impl From<VhfTouchpadSessionError> for LabSinkError {
        fn from(error: VhfTouchpadSessionError) -> Self {
            Self::Vhf(error)
        }
    }

    impl LabSink {
        fn record_synthetic(&self, submitted: capyio_windows_input::SyntheticTouchpadSubmission) {
            let mut metrics = self.metrics.borrow_mut();
            metrics.batches_submitted += u64::from(submitted.batches_submitted);
            metrics.contact_records_submitted += u64::from(submitted.contact_records_submitted);
        }
    }

    impl PrivateTouchpadSink for LabSink {
        type Error = LabSinkError;

        fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
            let projected = self
                .tap_drag_button_latch
                .as_mut()
                .map(|latch| latch.project(frame));
            let projected_frame = projected.as_ref().map_or(frame, |(frame, _)| frame);
            match &mut self.projection {
                LabProjection::Synthetic(session) => {
                    let submitted =
                        SyntheticTouchpadSession::submit_frame(session, projected_frame)?;
                    self.record_synthetic(submitted);
                }
                LabProjection::Vhf(session) => {
                    VhfTouchpadSession::submit_frame(session, projected_frame)?;
                    self.metrics.borrow_mut().vhf_frames_submitted += 1;
                }
            }
            if projected.is_some_and(|(_, started)| started) {
                self.metrics.borrow_mut().tap_drag_button_latches += 1;
            }
            self.metrics
                .borrow_mut()
                .record_frame_at(frame, elapsed_nanos(self.arrival_epoch));
            Ok(())
        }

        fn advance_epoch(
            &mut self,
            new_epoch: u64,
            first_sequence: u64,
        ) -> Result<(), Self::Error> {
            if let Some(latch) = self.tap_drag_button_latch.as_mut() {
                latch.reset();
            }
            match &mut self.projection {
                LabProjection::Synthetic(session) => {
                    let submitted = SyntheticTouchpadSession::advance_epoch(
                        session,
                        new_epoch,
                        first_sequence,
                    )?;
                    self.record_synthetic(submitted);
                }
                LabProjection::Vhf(session) => {
                    VhfTouchpadSession::advance_epoch(session, new_epoch)?;
                }
            }
            Ok(())
        }

        fn close(&mut self) -> Result<(), Self::Error> {
            match &mut self.projection {
                LabProjection::Synthetic(session) => {
                    let submitted = SyntheticTouchpadSession::close(session)?;
                    self.record_synthetic(submitted);
                }
                LabProjection::Vhf(session) => VhfTouchpadSession::close(session)?,
            }
            Ok(())
        }
    }

    struct LabSinkFactory {
        vhf: bool,
        metrics: Rc<RefCell<LabMetrics>>,
    }

    impl PrivateTouchpadSinkFactory for LabSinkFactory {
        type Sink = LabSink;
        type Error = LabSinkError;

        fn open(
            &mut self,
            stream: &InputStreamDescriptor,
            descriptor: TouchpadDescriptor,
            first_sequence: u64,
        ) -> Result<Self::Sink, Self::Error> {
            let projection = if self.vhf {
                let mut projected_descriptor = descriptor;
                projected_descriptor.button_type = TouchpadButtonType::ClickPad;
                LabProjection::Vhf(VhfTouchpadSession::<VhfWin32Transport>::open_win32(
                    projected_descriptor,
                    stream.stream_epoch,
                )?)
            } else {
                LabProjection::Synthetic(SyntheticTouchpadSession::open(
                    stream,
                    descriptor,
                    first_sequence,
                )?)
            };
            Ok(LabSink {
                projection,
                metrics: Rc::clone(&self.metrics),
                arrival_epoch: Instant::now(),
                tap_drag_button_latch: self.vhf.then(TapDragButtonLatch::default),
            })
        }
    }

    fn elapsed_nanos(epoch: Instant) -> u64 {
        u64::try_from(epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    #[repr(C)]
    struct Win32Point {
        x: i32,
        y: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetCursorPos(point: *mut Win32Point) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
    }

    fn get_cursor_position() -> Result<Win32Point, std::io::Error> {
        let mut point = Win32Point { x: 0, y: 0 };
        // SAFETY: `point` is valid writable storage for one Win32 POINT.
        if unsafe { GetCursorPos(&raw mut point) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(point)
    }

    fn anchor_cursor_to_virtual_desktop_center() -> Result<Win32Point, Box<dyn Error>> {
        const SM_XVIRTUALSCREEN: i32 = 76;
        const SM_YVIRTUALSCREEN: i32 = 77;
        const SM_CXVIRTUALSCREEN: i32 = 78;
        const SM_CYVIRTUALSCREEN: i32 = 79;

        // SAFETY: these metric indices have no pointer or lifetime requirements.
        let (left, top, width, height) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        if width <= 0 || height <= 0 {
            return Err(
                format!("invalid virtual desktop bounds {left},{top},{width},{height}").into(),
            );
        }
        let anchor = Win32Point {
            x: left + width / 2,
            y: top + height / 2,
        };
        // SAFETY: the coordinates were derived from the current virtual desktop bounds.
        if unsafe { SetCursorPos(anchor.x, anchor.y) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        thread::sleep(Duration::from_millis(100));
        let actual = get_cursor_position()?;
        println!("cursor_anchor={},{}", actual.x, actual.y);
        Ok(actual)
    }

    fn configure(connection: &TcpStream, io_timeout: Duration) -> std::io::Result<()> {
        connection.set_nodelay(true)?;
        connection.set_read_timeout(Some(io_timeout))?;
        connection.set_write_timeout(Some(io_timeout))
    }

    fn read_data_record(
        connection: &mut TcpStream,
        header: [u8; PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES],
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut packet_header = [0_u8; PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES];
        connection.read_exact(&mut packet_header)?;
        let contact_count = packet_header[7];
        if contact_count > MAX_CONTACTS {
            return Err(format!("contact count {contact_count} exceeds {MAX_CONTACTS}").into());
        }
        let tail_len = usize::from(contact_count) * PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES;
        let mut record = Vec::with_capacity(
            PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES
                + PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES
                + tail_len,
        );
        record.extend_from_slice(&header);
        record.extend_from_slice(&packet_header);
        record.resize(record.len() + tail_len, 0);
        let tail_start = record.len() - tail_len;
        connection.read_exact(&mut record[tail_start..])?;
        Ok(record)
    }

    fn id<T: std::str::FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().expect("fixed lab ID")
    }

    fn port(node: &str, capability: &str, port: &str) -> PortRef {
        PortRef {
            node_id: id(node),
            capability_id: id(capability),
            port_id: id(port),
        }
    }

    fn binding() -> PrivateTouchpadRouteBinding {
        PrivateTouchpadRouteBinding {
            route_id: id::<RouteId>("00000000-0000-4000-8000-00000000f101"),
            session_id: id::<SessionId>("00000000-0000-4000-8000-00000000f102"),
            source: port(
                "00000000-0000-4000-8000-00000000f103",
                "00000000-0000-4000-8000-00000000f104",
                "00000000-0000-4000-8000-00000000f105",
            ),
            sink: port(
                "00000000-0000-4000-8000-00000000f106",
                "00000000-0000-4000-8000-00000000f107",
                "00000000-0000-4000-8000-00000000f108",
            ),
            route_epoch: ROUTE_EPOCH,
            authorization_expires_at_ms: None,
        }
    }

    fn stream() -> InputStreamDescriptor {
        InputStreamDescriptor {
            stream_id: id("00000000-0000-4000-8000-00000000f109"),
            stream_epoch: ROUTE_EPOCH,
            clock_domain_id: "android.uptime-nanos".to_owned(),
        }
    }

    fn descriptor() -> TouchpadDescriptor {
        TouchpadDescriptor {
            physical_size: TouchpadPhysicalSize {
                width_himetric: 12_000,
                height_himetric: 7_000,
            },
            max_contacts: MAX_CONTACTS,
            button_type: TouchpadButtonType::NonClickable,
            reports_contact_size: false,
            reports_pressure: true,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use capyio_input::{InputFrameHeader, TouchpadContact, TouchpadPosition};

        fn args(values: &[&str]) -> Vec<String> {
            values.iter().map(|value| (*value).to_owned()).collect()
        }

        fn latch_frame(
            sequence: u64,
            timestamp_nanos: u64,
            positions: &[(u32, u32)],
        ) -> TouchpadFrame {
            TouchpadFrame {
                header: InputFrameHeader {
                    stream_id: stream().stream_id,
                    stream_epoch: ROUTE_EPOCH,
                    sequence,
                    source_timestamp_nanos: timestamp_nanos,
                },
                kind: TouchpadFrameKind::Update,
                button: TouchpadButtonState::Released,
                contacts: positions
                    .iter()
                    .enumerate()
                    .map(|(index, &(x, y))| TouchpadContact {
                        contact_id: index as u32 + 1,
                        position: TouchpadPosition {
                            x_himetric: x,
                            y_himetric: y,
                        },
                        confidence: true,
                        size: None,
                        pressure: None,
                    })
                    .collect(),
            }
        }

        #[test]
        fn tap_drag_button_latch_presses_on_second_down_and_releases_on_up() {
            let mut latch = TapDragButtonLatch::default();
            let (_, started) = latch.project(&latch_frame(0, 100_000_000, &[(1_000, 2_000)]));
            assert!(!started);
            let (first_up, _) = latch.project(&latch_frame(1, 160_000_000, &[]));
            assert_eq!(first_up.button, TouchpadButtonState::Released);

            let (second_down, started) =
                latch.project(&latch_frame(2, 161_000_000, &[(1_020, 2_000)]));
            assert!(started);
            assert_eq!(second_down.button, TouchpadButtonState::Pressed);
            let (moving, started) = latch.project(&latch_frame(3, 180_000_000, &[(1_500, 2_000)]));
            assert!(!started);
            assert_eq!(moving.button, TouchpadButtonState::Pressed);
            let (released, _) = latch.project(&latch_frame(4, 220_000_000, &[]));
            assert_eq!(released.button, TouchpadButtonState::Released);

            let mut output_descriptor = descriptor();
            output_descriptor.button_type = TouchpadButtonType::ClickPad;
            for frame in [second_down, moving, released] {
                frame.validate(&output_descriptor).expect("clickpad frame");
            }
        }

        #[test]
        fn tap_drag_button_latch_rejects_far_second_down_and_releases_for_multitouch() {
            let mut latch = TapDragButtonLatch::default();
            latch.project(&latch_frame(0, 100_000_000, &[(1_000, 2_000)]));
            latch.project(&latch_frame(1, 160_000_000, &[]));
            let (far, started) = latch.project(&latch_frame(2, 200_000_000, &[(1_501, 2_000)]));
            assert!(!started);
            assert_eq!(far.button, TouchpadButtonState::Released);

            latch.project(&latch_frame(3, 260_000_000, &[]));
            let (second_down, started) =
                latch.project(&latch_frame(4, 300_000_000, &[(1_500, 2_000)]));
            assert!(started);
            assert_eq!(second_down.button, TouchpadButtonState::Pressed);
            let (multi, _) = latch.project(&latch_frame(
                5,
                320_000_000,
                &[(1_500, 2_000), (2_000, 2_000)],
            ));
            assert_eq!(multi.button, TouchpadButtonState::Released);
        }

        #[test]
        fn real_listener_requires_both_explicit_input_gates() {
            assert!(parse_args(Vec::<String>::new()).is_err());
            assert!(parse_args(args(&["--inject"])).is_err());
            let synthetic = parse_args(args(&["--inject", "--acknowledge-desktop-input"]))
                .expect("explicit desktop-input gates");
            assert!(synthetic.inject && synthetic.acknowledged && !synthetic.vhf);
            let options = parse_args(args(&[
                "--inject",
                "--acknowledge-desktop-input",
                "--exit-after-release",
                "--manual-session",
                "--vhf",
            ]))
            .expect("explicit gates");
            assert!(
                options.inject
                    && options.acknowledged
                    && !options.anchor_and_observe_cursor
                    && !options.trace_tap_drag
                    && options.exit_after_release_exact_contacts.is_none()
                    && options.exit_after_release_min_contacts == Some(1)
                    && options.manual_session
                    && options.vhf
            );
            assert!(
                parse_args(args(&[
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--vhf",
                    "--vhf",
                ]))
                .is_err()
            );
        }

        #[test]
        fn tap_drag_trace_is_vhf_only_and_exclusive() {
            let options = parse_args(args(&[
                "--inject",
                "--acknowledge-desktop-input",
                "--vhf",
                "--trace-tap-drag",
            ]))
            .expect("bounded VHF tap-and-drag trace");
            assert!(options.trace_tap_drag && options.vhf);

            for invalid in [
                vec![
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--trace-tap-drag",
                ],
                vec![
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--vhf",
                    "--trace-tap-drag",
                    "--manual-session",
                ],
                vec![
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--vhf",
                    "--trace-tap-drag",
                    "--exit-after-release",
                ],
                vec![
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--vhf",
                    "--trace-tap-drag",
                    "--anchor-and-observe-cursor",
                ],
            ] {
                assert!(parse_args(args(&invalid)).is_err());
            }
        }

        #[test]
        fn release_exit_contact_gates_are_bounded_and_exclusive() {
            let options = parse_args(args(&[
                "--inject",
                "--acknowledge-desktop-input",
                "--exit-after-release-at-least=3",
            ]))
            .expect("bounded threshold");
            assert_eq!(options.exit_after_release_min_contacts, Some(3));
            assert!(options.exit_after_release_exact_contacts.is_none());

            let exact = parse_args(args(&[
                "--inject",
                "--acknowledge-desktop-input",
                "--exit-after-release-exactly=2",
            ]))
            .expect("bounded exact contact count");
            assert_eq!(exact.exit_after_release_exact_contacts, Some(2));
            assert!(exact.exit_after_release_min_contacts.is_none());

            for invalid in [
                "--exit-after-release-at-least=0",
                "--exit-after-release-at-least=6",
                "--exit-after-release-at-least=three",
                "--exit-after-release-exactly=0",
                "--exit-after-release-exactly=6",
                "--exit-after-release-exactly=three",
            ] {
                assert!(
                    parse_args(args(&["--inject", "--acknowledge-desktop-input", invalid,]))
                        .is_err()
                );
            }
            assert!(
                parse_args(args(&[
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--exit-after-release",
                    "--exit-after-release-at-least=3",
                ]))
                .is_err()
            );
            assert!(
                parse_args(args(&[
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--exit-after-release-at-least=3",
                    "--exit-after-release-exactly=3",
                ]))
                .is_err()
            );
        }

        #[test]
        fn cursor_observation_requires_vhf_one_contact_one_shot() {
            let options = parse_args(args(&[
                "--inject",
                "--acknowledge-desktop-input",
                "--vhf",
                "--exit-after-release-exactly=1",
                "--anchor-and-observe-cursor",
            ]))
            .expect("bounded VHF cursor observation");
            assert!(options.anchor_and_observe_cursor);

            for invalid in [
                vec![
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--exit-after-release",
                    "--anchor-and-observe-cursor",
                ],
                vec![
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--vhf",
                    "--exit-after-release",
                    "--anchor-and-observe-cursor",
                ],
                vec![
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--vhf",
                    "--exit-after-release-at-least=2",
                    "--anchor-and-observe-cursor",
                ],
                vec![
                    "--inject",
                    "--acknowledge-desktop-input",
                    "--vhf",
                    "--exit-after-release-exactly=1",
                    "--anchor-and-observe-cursor",
                    "--manual-session",
                ],
            ] {
                assert!(parse_args(args(&invalid)).is_err());
            }
        }

        #[test]
        fn cursor_observation_requires_meaningful_source_motion() {
            let first = SingleContactSample {
                timestamp_nanos: 100,
                x_himetric: 1_000,
                y_himetric: 2_000,
            };
            let mut metrics = LabMetrics {
                first_single_contact: Some(first),
                last_single_contact: Some(first),
                ..LabMetrics::default()
            };
            assert!(!metrics.single_contact_motion_exceeds(100));

            metrics.last_single_contact = Some(SingleContactSample {
                timestamp_nanos: 200,
                x_himetric: 1_099,
                y_himetric: 2_100,
            });
            assert!(metrics.single_contact_motion_exceeds(100));

            metrics.reset_single_contact_observation();
            assert!(!metrics.single_contact_motion_exceeds(100));
        }

        #[test]
        fn tap_drag_trace_records_two_released_one_contact_gestures() {
            use capyio_input::{
                InputFrameHeader, TouchpadButtonState, TouchpadContact, TouchpadFrameKind,
                TouchpadPosition,
            };

            fn trace_frame(
                sequence: u64,
                timestamp_nanos: u64,
                position: Option<(u32, u32)>,
            ) -> TouchpadFrame {
                TouchpadFrame {
                    header: InputFrameHeader {
                        stream_id: stream().stream_id,
                        stream_epoch: ROUTE_EPOCH,
                        sequence,
                        source_timestamp_nanos: timestamp_nanos,
                    },
                    kind: TouchpadFrameKind::Update,
                    button: TouchpadButtonState::Released,
                    contacts: position.map_or_else(Vec::new, |(x, y)| {
                        vec![TouchpadContact {
                            contact_id: sequence as u32 + 1,
                            position: TouchpadPosition {
                                x_himetric: x,
                                y_himetric: y,
                            },
                            confidence: true,
                            size: None,
                            pressure: None,
                        }]
                    }),
                }
            }

            let mut metrics = LabMetrics::default();
            metrics.record_frame(&trace_frame(0, 100_000_000, Some((1_000, 2_000))));
            metrics.record_frame(&trace_frame(1, 150_000_000, Some((1_000, 2_000))));
            metrics.record_frame(&trace_frame(2, 200_000_000, None));
            metrics.record_frame(&trace_frame(3, 240_000_000, Some((1_000, 2_000))));
            metrics.record_frame(&trace_frame(4, 260_000_000, Some((1_000, 2_000))));
            metrics.record_frame(&trace_frame(5, 300_000_000, Some((1_500, 2_100))));
            metrics.record_frame(&trace_frame(6, 400_000_000, None));

            assert_eq!(metrics.tap_drag_gesture_count, 2);
            let first = metrics.tap_drag_gestures[0].expect("first tap");
            let second = metrics.tap_drag_gestures[1].expect("second drag");
            assert_eq!(first.frames, 2);
            assert_eq!(first.released_at_nanos, 200_000_000);
            assert_eq!(first.first_motion_at_nanos, None);
            assert_eq!(second.frames, 3);
            assert_eq!(second.released_at_nanos, 400_000_000);
            assert_eq!(second.first_motion_at_nanos, Some(300_000_000));
            assert_eq!(second.last.x_himetric - second.first.x_himetric, 500);
            assert_eq!(metrics.tap_drag_completed_one_contact_gestures, 2);
            assert_eq!(metrics.tap_drag_rejected_candidates, 0);
            assert!(tap_drag_pair_qualifies(first, second));
            let mut far_second = second;
            far_second.first.x_himetric = first.first.x_himetric
                + u32::try_from(TAP_DRAG_MAX_START_POSITION_DELTA_HIMETRIC + 1)
                    .expect("positive bounded threshold");
            assert!(!tap_drag_pair_qualifies(first, far_second));
        }

        #[test]
        fn tap_drag_trace_ignores_invalid_gestures_until_a_qualified_pair() {
            use capyio_input::{
                InputFrameHeader, TouchpadButtonState, TouchpadContact, TouchpadFrameKind,
                TouchpadPosition,
            };

            fn trace_frame(
                sequence: u64,
                timestamp_nanos: u64,
                position: Option<(u32, u32)>,
            ) -> TouchpadFrame {
                TouchpadFrame {
                    header: InputFrameHeader {
                        stream_id: stream().stream_id,
                        stream_epoch: ROUTE_EPOCH,
                        sequence,
                        source_timestamp_nanos: timestamp_nanos,
                    },
                    kind: TouchpadFrameKind::Update,
                    button: TouchpadButtonState::Released,
                    contacts: position.map_or_else(Vec::new, |(x, y)| {
                        vec![TouchpadContact {
                            contact_id: sequence as u32 + 1,
                            position: TouchpadPosition {
                                x_himetric: x,
                                y_himetric: y,
                            },
                            confidence: true,
                            size: None,
                            pressure: None,
                        }]
                    }),
                }
            }

            let mut metrics = LabMetrics::default();

            // The one-contact tail of a multi-contact gesture is not a new gesture.
            let mut multi = trace_frame(100, 10_000_000, Some((400, 400)));
            multi.contacts.push(TouchpadContact {
                contact_id: 102,
                position: TouchpadPosition {
                    x_himetric: 600,
                    y_himetric: 400,
                },
                confidence: true,
                size: None,
                pressure: None,
            });
            metrics.record_frame(&multi);
            metrics.record_frame(&trace_frame(101, 20_000_000, Some((400, 400))));
            metrics.record_frame(&trace_frame(102, 30_000_000, None));
            assert_eq!(metrics.tap_drag_completed_one_contact_gestures, 0);

            // A zero-duration shell tap is not a valid first tap.
            metrics.record_frame(&trace_frame(0, 100_000_000, Some((500, 500))));
            metrics.record_frame(&trace_frame(1, 100_000_000, None));
            // A moving first contact is also not a valid first tap.
            metrics.record_frame(&trace_frame(2, 200_000_000, Some((500, 500))));
            metrics.record_frame(&trace_frame(3, 250_000_000, Some((900, 500))));
            metrics.record_frame(&trace_frame(4, 300_000_000, None));
            // This stationary gesture becomes the retained first-tap candidate.
            metrics.record_frame(&trace_frame(5, 400_000_000, Some((800, 800))));
            metrics.record_frame(&trace_frame(6, 480_000_000, Some((800, 800))));
            metrics.record_frame(&trace_frame(7, 500_000_000, None));
            // A too-late drag rejects that candidate and cannot itself be a tap.
            metrics.record_frame(&trace_frame(8, 1_100_000_001, Some((800, 800))));
            metrics.record_frame(&trace_frame(9, 1_200_000_000, Some((1_200, 800))));
            metrics.record_frame(&trace_frame(10, 1_250_000_000, None));
            assert_eq!(metrics.tap_drag_gesture_count, 0);
            assert_eq!(metrics.tap_drag_rejected_candidates, 1);

            // The following pair is within all qualification bounds.
            metrics.record_frame(&trace_frame(11, 1_400_000_000, Some((900, 900))));
            metrics.record_frame(&trace_frame(12, 1_480_000_000, Some((900, 900))));
            metrics.record_frame(&trace_frame(13, 1_500_000_000, None));
            metrics.record_frame(&trace_frame(14, 1_700_000_000, Some((900, 900))));
            metrics.record_frame(&trace_frame(15, 1_800_000_000, Some((1_200, 900))));
            metrics.record_frame(&trace_frame(16, 1_900_000_000, None));

            assert_eq!(metrics.tap_drag_gesture_count, 2);
            assert_eq!(metrics.tap_drag_completed_one_contact_gestures, 6);
            assert_eq!(metrics.tap_drag_rejected_candidates, 1);
            assert!(tap_drag_pair_qualifies(
                metrics.tap_drag_gestures[0].expect("qualified tap"),
                metrics.tap_drag_gestures[1].expect("qualified drag")
            ));
        }
    }
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    windows_lab::main()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-ptp-adb-lab is available only on Windows");
}
