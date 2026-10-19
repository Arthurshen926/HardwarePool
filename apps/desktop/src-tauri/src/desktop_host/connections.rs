//! Outbound authenticated connections owned by the process host.
use super::*;

impl DesktopHost {
    pub fn resolve_resource_request(
        &self,
        route_id: String,
        allow: bool,
        remember: bool,
    ) -> Result<(), String> {
        let route = RouteId::from_str(&route_id).map_err(|e| e.to_string())?;
        let request = controller_control::pending()?
            .into_iter()
            .find(|r| r.route_id == route)
            .ok_or("资源请求已结束")?;
        let (reply, receive) = mpsc::channel();
        {
            let links = self
                .control_links
                .lock()
                .map_err(|_| "control links lock poisoned")?;
            let link = links.get(&request.peer_node_id).ok_or("节点已断开")?;
            link.commands
                .try_send(ControlLinkCommand::ResolveController {
                    route,
                    allow,
                    remember,
                    reply,
                })
                .map_err(|_| "控制连接正忙，请稍后重试")?;
        }
        receive
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| "资源授权请求超时")?
    }

    pub fn get_paired_peers(&self) -> Result<Vec<UiPairedPeer>, String> {
        let mut control = self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned")?;
        control.refresh_pairing_directory()?;
        Ok(paired_peer_metadata(&control))
    }

    pub fn check_paired_credential(&self, request: PeerControlRequest) -> Result<(), String> {
        let peer_id = NodeId::from_str(&request.node_id)
            .map_err(|_| "peer Node ID must be a canonical UUID".to_owned())?;
        let (store, handle) = {
            let mut control = self
                .control_plane
                .lock()
                .map_err(|_| "control-plane state lock poisoned".to_owned())?;
            control.refresh_pairing_directory()?;
            let peer = control
                .paired_peers
                .peers()
                .find(|peer| peer.node_id == peer_id && matches!(peer.state, PairingState::Active))
                .ok_or("selected peer is not actively paired")?;
            (
                control
                    .pairing_store
                    .clone()
                    .ok_or("pairing store is unavailable")?,
                peer.credential_handle.clone(),
            )
        };
        verify_credential_readable(&store, &handle)
    }

    pub fn get_discovery_candidates(&self) -> Result<Vec<UiDiscoveryCandidate>, String> {
        let mut control = self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned")?;
        let now_ms = u64::try_from(control.clock.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(discovery_candidates(&mut control, now_ms))
    }

    pub fn get_paired_listener_status(&self) -> Result<Option<UiPairedListenerStatus>, String> {
        let listener = self
            .paired_listener
            .lock()
            .map_err(|_| "paired listener owner lock poisoned".to_owned())?;
        Ok(listener.as_ref().map(|current| UiPairedListenerStatus {
            node_id: current.peer_id.to_string(),
            bind_host: current.local_addr.ip().to_string(),
            port: current.local_addr.port(),
        }))
    }

    pub fn start_paired_listener(&self, request: PairedListenerRequest) -> Result<u16, String> {
        let peer_id = NodeId::from_str(&request.node_id)
            .map_err(|_| "peer Node ID must be a canonical UUID".to_owned())?;
        let host = IpAddr::from_str(&request.bind_host)
            .map_err(|_| "listen host must be a numeric local IP".to_owned())?;
        if request.port == 0 || host.is_unspecified() || host.is_multicast() {
            return Err("listen endpoint is invalid".to_owned());
        }
        let mut controller = self
            .paired_listener
            .lock()
            .map_err(|_| "paired listener owner lock poisoned".to_owned())?;
        if let Some(current) = controller.as_ref() {
            if current.peer_id == peer_id
                && current.local_addr == SocketAddr::new(host, request.port)
            {
                return Ok(current.local_addr.port());
            }
            return Err(
                "stop the existing paired listener before changing its endpoint".to_owned(),
            );
        }
        if self
            .control_links
            .lock()
            .map_err(|_| "control-link owner lock poisoned".to_owned())?
            .get(&peer_id)
            .is_some_and(ControlLinkController::is_active)
        {
            return Err(
                "disconnect the existing peer link before starting its listener".to_owned(),
            );
        }
        let local_id = self
            .node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?
            .runtime
            .snapshot()
            .local_node
            .id;
        let (store, fingerprint, handle) = {
            let mut control = self
                .control_plane
                .lock()
                .map_err(|_| "control-plane state lock poisoned".to_owned())?;
            control.refresh_pairing_directory()?;
            let peer = control
                .paired_peers
                .peers()
                .find(|peer| peer.node_id == peer_id && matches!(peer.state, PairingState::Active))
                .ok_or("peer is not actively paired")?;
            (
                control
                    .pairing_store
                    .clone()
                    .ok_or("pairing store is unavailable")?,
                peer.peer_certificate_sha256,
                peer.credential_handle.clone(),
            )
        };
        let credential = store
            .load_control_credential(&handle)
            .map_err(|error| format!("paired control credential unavailable: {error}"))?;
        let identity = ControlTlsServerIdentity::from_bundle(fingerprint, credential);
        let expected =
            ControlTlsNodeBinding::new(local_id, peer_id).map_err(|error| error.to_string())?;
        let socket = TcpListener::bind(SocketAddr::new(host, request.port))
            .map_err(|error| format!("paired control bind failed: {error}"))?;
        socket
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let server = AuthenticatedControlServer::from_listener_fresh_session(
            socket,
            identity,
            expected,
            Duration::from_secs(5),
        )
        .map_err(|error| error.to_string())?;
        let local_addr = server.local_addr().map_err(|error| error.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let context = PairedListenerContext {
            gamepad_lab: Arc::clone(&self.gamepad_lab),
            android_receivers: Arc::clone(&self.android_receivers),
            node: Arc::clone(&self.node),
            control_plane: Arc::clone(&self.control_plane),
            links: Arc::clone(&self.control_links),
            selected: Arc::clone(&self.local_shared_capabilities),
        };
        let worker = thread::Builder::new()
            .name("capyio-paired-listener".to_owned())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match server.accept() {
                        Ok(channel) => {
                            let _ = attach_accepted_control_channel(channel, peer_id, &context);
                        }
                        Err(ControlTlsError::Io(error))
                            if error.kind() == ErrorKind::WouldBlock =>
                        {
                            thread::sleep(Duration::from_millis(25));
                        }
                        Err(_) => thread::sleep(Duration::from_millis(25)),
                    }
                }
            })
            .map_err(|error| format!("paired listener worker start failed: {error}"))?;
        *controller = Some(PairedListenerController {
            stop,
            worker,
            peer_id,
            local_addr,
        });
        Ok(local_addr.port())
    }

    pub fn create_pairing_invitation(
        &self,
        request: PairingInvitationRequest,
    ) -> Result<String, String> {
        let target_node_id = NodeId::from_str(&request.node_id)
            .map_err(|_| "target Node ID must be a canonical UUID".to_owned())?;
        if request.renew_existing {
            if self
                .control_links
                .lock()
                .map_err(|_| "control-link lock poisoned")?
                .get(&target_node_id)
                .is_some_and(ControlLinkController::is_active)
            {
                return Err("更新配对前请先断开该设备的现有控制连接".to_owned());
            }
            if self
                .paired_listener
                .lock()
                .map_err(|_| "listener lock poisoned")?
                .as_ref()
                .is_some_and(|listener| listener.peer_id == target_node_id)
            {
                return Err("更新配对前请先停止该设备的入站监听".to_owned());
            }
        }
        let (local_node_id, local_display_name) = {
            let node = self
                .node
                .lock()
                .map_err(|_| "Node state lock poisoned".to_owned())?;
            let snapshot = node.runtime.snapshot();
            (
                snapshot.local_node.id,
                snapshot.local_node.display_name.clone(),
            )
        };
        let mut control = self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned".to_owned())?;
        pairing_enrollment::create_invitation(
            &mut control,
            local_node_id,
            &local_display_name,
            target_node_id,
            &request.display_name,
            request.renew_existing,
        )
        .map(|path| path.to_string_lossy().into_owned())
    }

    fn reconcile_control_link_failure(&self, link: &mut CatalogControlLink) {
        if let Ok(mut node) = self.node.lock() {
            let _ = link.close_and_reconcile(&mut node.runtime);
        } else {
            link.close();
        }
    }

    pub fn connect_discovered_peer_blocking(
        &self,
        request: PeerControlRequest,
    ) -> Result<(), String> {
        let peer_id = NodeId::from_str(&request.node_id)
            .map_err(|_| "peer Node ID must be a canonical UUID".to_owned())?;
        if peer_id.as_uuid().is_nil() {
            return Err("peer Node ID must not be nil".to_owned());
        }
        let plan = {
            let mut control = self
                .control_plane
                .lock()
                .map_err(|_| "control-plane state lock poisoned".to_owned())?;
            let now_ms = u64::try_from(control.clock.elapsed().as_millis()).unwrap_or(u64::MAX);
            control.resolve_control_dial(peer_id, now_ms)?
        };
        self.connect_control_plan(peer_id, plan)
    }

    pub fn connect_paired_peer_at_blocking(
        &self,
        request: ManualPeerControlRequest,
    ) -> Result<(), String> {
        let peer_id = NodeId::from_str(&request.node_id)
            .map_err(|_| "peer Node ID must be a canonical UUID".to_owned())?;
        let host = IpAddr::from_str(&request.host)
            .map_err(|_| "manual control host must be a numeric IP address".to_owned())?;
        if request.port == 0 || host.is_unspecified() || host.is_multicast() {
            return Err("manual control endpoint is invalid".to_owned());
        }
        let plan = self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned".to_owned())?
            .resolve_manual_control_dial(peer_id, SocketAddr::new(host, request.port))?;
        self.connect_control_plan(peer_id, plan)
    }

    fn connect_control_plan(
        &self,
        peer_id: NodeId,
        plan: ResolvedControlDial,
    ) -> Result<(), String> {
        // This lock also arbitrates listener startup while an outbound handshake
        // is in flight. A Node may use either direction, but not race both for the
        // same one-per-peer live control slot.
        let _listener_arbiter = self
            .paired_listener
            .lock()
            .map_err(|_| "paired listener owner lock poisoned".to_owned())?;
        if _listener_arbiter
            .as_ref()
            .is_some_and(|listener| listener.peer_id == peer_id)
        {
            return Err("stop the paired listener before dialing this peer".to_owned());
        }
        let mut links = self
            .control_links
            .lock()
            .map_err(|_| "control-link owner lock poisoned".to_owned())?;
        if let Some(existing) = links.get(&peer_id) {
            if existing.is_active() {
                return Err("peer already has an authenticated control connection".to_owned());
            }
            let stale = links
                .remove(&peer_id)
                .expect("observed control-link controller must remain present");
            let _ = stale.stop_and_join();
        }
        let local_node_id = self
            .node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?
            .runtime
            .snapshot()
            .local_node
            .id;
        let channel = connect_authenticated_control_channel(peer_id, local_node_id, &plan)?;
        // Authenticate the remote endpoint before probing the local VHF-backed
        // projection. Besides minimizing failed-connection side effects, this keeps
        // platform driver discovery outside the TLS credential/signature path.
        {
            let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
            touchpad_data::install_catalog(&mut node.runtime)?;
            controller_workflow::install_catalog(&mut node.runtime)?;
            native_audio_route::install_catalog(&mut node.runtime)?;
            camera_route::install_catalog(&mut node.runtime)?;
            display_workflow::install_catalog(&mut node.runtime)?;
        }
        let selected = self
            .local_shared_capabilities
            .lock()
            .map_err(|_| "local Capability selection lock poisoned".to_owned())?
            .clone();
        let (mut link, prepared) = {
            let mut node = self
                .node
                .lock()
                .map_err(|_| "Node state lock poisoned".to_owned())?;
            let mut link = CatalogControlLink::admit_dialed_peer(
                &mut node.runtime,
                channel,
                plan.display_name.clone(),
            )
            .map_err(|error| format!("authenticated Session admission failed: {error}"))?;
            let prepared = match link.prepare_catalog(&node.runtime, &selected) {
                Ok(prepared) => prepared,
                Err(error) => {
                    let _ = link.close_and_reconcile(&mut node.runtime);
                    return Err(format!("local catalog preparation failed: {error}"));
                }
            };
            (link, prepared)
        };
        if let Err(error) = link.send_catalog(prepared) {
            self.reconcile_control_link_failure(&mut link);
            return Err(format!("local catalog send failed: {error}"));
        }
        let received = match link.receive_catalog() {
            Ok(received) => received,
            Err(error) => {
                self.reconcile_control_link_failure(&mut link);
                return Err(format!("peer catalog receive failed: {error}"));
            }
        };
        let apply_result = {
            let mut node = self
                .node
                .lock()
                .map_err(|_| "Node state lock poisoned".to_owned())?;
            link.apply_catalog(&mut node.runtime, received)
        };
        if let Err(error) = apply_result {
            self.reconcile_control_link_failure(&mut link);
            return Err(format!("peer catalog admission failed: {error}"));
        }
        let controller = ControlLinkController::spawn(
            link,
            Arc::clone(&self.node),
            Arc::clone(&self.control_plane),
            Arc::clone(&self.local_shared_capabilities),
            plan,
            Arc::clone(&self.gamepad_lab),
            Arc::clone(&self.android_receivers),
        )?;
        links.insert(peer_id, controller);
        let connection_count = links
            .values()
            .filter(|controller| controller.is_active())
            .count();
        drop(links);
        let mut control = self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned".to_owned())?;
        control.authenticated_connection_count = connection_count;
        control.catalog_exchange_active = connection_count > 0;
        Ok(())
    }
}

