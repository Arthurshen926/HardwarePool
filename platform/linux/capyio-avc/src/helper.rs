// Native code is linked only into this explicitly opted-in subprocess.
#[link(name = "capyio_avc_native", kind = "static")]
#[link(name = "avcodec")]
#[link(name = "avutil")]
unsafe extern "C" {
    fn capyio_avc_main(lifetime_ms: u32) -> std::ffi::c_int;
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let lifetime = if args.len() == 3 && args[1] == "--lifetime-ms" {
        args[2]
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=60_000).contains(n))
    } else {
        None
    };
    let Some(lifetime_ms) = lifetime else {
        eprintln!("AVC helper: expected --lifetime-ms 1..60000");
        std::process::exit(2);
    };
    // SAFETY: a bounded integer and no borrowed memory cross this process-local ABI.
    std::process::exit(unsafe { capyio_avc_main(lifetime_ms) });
}
