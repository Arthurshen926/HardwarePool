use capyio_mock_sidecar_support::{MockKind, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(
        MockKind::Source,
        std::env::args().any(|arg| arg == "--crash-on-probe"),
    )
}
