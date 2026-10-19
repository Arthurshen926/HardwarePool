#![cfg(target_os = "linux")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const ID: &str = "00000000-0000-4000-8000-000000000301";
struct Host {
    child: Child,
    replies: Receiver<String>,
}
impl Host {
    fn spawn() -> Self {
        Self::spawn_mode(false)
    }
    fn spawn_mode(mock_routes: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_capyio-node"));
        command.args([
            "linux-host",
            "--node-id",
            ID,
            "--name",
            "Linux process test",
        ]);
        if mock_routes {
            command.arg("--mock-routes");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, replies) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        Self { child, replies }
    }
    fn send(&mut self, command: &str, id: u64) {
        writeln!(
            self.child.stdin.as_mut().unwrap(),
            "{}",
            json!({"version":1,"id":id,"command":command})
        )
        .unwrap();
    }
    fn reply(&self) -> Value {
        serde_json::from_str(
            &self
                .replies
                .recv_timeout(Duration::from_secs(5))
                .expect("host response deadline"),
        )
        .unwrap()
    }
    fn expect_exit(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline, "host exit deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.child.stdin.take(); // EOF gives the owner an opportunity to reap its mocks.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn linux_host_keeps_one_runtime_until_explicit_shutdown() {
    let mut host = Host::spawn();
    let pid = host.child.id();
    host.send("get_snapshot", 1);
    let first = host.reply();
    assert_eq!(first["result"]["local_node"]["platform"], "linux");
    assert_eq!(first["result"]["local_node"]["id"], ID);
    assert_eq!(first["result"]["local_node"]["capabilities"], json!({}));
    assert_eq!(first["result"]["routes"], json!([]));
    host.send("host_status", 2);
    assert_eq!(host.reply()["result"]["state"], "running");
    assert_eq!(host.child.id(), pid);
    assert!(host.child.try_wait().unwrap().is_none());
    host.send("get_snapshot", 3);
    assert_eq!(host.reply()["result"], first["result"]);
    // Both records are queued together: only shutdown may dispatch.
    host.child.stdin.as_mut().unwrap().write_all(b"{\"version\":1,\"id\":4,\"command\":\"host_shutdown\"}\n{\"version\":1,\"id\":5,\"command\":\"get_snapshot\"}\n").unwrap();
    assert_eq!(host.reply()["result"]["state"], "stopped");
    host.expect_exit();
    assert!(host.replies.recv_timeout(Duration::from_secs(1)).is_err());
}

#[test]
fn invalid_record_recovers_then_eof_stops_host() {
    let mut host = Host::spawn();
    host.child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"not-json\n")
        .unwrap();
    assert_eq!(host.reply()["error"]["code"], "invalid_request");
    host.send("host_status", 9);
    assert_eq!(host.reply()["id"], 9);
    drop(host.child.stdin.take());
    host.expect_exit();
}

#[test]
fn oversized_unterminated_record_exits_without_waiting_for_eof() {
    let mut host = Host::spawn();
    host.child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&vec![b'x'; capyio_linux_host::MAX_REQUEST_BYTES + 1])
        .unwrap();
    assert_eq!(host.reply()["error"]["code"], "request_too_large");
    host.expect_exit();
}

#[test]
fn positional_shutdown_is_rejected_and_process_remains_usable() {
    let mut host = Host::spawn();
    host.child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"[1,9,\"host_shutdown\"]\n")
        .unwrap();
    assert_eq!(host.reply()["error"]["code"], "invalid_request");
    host.send("host_status", 10);
    assert_eq!(host.reply()["result"]["state"], "running");
    host.send("host_shutdown", 11);
    assert_eq!(host.reply()["result"]["state"], "stopped");
    host.expect_exit();
}

// Requires: cargo build -p capyio-mock-source -p capyio-mock-sink
// The explicit opt-in resolves these repository-built binaries beside capyio-node.
#[test]
fn mock_children_drive_runtime_lifecycle_and_host_exits_on_shutdown() {
    let mut host = Host::spawn_mode(true);
    host.send("get_snapshot", 1);
    let snapshot = host.reply()["result"].clone();
    assert_eq!(
        snapshot["local_node"]["capabilities"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(snapshot["peers"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["routes"][0]["state"], "draft");
    assert_eq!(
        snapshot["routes"][0]["profile"]["name"],
        "capyio.test.samples"
    );
    host.send("mock_route_start", 2);
    assert_eq!(host.reply()["result"]["routes"][0]["state"], "active");
    host.send("mock_route_start", 3);
    assert_eq!(host.reply()["result"]["routes"][0]["epoch"], 1);
    host.send("mock_route_stop", 4);
    assert_eq!(host.reply()["result"]["routes"][0]["state"], "stopped");
    host.send("mock_route_start", 5);
    assert_eq!(host.reply()["result"]["routes"][0]["epoch"], 2);
    // Saturate retained events and prove the management response remains bounded.
    for id in 6..106 {
        host.send(
            if id % 2 == 0 {
                "mock_route_stop"
            } else {
                "mock_route_start"
            },
            id,
        );
        assert!(host.reply().get("error").is_none());
    }
    host.send("get_snapshot", 106);
    assert_eq!(
        host.reply()["result"]["events"].as_array().unwrap().len(),
        256
    );
    host.send("host_shutdown", 107);
    assert_eq!(host.reply()["result"]["state"], "stopped");
    host.expect_exit();
}
#[test]
fn mock_host_exits_on_eof_and_oversized_input() {
    for oversized in [false, true] {
        let mut host = Host::spawn_mode(true);
        host.send("mock_route_start", 1);
        assert_eq!(host.reply()["result"]["routes"][0]["state"], "active");
        if oversized {
            host.child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(&vec![b'x'; 4097])
                .unwrap();
            assert_eq!(host.reply()["error"]["code"], "request_too_large");
        } else {
            host.child.stdin.take();
        }
        host.expect_exit();
    }
}
#[test]
fn unexpected_mock_exit_is_reconciled_before_snapshot() {
    let mut host = Host::spawn_mode(true);
    host.send("mock_route_start", 1);
    assert_eq!(host.reply()["result"]["routes"][0]["state"], "active");
    // Existing test-only child RPC causes a real exit(23), not a fabricated status.
    host.send("mock_source_crash", 10);
    assert!(host.reply().get("error").is_none());
    host.send("get_snapshot", 2);
    assert_eq!(host.reply()["result"]["routes"][0]["state"], "failed");
    host.send("host_status", 3);
    assert_eq!(host.reply()["result"]["state"], "running");
    host.send("host_shutdown", 4);
    host.reply();
    host.expect_exit();
}
#[test]
fn mock_commands_require_opt_in() {
    let mut host = Host::spawn();
    host.send("mock_route_start", 1);
    assert_eq!(host.reply()["error"]["code"], "mock_mode_disabled");
    host.send("get_snapshot", 2);
    assert_eq!(host.reply()["result"]["routes"], json!([]));
}
