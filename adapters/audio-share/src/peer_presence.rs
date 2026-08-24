use std::net::SocketAddr;

use crate::AudioShareError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiverTcpPresence {
    /// The process is not running, so receiver presence cannot be queried.
    SupervisorNotRunning,
    /// The platform has no reviewed process-owned TCP table implementation.
    UnsupportedPlatform,
    /// The server runs but owns no established TCP connection on its bind port.
    Disconnected,
    /// One or more process-owned TCP connections are established. This proves
    /// transport presence only, not Audio Share negotiation or audible playback.
    Established { connection_count: usize },
}

pub(crate) fn receiver_tcp_presence(
    process_id: u32,
    bind_address: SocketAddr,
) -> Result<ReceiverTcpPresence, AudioShareError> {
    platform::receiver_tcp_presence(process_id, bind_address)
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) fn receiver_tcp_presence(
        _process_id: u32,
        _bind_address: SocketAddr,
    ) -> Result<ReceiverTcpPresence, AudioShareError> {
        Ok(ReceiverTcpPresence::UnsupportedPlatform)
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

    pub(super) fn receiver_tcp_presence(
        process_id: u32,
        bind_address: SocketAddr,
    ) -> Result<ReceiverTcpPresence, AudioShareError> {
        if !matches!(bind_address.ip(), IpAddr::V4(_)) {
            return Ok(ReceiverTcpPresence::UnsupportedPlatform);
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
            return Err(AudioShareError::PeerTableQueryFailed { code: first });
        }
        let required = required_bytes as usize;
        if required > MAX_TCP_TABLE_BYTES {
            return Err(AudioShareError::PeerTableTooLarge {
                limit: MAX_TCP_TABLE_BYTES,
            });
        }
        if required < mem::size_of::<u32>() {
            return Err(AudioShareError::InvalidPeerTableLayout);
        }

        let word_size = mem::size_of::<usize>();
        let word_count = required.div_ceil(word_size);
        let mut storage = vec![0_usize; word_count];
        let mut actual_bytes = (storage.len() * word_size) as u32;
        // SAFETY: `storage` is aligned at least to `usize`, lives through the
        // call and has `actual_bytes` writable bytes. The API initializes the
        // documented owner-PID table layout for AF_INET.
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
            return Err(AudioShareError::PeerTableQueryFailed { code: result });
        }

        let table = storage.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
        // SAFETY: The successful API call initialized at least the table header.
        let count = unsafe { (*table).dwNumEntries as usize };
        let header_bytes = mem::size_of::<u32>();
        let rows_bytes = count
            .checked_mul(mem::size_of::<MIB_TCPROW_OWNER_PID>())
            .and_then(|bytes| header_bytes.checked_add(bytes))
            .ok_or(AudioShareError::InvalidPeerTableLayout)?;
        if rows_bytes > actual_bytes as usize || rows_bytes > storage.len() * word_size {
            return Err(AudioShareError::InvalidPeerTableLayout);
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
            Ok(ReceiverTcpPresence::Disconnected)
        } else {
            Ok(ReceiverTcpPresence::Established { connection_count })
        }
    }
}
