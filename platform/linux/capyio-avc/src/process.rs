use crate::{FiniteStreamBudget, admission::Request, error};
use capyio_linux_camera::FRAME_BYTES;
use std::{
    fs,
    io::{self, Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{MetadataExt, PermissionsExt},
            net::UnixStream,
        },
    },
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const TRANSACTION_BUDGET: Duration = Duration::from_secs(2);
pub(crate) struct Decoder {
    child: Child,
    socket: UnixStream,
    deadline: Instant,
    stopped: bool,
    reaped: bool,
}
impl Decoder {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn spawn(lifetime: Duration) -> io::Result<Self> {
        Self::spawn_profile(lifetime, None)
    }
    pub fn spawn_profile(
        lifetime: Duration,
        budget: Option<FiniteStreamBudget>,
    ) -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        let helper = exe
            .parent()
            .ok_or_else(|| error("missing executable parent"))?
            .join("capyio-linux-avc-helper");
        let metadata = fs::symlink_metadata(&helper)?;
        if !metadata.file_type().is_file()
            || metadata.permissions().mode() & 0o022 != 0
            || metadata.uid() != fs::metadata(&exe)?.uid()
        {
            return Err(error(
                "decoder helper must be owned non-writable regular sibling",
            ));
        }
        let (socket, child_socket) = UnixStream::pair()?;
        socket.set_nonblocking(true)?;
        let input: OwnedFd = child_socket.try_clone()?.into();
        let output: OwnedFd = child_socket.into();
        let deadline = Instant::now() + lifetime;
        let mut command = Command::new(helper);
        command.env_clear();
        let lifetime_ms =
            budget.map_or_else(|| lifetime.as_millis(), |b| u128::from(b.lifetime_millis()));
        command.args(["--lifetime-ms", &lifetime_ms.to_string()]);
        if budget.is_some() {
            command.args(["--profile", crate::FINITE_STREAM_FORMAT]);
        }
        let child = command
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output))
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self {
            child,
            socket,
            deadline,
            stopped: false,
            reaped: false,
        })
    }
    pub fn pid(&self) -> Option<u32> {
        (!self.stopped).then_some(self.child.id())
    }
    pub fn check(&mut self) -> io::Result<()> {
        if self.stopped || Instant::now() >= self.deadline || self.child.try_wait()?.is_some() {
            return Err(error("decoder stopped, expired or exited"));
        }
        Ok(())
    }
    pub fn exchange(
        &mut self,
        request: &Request,
        mut poll_authority: impl FnMut() -> io::Result<()>,
    ) -> io::Result<Vec<u8>> {
        let deadline = self.deadline.min(Instant::now() + TRANSACTION_BUDGET);
        let header = input_header(request);
        let mut written = 0usize;
        let total = header.len() + request.payload.len();
        let mut output_header = [0u8; 32];
        let mut header_read = 0;
        let mut payload = Vec::new();
        let mut payload_read = 0;
        let mut validated = false;
        loop {
            poll_authority()?;
            if Instant::now() >= deadline {
                return Err(error("decoder IPC deadline"));
            }
            // EOS may close immediately after its complete acknowledgement. Read
            // buffered socket bytes before observing child exit in that case.
            if request.kind != 3 {
                self.check()?;
            }
            let mut progress = false;
            if written < total {
                let bytes = if written < header.len() {
                    &header[written..]
                } else {
                    &request.payload[written - header.len()..]
                };
                match self.socket.write(bytes) {
                    Ok(0) => return Err(error("decoder input closed")),
                    Ok(n) => {
                        written += n;
                        progress = true;
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e),
                }
            }
            if header_read < 32 {
                match self.socket.read(&mut output_header[header_read..]) {
                    Ok(0) => return Err(error("decoder truncated output header")),
                    Ok(n) => {
                        header_read += n;
                        progress = true;
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e),
                }
            }
            if header_read == 32 && !validated {
                validate_output_header(&output_header, request)?;
                payload.resize(if request.kind == 2 { FRAME_BYTES } else { 0 }, 0);
                validated = true;
            }
            if validated && payload_read < payload.len() {
                match self.socket.read(&mut payload[payload_read..]) {
                    Ok(0) => return Err(error("decoder truncated frame")),
                    Ok(n) => {
                        payload_read += n;
                        progress = true;
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e),
                }
            }
            if validated && payload_read == payload.len() {
                if written != total {
                    return Err(error("decoder replied before complete request"));
                }
                poll_authority()?;
                return Ok(payload);
            }
            // This is nonblocking IPC readiness pacing, never a codec EAGAIN
            // retry. Partial offsets persist and absolute deadlines stay fixed.
            if !progress {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
    pub fn stop(&mut self) -> io::Result<()> {
        self.stopped = true;
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
        if self.reaped {
            return Ok(());
        }
        if self.child.try_wait()?.is_none()
            && let Err(reason) = self.child.kill()
        {
            // Normal EOS can race between try_wait and kill. Reap it rather
            // than reporting a cleanup failure for an already exited child.
            if self.child.try_wait()?.is_none() {
                return Err(reason);
            }
            self.reaped = true;
            return Ok(());
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if self.child.try_wait()?.is_some() {
                self.reaped = true;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(error("decoder kill/reap deadline"));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
fn input_header(r: &Request) -> [u8; 32] {
    let mut h = [0u8; 32];
    h[..4].copy_from_slice(b"CAD1");
    h[4..8].copy_from_slice(&r.kind.to_be_bytes());
    h[8..12].copy_from_slice(&(r.payload.len() as u32).to_be_bytes());
    h[12..16].copy_from_slice(&r.flags.to_be_bytes());
    h[16..24].copy_from_slice(&r.sequence.to_be_bytes());
    h[24..32].copy_from_slice(&r.pts_us.to_be_bytes());
    h
}
fn validate_output_header(h: &[u8; 32], r: &Request) -> io::Result<()> {
    let expected_length = if r.kind == 2 { FRAME_BYTES as u32 } else { 0 };
    if &h[..4] != b"CAN1"
        || h[4..8] != r.kind.to_be_bytes()
        || h[8..12] != expected_length.to_be_bytes()
        || h[12..16] != [0; 4]
        || h[16..24] != r.sequence.to_be_bytes()
        || h[24..32] != r.pts_us.to_be_bytes()
    {
        return Err(error(
            "decoder response identity, format or length mismatch",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            kind: 2,
            flags: 1,
            sequence: 3,
            pts_us: 33333,
            payload: vec![1],
            descriptor: None,
        }
    }
    #[test]
    fn response_headers_bind_exact_request_and_length() {
        let r = request();
        let mut h = input_header(&r);
        h[..4].copy_from_slice(b"CAN1");
        h[8..12].copy_from_slice(&(FRAME_BYTES as u32).to_be_bytes());
        h[12..16].fill(0);
        validate_output_header(&h, &r).unwrap();
        for i in 0..32 {
            let mut bad = h;
            bad[i] ^= 0xff;
            assert!(validate_output_header(&bad, &r).is_err());
        }
    }
    #[cfg(feature = "native-avc")]
    fn fixture_requests() -> Vec<Request> {
        let bytes = include_bytes!("../../../../fixtures/linux-avc/solid-720p30.cavc");
        let mut id = [0; 16];
        for (i, b) in id.iter_mut().enumerate() {
            *b = i as u8 + 1;
        }
        let mut a = crate::admission::Admission::new(capyio_vcamdroid_adapter::AvcStreamKey {
            stream_id: id,
            epoch: 1,
        })
        .unwrap();
        let mut pos = 0;
        let mut out = vec![];
        while pos < bytes.len() {
            let n = u32::from_be_bytes(bytes[pos + 52..pos + 56].try_into().unwrap()) as usize + 56;
            out.push(a.accept(&bytes[pos..pos + n]).unwrap());
            pos += n;
        }
        out
    }
    #[test]
    #[cfg(feature = "native-avc")]
    #[ignore = "explicit isolated hosted decoder gate only; requires sibling helper"]
    fn hosted_decoder_pixels_order_pts_and_cleanup() {
        assert_eq!(std::env::var("CAPYIO_AVC_HOSTED_GATE").as_deref(), Ok("1"));
        let mut d = Decoder::spawn(Duration::from_secs(10)).unwrap();
        let mut polls = 0;
        let colors = [(34, 118, 154), (95, 101, 138), (161, 118, 119)];
        for r in fixture_requests() {
            let pixels = d
                .exchange(&r, || {
                    polls += 1;
                    Ok(())
                })
                .unwrap();
            if r.kind == 2 {
                let (y, u, v) = colors[r.sequence as usize - 1];
                assert!(pixels[..921600].iter().all(|n| *n == y));
                assert!(pixels[921600..].chunks_exact(2).all(|c| c == [u, v]));
            } else {
                assert!(pixels.is_empty());
            }
        }
        assert!(polls >= 5);
        d.stop().unwrap();
        assert!(d.child.try_wait().unwrap().is_some());
        println!("CAPYIO_AVC_PIXELS_ORDER_PTS_CLEANUP_PASS");
    }
    #[test]
    #[cfg(feature = "native-avc")]
    #[ignore = "explicit isolated hosted decoder gate only; requires sibling helper"]
    fn hosted_decoder_rejects_forged_key_reconfiguration_and_revoke() {
        assert_eq!(std::env::var("CAPYIO_AVC_HOSTED_GATE").as_deref(), Ok("1"));
        for scenario in 0..5 {
            let mut d = Decoder::spawn(Duration::from_secs(10)).unwrap();
            let mut rs = fixture_requests();
            d.exchange(&rs[0], || Ok(())).unwrap();
            let config_payload = rs[0].payload.clone();
            let r = &mut rs[1];
            match scenario {
                0 => r.flags = 0,
                1 => r.payload = vec![0, 0, 0, 1, 0x65],
                2 => {
                    r.payload = config_payload;
                    r.payload[8] ^= 1;
                }
                3 => {}
                4 => {
                    d.child.kill().unwrap();
                }
                _ => unreachable!(),
            }
            let result = d.exchange(&rs[1], || {
                if scenario == 3 {
                    Err(error("revoked"))
                } else {
                    Ok(())
                }
            });
            assert!(result.is_err());
            d.stop().unwrap();
            assert!(d.child.try_wait().unwrap().is_some());
        }
        println!("CAPYIO_AVC_NEGATIVE_REVOKE_CLEANUP_PASS");
    }
}
