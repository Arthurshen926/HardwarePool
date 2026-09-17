use std::{fmt, io};

use windows_sys::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTSGetActiveConsoleSessionId,
};

const NO_SESSION: u32 = u32::MAX;

/// A display session selected by an authenticated local control plane.
///
/// Session 0 is deliberately excluded: Windows services live there, while the
/// display capture engine must run in the authorized interactive user session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractiveDisplaySessionId(u32);

impl InteractiveDisplaySessionId {
    pub fn new(value: u32) -> Result<Self, DisplaySessionError> {
        if value == 0 || value == NO_SESSION {
            return Err(DisplaySessionError::NotInteractive(value));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Read-only diagnostic snapshot. The active console ID is informational and
/// is never used as authorization because remote and multi-user sessions can
/// legitimately differ from it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplaySessionContext {
    process_session_id: u32,
    active_console_session_id: Option<u32>,
}

impl DisplaySessionContext {
    pub fn query() -> Result<Self, DisplaySessionError> {
        let mut process_session_id = NO_SESSION;
        // SAFETY: the output points to valid writable u32 storage and the PID
        // identifies this live process.
        if unsafe { ProcessIdToSessionId(std::process::id(), &mut process_session_id) } == 0 {
            return Err(DisplaySessionError::Query(io::Error::last_os_error()));
        }
        // SAFETY: this function takes no pointers and has no preconditions.
        let active_console_session_id = unsafe { WTSGetActiveConsoleSessionId() };
        Ok(Self {
            process_session_id,
            active_console_session_id: (active_console_session_id != NO_SESSION)
                .then_some(active_console_session_id),
        })
    }

    #[must_use]
    pub const fn process_session_id(self) -> u32 {
        self.process_session_id
    }

    #[must_use]
    pub const fn active_console_session_id(self) -> Option<u32> {
        self.active_console_session_id
    }

    pub fn verify_authorized(
        self,
        authorized: InteractiveDisplaySessionId,
    ) -> Result<VerifiedInteractiveDisplaySession, DisplaySessionError> {
        if self.process_session_id != authorized.get() {
            return Err(DisplaySessionError::WrongProcessSession {
                expected: authorized.get(),
                actual: self.process_session_id,
            });
        }
        Ok(VerifiedInteractiveDisplaySession {
            session_id: authorized,
        })
    }
}

/// Proof that the calling process is executing in the authorized interactive
/// display session. Its field is private so ambient configuration cannot forge
/// the proof consumed by the engine host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedInteractiveDisplaySession {
    session_id: InteractiveDisplaySessionId,
}

impl VerifiedInteractiveDisplaySession {
    #[must_use]
    pub const fn session_id(self) -> InteractiveDisplaySessionId {
        self.session_id
    }
}

pub fn verify_current_process_display_session(
    authorized: InteractiveDisplaySessionId,
) -> Result<VerifiedInteractiveDisplaySession, DisplaySessionError> {
    DisplaySessionContext::query()?.verify_authorized(authorized)
}

#[derive(Debug)]
pub enum DisplaySessionError {
    NotInteractive(u32),
    Query(io::Error),
    WrongProcessSession { expected: u32, actual: u32 },
}

impl fmt::Display for DisplaySessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInteractive(value) => {
                write!(formatter, "display session {value} is not interactive")
            }
            Self::Query(error) => write!(formatter, "could not query process session: {error}"),
            Self::WrongProcessSession { expected, actual } => write!(
                formatter,
                "display engine process is in session {actual}, expected authorized session {expected}"
            ),
        }
    }
}

impl std::error::Error for DisplaySessionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_zero_and_no_session_are_not_interactive() {
        assert!(matches!(
            InteractiveDisplaySessionId::new(0),
            Err(DisplaySessionError::NotInteractive(0))
        ));
        assert!(matches!(
            InteractiveDisplaySessionId::new(NO_SESSION),
            Err(DisplaySessionError::NotInteractive(NO_SESSION))
        ));
    }

    #[test]
    fn exact_authorized_process_session_produces_proof() {
        let authorized = InteractiveDisplaySessionId::new(7).unwrap();
        let context = DisplaySessionContext {
            process_session_id: 7,
            active_console_session_id: Some(3),
        };
        let verified = context.verify_authorized(authorized).unwrap();
        assert_eq!(verified.session_id(), authorized);
    }

    #[test]
    fn another_process_session_is_rejected_even_if_it_is_active_console() {
        let context = DisplaySessionContext {
            process_session_id: 4,
            active_console_session_id: Some(4),
        };
        assert!(matches!(
            context.verify_authorized(InteractiveDisplaySessionId::new(9).unwrap()),
            Err(DisplaySessionError::WrongProcessSession {
                expected: 9,
                actual: 4
            })
        ));
    }
}
