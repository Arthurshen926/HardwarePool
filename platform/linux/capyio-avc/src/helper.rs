// Native code is linked only into this explicitly opted-in subprocess.
#[link(name = "capyio_avc_native", kind = "static")]
#[link(name = "avcodec")]
#[link(name = "avutil")]
unsafe extern "C" {
    fn capyio_avc_main(lifetime_ms: u32, profile: u32) -> std::ffi::c_int;
}
fn parse_args(args: &[String]) -> Option<(u32, u32)> {
    if !matches!(args.len(), 3 | 5) || args[1] != "--lifetime-ms" {
        return None;
    }
    let lifetime = args[2]
        .parse::<u32>()
        .ok()
        .filter(|n| (1..=60_000).contains(n))?;
    let profile = if args.len() == 3 {
        0 // Historical invocation remains the fixed 120-frame profile.
    } else if args[3] == "--profile" {
        match args[4].as_str() {
            "legacy" => 0,
            "cavc-avc-finite-720p30-v1" => 1,
            _ => return None,
        }
    } else {
        return None;
    };
    Some((lifetime, profile))
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let Some((lifetime_ms, profile)) = parse_args(&args) else {
        eprintln!(
            "AVC helper: expected --lifetime-ms 1..60000 [--profile legacy|cavc-avc-finite-720p30-v1]"
        );
        std::process::exit(2);
    };
    // SAFETY: only bounded integers cross this process-local ABI. C independently
    // validates both, derives its own work caps and owns its monotonic clocks.
    std::process::exit(unsafe { capyio_avc_main(lifetime_ms, profile) });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Option<(u32, u32)> {
        parse_args(&args.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>())
    }
    #[test]
    fn closed_profiles_and_lifetime_bounds() {
        assert_eq!(
            parse(&["helper", "--lifetime-ms", "60000"]),
            Some((60000, 0))
        );
        assert_eq!(
            parse(&["helper", "--lifetime-ms", "1", "--profile", "legacy"]),
            Some((1, 0))
        );
        assert_eq!(
            parse(&[
                "helper",
                "--lifetime-ms",
                "1",
                "--profile",
                "cavc-avc-finite-720p30-v1"
            ]),
            Some((1, 1))
        );
        for args in [
            vec!["helper"],
            vec!["helper", "--lifetime-ms", "0"],
            vec!["helper", "--lifetime-ms", "60001"],
            vec!["helper", "--lifetime-ms", "4294967296"],
            vec!["helper", "--lifetime-ms", "1", "--profile", "finite"],
            vec!["helper", "--lifetime-ms", "1", "--max-frames", "1800"],
        ] {
            assert_eq!(parse(&args), None);
        }
    }
}
