//! Read-only, bounded local IPv4 inventory. No socket or peer authority.
#[cfg(any(windows, test))]
use std::net::Ipv4Addr;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryInterface {
    pub ipv4: String,
    pub interface_index: u32,
}

#[cfg(any(windows, test))]
fn eligible(address: Ipv4Addr, index: u32, flags: u16) -> bool {
    index != 0
        && flags & (8 | 64 | 128) == 0 // disconnected, deleted, transient
        && address.octets()[0] != 0
        && address.octets()[0] < 224
        && !address.is_loopback()
}

#[cfg(windows)]
pub fn enumerate() -> Result<Vec<DiscoveryInterface>, String> {
    use std::mem::{offset_of, size_of};
    use windows_sys::Win32::{
        Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
        NetworkManagement::IpHelper::{GetIpAddrTable, MIB_IPADDRROW_XP, MIB_IPADDRTABLE},
    };

    // u32 storage matches the table/row alignment; the OS can resize between
    // calls, so retries and allocation are both independently bounded.
    let mut bytes = 16_384_u32;
    for _ in 0..3 {
        if !(size_of::<MIB_IPADDRTABLE>() as u32..=1_048_576).contains(&bytes) {
            return Err("local interface inventory exceeds its size bound".into());
        }
        let mut buffer = vec![0_u32; (bytes as usize).div_ceil(4)];
        let capacity = buffer.len() * 4;
        let table = buffer.as_mut_ptr().cast::<MIB_IPADDRTABLE>();
        // SAFETY: aligned writable allocation of at least `bytes` bytes. The
        // synchronous OS call retains no pointers and reports its result size.
        let result = unsafe { GetIpAddrTable(table, &mut bytes, 0) };
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if result != ERROR_SUCCESS {
            return Err(format!("local interface enumeration failed ({result})"));
        }
        if bytes as usize > capacity {
            return Err("invalid local interface inventory size".into());
        }
        let count = buffer[0] as usize;
        let offset = offset_of!(MIB_IPADDRTABLE, table);
        let required = count
            .checked_mul(size_of::<MIB_IPADDRROW_XP>())
            .and_then(|size| size.checked_add(offset))
            .ok_or("invalid local interface inventory count")?;
        if required > bytes as usize || count > 4096 {
            return Err("local interface inventory exceeds its row bound".into());
        }
        let mut interfaces = Vec::new();
        for index in 0..count {
            // SAFETY: every complete row is within the checked initialized OS
            // output. Reading by value does not let the allocation escape.
            let row = unsafe {
                buffer
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset + index * size_of::<MIB_IPADDRROW_XP>())
                    .cast::<MIB_IPADDRROW_XP>()
                    .read_unaligned()
            };
            let address = Ipv4Addr::from(row.dwAddr.to_ne_bytes());
            if eligible(address, row.dwIndex, row.wType) {
                interfaces.push(DiscoveryInterface {
                    ipv4: address.to_string(),
                    interface_index: row.dwIndex,
                });
            }
        }
        interfaces.sort_by(|a, b| (a.interface_index, &a.ipv4).cmp(&(b.interface_index, &b.ipv4)));
        interfaces.dedup();
        if interfaces.len() > 64 {
            return Err("too many discovery interfaces; inventory limit is 64".into());
        }
        return Ok(interfaces);
    }
    Err("local interfaces changed during enumeration; refresh and retry".into())
}

#[cfg(not(windows))]
pub fn enumerate() -> Result<Vec<DiscoveryInterface>, String> {
    Err("local interface enumeration is unavailable on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_unusable_addresses_and_interface_states() {
        for ip in [
            "0.0.0.0",
            "0.1.2.3",
            "127.0.0.1",
            "224.0.0.1",
            "240.1.1.1",
            "255.255.255.255",
        ] {
            assert!(!eligible(ip.parse().unwrap(), 1, 0));
        }
        let address = "192.168.1.2".parse().unwrap();
        assert!(!eligible(address, 0, 0));
        for flags in [8, 64, 128, 9, 68] {
            assert!(!eligible(address, 1, flags));
        }
        for ip in [
            "192.168.1.2",
            "10.0.0.2",
            "172.16.0.2",
            "169.254.1.2",
            "100.100.1.2",
        ] {
            assert!(eligible(ip.parse().unwrap(), 1, 1 | 4));
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_inventory_is_bounded_unique_and_read_only() {
        let interfaces = enumerate().expect("Windows local IP table");
        assert!(interfaces.len() <= 64);
        for item in &interfaces {
            assert!(eligible(
                item.ipv4.parse().unwrap(),
                item.interface_index,
                0
            ));
        }
        assert!(interfaces.windows(2).all(|pair| pair[0] != pair[1]));
    }
}
