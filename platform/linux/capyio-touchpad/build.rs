use std::{env, path::PathBuf, process::Command};
fn archive(out: &std::path::Path, name: &str, defines: &[&str], flags: &[&str]) {
    let object = out.join(format!("{name}.o"));
    let library = out.join(format!("lib{name}.a"));
    let mut cc = Command::new("cc");
    cc.args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c"])
        .args(defines)
        .args(flags)
        .arg("native/touchpad_helper.c")
        .arg("-o")
        .arg(&object);
    assert!(
        cc.status().expect("C compiler is required").success(),
        "touchpad helper compilation failed"
    );
    assert!(
        Command::new("ar")
            .arg("crs")
            .arg(library)
            .arg(object)
            .status()
            .expect("ar is required")
            .success()
    );
}
fn main() {
    println!("cargo:rerun-if-changed=native/touchpad_helper.c");
    let native = env::var_os("CARGO_FEATURE_NATIVE_UINPUT").is_some();
    let fixture = env::var_os("CARGO_FEATURE_TEST_FIXTURE").is_some();
    if !native && !fixture {
        return;
    }
    assert_eq!(env::var("CARGO_CFG_TARGET_OS").unwrap(), "linux");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rustc-link-search=native={}", out.display());
    if fixture {
        // A distinct entrypoint/archive prevents feature unification from
        // accidentally resolving the fake helper to the production backend.
        archive(
            &out,
            "capyio_touchpad_fixture",
            &[
                "-DCAPYIO_FAKE_UINPUT",
                "-DCAPYIO_FAKE_LIBRARY",
                "-Dcapyio_touchpad_main=capyio_touchpad_fixture_main",
            ],
            &[],
        );
    }
    if !native {
        return;
    }
    let flags = Command::new("pkg-config")
        .args(["--cflags", "--libs", "libevdev"])
        .output()
        .expect("pkg-config is required");
    assert!(
        flags.status.success(),
        "native-uinput requires official libevdev development headers"
    );
    let flags = String::from_utf8(flags.stdout).unwrap();
    let compile_flags: Vec<_> = flags
        .split_whitespace()
        .filter(|f| !f.starts_with("-l") && !f.starts_with("-L"))
        .collect();
    archive(&out, "capyio_touchpad_native", &[], &compile_flags);
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
