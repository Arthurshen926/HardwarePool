//! Bounded readiness observations, not an additional Route state machine.
//! Callers validate the Route/Session/epoch before using an observation.
#[derive(Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RouteReadiness {
    pub worker_ready: bool,
    pub peer_media_observed: bool,
    pub local_frames: Option<FrameProgress>,
}

#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FrameProgress {
    pub baseline: u64,
    pub observed: u64,
}

pub(super) enum MediaRequirement {
    LocalFrames,
    PeerMedia,
    PeerAndLocalFrames,
}

impl RouteReadiness {
    pub fn media_ready(self, requirement: MediaRequirement) -> bool {
        if !self.worker_ready {
            return false;
        }
        let local = self.local_frames.is_some_and(|p| p.observed > p.baseline);
        match requirement {
            MediaRequirement::LocalFrames => local,
            MediaRequirement::PeerMedia => self.peer_media_observed,
            MediaRequirement::PeerAndLocalFrames => self.peer_media_observed && local,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_requires_the_adapters_actual_media_evidence() {
        let mut evidence = RouteReadiness {
            worker_ready: true,
            ..Default::default()
        };
        assert!(!evidence.media_ready(MediaRequirement::LocalFrames));
        assert!(!evidence.media_ready(MediaRequirement::PeerMedia));
        evidence.peer_media_observed = true;
        assert!(evidence.media_ready(MediaRequirement::PeerMedia));
        assert!(!evidence.media_ready(MediaRequirement::PeerAndLocalFrames));
        evidence.local_frames = Some(FrameProgress {
            baseline: 10,
            observed: 10,
        });
        assert!(!evidence.media_ready(MediaRequirement::PeerAndLocalFrames));
        evidence.local_frames = Some(FrameProgress {
            baseline: 10,
            observed: 11,
        });
        assert!(evidence.media_ready(MediaRequirement::PeerAndLocalFrames));
        evidence.worker_ready = false;
        assert!(!evidence.media_ready(MediaRequirement::PeerAndLocalFrames));
    }
}

/// Last observed evidence, not a synthetic throughput or loss estimate.
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RouteMediaSnapshot {
    pub session_id: capyio_core::SessionId,
    pub route_id: capyio_core::RouteId,
    pub epoch: u64,
    pub sampled_at_ms: u64,
    pub readiness: RouteReadiness,
    pub peer_frames: Option<u64>,
}