fn attach_accepted_control_channel(
    channel: AuthenticatedControlServerChannel,
    peer_id: NodeId,
    context: &PairedListenerContext,
) -> Result<(), String> {
    let endpoint = channel.peer_addr().map_err(|error| error.to_string())?;
    let mut links = context
        .links
        .lock()
        .map_err(|_| "control-link owner lock poisoned".to_owned())?;
    if let Some(existing) = links.get(&peer_id) {
        if existing.is_active() {
            return Err("peer already connected".to_owned());
        }
        let old = links
            .remove(&peer_id)
            .ok_or("stale control link disappeared")?;
        let _ = old.stop_and_join();
    }
    let plan = context
        .control_plane
        .lock()
        .map_err(|_| "control-plane state lock poisoned".to_owned())?
        .resolve_manual_control_dial(peer_id, endpoint)?;
    let mut link = {
        let mut node = context
            .node
            .lock()
            .map_err(|_| "Node state lock poisoned".to_owned())?;
        CatalogControlLink::admit_accepted_peer(
            &mut node.runtime,
            channel,
            plan.display_name.clone(),
        )
        .map_err(|error| error.to_string())?
    };
    let exchanged = (|| -> Result<(), String> {
        let catalog = link.receive_catalog().map_err(|error| error.to_string())?;
        let selected = context
            .selected
            .lock()
            .map_err(|_| "local Capability selection lock poisoned".to_owned())?
            .clone();
        let prepared = {
            let mut node = context
                .node
                .lock()
                .map_err(|_| "Node state lock poisoned".to_owned())?;
            touchpad_data::install_catalog(&mut node.runtime)?;
            controller_workflow::install_catalog(&mut node.runtime)?;
            native_audio_route::install_catalog(&mut node.runtime)?;
            camera_route::install_catalog(&mut node.runtime)?;
            display_workflow::install_catalog(&mut node.runtime)?;
            link.apply_catalog(&mut node.runtime, catalog)
                .map_err(|error| error.to_string())?;
            link.prepare_catalog(&node.runtime, &selected)
                .map_err(|error| error.to_string())?
        };
        link.send_catalog(prepared)
            .map_err(|error| error.to_string())
    })();
    if let Err(error) = exchanged {
        if let Ok(mut node) = context.node.lock() {
            let _ = link.close_and_reconcile(&mut node.runtime);
        } else {
            link.close();
        }
        return Err(error);
    }
    let controller = ControlLinkController::spawn(
        link,
        Arc::clone(&context.node),
        Arc::clone(&context.control_plane),
        Arc::clone(&context.selected),
        plan,
        Arc::clone(&context.gamepad_lab),
        Arc::clone(&context.android_receivers),
    )?;
    links.insert(peer_id, controller);
    let count = links
        .values()
        .filter(|controller| controller.is_active())
        .count();
    drop(links);
    let mut control = context
        .control_plane
        .lock()
        .map_err(|_| "control-plane state lock poisoned".to_owned())?;
    control.authenticated_connection_count = count;
    control.catalog_exchange_active = count > 0;
    Ok(())
}
