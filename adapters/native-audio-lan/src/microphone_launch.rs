use std::{
    net::{Ipv4Addr, SocketAddrV4},
    time::{Duration, Instant},
};

use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};

use crate::{NativeLanRouteBinding, microphone_lab_binding};

/// Private worker-process arguments, not network authority or a public wire schema.
#[derive(Clone, Debug)]
pub struct NativeMicrophoneLaunch {
    pub local: SocketAddrV4,
    pub peer: SocketAddrV4,
    pub binding: AudioMediaStreamBinding,
    pub expires_at_ms: Option<u64>,
    pub lab_duration: Option<Duration>,
}

impl NativeMicrophoneLaunch {
    /// Parse before opening the socket or capture ring. Dynamic mode never falls
    /// back to lab identities on malformed, incomplete or expired arguments.
    pub fn parse(args: &[String], now_ms: u64) -> Result<Self, String> {
        Self::parse_preset(args, now_ms, false)
    }

    pub fn parse_speaker(args: &[String], now_ms: u64) -> Result<Self, String> {
        Self::parse_preset(args, now_ms, true)
    }

    fn parse_preset(args: &[String], now_ms: u64, speaker: bool) -> Result<Self, String> {
        if args.len() < 2 || args.len() > 8 || args.iter().any(|arg| arg.len() > 64) {
            return Err("invalid bounded microphone worker arguments".into());
        }
        let local = parse_endpoint(&args[0])?;
        let peer = parse_endpoint(&args[1])?;
        if local == peer {
            return Err("microphone endpoints must differ".into());
        }
        let (binding, expires_at_ms, lab_duration) = match &args[2..] {
            [] => (
                if speaker {
                    crate::speaker_lab_binding()
                } else {
                    microphone_lab_binding()
                },
                None,
                None,
            ),
            [seconds] if seconds != "--route-v1" => {
                let seconds = seconds
                    .parse::<u64>()
                    .ok()
                    .filter(|n| (1..=300).contains(n))
                    .ok_or("lab seconds must be between 1 and 300")?;
                (
                    if speaker {
                        crate::speaker_lab_binding()
                    } else {
                        microphone_lab_binding()
                    },
                    None,
                    Some(Duration::from_secs(seconds)),
                )
            }
            [flag, session, route, stream, epoch, expiry] if flag == "--route-v1" => {
                let binding = AudioMediaStreamBinding {
                    session_id: session.parse().map_err(|_| "invalid Session ID")?,
                    route_id: route.parse().map_err(|_| "invalid Route ID")?,
                    stream_id: stream.parse().map_err(|_| "invalid Stream ID")?,
                    stream_epoch: epoch.parse().map_err(|_| "invalid audio epoch")?,
                    selected_spec: if speaker {
                        AudioStreamSpec::media_balanced()
                    } else {
                        AudioStreamSpec::voice_interactive()
                    },
                };
                binding
                    .validate()
                    .map_err(|_| "invalid microphone binding")?;
                if binding.stream_epoch > i32::MAX as u32 {
                    return Err("audio epoch exceeds Android bound".into());
                }
                let expiry = expiry.parse::<u64>().map_err(|_| "invalid audio expiry")?;
                if expiry <= now_ms || expiry > 9_007_199_254_740_991 {
                    return Err("audio grant expired or exceeds timestamp bound".into());
                }
                (binding, Some(expiry), None)
            }
            _ => return Err("incomplete or unknown microphone worker arguments".into()),
        };
        Ok(Self {
            local,
            peer,
            binding,
            expires_at_ms,
            lab_duration,
        })
    }

    pub fn route_arguments(
        local: SocketAddrV4,
        peer: SocketAddrV4,
        route: &NativeLanRouteBinding,
    ) -> Result<Vec<String>, String> {
        Self::preset_arguments(local, peer, route, false)
    }

    pub fn speaker_route_arguments(
        local: SocketAddrV4,
        peer: SocketAddrV4,
        route: &NativeLanRouteBinding,
    ) -> Result<Vec<String>, String> {
        Self::preset_arguments(local, peer, route, true)
    }

    fn preset_arguments(
        local: SocketAddrV4,
        peer: SocketAddrV4,
        route: &NativeLanRouteBinding,
        speaker: bool,
    ) -> Result<Vec<String>, String> {
        if route.media().selected_spec
            != if speaker {
                AudioStreamSpec::media_balanced()
            } else {
                AudioStreamSpec::voice_interactive()
            }
        {
            return Err("audio worker requires its exact directional PCM preset".into());
        }
        Ok(vec![
            local.to_string(),
            peer.to_string(),
            "--route-v1".into(),
            route.media().session_id.to_string(),
            route.media().route_id.to_string(),
            route.media().stream_id.to_string(),
            route.media().stream_epoch.to_string(),
            route.expires_at_ms().to_string(),
        ])
    }

    /// Capture the remaining duration once so a backwards wall-clock change
    /// cannot extend the worker's original budget.
    pub fn deadline(&self, now_ms: u64, now: Instant) -> Result<Option<Instant>, String> {
        let duration = if let Some(expiry) = self.expires_at_ms {
            Some(Duration::from_millis(
                expiry
                    .checked_sub(now_ms)
                    .filter(|remaining| *remaining > 0)
                    .ok_or("audio grant expired")?,
            ))
        } else {
            self.lab_duration
        };
        duration
            .map(|value| {
                now.checked_add(value)
                    .ok_or_else(|| "audio deadline overflow".into())
            })
            .transpose()
    }

    pub fn is_live(&self, deadline: Option<Instant>, now_ms: u64, now: Instant) -> bool {
        !self.expires_at_ms.is_some_and(|expiry| now_ms >= expiry)
            && !deadline.is_some_and(|deadline| now >= deadline)
    }
}

fn parse_endpoint(value: &str) -> Result<SocketAddrV4, String> {
    let endpoint: SocketAddrV4 = value.parse().map_err(|_| "expected literal IPv4:port")?;
    let ip = endpoint.ip();
    if endpoint.port() == 0
        || ip.is_unspecified()
        || ip.is_multicast()
        || *ip == Ipv4Addr::BROADCAST
    {
        return Err("expected concrete unicast IPv4 and nonzero port".into());
    }
    Ok(endpoint)
}
