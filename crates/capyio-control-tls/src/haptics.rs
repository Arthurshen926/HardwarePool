//! Independently authorized game feedback data channel. Hosts must validate
//! the current Runtime grant and local Sink lease before actuator execution.
use crate::route_tls::route_tls;
route_tls!(
    HapticsTlsBinding,
    HapticsTlsListener,
    HapticsTlsChannel,
    HapticsTlsStopHandle,
    HAPTICS_TLS_ALPN,
    MAX_HAPTICS_TLS_RECORD_BYTES,
    b"capyio-haptics/1",
    b"CPYHAP01",
    "capyio.haptics.feedback"
);

/// Private AdapterManaged record, not the in-memory StandardPort representation.
/// One command is acknowledged before another is sent. Every rumble expires on
/// the actuator within 160 ms, even if the network or owning worker disappears.
pub const HAPTICS_COMMAND_BYTES: usize = 24;
pub const MAX_HAPTICS_DURATION_MS: u16 = 160;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HapticsRecord {
    Stop {
        sequence: u64,
    },
    Rumble {
        sequence: u64,
        low_frequency: u16,
        high_frequency: u16,
        duration_ms: u16,
    },
    Ack {
        sequence: u64,
    },
}

impl HapticsRecord {
    pub const fn sequence(self) -> u64 {
        match self {
            Self::Stop { sequence } | Self::Rumble { sequence, .. } | Self::Ack { sequence } => {
                sequence
            }
        }
    }

    pub fn encode(self) -> Result<[u8; HAPTICS_COMMAND_BYTES], ControlTlsError> {
        let (kind, low, high, duration) = match self {
            Self::Stop { .. } => (0, 0, 0, 0),
            Self::Ack { .. } => (2, 0, 0, 0),
            Self::Rumble {
                low_frequency,
                high_frequency,
                duration_ms,
                ..
            } if duration_ms > 0
                && duration_ms <= MAX_HAPTICS_DURATION_MS
                && (low_frequency > 0 || high_frequency > 0) =>
            {
                (1, low_frequency, high_frequency, duration_ms)
            }
            _ => return Err(ControlTlsError::UnauthorizedBinding),
        };
        let mut bytes = [0; HAPTICS_COMMAND_BYTES];
        bytes[..4].copy_from_slice(b"CPHF");
        bytes[4] = 1;
        bytes[5] = kind;
        bytes[6..8].copy_from_slice(&duration.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.sequence().to_be_bytes());
        bytes[16..18].copy_from_slice(&low.to_be_bytes());
        bytes[18..20].copy_from_slice(&high.to_be_bytes());
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ControlTlsError> {
        if bytes.len() != HAPTICS_COMMAND_BYTES
            || &bytes[..4] != b"CPHF"
            || bytes[4] != 1
            || bytes[20..] != [0; 4]
        {
            return Err(ControlTlsError::UnauthorizedBinding);
        }
        let sequence = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        let low_frequency = u16::from_be_bytes(bytes[16..18].try_into().unwrap());
        let high_frequency = u16::from_be_bytes(bytes[18..20].try_into().unwrap());
        let duration_ms = u16::from_be_bytes(bytes[6..8].try_into().unwrap());
        let value = match bytes[5] {
            0 if low_frequency == 0 && high_frequency == 0 && duration_ms == 0 => {
                Self::Stop { sequence }
            }
            1 => Self::Rumble {
                sequence,
                low_frequency,
                high_frequency,
                duration_ms,
            },
            2 if low_frequency == 0 && high_frequency == 0 && duration_ms == 0 => {
                Self::Ack { sequence }
            }
            _ => return Err(ControlTlsError::UnauthorizedBinding),
        };
        value.encode()?;
        Ok(value)
    }
}

/// The first command clears the actuator; commands cannot replay or skip a
/// sequence within this TLS/Route epoch. An error requires a new channel.
#[derive(Default)]
pub struct HapticsSequence {
    next: u64,
    failed: bool,
}
impl HapticsSequence {
    pub fn accept(&mut self, record: HapticsRecord) -> Result<(), ControlTlsError> {
        if self.failed
            || record.sequence() != self.next
            || matches!(record, HapticsRecord::Ack { .. })
            || (self.next == 0 && !matches!(record, HapticsRecord::Stop { .. }))
        {
            self.failed = true;
            return Err(ControlTlsError::UnauthorizedBinding);
        }
        match self.next.checked_add(1) {
            Some(next) => {
                self.next = next;
                Ok(())
            }
            None => {
                self.failed = true;
                Err(ControlTlsError::UnauthorizedBinding)
            }
        }
    }
}

#[cfg(test)]
mod feedback_tests {
    use super::*;
    #[test]
    fn command_ack_and_stop_preserve_both_normalized_motors() {
        for record in [
            HapticsRecord::Stop { sequence: 0 },
            HapticsRecord::Rumble {
                sequence: 1,
                low_frequency: 65535,
                high_frequency: 32768,
                duration_ms: 160,
            },
            HapticsRecord::Ack { sequence: 1 },
        ] {
            assert_eq!(
                HapticsRecord::decode(&record.encode().unwrap()).unwrap(),
                record
            );
        }
    }
    #[test]
    fn initial_release_and_no_replay_are_required() {
        let rumble = HapticsRecord::Rumble {
            sequence: 1,
            low_frequency: 1,
            high_frequency: 0,
            duration_ms: 80,
        };
        assert!(HapticsSequence::default().accept(rumble).is_err());
        let mut state = HapticsSequence::default();
        state.accept(HapticsRecord::Stop { sequence: 0 }).unwrap();
        state.accept(rumble).unwrap();
        assert!(state.accept(rumble).is_err());
        assert!(state.accept(HapticsRecord::Stop { sequence: 2 }).is_err());
        assert!(
            HapticsRecord::Rumble {
                sequence: 2,
                low_frequency: 1,
                high_frequency: 0,
                duration_ms: 161
            }
            .encode()
            .is_err()
        );
    }
}
