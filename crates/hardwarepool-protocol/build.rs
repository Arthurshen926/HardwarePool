use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let proto_root = manifest_dir.join("../../protocol/proto");
    let protos = [
        proto_root.join("hardwarepool/v1/common.proto"),
        proto_root.join("hardwarepool/v1/capability.proto"),
        proto_root.join("hardwarepool/v1/control.proto"),
    ];

    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc path");
    // Build scripts execute before the compiler starts compiling this package. Setting the
    // process-local PROTOC variable here cannot race with application threads.
    unsafe {
        env::set_var("PROTOC", protoc);
    }

    let mut config = prost_build::Config::new();
    config
        .compile_protos(&protos, &[proto_root])
        .expect("compile protobuf schema");

    for proto in protos {
        println!("cargo:rerun-if-changed={}", proto.display());
    }
}
