#![deny(unsafe_op_in_unsafe_fn)]

use std::net::SocketAddr;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpPeerPresence {
    UnsupportedPlatform,
    Disconnected,
    Established { connection_count: usize },
}

pub fn process_owned_tcp_peer_presence(
    process_id: u32,
    bind_address: SocketAddr,
) -> Result<TcpPeerPresence, ProcessPresenceError> {
    platform::process_owned_tcp_peer_presence(process_id, bind_address)
}

#[derive(Debug, Error)]
pub enum ProcessPresenceError {
    #[error("Windows TCP owner table query failed with code {code}")]
    TableQueryFailed { code: u32 },
    #[error("Windows TCP owner table exceeded the {limit} byte safety bound")]
    TableTooLarge { limit: usize },
    #[error("Windows TCP owner table returned an invalid bounded layout")]
    InvalidTableLayout,
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) fn process_owned_tcp_peer_presence(
        _process_id: u32,
        _bind_address: SocketAddr,
    ) -> Result<TcpPeerPresence, ProcessPresenceError> {
        Ok(TcpPeerPresence::UnsupportedPlatform)
    }
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, mem, net::IpAddr, ptr, slice};

    use windows_sys::Win32::{
        NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCP_STATE_ESTAB, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
            TCP_TABLE_OWNER_PID_ALL,
        },
        Networking::WinSock::AF_INET,
    };

    use super::*;

    const NO_ERROR: u32 = 0;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    const MAX_TCP_TABLE_BYTES: usize = 16 * 1024 * 1024;

    pub(super) fn process_owned_tcp_peer_presence(
        process_id: u32,
        bind_address: SocketAddr,
    ) -> Result<TcpPeerPresence, ProcessPresenceError> {
        if !matches!(bind_address.ip(), IpAddr::V4(_)) {
            return Ok(TcpPeerPresence::UnsupportedPlatform);
        }

        let mut required_bytes = 0_u32;
        // SAFETY: A null first buffer is the documented size-query form. The
        // size pointer is valid and all enum/family values are fixed constants.
        let first = unsafe {
            GetExtendedTcpTable(
                ptr::null_mut(),
                &mut required_bytes,
                0,
                AF_INET.into(),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if first != ERROR_INSUFFICIENT_BUFFER && first != NO_ERROR {
            return Err(ProcessPresenceError::TableQueryFailed { code: first });
        }
        let required = required_bytes as usize;
        if required > MAX_TCP_TABLE_BYTES {
            return Err(ProcessPresenceError::TableTooLarge {
                limit: MAX_TCP_TABLE_BYTES,
            });
        }
        if required < mem::size_of::<u32>() {
            return Err(ProcessPresenceError::InvalidTableLayout);
        }

        let word_size = mem::size_of::<usize>();
        let word_count = required.div_ceil(word_size);
        let mut storage = vec![0_usize; word_count];
        let mut actual_bytes = (storage.len() * word_size) as u32;
        // SAFETY: `storage` is sufficiently aligned, lives through the call and
        // has `actual_bytes` writable bytes. The API initializes the documented
        // owner-PID table layout for AF_INET.
        let result = unsafe {
            GetExtendedTcpTable(
                storage.as_mut_ptr().cast::<c_void>(),
                &mut actual_bytes,
                0,
                AF_INET.into(),
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if result != NO_ERROR {
            return Err(ProcessPresenceError::TableQueryFailed { code: result });
        }

        let table = storage.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
        // SAFETY: The successful API call initialized at least the table header.
        let count = unsafe { (*table).dwNumEntries as usize };
        let header_bytes = mem::size_of::<u32>();
        let rows_bytes = count
            .checked_mul(mem::size_of::<MIB_TCPROW_OWNER_PID>())
            .and_then(|bytes| header_bytes.checked_add(bytes))
            .ok_or(ProcessPresenceError::InvalidTableLayout)?;
        if rows_bytes > actual_bytes as usize || rows_bytes > storage.len() * word_size {
            return Err(ProcessPresenceError::InvalidTableLayout);
        }
        // SAFETY: The count and byte layout were checked against both the API's
        // returned byte count and the aligned backing allocation.
        let rows = unsafe {
            slice::from_raw_parts(
                ptr::addr_of!((*table).table).cast::<MIB_TCPROW_OWNER_PID>(),
                count,
            )
        };
        let port = bind_address.port();
        let connection_count = rows
            .iter()
            .filter(|row| {
                row.dwOwningPid == process_id
                    && row.dwState == MIB_TCP_STATE_ESTAB as u32
                    && u16::from_be(row.dwLocalPort as u16) == port
            })
            .count();
        if connection_count == 0 {
            Ok(TcpPeerPresence::Disconnected)
        } else {
            Ok(TcpPeerPresence::Established { connection_count })
        }
    }
}
