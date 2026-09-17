#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-broker-probe is only available on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use capyio_windows_display::IddFrameBrokerMapping;

    let timeout = std::env::args()
        .nth(1)
        .map(|value| value.parse::<u64>())
        .transpose()
        .unwrap_or_else(|error| fail(&format!("invalid timeout seconds: {error}")))
        .unwrap_or(30);
    if !(1..=300).contains(&timeout) {
        fail("timeout seconds must be in 1..=300");
    }
    let expected_width = parse_optional_dimension(2, "expected width");
    let expected_height = parse_optional_dimension(3, "expected height");
    if expected_width.is_some() != expected_height.is_some() {
        fail("expected width and height must be supplied together");
    }

    let mut mapping = IddFrameBrokerMapping::create()
        .unwrap_or_else(|error| fail(&format!("create Broker mapping: {error}")));
    println!(
        "READY timeout_seconds={timeout} expected={}x{}",
        expected_width
            .map(|value| value.to_string())
            .unwrap_or_else(|| "any".to_owned()),
        expected_height
            .map(|value| value.to_string())
            .unwrap_or_else(|| "any".to_owned()),
    );
    let deadline = Instant::now() + Duration::from_secs(timeout);
    let mut last_observed_dimensions = None;
    while Instant::now() < deadline {
        match mapping.try_read() {
            Ok(Some(frame)) => {
                if let (Some(width), Some(height)) = (expected_width, expected_height) {
                    let dimensions = (frame.header.width, frame.header.height);
                    if dimensions != (width, height) {
                        if last_observed_dimensions != Some(dimensions) {
                            println!("OBSERVED width={} height={}", dimensions.0, dimensions.1);
                            last_observed_dimensions = Some(dimensions);
                        }
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                }
                println!(
                    "FRAME generation={} sequence={} width={} height={} stride={} payload_bytes={} timestamp_nanos={}",
                    frame.header.generation,
                    frame.header.sequence_lock,
                    frame.header.width,
                    frame.header.height,
                    frame.header.stride_bytes,
                    frame.payload.len(),
                    frame.header.source_timestamp_nanos,
                );
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(error) => fail(&format!("read Broker mapping: {error}")),
        }
    }
    fail("timed out before the IDD published a frame");
}

#[cfg(windows)]
fn parse_optional_dimension(index: usize, label: &str) -> Option<u32> {
    std::env::args().nth(index).map(|value| {
        value
            .parse::<u32>()
            .unwrap_or_else(|error| fail(&format!("invalid {label}: {error}")))
    })
}

#[cfg(windows)]
fn fail(message: &str) -> ! {
    eprintln!("ERROR {message}");
    std::process::exit(1);
}
