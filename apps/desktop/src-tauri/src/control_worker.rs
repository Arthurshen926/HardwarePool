//! Authenticated connection worker: command scheduling, control messages and Route cleanup.
//! UI commands reach this worker through DesktopHost; no window owns its lifetime.
use super::*;

impl ControlLinkController {
    pub(super) fn spawn(
        mut link: CatalogControlLink,
        node: Arc<Mutex<DesktopNodeState>>,
        control_plane: Arc<Mutex<DesktopControlPlaneState>>,
        local_shared_capabilities: Arc<Mutex<BTreeSet<CapabilityId>>>,
        dial: ResolvedControlDial,
        gamepad_lab: Arc<Mutex<GamepadLab>>,
        android_receivers: Arc<Mutex<android_receivers::AndroidReceiverActions>>,
    ) -> Result<Self, String> {
        let active = Arc::new(AtomicBool::new(true));
        let worker_active = Arc::clone(&active);
        let (commands, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("capyio-control-link".to_owned())
            .spawn(move || {
                let mut sequence = 1_u64;
                let mut outcome = Ok(());
                let mut advertised_controller = None;
                let mut controller_endpoint_announced = false;
                let mut routes = route_use_coordinator::RouteUseCoordinator::new(
                    &node,
                    &dial,
                    &android_receivers,
                    &local_shared_capabilities,
                    &gamepad_lab,
                );
                let settings_node = link.peer_node_id().to_string();
                let settings_session = link.session_id().to_string();
                controller_settings::connected(&settings_node, &settings_session);
                loop {
                    routes.tick(&mut link);
                    let endpoint_result = (|| -> Result<(), String> {
                        if let Some(request) =
                            controller_settings::outgoing(&settings_node, &settings_session)
                        {
                            link.send_controller_settings_request(request)
                                .map_err(|error| error.to_string())?;
                        }
                        let controller_endpoint = {
                            let lab = gamepad_lab
                                .lock()
                                .map_err(|_| "gamepad state lock poisoned")?;
                            if lab
                                .workflow
                                .peer()
                                .is_none_or(|peer| peer == link.peer_node_id())
                            {
                                lab.controller_receiver_endpoint()
                            } else {
                                None
                            }
                        };
                        if !controller_endpoint_announced
                            || controller_endpoint != advertised_controller
                        {
                            let socket = std::net::UdpSocket::bind("0.0.0.0:0")
                                .map_err(|error| error.to_string())?;
                            socket
                                .connect(dial.candidate.endpoint)
                                .map_err(|error| error.to_string())?;
                            let host = socket
                                .local_addr()
                                .map_err(|error| error.to_string())?
                                .ip()
                                .to_string();
                            if !controller_endpoint_announced
                                && let (Ok(bind), std::net::IpAddr::V4(peer)) =
                                    (host.parse(), dial.candidate.endpoint.ip())
                            {
                                android_receivers
                                    .lock()
                                    .map_err(|_| "Android receiver lock poisoned")?
                                    .configure_camera_peer(bind, peer)?;
                            }
                            let offer = if let Some((port, token)) = &controller_endpoint {
                                capyio_protocol::v1::ControllerReceiverOffer {
                                    host,
                                    port: u32::from(*port),
                                    token: token.clone(),
                                }
                            } else {
                                capyio_protocol::v1::ControllerReceiverOffer {
                                    host,
                                    port: 0,
                                    token: String::new(),
                                }
                            };
                            link.send_controller_receiver_offer(offer)
                                .map_err(|error| error.to_string())?;
                            advertised_controller = controller_endpoint;
                            controller_endpoint_announced = true;
                        }
                        Ok(())
                    })();
                    if let Err(error) = endpoint_result {
                        outcome = Err(error);
                        break;
                    }
                    if let Err(error) = routes.tick_input(&mut link) {
                        outcome = Err(error);
                        break;
                    }
                    match receiver.recv_timeout(Duration::from_millis(500)) {
                        Ok(ControlLinkCommand::ObserveMedia { reply }) => {
                            let _ = reply.send(routes.observations());
                            continue;
                        }

                        Ok(ControlLinkCommand::StartDisplay {
                            selection,
                            automatic,
                            reply,
                        }) => {
                            let result = (|| -> Result<(), String> {
                                // This explicit local Start publishes only this additionally
                                // selected resource for this connection; it does not overwrite
                                // the user's persisted publication selection.
                                let mut selected = local_shared_capabilities
                                    .lock()
                                    .map_err(|_| "publication lock poisoned")?
                                    .clone();
                                selected.insert(display_workflow::source().0);
                                let prepared = link
                                    .prepare_catalog(
                                        &node.lock().map_err(|_| "node lock poisoned")?.runtime,
                                        &selected,
                                    )
                                    .map_err(|e| e.to_string())?;
                                link.send_catalog(prepared).map_err(|e| e.to_string())?;
                                routes.begin_display(&mut link, selection, automatic)
                            })();
                            let _ = reply.send(result);
                            continue;
                        }
                        Ok(ControlLinkCommand::StopDisplay { reply }) => {
                            let result = routes.stop_display(&mut link);
                            let _ = reply.send(result);
                            continue;
                        }
                        Ok(ControlLinkCommand::StartController { mode, reply }) => {
                            let result = routes.begin_controller(&mut link, mode);
                            let _ = reply.send(result);
                            continue;
                        }
                        Ok(ControlLinkCommand::StopController { reply }) => {
                            let result = routes.stop_controller(&mut link);
                            let _ = reply.send(result);
                            continue;
                        }
                        Ok(ControlLinkCommand::ResolveController {
                            route,
                            allow,
                            remember,
                            reply,
                        }) => {
                            let result = routes.resolve(route, allow, remember);
                            let _ = reply.send(result);
                            continue;
                        }
                        Ok(ControlLinkCommand::Publish) => {
                            let selected = match local_shared_capabilities.lock() {
                                Ok(selected) => selected.clone(),
                                Err(_) => {
                                    outcome =
                                        Err("local Capability selection lock poisoned".to_owned());
                                    break;
                                }
                            };
                            let prepared = match node.lock() {
                                Ok(node) => link.prepare_catalog(&node.runtime, &selected),
                                Err(_) => {
                                    outcome = Err("Node state lock poisoned".to_owned());
                                    break;
                                }
                            };
                            if let Err(error) =
                                prepared.and_then(|catalog| link.send_catalog(catalog))
                            {
                                outcome = Err(format!("catalog publication failed: {error}"));
                                break;
                            }
                            continue;
                        }
                        Ok(ControlLinkCommand::StartRemoteCapability {
                            capability_id,
                            reply,
                        }) => {
                            let result = routes.begin_capability(&mut link, capability_id);
                            let _ = reply.send(result);
                            continue;
                        }
                        Ok(ControlLinkCommand::StopRemoteCapability {
                            capability_id,
                            reply,
                        }) => {
                            let result = routes.stop_capability(&mut link, capability_id);
                            let _ = reply.send(result);
                            continue;
                        }
                        Ok(ControlLinkCommand::Stop)
                        | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if let Err(error) = link.send_heartbeat(sequence, false) {
                        outcome = Err(format!("control heartbeat send failed: {error}"));
                        break;
                    }
                    if let Ok(mut node) = node.lock() {
                        node.runtime
                            .expire_port_leases(unix_now_ms().unwrap_or(u64::MAX));
                    }
                    loop {
                        match link.receive_control_message() {
                            Ok(ReceivedControlMessage::Heartbeat(heartbeat))
                                if heartbeat.acknowledgement && heartbeat.sequence == sequence =>
                            {
                                break;
                            }
                            Ok(ReceivedControlMessage::Heartbeat(heartbeat))
                                if !heartbeat.acknowledgement =>
                            {
                                if let Err(error) = link.send_heartbeat(heartbeat.sequence, true) {
                                    outcome =
                                        Err(format!("control heartbeat reply failed: {error}"));
                                    break;
                                }
                            }
                            Ok(ReceivedControlMessage::Catalog(catalog)) => {
                                let applied = node
                                    .lock()
                                    .map_err(|_| "Node state lock poisoned".to_owned())
                                    .and_then(|mut node| {
                                        link.apply_catalog(&mut node.runtime, catalog)
                                            .map(|_| ())
                                            .map_err(|error| error.to_string())
                                    });
                                if let Err(error) = applied {
                                    outcome = Err(format!("peer catalog update failed: {error}"));
                                    break;
                                }
                            }
                            Ok(ReceivedControlMessage::Heartbeat(_)) => {
                                outcome =
                                    Err("peer returned a mismatched control heartbeat".to_owned());
                                break;
                            }
                            Ok(ReceivedControlMessage::PortLeaseRequest(request)) => {
                                let answered = node
                                    .lock()
                                    .map_err(|_| "Node state lock poisoned".to_owned())
                                    .and_then(|mut node| {
                                        link.answer_port_lease_request(
                                            &mut node.runtime,
                                            request,
                                            unix_now_ms()?,
                                        )
                                        .map_err(|error| error.to_string())
                                    });
                                if let Err(error) = answered {
                                    outcome = Err(format!("Port lease response failed: {error}"));
                                    break;
                                }
                            }
                            Ok(ReceivedControlMessage::RouteStatus(status)) => {
                                match routes.receive_status(&mut link, &status) {
                                    Ok(true) => continue,
                                    Ok(false) => {}
                                    Err(error) => {
                                        outcome = Err(error);
                                        break;
                                    }
                                }
                                let applied = node
                                    .lock()
                                    .map_err(|_| "Node state lock poisoned".to_owned())
                                    .and_then(|mut node| {
                                        link.apply_route_status(&mut node.runtime, &status, true)
                                            .map(|_| ())
                                            .map_err(|error| error.to_string())
                                    });
                                if let Err(error) = applied {
                                    outcome = Err(format!("peer Route status failed: {error}"));
                                    break;
                                }
                            }
                            Ok(ReceivedControlMessage::RouteCommand(command)) => {
                                if command.action == RouteCommandAction::Stop {
                                    routes.remote_stop(&mut link, command.route_id);
                                }
                                let route = node
                                    .lock()
                                    .map_err(|_| "Node state lock poisoned".to_owned())
                                    .and_then(|mut node| {
                                        link.apply_route_command(
                                            &mut node.runtime,
                                            &command,
                                            unix_now_ms()?,
                                        )
                                        .map_err(|error| error.to_string())
                                    });
                                match route {
                                    Ok(route) => {
                                        if let Err(error) = link.send_route_status(&route) {
                                            outcome =
                                                Err(format!("Route status send failed: {error}"));
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        outcome =
                                            Err(format!("peer Route command failed: {error}"));
                                        break;
                                    }
                                }
                            }
                            Ok(ReceivedControlMessage::PortLeaseResponse(_)) => {
                                outcome =
                                    Err("peer returned an unsolicited Port lease response"
                                        .to_owned());
                                break;
                            }
                            Ok(ReceivedControlMessage::ControllerReceiverOffer(_)) => {
                                // This host has no Android controller Source to configure.
                            }
                            Ok(ReceivedControlMessage::DisplaySessionControl(value)) => {
                                routes.receive_display(&mut link, value);
                            }
                            Ok(ReceivedControlMessage::ControllerSettingsState(value)) => {
                                controller_settings::received(
                                    &settings_node,
                                    &settings_session,
                                    value,
                                );
                            }
                            Ok(ReceivedControlMessage::ControllerSettingsRequest(_)) => {
                                // This node has no Android controller preferences to own.
                            }
                            Err(error) => {
                                outcome = Err(format!("control heartbeat receive failed: {error}"));
                                break;
                            }
                        }
                    }
                    if outcome.is_err() {
                        break;
                    }
                    sequence = sequence.checked_add(1).unwrap_or(1);
                }
                controller_settings::disconnected(&settings_node, &settings_session);
                routes.shutdown(&mut link, &outcome);
                let cleanup = node
                    .lock()
                    .map_err(|_| "Node state lock poisoned".to_owned())
                    .and_then(|mut node| {
                        link.close_and_reconcile(&mut node.runtime)
                            .map_err(|error| error.to_string())
                    });
                if outcome.is_ok() {
                    outcome = cleanup;
                }
                worker_active.store(false, Ordering::Release);
                if let Ok(mut control) = control_plane.lock() {
                    control.authenticated_connection_count =
                        control.authenticated_connection_count.saturating_sub(1);
                    control.catalog_exchange_active = control.authenticated_connection_count > 0;
                }
                outcome
            })
            .map_err(|error| format!("control-link worker could not start: {error}"))?;
        Ok(Self {
            active,
            commands,
            worker: Some(worker),
        })
    }

    pub(super) fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub(super) fn publish_latest(&self) {
        match self.commands.try_send(ControlLinkCommand::Publish) {
            Ok(()) | Err(mpsc::TrySendError::Full(ControlLinkCommand::Publish)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) | Err(mpsc::TrySendError::Full(_)) => {}
        }
    }

    pub(super) fn set_remote_capability_use(
        &self,
        capability_id: CapabilityId,
        active: bool,
    ) -> Result<(), String> {
        let (reply, result) = mpsc::channel();
        let command = if active {
            ControlLinkCommand::StartRemoteCapability {
                capability_id,
                reply,
            }
        } else {
            ControlLinkCommand::StopRemoteCapability {
                capability_id,
                reply,
            }
        };
        self.commands
            .send(command)
            .map_err(|_| "authenticated control connection is closed".to_owned())?;
        result
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| "remote Route request timed out".to_owned())?
    }

    pub(super) fn stop_and_join(mut self) -> Result<(), String> {
        let _ = self.commands.send(ControlLinkCommand::Stop);
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| "control-link worker panicked".to_owned())?,
            None => Ok(()),
        }
    }
}
