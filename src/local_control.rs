//! Local-only dictation lifecycle control for trusted desktop integrations.
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver};
use serde::{Deserialize, Serialize};

pub const CONTROL_ADDR: &str = "127.0.0.1:47832";
const MAX_REQUEST_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlCommand {
    StopDictation { submit: bool },
}

#[derive(Debug)]
pub struct ControlRequest {
    pub command: ControlCommand,
    pub respond_to: Sender<ControlResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlResponse {
    pub ok: bool,
    pub status: String,
}

impl ControlResponse {
    pub fn ok(status: impl Into<String>) -> Self {
        Self {
            ok: true,
            status: status.into(),
        }
    }

    pub fn error(status: impl Into<String>) -> Self {
        Self {
            ok: false,
            status: status.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct WireRequest {
    command: String,
    #[serde(default)]
    submit: Option<bool>,
}

fn parse_request(line: &str) -> Result<ControlCommand, String> {
    let request: WireRequest =
        serde_json::from_str(line).map_err(|e| format!("invalid_json: {e}"))?;
    match request.command.as_str() {
        "dictation.stop" => Ok(ControlCommand::StopDictation {
            submit: request.submit.unwrap_or(true),
        }),
        other => Err(format!("unsupported_command: {other}")),
    }
}

fn write_response(stream: &mut TcpStream, response: &ControlResponse) {
    if let Ok(mut encoded) = serde_json::to_vec(response) {
        encoded.push(b'\n');
        let _ = stream.write_all(&encoded);
        let _ = stream.flush();
    }
}

fn handle_connection(mut stream: TcpStream, tx: crossbeam_channel::Sender<ControlRequest>) {
    let peer = stream.peer_addr().ok();
    if !peer.map(|addr| addr.ip().is_loopback()).unwrap_or(false) {
        write_response(
            &mut stream,
            &ControlResponse::error("non_loopback_rejected"),
        );
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let reader_stream = match stream.try_clone() {
        Ok(value) => value,
        Err(_) => return,
    };
    let mut reader = BufReader::new(reader_stream);
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => write_response(&mut stream, &ControlResponse::error("empty_request")),
        Ok(n) if n > MAX_REQUEST_BYTES => {
            write_response(&mut stream, &ControlResponse::error("request_too_large"))
        }
        Ok(_) => match parse_request(line.trim()) {
            Ok(command) => {
                let (response_tx, response_rx) = channel();
                if tx
                    .send(ControlRequest {
                        command,
                        respond_to: response_tx,
                    })
                    .is_err()
                {
                    write_response(
                        &mut stream,
                        &ControlResponse::error("control_loop_unavailable"),
                    );
                    return;
                }
                match response_rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(response) => write_response(&mut stream, &response),
                    Err(_) => {
                        write_response(&mut stream, &ControlResponse::error("control_timeout"))
                    }
                }
            }
            Err(error) => write_response(&mut stream, &ControlResponse::error(error)),
        },
        Err(error) => write_response(
            &mut stream,
            &ControlResponse::error(format!("read_failed: {error}")),
        ),
    }
}

pub fn start() -> Receiver<ControlRequest> {
    let (tx, rx) = unbounded();
    thread::spawn(move || {
        let listener = match TcpListener::bind(CONTROL_ADDR) {
            Ok(listener) => listener,
            Err(error) => {
                log::warn!("Local control unavailable on {}: {}", CONTROL_ADDR, error);
                return;
            }
        };
        log::info!("Local control listening on {}", CONTROL_ADDR);
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => handle_connection(stream, tx.clone()),
                Err(error) => log::warn!("Local control accept failed: {}", error),
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_general_stop_contract() {
        assert_eq!(
            parse_request(r#"{"command":"dictation.stop","submit":false}"#).unwrap(),
            ControlCommand::StopDictation { submit: false }
        );
        assert_eq!(
            parse_request(r#"{"command":"dictation.stop"}"#).unwrap(),
            ControlCommand::StopDictation { submit: true }
        );
    }

    #[test]
    fn rejects_unrelated_commands() {
        assert!(parse_request(r#"{"command":"shell.exec"}"#).is_err());
    }
}
