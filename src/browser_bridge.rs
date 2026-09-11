use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

pub const BRIDGE_ADDR: &str = "127.0.0.1:47831";
pub const NATIVE_HOST_NAME: &str = "com.brstk.vibe_dictate";

#[derive(Debug, Clone)]
pub struct BrowserDelivery {
    pub target_kind: String,
    pub score: i64,
}

#[derive(Clone)]
pub struct BrowserBridge {
    client: Arc<Mutex<Option<TcpStream>>>,
    next_id: Arc<AtomicU64>,
}

impl BrowserBridge {
    pub fn start() -> Self {
        let client = Arc::new(Mutex::new(None));
        let listener_client = Arc::clone(&client);
        thread::spawn(move || run_listener(listener_client));
        Self {
            client,
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }
    pub fn deliver_text(&self, text: &str) -> Result<BrowserDelivery> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let payload = json!({
            "id": id,
            "action": "deliver_text",
            "text": text,
        });

        let mut guard = self.client.lock().map_err(|_| anyhow!("browser bridge lock poisoned"))?;
        let stream = guard.as_mut().ok_or_else(|| anyhow!("browser extension not connected"))?;
        stream
            .set_read_timeout(Some(Duration::from_millis(1200)))
            .context("set browser bridge read timeout")?;
        stream
            .set_write_timeout(Some(Duration::from_millis(500)))
            .context("set browser bridge write timeout")?;

        let line = serde_json::to_string(&payload).context("encode browser delivery request")?;
        if let Err(error) = writeln!(stream, "{line}").and_then(|_| stream.flush()) {
            *guard = None;
            return Err(error).context("send browser delivery request");
        }

        let cloned = stream.try_clone().context("clone browser bridge stream")?;
        let mut reader = BufReader::new(cloned);
        let mut response_line = String::new();
        if let Err(error) = reader.read_line(&mut response_line) {
            *guard = None;
            return Err(error).context("read browser delivery response");
        }
        if response_line.trim().is_empty() {
            *guard = None;
            return Err(anyhow!("browser bridge closed before response"));
        }
        parse_delivery_response(id, &response_line)
    }

    pub fn connected(&self) -> bool {
        self.client.lock().map(|guard| guard.is_some()).unwrap_or(false)
    }
}
fn run_listener(client: Arc<Mutex<Option<TcpStream>>>) {
    let listener = match TcpListener::bind(BRIDGE_ADDR) {
        Ok(listener) => listener,
        Err(error) => {
            log::warn!("Browser bridge unavailable on {}: {}", BRIDGE_ADDR, error);
            return;
        }
    };
    log::info!("Browser bridge listening on {}", BRIDGE_ADDR);
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer = stream.peer_addr().ok();
                if !peer.map(|addr| addr.ip().is_loopback()).unwrap_or(false) {
                    log::warn!("Rejected non-loopback browser bridge client: {:?}", peer);
                    continue;
                }
                let _ = stream.set_nodelay(true);
                match client.lock() {
                    Ok(mut guard) => {
                        *guard = Some(stream);
                        log::info!("Browser native host connected");
                    }
                    Err(_) => return,
                }
            }
            Err(error) => log::warn!("Browser bridge accept failed: {}", error),
        }
    }
}

fn parse_delivery_response(expected_id: u64, line: &str) -> Result<BrowserDelivery> {
    let value: Value = serde_json::from_str(line).context("parse browser delivery response")?;
    let id = value.get("id").and_then(Value::as_u64).ok_or_else(|| anyhow!("browser response missing id"))?;
    if id != expected_id {
        return Err(anyhow!("browser response id mismatch"));
    }
    if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        let code = value.get("code").and_then(Value::as_str).unwrap_or("browser_delivery_failed");
        return Err(anyhow!(code.to_string()));
    }
    Ok(BrowserDelivery {
        target_kind: value.get("target_kind").and_then(Value::as_str).unwrap_or("editable").to_string(),
        score: value.get("score").and_then(Value::as_i64).unwrap_or_default(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_success_response() {
        let parsed = parse_delivery_response(7, r#"{"id":7,"ok":true,"target_kind":"textarea","score":180}"#).unwrap();
        assert_eq!(parsed.target_kind, "textarea");
        assert_eq!(parsed.score, 180);
    }

    #[test]
    fn rejects_mismatched_response() {
        assert!(parse_delivery_response(7, r#"{"id":8,"ok":true}"#).is_err());
    }
}
