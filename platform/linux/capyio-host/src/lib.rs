#![forbid(unsafe_code)]

//! Local, inherited-pipe Linux host management. This is not a network or Adapter API.
//! No hardware, credentials, peers or synthetic authorizations are created.

use std::io::{self, BufRead, Write};

use capyio_core::{NodeDescriptor, NodeId, Platform, ProtocolVersion};
use capyio_runtime::NodeRuntime;
use serde::Deserialize;
use serde_json::{Value, json};

pub const MAX_REQUEST_BYTES: usize = 4096;
pub const MAX_RESPONSE_BYTES: usize = 65_536;
const MAX_NAME_BYTES: usize = 128;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    id: u64,
    command: String,
}

/// Owns one Runtime until explicit shutdown, EOF, or an unrecoverable pipe error.
/// The caller supplies a stable identity; this slice does not persist identities.
pub struct LinuxNodeHost {
    runtime: NodeRuntime,
}

impl LinuxNodeHost {
    pub fn new(node_id: NodeId, display_name: &str) -> io::Result<Self> {
        if node_id.as_uuid().is_nil()
            || display_name.trim().is_empty()
            || display_name.len() > MAX_NAME_BYTES
            || display_name.chars().any(char::is_control)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid Linux Node identity or name",
            ));
        }
        let descriptor = NodeDescriptor::new(
            node_id,
            display_name,
            Platform::Linux,
            "linux (kernel version not probed)",
            env!("CARGO_PKG_VERSION"),
            [ProtocolVersion::new(1, 0)],
        );
        let runtime = NodeRuntime::new(descriptor)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        Ok(Self { runtime })
    }

    /// Sequential bounded NDJSON management. Malformed complete records are
    /// recoverable; oversized records terminate without waiting for a newline.
    /// EOF drops the owned Runtime. stdout backpressure is inherited local IPC,
    /// not a promise of remote-client timeouts or an independently running daemon.
    pub fn serve(self, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
        loop {
            let bytes = match read_record(&mut input)? {
                Record::Eof => return Ok(()),
                Record::Oversized => {
                    write_response(&mut output, error(None, "request_too_large"))?;
                    return Ok(());
                }
                Record::Line(bytes) => bytes,
            };
            // Serde structs also accept positional sequences. This local contract
            // accepts objects only; deserialize the original bytes to retain
            // duplicate-field rejection rather than normalizing through Value.
            if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
                write_response(&mut output, error(None, "invalid_request"))?;
                continue;
            }
            let request = match serde_json::from_slice::<Request>(&bytes) {
                Ok(request) => request,
                Err(_) => {
                    write_response(&mut output, error(None, "invalid_request"))?;
                    continue;
                }
            };
            if request.version != 1 {
                write_response(&mut output, error(Some(request.id), "unsupported_version"))?;
                continue;
            }
            let (result, stop) = match request.command.as_str() {
                "get_snapshot" => (json!(self.runtime.snapshot()), false),
                "host_status" => (
                    json!({
                        "state": "running", "platform": "linux",
                        "node_id": self.runtime.snapshot().local_node.id,
                        "transport": "inherited_stdio", "hardware_enabled": false,
                        "network_enabled": false, "identity_persisted": false
                    }),
                    false,
                ),
                "host_shutdown" => (json!({"state": "stopped"}), true),
                _ => {
                    write_response(&mut output, error(Some(request.id), "unknown_command"))?;
                    continue;
                }
            };
            write_response(
                &mut output,
                json!({"version": 1, "id": request.id, "result": result}),
            )?;
            if stop {
                return Ok(());
            }
        }
    }
}

fn error(id: Option<u64>, code: &str) -> Value {
    json!({"version": 1, "id": id, "error": {"code": code}})
}

fn write_response(output: &mut impl Write, response: Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(&response)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "response_too_large",
        ));
    }
    output.write_all(&bytes)?;
    output.write_all(b"\n")?;
    output.flush()
}

enum Record {
    Eof,
    Line(Vec<u8>),
    Oversized,
}

