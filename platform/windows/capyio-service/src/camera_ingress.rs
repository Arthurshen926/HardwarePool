//! Privileged allocation only. Frames, decoding and camera lifetime stay with the user.
use crate::{
    camera_control::camera_pipe_sddl,
    local_pipe::{NamedPipe, invoke_service_handoff, wake},
};
use capyio_core::StreamId;
use capyio_windows_camera_share::CameraSharedIngressProducer;
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
const PIPE: &str = r"\\.\pipe\CapyIO.Camera.Allocation.v1";
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    stream: StreamId,
    epoch: u64,
}

pub fn allocate_camera_producer(
    stream: StreamId,
    epoch: u64,
) -> Result<CameraSharedIngressProducer, String> {
    invoke_service_handoff(
        PIPE,
        &Request {
            version: 1,
            stream,
            epoch,
        },
        || CameraSharedIngressProducer::open_reserved(stream, epoch).map_err(|e| e.to_string()),
    )
}

pub struct CameraIngressService {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), String>>>,
}
impl CameraIngressService {
    pub fn open(sid: &str) -> Result<Self, String> {
        crate::verify_local_system_service_context().map_err(|e| e.to_string())?;
        let pipe_acl = camera_pipe_sddl(sid)?;
        // Exactly one configured user can write. Frame Server keeps read-only access.
        let mapping_acl = format!("D:P(A;;GA;;;SY)(A;;GR;;;LS)(A;;GA;;;{sid})S:(ML;;NW;;;ME)");
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (ready, response) = std::sync::mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let pipe = match NamedPipe::create_exclusive(PIPE, &pipe_acl) {
                Ok(pipe) => pipe,
                Err(error) => {
                    let _ = ready.send(Err(error.clone()));
                    return Err(error);
                }
            };
            let _ = ready.send(Ok(()));
            while !stopping.load(Ordering::Acquire) && pipe.wait_for_client(&stopping)? {
                // Reservation stays alive until the client's bounded acknowledgement.
                let allocation = pipe.read_json::<Request>().and_then(|request| {
                    if request.version != 1 {
                        return Err("unsupported camera allocation version".into());
                    }
                    CameraSharedIngressProducer::reserve_for_user(
                        request.stream,
                        request.epoch,
                        &mapping_acl,
                    )
                    .map_err(|e| e.to_string())
                });
                let response: Result<(), String> =
                    allocation.as_ref().map(|_| ()).map_err(Clone::clone);
                let _ = pipe.write_node_response(&response);
                drop(allocation);
                pipe.disconnect();
            }
            Ok(())
        });
        let service = Self {
            stop,
            worker: Some(worker),
        };
        response
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "camera allocation listener timed out")??;
        Ok(service)
    }
}
impl Drop for CameraIngressService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        wake(PIPE);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
