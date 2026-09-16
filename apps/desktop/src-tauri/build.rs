fn main() {
    for name in ["CAPYIO_BUILD_REVISION", "CAPYIO_BUILD_TIME"] {
        println!("cargo:rerun-if-env-changed={name}");
        let value = std::env::var(name).unwrap_or_else(|_| "unrecorded".to_owned());
        println!("cargo:rustc-env={name}={value}");
    }
    tauri_build::build();
}
