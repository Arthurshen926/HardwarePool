//! Connection-owned Route lifecycle dispatch. Adapter coordinators retain their
//! readiness and data-plane contracts; Runtime remains the authorization authority.
use super::*;
use capyio_protocol::{
    ValidatedRouteStatus,
    v1::{DisplaySessionControl, DisplayStreamSelection},
};

type Receivers = Arc<Mutex<android_receivers::AndroidReceiverActions>>;

pub(super) struct RouteUseCoordinator<'a> {
    node: &'a Arc<Mutex<DesktopNodeState>>,
    dial: &'a ResolvedControlDial,
    receivers: &'a Receivers,
    selected: &'a Arc<Mutex<BTreeSet<CapabilityId>>>,
    gamepad: &'a Arc<Mutex<GamepadLab>>,
    controller: controller_control::Coordinator,
    input: input_routes::InputRoutes<'a>,
    audio: native_audio_route::Coordinator,
    camera: camera_route::Coordinator,
    display: display_workflow::Coordinator,
}

impl<'a> RouteUseCoordinator<'a> {
    pub fn new(
        node: &'a Arc<Mutex<DesktopNodeState>>,
        dial: &'a ResolvedControlDial,
        receivers: &'a Receivers,
        selected: &'a Arc<Mutex<BTreeSet<CapabilityId>>>,
        gamepad: &'a Arc<Mutex<GamepadLab>>,
    ) -> Self {
        Self {
            node,
            dial,
            receivers,
            selected,
            gamepad,
            controller: Default::default(),
            input: input_routes::InputRoutes::new(node, dial, gamepad, selected),
            audio: Default::default(),
            camera: Default::default(),
            display: Default::default(),
        }
    }

    pub fn tick_input(&mut self, link: &mut CatalogControlLink) -> Result<(), String> {
        self.input.tick(link)
    }

    fn tick_controller(&mut self, link: &mut CatalogControlLink) {
        if let Err(error) = self
            .controller
            .tick(link, self.node, self.gamepad, self.dial)
        {
            let _ = self.controller.stop(link, self.node, self.gamepad);
            if let Ok(mut lab) = self.gamepad.lock() {
                lab.workflow.fail(error);
            }
        }
    }

    fn request_controller_feedback(&mut self, peer: NodeId) -> bool {
        if self.controller.feedback_requested
            || !self.gamepad.lock().is_ok_and(|lab| {
                lab.workflow.peer() == Some(peer)
                    && lab.workflow.snapshot().is_some_and(|s| {
                        s.status == "active" && s.mode != controller_workflow::ControllerMode::Dsu
                    })
            })
        {
            return false;
        }
        self.controller.feedback_requested = true;
        true
    }

    pub fn begin_controller(
        &mut self,
        link: &mut CatalogControlLink,
        mode: controller_workflow::ControllerMode,
    ) -> Result<(), String> {
        let result = self.controller.begin(link, self.node, self.gamepad, mode);
        if let Err(error) = &result {
            let _ = self.controller.stop(link, self.node, self.gamepad);
            if let Ok(mut lab) = self.gamepad.lock() {
                lab.workflow.fail(error.clone());
            }
        }
        result
    }

    pub fn stop_controller(&mut self, link: &mut CatalogControlLink) -> Result<(), String> {
        self.controller.stop(link, self.node, self.gamepad)
    }

    pub fn observations(&self) -> Vec<route_readiness::RouteMediaSnapshot> {
        self.audio
            .observations()
            .chain(self.camera.observation())
            .chain(self.display.observation())
            .take(32)
            .collect()
    }

    pub fn tick(&mut self, link: &mut CatalogControlLink) {
        self.audio.tick(link, self.node, self.dial);
        self.camera.tick(link, self.node, self.receivers, self.dial);
        self.display.tick(link, self.node, self.dial);
        self.input.cleanup_feedback(link);
        self.tick_controller(link);
        if self.request_controller_feedback(link.peer_node_id()) {
            self.input.begin_feedback(link);
        }
    }

