#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;

    use capyio_display_engine_host::DisplayEngineLibrary;

    let mut arguments = std::env::args_os().skip(1);
    let path = arguments.next().ok_or("missing absolute DLL path")?;
    let hash = arguments.next().ok_or("missing lowercase SHA-256")?;
    if arguments.next().is_some() {
        return Err("unexpected capability-probe argument".into());
    }
    let expected = decode_sha256(hash.to_str().ok_or("SHA-256 must be UTF-8")?)?;
    let mut library = DisplayEngineLibrary::load_verified(Path::new(&path), expected)?;
    println!(
        "verified_path={} features={} max_sessions={}",
        library.canonical_path().display(),
        library.probe().enabled_features(),
        library.probe().max_concurrent_sessions()
    );
    library.try_unload()?;
    Ok(())
}

#[cfg(windows)]
fn decode_sha256(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("SHA-256 must contain exactly 64 hexadecimal characters".into());
    }
    let mut decoded = [0_u8; 32];
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(decoded)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-engine-capability-probe is Windows-only");
    std::process::exit(2);
}
