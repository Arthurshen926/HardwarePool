use std::{
    io::{self, Read},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use crate::android_receivers::{capture_stderr_tail, sibling_executable};

pub(crate) const EXECUTABLE: &str = "capyio-camera-session-host.exe";
const READY: &[u8] = b"CAPYIO_CAMERA_SESSION_READY";
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) struct CameraSession {
    child: Child,
    stderr: Arc<Mutex<Vec<u8>>>,
}

impl CameraSession {
    pub(crate) fn start() -> Result<Self, String> {
        Self::spawn(Command::new(sibling_executable(EXECUTABLE)?))
    }

    fn spawn(mut command: Command) -> Result<Self, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("无法启动系统相机宿主: {error}"))?;
        let stderr = Arc::new(Mutex::new(Vec::with_capacity(4096)));
        if let Some(stream) = child.stderr.take() {
            capture_stderr_tail(stream, Arc::clone(&stderr));
        }
        let mut session = Self { child, stderr };
        let stdout = session
            .child
            .stdout
            .take()
            .ok_or("相机宿主就绪通道不可用")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut stdout = stdout;
            let ready = read_ready(&mut stdout);
            let _ = sender.send(ready);
            let _ = io::copy(&mut stdout, &mut io::sink());
        });
        match receiver.recv_timeout(READY_TIMEOUT) {
            Ok(Ok(())) => Ok(session),
            result => {
                let detail = format!("系统相机未就绪: {result:?}");
                let cleanup = session.stop();
                Err(format!(
                    "{detail} · {} · cleanup={cleanup:?}",
                    session.error_tail()
                ))
            }
        }
    }

    fn error_tail(&self) -> String {
        self.stderr
            .lock()
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
            .unwrap_or_else(|_| "无法读取相机错误".to_owned())
    }

    pub(crate) fn poll(&mut self) -> Result<(), String> {
        match self.child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(format!(
                "系统相机宿主已退出（{status}）: {}",
                self.error_tail()
            )),
            Err(error) => Err(format!("无法读取系统相机宿主状态: {error}")),
        }
    }

    pub(crate) fn stop(&mut self) -> Result<(), String> {
        // EOF requests normal Stop/Shutdown in the camera owner.
        drop(self.child.stdin.take());
        let deadline = Instant::now() + STOP_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().map_err(|error| error.to_string())? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "系统相机关闭失败（{status}）: {}",
                        self.error_tail()
                    ))
                };
            }
            if Instant::now() >= deadline {
                self.child.kill().map_err(|error| error.to_string())?;
                self.child.wait().map_err(|error| error.to_string())?;
                return Err("系统相机正常关闭超时，已终止宿主；请检查相机清理状态".to_owned());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for CameraSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn read_ready(reader: &mut impl Read) -> Result<(), String> {
    let mut line = Vec::with_capacity(64);
    for _ in 0..64 {
        let mut byte = [0_u8];
        if reader.read(&mut byte).map_err(|error| error.to_string())? == 0 {
            return Err("相机宿主在就绪前关闭输出".to_owned());
        }
        if byte[0] == b'\n' {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return if line == READY {
                Ok(())
            } else {
                Err("相机宿主就绪记录不匹配".to_owned())
            };
        }
        line.push(byte[0]);
    }
    Err("相机宿主就绪记录超出长度限制".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    #[ignore = "requires explicitly approved deployed camera COM source on the local lab host"]
    fn physical_deployed_session_starts_and_stops_via_control_pipe() {
        let mut session = CameraSession::start().expect("deployed camera ready");
        session.poll().expect("camera host alive");
        session.stop().expect("normal camera Stop/Shutdown");
    }

    #[test]
    fn readiness_requires_exact_bounded_record() {
        assert!(read_ready(&mut &b"CAPYIO_CAMERA_SESSION_READY\n"[..]).is_ok());
        assert!(read_ready(&mut &b"CAPYIO_CAMERA_SESSION_READY\r\n"[..]).is_ok());
        assert!(read_ready(&mut &b"CAPYIO_CAMERA_SESSION_READY"[..]).is_err());
        assert!(read_ready(&mut &b"other\n"[..]).is_err());
        assert!(read_ready(&mut &[b'x'; 65][..]).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn owned_child_exits_on_control_eof() {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command",
            "[Console]::Out.WriteLine('CAPYIO_CAMERA_SESSION_READY'); [Console]::Out.Flush(); $null = [Console]::In.ReadToEnd(); exit 0"]);
        let mut child = CameraSession::spawn(command).expect("ready fixture");
        child.poll().expect("alive before stop");
        child.stop().expect("EOF shutdown");
        assert!(child.child.try_wait().unwrap().unwrap().success());
    }

    #[cfg(windows)]
    #[test]
    fn rejected_readiness_rolls_back_with_normal_eof_shutdown() {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command",
            "[Console]::Out.WriteLine('INVALID_READY'); [Console]::Out.Flush(); $null = [Console]::In.ReadToEnd(); exit 0"]);
        let error = CameraSession::spawn(command)
            .err()
            .expect("reject invalid readiness");
        assert!(error.contains("记录不匹配"));
        assert!(error.contains("cleanup=Ok(())"), "{error}");
    }

    #[cfg(windows)]
    #[test]
    fn premature_host_failure_is_not_reported_as_ready() {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", "exit 7"]);
        let error = CameraSession::spawn(command)
            .err()
            .expect("reject premature exit");
        assert!(error.contains("就绪前关闭输出"));
        assert!(error.contains("cleanup=Err"), "{error}");
    }
}