    pub fn begin_capability(
        &mut self,
        link: &mut CatalogControlLink,
        cap: CapabilityId,
    ) -> Result<(), String> {
        if self.audio.handles(self.node, link.peer_node_id(), cap) {
            self.audio
                .begin(link, self.node, self.dial, self.selected, cap)
        } else if self.camera.handles(self.node, link.peer_node_id(), cap) {
            self.camera.begin(link, self.node, self.dial, cap)
        } else {
            self.input.begin(link, cap)
        }
    }

    pub fn stop_capability(
        &mut self,
        link: &mut CatalogControlLink,
        cap: CapabilityId,
    ) -> Result<(), String> {
        if self.audio.stop_capability(link, self.node, cap)?
            || self
                .camera
                .stop_capability(link, self.node, self.receivers, cap)?
        {
            return Ok(());
        }
        self.input.stop(link, cap)
    }

    pub fn begin_display(
        &mut self,
        link: &mut CatalogControlLink,
        selection: DisplayStreamSelection,
        automatic: bool,
    ) -> Result<(), String> {
        self.display
            .begin(link, self.node, self.dial, selection, automatic, None)
    }

    pub fn stop_display(&mut self, link: &mut CatalogControlLink) -> Result<(), String> {
        self.display.stop(link, self.node, true)
    }

    pub fn resolve(&mut self, route: RouteId, allow: bool, remember: bool) -> Result<(), String> {
        if self.display.owns(route) {
            self.display
                .resolve(self.node, self.dial, route, allow, remember)
        } else if self.camera.owns(route) {
            self.camera
                .resolve(self.node, self.dial, route, allow, remember)
        } else if self.audio.owns(route) {
            self.audio
                .resolve(self.node, self.dial, route, allow, remember)
        } else {
            self.controller
                .resolve(self.node, self.gamepad, self.dial, route, allow, remember)
        }
    }

    pub fn receive_status(
        &mut self,
        link: &mut CatalogControlLink,
        status: &ValidatedRouteStatus,
    ) -> Result<bool, String> {
        match self.display.receive_status(link, self.node, status) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(error) => {
                self.display.fail(link, self.node, error);
                return Ok(true);
            }
        }
        if self
            .camera
            .receive(link, self.node, self.receivers, self.dial, status)?
        {
            return Ok(true);
        }
        if self.audio.receive(link, self.node, self.dial, status)? {
            return Ok(true);
        }
        self.controller
            .receive(link, self.node, self.gamepad, self.dial, status)
    }

    pub fn remote_stop(&mut self, link: &mut CatalogControlLink, route: RouteId) {
        if self.camera.owns(route) {
            let _ = self.camera.stop(link, self.node, self.receivers, false);
        }
        if self.audio.owns(route) {
            let _ = self.audio.stop_id(link, self.node, route, false);
        }
        if self.controller.owns(route) {
            let _ = self.controller.local_stop(self.node, self.gamepad);
        }
    }

    pub fn receive_display(
        &mut self,
        link: &mut CatalogControlLink,
        message: DisplaySessionControl,
    ) {
        if let Err(error) = self.display.receive(link, self.node, self.dial, message) {
            self.display.fail(link, self.node, error);
        }
    }

    pub fn shutdown(&mut self, link: &mut CatalogControlLink, outcome: &Result<(), String>) {
        self.audio.shutdown(link, self.node);
        self.display.shutdown(link, self.node, self.dial);
        let _ = self.camera.stop(link, self.node, self.receivers, true);
        let interrupted = self.gamepad.lock().is_ok_and(|lab| {
            lab.workflow.peer() == Some(link.peer_node_id()) && lab.workflow.route().is_some()
        });
        let _ = self.controller.local_stop(self.node, self.gamepad);
        if interrupted
            && let Err(error) = outcome
            && let Ok(mut lab) = self.gamepad.lock()
        {
            lab.workflow.fail(format!("控制连接中断：{error}"));
        }
    }
}
