//! Exact display-mode selection. This is not authorization or a media offer.
use crate::{ProtocolError, v1};

const MAX_MODES: usize = 64;

/// Shape validation only; the receiving runtime must also match the admitted
/// peer, current operation, authorized Route and finite lease before acting.
pub fn validate_display_session_control(
    value: &v1::DisplaySessionControl,
) -> Result<(), ProtocolError> {
    use v1::DisplaySessionAction as Action;
    let uuid = |id: &str| uuid::Uuid::parse_str(id).is_ok_and(|id| !id.is_nil());
    if !uuid(&value.operation_id) || (!value.route_id.is_empty() && !uuid(&value.route_id)) {
        return Err(invalid());
    }
    let action = Action::try_from(value.action).map_err(|_| invalid())?;
    if action == Action::Unspecified {
        return Err(invalid());
    }
    if value.automatic && action != Action::Start {
        return Err(invalid());
    }
    if let Some(caps) = &value.capabilities {
        validate_capabilities(caps)?;
    }
    if let Some(selection) = &value.selection
        && (!selection.mode.as_ref().is_some_and(valid_mode)
            || !(500..=100_000).contains(&selection.bitrate_kbps))
    {
        return Err(invalid());
    }
    if (action == Action::Launch) != (value.launch_record.len() == 176)
        || (action != Action::Launch && !value.launch_record.is_empty())
        || (action == Action::Ack) != (value.acknowledgement.len() == 64)
        || (action != Action::Ack && !value.acknowledgement.is_empty())
        || (action != Action::Presented && value.presented_frames != 0)
        || (action == Action::Presented && value.presented_frames == 0)
        || (action != Action::Failed && !value.problem_code.is_empty())
        || (action == Action::Failed
            && (value.problem_code.len() > 128
                || !value.problem_code.starts_with("CAPY.DISPLAY.")
                || !value
                    .problem_code
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c == b'.' || c == b'_')))
    {
        return Err(invalid());
    }
    if matches!(
        action,
        Action::Launch | Action::Ack | Action::Presented | Action::Ready
    ) && (value.route_id.is_empty() || value.epoch == 0)
    {
        return Err(invalid());
    }
    if action == Action::Start && value.selection.is_none() {
        return Err(invalid());
    }
    if action == Action::Capabilities && value.capabilities.is_none() {
        return Err(invalid());
    }
    if action == Action::Launch && value.selection.is_none() {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> ProtocolError {
    ProtocolError::InvalidRouteControl("unsupported display stream selection")
}

fn valid_mode(mode: &v1::DisplayStreamMode) -> bool {
    (2..=4096).contains(&mode.width)
        && (2..=4096).contains(&mode.height)
        && mode.width.is_multiple_of(2)
        && mode.height.is_multiple_of(2)
        && (1_000..=120_000).contains(&mode.refresh_millihertz)
}

fn same_mode(a: &v1::DisplayStreamMode, b: &v1::DisplayStreamMode) -> bool {
    a.width == b.width && a.height == b.height && a.refresh_millihertz == b.refresh_millihertz
}

fn validate_capabilities(caps: &v1::DisplayStreamCapabilities) -> Result<(), ProtocolError> {
    if !caps.h264_sdr
        || caps.modes.is_empty()
        || caps.modes.len() > MAX_MODES
        || caps.minimum_bitrate_kbps < 500
        || caps.maximum_bitrate_kbps > 100_000
        || caps.minimum_bitrate_kbps > caps.maximum_bitrate_kbps
        || caps.modes.iter().any(|mode| !valid_mode(mode))
        || caps.modes.iter().enumerate().any(|(index, mode)| {
            caps.modes[..index]
                .iter()
                .any(|previous| same_mode(previous, mode))
        })
    {
        return Err(invalid());
    }
    Ok(())
}

/// Both peers run the same exact selection check. An unsupported explicit
/// request is rejected, never silently downgraded or clamped. Capability values
/// must come from actual adapter probes; these bounds are not support claims.
pub fn negotiate_display_settings(
    sender: &v1::DisplayStreamCapabilities,
    receiver: &v1::DisplayStreamCapabilities,
    requested: &v1::DisplayStreamSelection,
) -> Result<v1::DisplayStreamSelection, ProtocolError> {
    validate_capabilities(sender)?;
    validate_capabilities(receiver)?;
    let mode = requested.mode.as_ref().ok_or_else(invalid)?;
    if !valid_mode(mode)
        || !sender
            .modes
            .iter()
            .any(|candidate| same_mode(candidate, mode))
        || !receiver
            .modes
            .iter()
            .any(|candidate| same_mode(candidate, mode))
        || requested.bitrate_kbps
            < sender
                .minimum_bitrate_kbps
                .max(receiver.minimum_bitrate_kbps)
        || requested.bitrate_kbps
            > sender
                .maximum_bitrate_kbps
                .min(receiver.maximum_bitrate_kbps)
    {
        return Err(invalid());
    }
    Ok(*requested)
}
