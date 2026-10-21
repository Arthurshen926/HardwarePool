//! Terminal controller only. Pairing and persistence remain in the sibling Node.
#[cfg(target_os = "linux")]
mod controller;
#[cfg(target_os = "linux")]
mod invitation;
#[cfg(target_os = "linux")]
mod terminal;

fn main() {
    #[cfg(target_os = "linux")]
    let result = controller::run();
    #[cfg(not(target_os = "linux"))]
    let result: std::io::Result<()> = Err(std::io::Error::other("unsupported platform"));
    if result.is_err() {
        eprintln!(
            "Linux pairing console failed; no automatic retry. Check explicit selectors, paired-store build and interactive terminal. Local persistence may require inspection."
        );
        std::process::exit(1);
    }
}
