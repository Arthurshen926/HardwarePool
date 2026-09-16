#[cfg(windows)]
fn main() {
    use capyio_windows_capture_ring::CaptureRingObserver;

    match CaptureRingObserver::attach() {
        Ok(observer) => {
            let metrics = observer.metrics();
            println!(
                "CAPYIO_CAPTURE_RING_METRICS produced_frames={} consumed_frames={} dropped_frames={} underrun_frames={} producer_attaches={} consumer_attaches={} last_stage={} last_error={}",
                metrics.produced_frames,
                metrics.consumed_frames,
                metrics.dropped_frames,
                metrics.underrun_frames,
                metrics.producer_attaches,
                metrics.consumer_attaches,
                metrics.last_stage,
                metrics.last_error,
            );
        }
        Err(error) => {
            eprintln!("CAPYIO_CAPTURE_RING_PROBE_FAILED {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("CAPYIO_CAPTURE_RING_PROBE_FAILED Windows only");
    std::process::exit(1);
}