fn read_record(input: &mut impl BufRead) -> io::Result<Record> {
    let mut line = Vec::with_capacity(MAX_REQUEST_BYTES);
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(if line.is_empty() {
                Record::Eof
            } else {
                Record::Line(line)
            });
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.unwrap_or(available.len());
        if count > MAX_REQUEST_BYTES - line.len() {
            return Ok(Record::Oversized);
        }
        line.extend_from_slice(&available[..count]);
        input.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Record::Line(line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const ID: &str = "00000000-0000-4000-8000-000000000301";
    fn host() -> LinuxNodeHost {
        LinuxNodeHost::new(ID.parse().unwrap(), "Linux test").unwrap()
    }
    fn run(input: &[u8]) -> Vec<Value> {
        let mut out = Vec::new();
        host().serve(Cursor::new(input), &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    #[test]
    fn snapshot_is_linux_and_does_not_invent_resources_or_trust() {
        let replies = run(b"{\"version\":1,\"id\":1,\"command\":\"get_snapshot\"}\n{\"version\":1,\"id\":2,\"command\":\"get_snapshot\"}\n");
        let snapshot = &replies[0]["result"];
        assert_eq!(snapshot, &replies[1]["result"]);
        assert_eq!(snapshot["local_node"]["id"], ID);
        assert_eq!(snapshot["local_node"]["platform"], "linux");
        assert_eq!(snapshot["local_node"]["capabilities"], json!({}));
        assert_eq!(snapshot["local_node"]["adapter_instances"], json!({}));
        for key in ["peers", "sessions", "routes", "events"] {
            assert_eq!(snapshot[key], json!([]));
        }
    }
    #[test]
    fn strict_schema_and_errors_recover_before_shutdown() {
        let replies = run(b"bad\n{\"version\":1,\"id\":1,\"command\":\"host_status\",\"extra\":true}\n{\"version\":2,\"id\":2,\"command\":\"host_status\"}\n{\"version\":1,\"id\":3,\"command\":\"exec\"}\n{\"version\":1,\"id\":4,\"command\":\"host_shutdown\"}\n{\"version\":1,\"id\":5,\"command\":\"get_snapshot\"}\n");
        assert_eq!(replies.len(), 5);
        for (reply, code) in replies.iter().zip([
            "invalid_request",
            "invalid_request",
            "unsupported_version",
            "unknown_command",
        ]) {
            assert_eq!(reply["error"]["code"], code);
        }
        assert_eq!(replies[4]["result"]["state"], "stopped");
    }
    #[test]
    fn positional_shutdown_and_duplicate_fields_are_rejected_without_stopping() {
        let replies = run(b"[1,9,\"host_shutdown\"]\n{\"version\":1,\"id\":1,\"id\":2,\"command\":\"host_shutdown\"}\n{\"version\":1,\"id\":10,\"command\":\"host_status\"}\n");
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[0]["error"]["code"], "invalid_request");
        assert_eq!(replies[1]["error"]["code"], "invalid_request");
        assert_eq!(replies[2]["result"]["state"], "running");
    }

    #[test]
    fn records_and_responses_are_bounded() {
        assert!(matches!(
            read_record(&mut Cursor::new(vec![b' '; MAX_REQUEST_BYTES])).unwrap(),
            Record::Line(_)
        ));
        assert!(matches!(
            read_record(&mut Cursor::new(vec![b' '; MAX_REQUEST_BYTES + 1])).unwrap(),
            Record::Oversized
        ));
        let replies = run(&vec![b'x'; MAX_REQUEST_BYTES + 1]);
        assert_eq!(replies[0]["error"]["code"], "request_too_large");
        let mut output = Vec::new();
        assert!(write_response(&mut output, json!("x".repeat(MAX_RESPONSE_BYTES))).is_err());
        assert!(output.is_empty());
    }
    #[test]
    fn eof_and_invalid_utf8_do_not_panic() {
        assert!(run(b"").is_empty());
        assert_eq!(run(&[255])[0]["error"]["code"], "invalid_request");
        assert_eq!(
            run(b"{\"version\":1,\"id\":1,\"command\":\"host_status\"}")[0]["result"]["state"],
            "running"
        );
    }
    #[test]
    fn validates_identity_and_name() {
        assert!(
            LinuxNodeHost::new(
                "00000000-0000-0000-0000-000000000000".parse().unwrap(),
                "host"
            )
            .is_err()
        );
        for name in [
            "".to_owned(),
            " ".to_owned(),
            "bad\nname".to_owned(),
            "x".repeat(129),
        ] {
            assert!(LinuxNodeHost::new(ID.parse().unwrap(), &name).is_err());
        }
    }
}
