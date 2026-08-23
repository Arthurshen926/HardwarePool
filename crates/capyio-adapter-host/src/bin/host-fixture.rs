#![forbid(unsafe_code)]

//! Process fixture used only by Adapter Host boundary tests.

use std::io::{self, BufRead, Write};
use std::thread;
use std::time::Duration;

use capyio_adapter_sdk::{
    MAX_NDJSON_LINE_BYTES, ProbeResult, RpcRequest, RpcResponse, decode_request_line,
    encode_response_line,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "normal".to_owned());
    if mode == "oversized-stderr" {
        let mut stderr = io::stderr().lock();
        stderr.write_all(&vec![b'e'; 4_096])?;
        stderr.flush()?;
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().split(b'\n') {
        let mut line = line?;
        line.push(b'\n');
        let request = decode_request_line(&line)?;
        match mode.as_str() {
            "late-first-response" => {
                thread::sleep(Duration::from_millis(250));
                write_probe(&mut stdout, request.id)?;
            }
            "oversized-stdout" => {
                stdout.write_all(&vec![b'x'; MAX_NDJSON_LINE_BYTES + 1])?;
                stdout.flush()?;
                thread::sleep(Duration::from_secs(30));
            }
            "unexpected-id" => write_probe(&mut stdout, request.id.saturating_add(1))?,
            "malformed-response" => {
                stdout.write_all(b"{malformed}\n")?;
                stdout.flush()?;
            }
            "closed-stdout" => {
                return Ok(());
            }
            "normal" | "oversized-stderr" => write_normal(&mut stdout, &request)?,
            _ => return Err(io::Error::other("unknown Host fixture mode").into()),
        }
    }
    Ok(())
}

fn write_probe(stdout: &mut impl Write, id: u64) -> Result<(), Box<dyn std::error::Error>> {
    write_response(
        stdout,
        RpcResponse::success(
            id,
            &ProbeResult {
                ready: true,
                detail: "Host process fixture".to_owned(),
            },
        )?,
    )
}

fn write_normal(
    stdout: &mut impl Write,
    request: &RpcRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    if request.method == "adapter.shutdown" {
        write_response(stdout, RpcResponse::success(request.id, &true)?)
    } else {
        write_probe(stdout, request.id)
    }
}

fn write_response(
    stdout: &mut impl Write,
    response: RpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    stdout.write_all(&encode_response_line(&response)?)?;
    stdout.flush()?;
    Ok(())
}
