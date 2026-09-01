use std::{env, process::ExitCode, thread, time::Duration};

use capyio_input::InputStreamDescriptor;
use capyio_windows_input::{
    SyntheticTouchpadGesture, SyntheticTouchpadSession, SyntheticTouchpadSessionError,
    TouchpadInjectionDryRun, build_touchpad_injection_fixture,
};

const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunMode {
    DryRun,
    Inject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Options {
    gesture: SyntheticTouchpadGesture,
    mode: RunMode,
    acknowledged: bool,
}

enum ParsedCommand {
    Help,
    Run(Options),
}

fn main() -> ExitCode {
    let command = match parse_args(env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", usage());
            return ExitCode::from(64);
        }
    };
    let ParsedCommand::Run(options) = command else {
        println!("{}", usage());
        return ExitCode::SUCCESS;
    };

    let stream = InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c602"
            .parse()
            .expect("fixed stream ID"),
        stream_epoch: 1,
        clock_domain_id: "windows.local.fixture".to_owned(),
    };
    let fixture = build_touchpad_injection_fixture(options.gesture, stream);
    let metrics = match fixture.dry_run() {
        Ok(metrics) => metrics,
        Err(error) => {
            eprintln!("fixture_projection=failed");
            eprintln!("fixture_projection_detail={error}");
            return ExitCode::FAILURE;
        }
    };
    print_summary(options, metrics);

    if options.mode == RunMode::DryRun {
        println!("device_creation=not_requested");
        println!("input_injected=false");
        return ExitCode::SUCCESS;
    }

    match inject_fixture(&fixture) {
        Ok(submitted) => {
            println!("device_creation=created_and_destroyed");
            println!("submitted_batches={}", submitted.batches);
            println!("submitted_contact_records={}", submitted.contacts);
            println!("input_injected={}", submitted.batches > 0);
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("injection_status=failed");
            println!("injection_detail={error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> &'static str {
    "Usage: capyio-ptp-inject --gesture <one-finger-tap|one-finger-double-tap-drag|one-finger-motion|two-finger-pan|three-finger-swipe|four-finger-swipe> [--dry-run | --inject --acknowledge-desktop-input]\nDefault: dry-run. Injection accepts only fixed one-shot fixtures and can click, drag, move the pointer, scroll content, or switch UI state."
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<ParsedCommand, String> {
    let mut arguments = arguments.into_iter();
    let mut gesture = None;
    let mut selected_mode = None;
    let mut acknowledged = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" | "-h" => return Ok(ParsedCommand::Help),
            "--gesture" => {
                if gesture.is_some() {
                    return Err("--gesture may be specified only once".to_owned());
                }
                let value = arguments
                    .next()
                    .ok_or_else(|| "--gesture requires a value".to_owned())?;
                gesture = Some(value.parse::<SyntheticTouchpadGesture>()?);
            }
            "--dry-run" => select_mode(&mut selected_mode, RunMode::DryRun)?,
            "--inject" => select_mode(&mut selected_mode, RunMode::Inject)?,
            "--acknowledge-desktop-input" => acknowledged = true,
            _ => return Err(format!("unsupported argument: {argument}")),
        }
    }
    let gesture = gesture.ok_or_else(|| "--gesture is required".to_owned())?;
    let mode = selected_mode.unwrap_or(RunMode::DryRun);
    if mode == RunMode::Inject && !acknowledged {
        return Err("--inject requires the separate --acknowledge-desktop-input flag".to_owned());
    }
    if mode == RunMode::DryRun && acknowledged {
        return Err("desktop-input acknowledgement is valid only with --inject".to_owned());
    }
    Ok(ParsedCommand::Run(Options {
        gesture,
        mode,
        acknowledged,
    }))
}

fn select_mode(selected: &mut Option<RunMode>, mode: RunMode) -> Result<(), String> {
    if selected.replace(mode).is_some() {
        return Err("select exactly one of --dry-run or --inject".to_owned());
    }
    Ok(())
}

fn print_summary(options: Options, metrics: TouchpadInjectionDryRun) {
    println!("schema_version={SCHEMA_VERSION}");
    println!("gesture={}", options.gesture);
    println!(
        "mode={}",
        match options.mode {
            RunMode::DryRun => "dry_run",
            RunMode::Inject => "inject",
        }
    );
    println!("desktop_input_acknowledged={}", options.acknowledged);
    println!("frames_projected={}", metrics.frames_projected);
    println!("batches_encoded={}", metrics.batches_encoded);
    println!(
        "contact_records_encoded={}",
        metrics.contact_records_encoded
    );
    println!("peak_batch_contacts={}", metrics.peak_batch_contacts);
    println!("peak_batches_per_frame={}", metrics.peak_batches_per_frame);
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SubmissionMetrics {
    batches: u64,
    contacts: u64,
}

#[derive(Debug)]
enum HarnessError {
    Session(SyntheticTouchpadSessionError),
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Session(error) => write!(formatter, "session failed: {error}"),
        }
    }
}

