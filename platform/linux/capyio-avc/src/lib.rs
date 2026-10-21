#![forbid(unsafe_code)]
//! Offline, finite Android CAVC -> owned decoder -> authorized RuntimeCamera.
//! No listener, demuxer, hardware capture, implicit conversion or executable path.
mod admission;
#[cfg(all(target_os = "linux", any(test, feature = "native-avc")))]
#[cfg_attr(not(feature = "native-avc"), allow(dead_code))]
mod process;
pub mod secure;

use capyio_core::StreamId;
use capyio_linux_camera::{CameraBinding, RuntimeCamera, SubmitOutcome};
use capyio_runtime::NodeRuntime;
use capyio_vcamdroid_adapter::AvcStreamKey;
use std::{io, time::Duration};

pub const MAX_LIFETIME: Duration = Duration::from_secs(60);
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_FRAMES: usize = 120;
fn error(reason: impl ToString) -> io::Error {
    io::Error::other(reason.to_string())
}

/// Owns both children. Only complete, bounded records from an already-admitted
/// offline source may enter. Every wait polls the same authoritative camera.
/// Invalid input, decoder error, EOS, timeout or Drop stops both owners.
pub struct RuntimeAvcCamera {
    camera: RuntimeCamera,
    admission: admission::Admission,
    #[cfg(all(target_os = "linux", feature = "native-avc"))]
    decoder: process::Decoder,
    stopped: bool,
}
impl RuntimeAvcCamera {
    pub fn start(
        runtime: NodeRuntime,
        binding: CameraBinding,
        lifetime: Duration,
    ) -> io::Result<Self> {
        Self::start_format(runtime, binding, lifetime, false)
    }
    /// Start only after authenticated CAVC data binding and live control authority.
    pub fn start_secure(
        runtime: NodeRuntime,
        binding: CameraBinding,
        lifetime: Duration,
    ) -> io::Result<Self> {
        Self::start_format(runtime, binding, lifetime, true)
    }
    fn start_format(
        runtime: NodeRuntime,
        binding: CameraBinding,
        lifetime: Duration,
        encoded_avc: bool,
    ) -> io::Result<Self> {
        if lifetime.is_zero() || lifetime > MAX_LIFETIME {
            return Err(error("decoder lifetime outside 1ms..=60s"));
        }
        let stream = stream_key(binding.stream_id, binding.stream_epoch);
        let admission = admission::Admission::new(stream)?;
        let mut camera = if encoded_avc {
            RuntimeCamera::start_decoded_avc(runtime, binding, lifetime)?
        } else {
            RuntimeCamera::start(runtime, binding, lifetime)?
        };
        #[cfg(all(target_os = "linux", feature = "native-avc"))]
        {
            let decoder = process::Decoder::spawn(lifetime)?;
            camera.poll()?;
            Ok(Self {
                camera,
                admission,
                decoder,
                stopped: false,
            })
        }
        #[cfg(not(all(target_os = "linux", feature = "native-avc")))]
        {
            let _ = admission;
            camera.stop()?;
            Err(error("native-avc feature is not enabled on Linux"))
        }
    }
    pub fn helper_pid(&self) -> Option<u32> {
        self.camera.helper_pid()
    }
    pub fn node_name(&self) -> Option<&str> {
        self.camera.node_name()
    }
    pub fn metrics(&self) -> capyio_linux_camera::CameraMetrics {
        self.camera.metrics()
    }
    pub fn decoder_pid(&self) -> Option<u32> {
        #[cfg(all(target_os = "linux", feature = "native-avc"))]
        {
            self.decoder.pid()
        }
        #[cfg(not(all(target_os = "linux", feature = "native-avc")))]
        {
            None
        }
    }
    pub fn runtime(&self) -> &NodeRuntime {
        self.camera.runtime()
    }
    pub fn runtime_mut(&mut self) -> &mut NodeRuntime {
        self.camera.runtime_mut()
    }
    pub fn poll(&mut self) -> io::Result<()> {
        let result = self.check();
        if result.is_err() {
            let _ = self.stop();
        }
        result
    }
    fn check(&mut self) -> io::Result<()> {
        if self.stopped {
            return Err(error("AVC owner stopped"));
        }
        self.camera.poll()?;
        #[cfg(all(target_os = "linux", feature = "native-avc"))]
        self.decoder.check()?;
        Ok(())
    }
    /// Process one entire CAVC record; never blocks indefinitely on partial IPC.
    /// Config yields None; AU yields camera queue outcome; EOS closes both owners.
    pub fn submit_record(&mut self, bytes: &[u8]) -> io::Result<Option<SubmitOutcome>> {
        self.submit_record_supervised(bytes, |_| Ok(()))
    }
    /// Keep authenticated control processing alive during bounded decoder IPC.
    /// A supervisor error is terminal and tears down both owned helpers.
    pub fn submit_record_supervised(
        &mut self,
        bytes: &[u8],
        mut supervise: impl FnMut(&mut NodeRuntime) -> io::Result<()>,
    ) -> io::Result<Option<SubmitOutcome>> {
        let result = self.submit_inner(bytes, &mut supervise);
        if result.is_err() {
            let _ = self.stop();
        }
        result
    }
    fn submit_inner(
        &mut self,
        bytes: &[u8],
        supervise: &mut impl FnMut(&mut NodeRuntime) -> io::Result<()>,
    ) -> io::Result<Option<SubmitOutcome>> {
        supervise(self.camera.runtime_mut())?;
        self.check()?;
        let request = self.admission.accept(bytes)?;
        #[cfg(all(target_os = "linux", feature = "native-avc"))]
        {
            let payload = self.decoder.exchange(&request, || {
                supervise(self.camera.runtime_mut())?;
                self.camera.poll()
            })?;
            supervise(self.camera.runtime_mut())?;
            self.camera.poll()?;
            match request.descriptor {
                Some(descriptor) => self.camera.submit(descriptor, payload).map(Some),
                None if request.kind == 3 => {
                    self.stop()?;
                    Ok(None)
                }
                None => Ok(None),
            }
        }
        #[cfg(not(all(target_os = "linux", feature = "native-avc")))]
        {
            let _ = request;
            Err(error("native-avc feature is not enabled on Linux"))
        }
    }
    pub fn stop(&mut self) -> io::Result<()> {
        self.stopped = true;
        #[cfg(all(target_os = "linux", feature = "native-avc"))]
        let decoder_result = self.decoder.stop();
        let camera_result = self.camera.stop();
        #[cfg(all(target_os = "linux", feature = "native-avc"))]
        decoder_result?;
        camera_result
    }
}
impl Drop for RuntimeAvcCamera {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
fn stream_key(id: StreamId, epoch: u64) -> AvcStreamKey {
    AvcStreamKey {
        stream_id: *id.as_uuid().as_bytes(),
        epoch,
    }
}
