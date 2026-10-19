use capyio_core::NodeId;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 67, 81);
const PORT: u16 = 54786;
const MAX_BYTES: usize = 512;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentCandidate {
    pub node_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hint {
    protocol: String,
    node_id: NodeId,
    name: String,
    port: u16,
}

pub(crate) struct Announcer {
    socket: UdpSocket,
    bytes: Vec<u8>,
}
impl Announcer {
    pub(crate) fn new(
        host: Ipv4Addr,
        node_id: NodeId,
        name: String,
        port: u16,
    ) -> Result<Self, String> {
        let socket = UdpSocket::bind((host, 0)).map_err(|e| e.to_string())?;
        socket.set_multicast_ttl_v4(1).map_err(|e| e.to_string())?;
        socket
            .set_multicast_loop_v4(false)
            .map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(&Hint {
            protocol: "capyio-enrollment/3".into(),
            node_id,
            name,
            port,
        })
        .map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("enrollment discovery hint too large".into());
        }
        Ok(Self { socket, bytes })
    }
    pub(crate) fn announce(&self) -> Result<(), String> {
        self.socket
            .send_to(&self.bytes, (GROUP, PORT))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn decode(bytes: &[u8], source: SocketAddr, local: NodeId) -> Option<EnrollmentCandidate> {
    if bytes.len() > MAX_BYTES
        || !source.is_ipv4()
        || source.ip().is_unspecified()
        || source.ip().is_multicast()
    {
        return None;
    }
    let hint: Hint = serde_json::from_slice(bytes).ok()?;
    if hint.protocol != "capyio-enrollment/3"
        || hint.node_id == local
        || hint.node_id.as_uuid().is_nil()
        || hint.port == 0
        || hint.name.trim().is_empty()
        || hint.name.len() > 128
        || hint.name.chars().any(char::is_control)
    {
        return None;
    }
    Some(EnrollmentCandidate {
        node_id: hint.node_id.to_string(),
        name: hint.name,
        host: source.ip().to_string(),
        port: hint.port,
    })
}

pub(crate) fn scan(
    local: NodeId,
    interface: Ipv4Addr,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<EnrollmentCandidate>, String> {
    if interface.is_unspecified()
        || interface.is_multicast()
        || interface.is_broadcast()
        || interface.is_loopback()
    {
        return Err("请选择具体局域网 IPv4 地址".into());
    }
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, PORT)).map_err(|e| e.to_string())?;
    socket
        .join_multicast_v4(&GROUP, &interface)
        .map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut found = BTreeMap::new();
    let mut packets = 0;
    while Instant::now() < deadline && !cancelled() && packets < 256 {
        let mut bytes = [0; MAX_BYTES + 1];
        match socket.recv_from(&mut bytes) {
            Ok((len, source)) => {
                packets += 1;
                if let Some(candidate) = decode(&bytes[..len], source, local) {
                    let key = (candidate.host.clone(), candidate.port);
                    if found.len() < 16 || found.contains_key(&key) {
                        found.insert(key, candidate);
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(found.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hints_use_observed_address_and_never_contain_pin() {
        let id = NodeId::new();
        let bytes = serde_json::to_vec(&Hint {
            protocol: "capyio-enrollment/3".into(),
            node_id: id,
            name: "Tablet".into(),
            port: 43194,
        })
        .unwrap();
        let candidate = decode(&bytes, "192.0.2.1:1234".parse().unwrap(), NodeId::new()).unwrap();
        assert_eq!(candidate.host, "192.0.2.1");
        assert_eq!(candidate.port, 43194);
        assert!(decode(&bytes, "192.0.2.1:1234".parse().unwrap(), id).is_none());
        assert!(!String::from_utf8(bytes.clone()).unwrap().contains("pin"));
        assert!(decode(&vec![0; 513], "192.0.2.1:1234".parse().unwrap(), id).is_none());
        let mut invalid: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        invalid["port"] = 0.into();
        assert!(
            decode(
                &serde_json::to_vec(&invalid).unwrap(),
                "192.0.2.1:1234".parse().unwrap(),
                NodeId::new()
            )
            .is_none()
        );
    }
}
