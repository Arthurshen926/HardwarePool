//! Existing approved Windows/Y700 pair for the unencrypted native media adapters.
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

const WINDOWS: Ipv4Addr = Ipv4Addr::new(100, 66, 231, 100);
const Y700: Ipv4Addr = Ipv4Addr::new(100, 110, 59, 35);

pub(super) fn resolve(
    endpoint: SocketAddr,
    outside_lab: &str,
) -> Result<(Ipv4Addr, Ipv4Addr), String> {
    let probe = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    probe.connect(endpoint).map_err(|e| e.to_string())?;
    let local = probe.local_addr().map_err(|e| e.to_string())?.ip();
    match (local, endpoint.ip()) {
        (IpAddr::V4(local), IpAddr::V4(peer)) if local == WINDOWS && peer == Y700 => {
            Ok((local, peer))
        }
        _ => Err(outside_lab.into()),
    }
}
