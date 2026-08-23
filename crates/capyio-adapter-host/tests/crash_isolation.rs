use std::process::Command;

use capyio_adapter_host::apply_unexpected_exit;
use capyio_core::RouteState;
use capyio_testkit::DemoLab;

#[test]
fn unexpected_exit_fails_only_adapter_owned_routes() {
    let mut lab = DemoLab::new().expect("demo lab");
    for (index, route_id) in lab.routes.all().into_iter().enumerate() {
        lab.set_route_active(route_id, true, index as u64 + 1)
            .expect("activate Route");
    }
    let local = lab.runtime.snapshot().local_node;
    let adapter = local
        .adapter_instances
        .values()
        .find(|adapter| adapter.adapter_type == "capyio.windows.audio")
        .expect("Windows audio Adapter");
    let status = exited_process();
    apply_unexpected_exit(&mut lab.runtime, local.id, adapter.id, &status).expect("apply failure");

    assert_eq!(
        lab.runtime
            .route(lab.routes.phone_microphone_to_windows)
            .expect("microphone Route")
            .state,
        RouteState::Failed
    );
    assert_eq!(
        lab.runtime
            .route(lab.routes.windows_system_mix_to_phone)
            .expect("speaker Route")
            .state,
        RouteState::Failed
    );
    assert_eq!(
        lab.runtime
            .route(lab.routes.phone_imu_to_gamepad)
            .expect("IMU Route")
            .state,
        RouteState::Active
    );
    assert_eq!(
        lab.runtime
            .route(lab.routes.phone_camera_to_panel)
            .expect("camera Route")
            .state,
        RouteState::Active
    );
}

fn exited_process() -> std::process::ExitStatus {
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "exit", "23"]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 23"]);
        command
    };
    command.status().expect("synthetic exited process")
}
