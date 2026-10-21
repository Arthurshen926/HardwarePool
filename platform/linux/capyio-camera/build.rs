use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=native/camera_helper.c");
    println!("cargo:rerun-if-changed=native/camera_protocol.h");
    println!("cargo:rerun-if-changed=native/camera_pw.h");
    if env::var_os("CARGO_FEATURE_NATIVE_PIPEWIRE").is_none() {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_OS").unwrap(),
        "linux",
        "native-pipewire requires Linux"
    );
    let flags = Command::new("pkg-config")
        .args(["--cflags", "--libs", "libpipewire-0.3"])
        .output()
        .expect("pkg-config is required");
    assert!(
        flags.status.success(),
        "native-pipewire requires official libpipewire-0.3 and libspa development headers"
    );
    let flags = String::from_utf8(flags.stdout).unwrap();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("camera_helper.o");
    let archive = out.join("libcapyio_camera_native.a");
    let mut cc = Command::new("cc");
    cc.args([
        "-std=gnu11",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-Dmain=capyio_camera_main",
        "-c",
        "native/camera_helper.c",
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
        "native camera compilation failed"
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
    println!("cargo:rustc-link-lib=static=capyio_camera_native");
    for flag in flags.split_whitespace() {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        }
        if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        }
    }
}
