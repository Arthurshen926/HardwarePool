//! Versioned, finite command boundary shared by the UI proxy and headless host.
use super::*;
use serde_json::Value;

pub(super) fn dispatch(state: &HostState, command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "get_controller_use" => encode(get_controller_use(state)?),
        "start_controller_use" => encode(start_controller_use(
            state,
            argument(args, "peerNodeId")?,
            argument(args, "mode")?,
        )?),
        "stop_controller_use" => encode(stop_controller_use(state)?),
        "get_resource_requests" => encode(get_resource_requests()?),
        "resolve_resource_request" => encode(resolve_resource_request(
            state,
            argument(args, "routeId")?,
            argument(args, "allow")?,
            argument(args, "remember")?,
        )?),
        "get_remote_controller_settings" => encode(
            controller_settings::get_remote_controller_settings(argument(args, "nodeId")?)?,
        ),
        "set_remote_controller_settings" => {
            encode(controller_settings::set_remote_controller_settings(
                argument(args, "nodeId")?,
                argument(args, "expectedRevision")?,
                argument(args, "motionEnabled")?,
                argument(args, "hapticsStrength")?,
            )?)
        }
        "get_diagnostics" => encode(diagnostics::get_diagnostics(state)),
        "get_display_sender_state" => encode(tauri::async_runtime::block_on(
            display_control::get_display_sender_state(),
        )?),
        "get_display_peers" => encode(display_workflow::get_display_peers(state)?),
        "get_display_use_state" => encode(display_workflow::get_display_use_state()?),
        "start_display_use" => encode(display_workflow::start_display_use(
            state,
            argument(args, "peerNodeId")?,
            argument(args, "width")?,
            argument(args, "height")?,
            argument(args, "refreshMillihertz")?,
            argument(args, "bitrateKbps")?,
            argument(args, "automatic")?,
        )?),
        "stop_display_use" => encode(display_workflow::stop_display_use(state)?),
        "start_display_sender" => encode(tauri::async_runtime::block_on(
            display_control::start_display_sender(),
        )?),
        "stop_display_sender" => encode(tauri::async_runtime::block_on(
            display_control::stop_display_sender(),
        )?),
        "get_snapshot" => encode(get_snapshot(state)?),
        "start_discovery" => encode(start_discovery(argument(args, "request")?, state)?),
        "get_discovery_interfaces" => encode(get_discovery_interfaces()?),
        "get_discovery_candidates" => encode(get_discovery_candidates(state)?),
        "create_pairing_invitation" => encode(create_pairing_invitation(
            argument(args, "request")?,
            state,
        )?),
        "enrollment_status" => encode(fileless_pairing::enrollment_status(state)?),
        "enrollment_action" => encode(fileless_pairing::enrollment_action(
            argument(args, "request")?,
            state,
        )?),
        "connect_discovered_peer" => {
            encode(connect_discovered_peer(argument(args, "request")?, state)?)
        }
        "connect_paired_peer_at" => {
            encode(connect_paired_peer_at(argument(args, "request")?, state)?)
        }
        "start_paired_listener" => {
            encode(start_paired_listener(argument(args, "request")?, state)?)
        }
        "stop_paired_listener" => encode(stop_paired_listener(state)?),
        "get_paired_listener_status" => encode(get_paired_listener_status(state)?),
        "get_paired_peers" => encode(get_paired_peers(state)?),
        "check_paired_credential" => {
            encode(check_paired_credential(argument(args, "request")?, state)?)
        }
        "disconnect_peer" => encode(disconnect_peer(argument(args, "request")?, state)?),
        "stop_discovery" => encode(stop_discovery(state)?),
        "set_route" => encode(set_route(argument(args, "request")?, state)?),
        "set_local_capability_shared" => encode(set_local_capability_shared(
            argument(args, "request")?,
            state,
        )?),
        "set_remote_capability_use" => encode(set_remote_capability_use(
            argument(args, "request")?,
            state,
        )?),
        "reset_demo" => encode(reset_demo(state)?),
        "get_live_imu" => encode(get_live_imu(state)?),
        "start_live_imu" => encode(start_live_imu(argument(args, "request")?, state)?),
        "stop_live_imu" => encode(stop_live_imu(state)?),
        "get_gamepad_state" => encode(get_gamepad_state(state)?),
        "refresh_windows_gamepad_preflight" => encode(tauri::async_runtime::block_on(
            refresh_windows_gamepad_preflight(argument(args, "request")?, state),
        )?),
        "update_gamepad_state" => encode(update_gamepad_state(argument(args, "request")?, state)?),
        "start_gamepad_dsu" => encode(start_gamepad_dsu(argument(args, "request")?, state)?),
        "stop_gamepad_dsu" => encode(stop_gamepad_dsu(state)?),
        "start_android_gamepad" => {
            encode(start_android_gamepad(argument(args, "request")?, state)?)
        }
        "stop_android_gamepad" => encode(stop_android_gamepad(state)?),
        "start_windows_gamepad_projection" => encode(start_windows_gamepad_projection(
            argument(args, "request")?,
            state,
        )?),
        "stop_windows_gamepad_projection" => encode(stop_windows_gamepad_projection(state)?),
        "start_controller_host" => {
            encode(start_controller_host(argument(args, "request")?, state)?)
        }
        "stop_controller_host" => encode(stop_controller_host(state)?),
        "get_quick_actions" => encode(get_quick_actions(state)?),
        "invoke_quick_action" => encode(invoke_quick_action(argument(args, "request")?, state)?),
        "get_audio_endpoints" => encode(get_audio_endpoints(state)?),
        "select_audio_endpoint" => {
            encode(select_audio_endpoint(argument(args, "request")?, state)?)
        }
        _ => Err("unknown node command".into()),
    }
}

fn argument<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))
        .map_err(|_| format!("invalid command argument: {name}"))
}
fn encode<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|_| "encode node command response".into())
}
