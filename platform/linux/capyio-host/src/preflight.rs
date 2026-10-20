//! Passive prerequisite hints, never authorization or proof of playback readiness.
//! No executable is launched and no PipeWire connection is made. Filesystem
//! metadata calls have bounded input/counts, not a wall-clock deadline (a mounted
//! filesystem can stall). Environment values and filesystem errors are not output.

use std::{
    ffi::OsStr,
    fs, io,
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, PermissionsExt},
    },
    path::Path,
};

use capyio_linux_audio::PlaybackConfig;
use serde::Serialize;

const MAX_COMPONENT_BYTES: usize = 4096;

/// Schema-versioned observations only. Even all positive hints cannot establish
/// executable trust, a live PipeWire session, an available target, or consent.
#[derive(Debug, Serialize)]
pub struct PlaybackPreflightReport {
    pub version: u32,
    pub readiness: &'static str,
    pub inspection: &'static str,
    pub pw_cat: &'static str,
    pub tool_versions: &'static str,
    pub default_pipewire_socket: &'static str,
    pub pipewire_environment_overrides_present: bool,
    pub session: &'static str,
    pub target: &'static str,
    pub required_inputs: &'static [&'static str],
    pub limitations: &'static [&'static str],
}

/// Inspect the fixed `/usr/bin/pw-cat` used by playback and default runtime metadata. This does not read stdin, secret
/// files, tool contents, device data or PipeWire objects. `target` is syntax
/// checked with the playback parser but is never echoed or resolved.
pub fn inspect_playback_prerequisites(target: Option<&str>) -> PlaybackPreflightReport {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR");
    let overrides = ["PIPEWIRE_RUNTIME_DIR", "PIPEWIRE_REMOTE"]
        .iter()
        .any(|key| std::env::var_os(key).is_some());
    inspect(runtime.as_deref(), overrides, target)
}

fn inspect(
    runtime: Option<&OsStr>,
    overrides: bool,
    target: Option<&str>,
) -> PlaybackPreflightReport {
    PlaybackPreflightReport {
        version: 1,
        readiness: "not_verified",
        inspection: "passive_metadata_only",
        pw_cat: executable_hint(fs::metadata("/usr/bin/pw-cat")),
        tool_versions: "not_executed_or_verified",
        default_pipewire_socket: socket_hint(runtime),
        pipewire_environment_overrides_present: overrides,
        session: "not_connected_or_verified",
        target: target_hint(target),
        required_inputs: &[
            "trusted_launcher_with_exclusive_private_stdin_stdout_pipes",
            "local_node_id_and_exact_remote_source_port_ref",
            "explicit_private_ipv4_control_and_media_endpoints",
            "explicit_pipewire_target_and_duration_1_to_300_seconds",
            "already_approved_paired_peer_directory_and_matching_issued_credentials",
            "authenticated_available_source_catalog_and_fresh_exact_route_approval",
        ],
        limitations: &[
            "metadata_candidates_do_not_establish_executable_trust_or_version",
            "default_socket_metadata_does_not_prove_a_live_or_accessible_session",
            "environment_overrides_may_select_a_different_pipewire_session",
            "target_existence_permissions_and_audio_delivery_are_unverified",
            "lab_udp_media_is_unauthenticated_and_unencrypted",
            "no_credential_provisioning_deployment_or_device_permission_is_granted",
            "filesystem_metadata_may_block_despite_bounded_inspection_count",
        ],
    }
}

fn target_hint(target: Option<&str>) -> &'static str {
    let Some(target) = target else {
        return "not_supplied";
    };
    if target.len() > 128 {
        return "invalid_syntax";
    }
    let args = ["--trusted-lab", "127.0.0.1:1", "127.0.0.1:2", "1", target].map(str::to_owned);
    if PlaybackConfig::parse(&args).is_ok() {
        "syntax_valid_existence_unverified"
    } else {
        "invalid_syntax"
    }
}

