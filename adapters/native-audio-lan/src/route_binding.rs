use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_core::{AuthorizationState, Route, RouteBackend, Session};

use crate::NativeLanError;

/// Bounded Rust/Java worker handoff. Not an authorization grant or secure
/// transport: the caller still owns negotiation, Port leases and peer trust.
#[derive(Clone, Debug)]
pub struct NativeLanRouteBinding {
    media: AudioMediaStreamBinding,
    expires_at_ms: u64,
}

impl NativeLanRouteBinding {
    pub fn from_live_context(
        media: AudioMediaStreamBinding,
        route: &Route,
        session: &Session,
        now_ms: u64,
    ) -> Result<Self, NativeLanError> {
        media.validate_route_context(route, session, now_ms)?;
        if route.backend != RouteBackend::AdapterManaged
            || media.stream_epoch > i32::MAX as u32
            || (media.selected_spec != AudioStreamSpec::voice_interactive()
                && media.selected_spec != AudioStreamSpec::media_balanced())
        {
            return Err(NativeLanError::InvalidConfiguration(
                "dynamic native audio requires AdapterManaged, a Java-compatible epoch and an exact supported PCM preset",
            ));
        }
        let AuthorizationState::Authorized {
            expires_at_ms: Some(expires_at_ms),
        } = route.authorization
        else {
            return Err(NativeLanError::InvalidConfiguration(
                "finite Route grant required",
            ));
        };
        if expires_at_ms > 9_007_199_254_740_991 {
            return Err(NativeLanError::InvalidConfiguration(
                "grant deadline exceeds cross-platform integer bound",
            ));
        }
        Ok(Self {
            media,
            expires_at_ms,
        })
    }

    /// Recheck a fresh runtime snapshot before reusing this handoff. A renewed
    /// short Port lease cannot silently extend or replace the original grant.
    pub fn require_live_context(
        &self,
        route: &Route,
        session: &Session,
        now_ms: u64,
    ) -> Result<(), NativeLanError> {
        self.media.validate_route_context(route, session, now_ms)?;
        if route.backend != RouteBackend::AdapterManaged
            || route.authorization
                != (AuthorizationState::Authorized {
                    expires_at_ms: Some(self.expires_at_ms),
                })
        {
            return Err(NativeLanError::InvalidConfiguration(
                "audio Route contract changed; create a fresh handoff",
            ));
        }
        Ok(())
    }

    pub fn media(&self) -> &AudioMediaStreamBinding {
        &self.media
    }
    pub const fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }
}
