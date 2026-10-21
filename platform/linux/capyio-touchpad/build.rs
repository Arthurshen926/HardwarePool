use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=native/touchpad_helper.c");
    if env::var_os("CARGO_FEATURE_NATIVE_UINPUT").is_none() {
        return;
    }
    assert_eq!(env::var("CARGO_CFG_TARGET_OS").unwrap(), "linux");
    let flags = Command::new("pkg-config")
        .args(["--cflags", "--libs", "libevdev"])
        .output()
        .expect("pkg-config is required");
    assert!(
        flags.status.success(),
        "native-uinput requires official libevdev development headers"
    );
    let flags = String::from_utf8(flags.stdout).unwrap();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("touchpad_helper.o");
    let archive = out.join("libcapyio_touchpad_native.a");
    let mut cc = Command::new("cc");
    cc.args([
        "-std=c11",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-c",
        "native/touchpad_helper.c",
        "-o",
    ])
    .arg(&object);
    for flag in flags
        .split_whitespace()
        .filter(|f| !f.starts_with("-l") && !f.starts_with("-L"))
    {
        cc.arg(flag);
    }
    assert!(
        cc.status().expect("C compiler is required").success(),
        "native touchpad compilation failed"
    );
    assert!(
        Command::new("ar")
            .arg("crs")
            .arg(&archive)
            .arg(&object)
            .status()
            .expect("ar is required")
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=capyio_touchpad_native");
    for flag in flags.split_whitespace() {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        }
        if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        }
    }
}