fn executable_hint(metadata: io::Result<fs::Metadata>) -> &'static str {
    match metadata {
        Ok(value) if value.is_file() && value.permissions().mode() & 0o111 != 0 => {
            "executable_file_candidate_unverified"
        }
        Ok(_) => "not_an_executable_file",
        Err(error) if error.kind() == io::ErrorKind::NotFound => "not_found",
        Err(_) => "metadata_unavailable",
    }
}

fn socket_hint(runtime: Option<&OsStr>) -> &'static str {
    let Some(runtime) = runtime else {
        return "runtime_directory_unset";
    };
    let directory = Path::new(runtime);
    if !directory.is_absolute() || runtime.as_bytes().len() > MAX_COMPONENT_BYTES {
        return "runtime_directory_not_inspected";
    }
    // Do not follow a socket symlink or read any file content. This only checks
    // the conventional default, not a session selected by environment overrides.
    match fs::symlink_metadata(directory.join("pipewire-0")) {
        Ok(value) if value.file_type().is_socket() => "socket_metadata_present_unverified",
        Ok(_) => "not_a_socket",
        Err(error) if error.kind() == io::ErrorKind::NotFound => "not_found",
        Err(_) => "metadata_unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_missing_environment_is_structured_and_never_ready() {
        let report = inspect(None, false, None);
        assert_eq!(report.default_pipewire_socket, "runtime_directory_unset");
        assert_eq!(report.target, "not_supplied");
        assert_eq!(report.readiness, "not_verified");
        assert_eq!(report.session, "not_connected_or_verified");
        assert!(serde_json::to_vec(&report).unwrap().len() < 4096);
    }

    #[test]
    fn preflight_target_uses_existing_playback_policy_without_echo() {
        for value in [
            "",
            "auto",
            "0",
            "-1",
            "--help",
            "secret/marker",
            "bad\nvalue",
        ] {
            assert_eq!(target_hint(Some(value)), "invalid_syntax");
        }
        assert_eq!(
            target_hint(Some("alsa_output.card:1")),
            "syntax_valid_existence_unverified"
        );
        let report = inspect(
            Some(OsStr::new("SECRET_RUNTIME_MARKER")),
            true,
            Some("SECRET_TARGET_MARKER"),
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("SECRET_"));
        assert!(report.pipewire_environment_overrides_present);
    }

    #[test]
    fn preflight_absent_and_denied_tool_metadata_are_distinct() {
        assert_eq!(
            executable_hint(Err(io::Error::from(io::ErrorKind::NotFound))),
            "not_found"
        );
        assert_eq!(
            executable_hint(Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "SECRET_PATH_MARKER"
            ))),
            "metadata_unavailable"
        );
    }

    #[test]
    fn preflight_runtime_directory_input_is_bounded() {
        assert_eq!(
            socket_hint(Some(OsStr::new("relative"))),
            "runtime_directory_not_inspected"
        );
        let long = format!("/{}", "x".repeat(MAX_COMPONENT_BYTES));
        assert_eq!(
            socket_hint(Some(OsStr::new(&long))),
            "runtime_directory_not_inspected"
        );
    }

    #[test]
    fn preflight_regular_file_is_not_a_pipewire_session() {
        let dir = std::env::temp_dir().join(format!(
            "capyio-preflight-{}-{}",
            std::process::id(),
            capyio_core::NodeId::new()
        ));
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("pipewire-0"), b"not a socket").unwrap();
        assert_eq!(socket_hint(Some(dir.as_os_str())), "not_a_socket");
        fs::remove_file(dir.join("pipewire-0")).unwrap();
        assert_eq!(socket_hint(Some(dir.as_os_str())), "not_found");
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn preflight_executable_metadata_is_only_an_unverified_candidate() {
        let executable = fs::metadata(std::env::current_exe().unwrap()).unwrap();
        assert_eq!(
            executable_hint(Ok(executable)),
            "executable_file_candidate_unverified"
        );
    }
}
