use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::browser_bridge::BRIDGE_ADDR;

const MAX_NATIVE_MESSAGE: usize = 1024 * 1024;

pub fn run_native_host() -> Result<()> {
    let stream = connect_bridge()?;
    let reader_stream = stream.try_clone().context("clone bridge stream")?;
    let writer_stream = Arc::new(Mutex::new(stream));

    let stdout = Arc::new(Mutex::new(io::stdout()));
    let stdout_for_bridge = Arc::clone(&stdout);
    thread::spawn(move || relay_bridge_to_chrome(reader_stream, stdout_for_bridge));

    relay_chrome_to_bridge(writer_stream)
}

fn connect_bridge() -> Result<TcpStream> {
    let mut last_error = None;
    for _ in 0..20 {
        match TcpStream::connect(BRIDGE_ADDR) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(anyhow!("cannot connect to Vibe Dictate browser bridge: {:?}", last_error))
}
fn relay_bridge_to_chrome(stream: TcpStream, stdout: Arc<Mutex<io::Stdout>>) {
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let payload = line.trim_end().as_bytes();
                if payload.is_empty() || payload.len() > MAX_NATIVE_MESSAGE {
                    continue;
                }
                if let Ok(mut out) = stdout.lock() {
                    if write_native_message(&mut *out, payload).is_err() {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }
    std::process::exit(0);
}

fn relay_chrome_to_bridge(stream: Arc<Mutex<TcpStream>>) -> Result<()> {
    let mut input = io::stdin();
    loop {
        let payload = match read_native_message(&mut input)? {
            Some(payload) => payload,
            None => return Ok(()),
        };
        let mut bridge = stream.lock().map_err(|_| anyhow!("bridge write lock poisoned"))?;
        bridge.write_all(&payload).context("write browser response to bridge")?;
        bridge.write_all(b"\n").context("terminate browser response line")?;
        bridge.flush().context("flush browser response")?;
    }
}
fn read_native_message<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error).context("read native message length"),
    }
    let length = u32::from_le_bytes(header) as usize;
    if length == 0 || length > MAX_NATIVE_MESSAGE {
        return Err(anyhow!("invalid native message length: {length}"));
    }
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload).context("read native message body")?;
    Ok(Some(payload))
}

fn write_native_message<W: Write>(writer: &mut W, payload: &[u8]) -> Result<()> {
    if payload.is_empty() || payload.len() > MAX_NATIVE_MESSAGE {
        return Err(anyhow!("invalid native message size"));
    }
    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .context("write native message length")?;
    writer.write_all(payload).context("write native message body")?;
    writer.flush().context("flush native message")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_message_round_trip() {
        let payload = br#"{"id":1,"ok":true}"#;
        let mut buffer = Vec::new();
        write_native_message(&mut buffer, payload).unwrap();
        let mut cursor = std::io::Cursor::new(buffer);
        assert_eq!(read_native_message(&mut cursor).unwrap().unwrap(), payload);
    }
}