impl From<SyntheticTouchpadSessionError> for HarnessError {
    fn from(error: SyntheticTouchpadSessionError) -> Self {
        Self::Session(error)
    }
}

fn inject_fixture(
    fixture: &capyio_windows_input::TouchpadInjectionFixture,
) -> Result<SubmissionMetrics, HarnessError> {
    let first_sequence = fixture
        .frames
        .first()
        .map_or(0, |frame| frame.header.sequence);
    let mut session =
        SyntheticTouchpadSession::open(&fixture.stream, fixture.descriptor, first_sequence)?;
    let mut metrics = SubmissionMetrics::default();
    for (index, frame) in fixture.frames.iter().enumerate() {
        let submission = session.submit_frame(frame)?;
        metrics.batches += u64::from(submission.batches_submitted);
        metrics.contacts += u64::from(submission.contact_records_submitted);
        if index + 1 < fixture.frames.len() {
            thread::sleep(Duration::from_millis(fixture.interval_millis));
        }
    }
    let cleanup = session.close()?;
    metrics.batches += u64::from(cleanup.batches_submitted);
    metrics.contacts += u64::from(cleanup.contact_records_submitted);
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn dry_run_is_default_and_requires_one_closed_gesture() {
        let ParsedCommand::Run(options) =
            parse_args(strings(&["--gesture", "one-finger-motion"])).expect("parse")
        else {
            panic!("expected run command");
        };
        assert_eq!(options.mode, RunMode::DryRun);
        assert!(!options.acknowledged);
        assert_eq!(options.gesture, SyntheticTouchpadGesture::OneFingerMotion);
        assert!(parse_args(Vec::<String>::new()).is_err());
        assert!(parse_args(strings(&["--gesture", "tap"])).is_err());
    }

    #[test]
    fn injection_requires_separate_acknowledgement() {
        assert!(parse_args(strings(&["--gesture", "two-finger-pan", "--inject"])).is_err());
        let ParsedCommand::Run(options) = parse_args(strings(&[
            "--gesture",
            "two-finger-pan",
            "--inject",
            "--acknowledge-desktop-input",
        ]))
        .expect("double gate") else {
            panic!("expected run command");
        };
        assert_eq!(options.mode, RunMode::Inject);
        assert!(options.acknowledged);
    }

    #[test]
    fn conflicting_or_irrelevant_safety_flags_are_rejected() {
        assert!(
            parse_args(strings(&[
                "--gesture",
                "three-finger-swipe",
                "--dry-run",
                "--inject",
                "--acknowledge-desktop-input",
            ]))
            .is_err()
        );
        assert!(
            parse_args(strings(&[
                "--gesture",
                "four-finger-swipe",
                "--acknowledge-desktop-input",
            ]))
            .is_err()
        );
    }
}
