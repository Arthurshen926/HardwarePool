//! Display command dispatch on the process host.
use super::super::display_workflow::{DisplayPeer, format, get_display_use_state};
use super::*;
use capyio_protocol::v1::{DisplayStreamMode, DisplayStreamSelection};

impl DesktopHost {
    pub fn get_display_peers(&self) -> Result<Vec<DisplayPeer>, String> {
        let links = self
            .control_links
            .lock()
            .map_err(|_| "control links lock poisoned")?;
        let node = self.node.lock().map_err(|_| "node lock poisoned")?;
        Ok(node
            .runtime
            .snapshot()
            .peers
            .iter()
            .filter(|p| {
                links
                    .get(&p.id)
                    .is_some_and(|l| l.active.load(Ordering::Acquire))
            })
            .filter(|p| {
                p.capabilities.values().any(|c| {
                    c.class == CapabilityClass::Display
                        && c.ports.values().any(|p| {
                            p.direction == PortDirection::Sink && p.formats.contains(&format())
                        })
                })
            })
            .map(|p| DisplayPeer {
                node_id: p.id.to_string(),
                display_name: p.display_name.clone(),
            })
            .collect())
    }

    pub fn start_display_use(
        &self,
        peer_node_id: String,
        width: u32,
        height: u32,
        refresh_millihertz: u32,
        bitrate_kbps: u32,
        automatic: Option<bool>,
    ) -> Result<(), String> {
        let peer = NodeId::from_str(&peer_node_id).map_err(|e| e.to_string())?;
        let automatic = automatic.unwrap_or(false);
        let selection = if automatic {
            DisplayStreamSelection {
                mode: Some(DisplayStreamMode {
                    width: 1920,
                    height: 1080,
                    refresh_millihertz: 60_000,
                }),
                bitrate_kbps: 15_000,
            }
        } else {
            DisplayStreamSelection {
                mode: Some(DisplayStreamMode {
                    width,
                    height,
                    refresh_millihertz,
                }),
                bitrate_kbps,
            }
        };
        let (reply, receive) = mpsc::channel();
        self.control_links
            .lock()
            .map_err(|_| "control links lock poisoned")?
            .get(&peer)
            .filter(|l| l.active.load(Ordering::Acquire))
            .ok_or("请先连接屏幕接收节点")?
            .commands
            .try_send(ControlLinkCommand::StartDisplay {
                selection,
                automatic,
                reply,
            })
            .map_err(|_| "控制连接正忙")?;
        receive
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| "屏幕启动请求超时")?
    }

    pub fn stop_display_use(&self) -> Result<(), String> {
        let Some(current) = get_display_use_state()? else {
            return Ok(());
        };
        let peer = NodeId::from_str(&current.peer_node_id).map_err(|e| e.to_string())?;
        let (reply, receive) = mpsc::channel();
        self.control_links
            .lock()
            .map_err(|_| "control links lock poisoned")?
            .get(&peer)
            .ok_or("节点已断开，后台正在清理共享")?
            .commands
            .try_send(ControlLinkCommand::StopDisplay { reply })
            .map_err(|_| "控制连接正忙")?;
        receive
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| "屏幕停止请求超时")?
    }
}
