//! Long-running state daemon.
//!
//! Listens on a Unix domain socket. Each accepted connection processes
//! one or more length-prefixed JSON requests using the shared
//! [`hermes_state::ops::run_operation`] dispatcher.
//!
//! Wire format (both directions):
//!
//!     [4-byte big-endian length] [JSON body of `length` bytes]
//!
//! Request body:  array of operation objects (same shape the probe reads
//!                from stdin).
//! Response body: object `{"ok": true, "results": [<value>, ...]}` on
//!                success, or `{"ok": false, "error": "<message>"}` on
//!                failure to dispatch the request.
//!
//! Args (positional):  <socket-path> <db-path> [idle-timeout-secs]
//!
//! Idle shutdown: if no client connects for `idle-timeout-secs`
//! (default: 300), the daemon exits cleanly. The Python adapter
//! re-spawns it on the next request.
//!
//! Tracked by bead `hermes-izz.1` (production state boundary). The
//! crate has zero Python coupling — this is a normal Rust binary.

use hermes_state::{ops::run_operation, SessionStore};
use serde_json::{json, Value};
use std::env;
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FRAME_BYTES: u32 = 64 * 1024 * 1024; // 64 MiB safety bound

fn main() {
    if let Err(err) = run() {
        eprintln!("hermes_state_daemon: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let socket_path = args.next().ok_or_else(|| {
        "usage: hermes_state_daemon <socket-path> <db-path> [idle-timeout-secs]".to_string()
    })?;
    let db_path = args
        .next()
        .ok_or_else(|| "missing <db-path> argument".to_string())?;
    let idle_timeout_secs = args
        .next()
        .map(|s| {
            s.parse::<u64>()
                .map_err(|_| format!("invalid idle-timeout-secs: {s}"))
        })
        .transpose()?
        .unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS);

    let socket_path = PathBuf::from(socket_path);
    let db_path = PathBuf::from(db_path);

    // Best-effort cleanup of a stale socket file from a previous crash.
    if socket_path.exists() {
        if let Err(err) = std::fs::remove_file(&socket_path) {
            return Err(format!(
                "could not remove stale socket {}: {err}",
                socket_path.display()
            ));
        }
    }

    let listener = UnixListener::bind(&socket_path)
        .map_err(|err| format!("bind {}: {err}", socket_path.display()))?;
    listener
        .set_nonblocking(false)
        .map_err(|err| err.to_string())?;

    // The daemon owns one SessionStore (one SQLite connection). Multiple
    // Python clients share it through this mutex. SQLite is single-
    // writer anyway, so a mutex on the store is the right serialization
    // point — concurrent reads and writes from many client threads are
    // safe and correct, and the mutex resolves the multi-process WAL
    // contention story that motivated hermes-izz.2.
    let store = Arc::new(Mutex::new(
        SessionStore::open(db_path.clone())
            .map_err(|err| format!("open {}: {err}", db_path.display()))?,
    ));

    let idle_timeout = Duration::from_secs(idle_timeout_secs);
    eprintln!(
        "hermes_state_daemon: listening on {} (db={}, idle={}s)",
        socket_path.display(),
        db_path.display(),
        idle_timeout_secs
    );

    // Set the listener to non-blocking so we can poll with a timeout.
    listener
        .set_nonblocking(true)
        .map_err(|err| err.to_string())?;
    let poll_interval = Duration::from_millis(100);
    let mut idle_elapsed = Duration::ZERO;
    let active_clients = Arc::new(AtomicUsize::new(0));

    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                idle_elapsed = Duration::ZERO;
                // The listener is non-blocking so we can poll for idle
                // shutdown above. Accepted streams inherit that flag on
                // some platforms, so explicitly make the stream blocking
                // — handle_connection uses read_exact and the read/write
                // timeouts to bound stalls.
                if let Err(err) = stream.set_nonblocking(false) {
                    eprintln!("hermes_state_daemon: failed to make stream blocking: {err}");
                    continue;
                }
                let store_handle = Arc::clone(&store);
                let active_handle = Arc::clone(&active_clients);
                active_handle.fetch_add(1, Ordering::SeqCst);
                std::thread::spawn(move || {
                    if let Err(err) = handle_connection(&store_handle, stream) {
                        eprintln!("hermes_state_daemon: connection error: {err}");
                    }
                    active_handle.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(poll_interval);
                // Idle timeout only fires when no clients are actively
                // connected — otherwise a long-lived client like the
                // gateway would never let us shut down.
                if active_clients.load(Ordering::SeqCst) > 0 {
                    idle_elapsed = Duration::ZERO;
                    continue;
                }
                idle_elapsed += poll_interval;
                if idle_elapsed >= idle_timeout {
                    eprintln!(
                        "hermes_state_daemon: idle for {}s; exiting",
                        idle_timeout_secs
                    );
                    let _ = std::fs::remove_file(&socket_path);
                    return Ok(());
                }
            }
            Err(err) => {
                let _ = std::fs::remove_file(&socket_path);
                return Err(format!("accept: {err}"));
            }
        }
    }
}

fn handle_connection(store: &Mutex<SessionStore>, mut stream: UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    loop {
        match read_frame(&mut stream)? {
            None => return Ok(()), // peer closed cleanly
            Some(body) => {
                let response = {
                    let mut guard = store.lock().unwrap_or_else(|e| e.into_inner());
                    process_request(&mut guard, &body)
                };
                write_frame(&mut stream, &response)?;
            }
        }
    }
}

/// Read one length-prefixed frame. Returns `Ok(None)` on a clean EOF
/// before any bytes of the frame have been read, so callers can
/// distinguish "peer closed" from "wire-protocol error".
fn read_frame(stream: &mut UnixStream) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    let length = u32::from_be_bytes(len_buf);
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("frame too large: {length} bytes (max {MAX_FRAME_BYTES})"),
        ));
    }
    let mut body = vec![0u8; length as usize];
    stream.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_frame(stream: &mut UnixStream, body: &[u8]) -> io::Result<()> {
    let length = u32::try_from(body.len()).map_err(|_| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("response too large: {} bytes", body.len()),
        )
    })?;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn process_request(store: &mut SessionStore, body: &[u8]) -> Vec<u8> {
    let value = match parse_request(body) {
        Ok(value) => value,
        Err(err) => return error_frame_bytes(&err),
    };
    let mut results = Vec::with_capacity(value.len());
    for op in value {
        match run_operation(store, op) {
            Ok(result) => results.push(result),
            Err(err) => return error_frame_bytes(&err),
        }
    }
    let response = json!({ "ok": true, "results": results });
    serde_json::to_vec(&response).unwrap_or_else(|err| error_frame_bytes(&err.to_string()))
}

fn parse_request(body: &[u8]) -> Result<Vec<Value>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|err| err.to_string())?;
    match value {
        Value::Array(ops) => Ok(ops),
        _ => Err("request body must be a JSON array of operations".to_string()),
    }
}

fn error_frame_bytes(message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({ "ok": false, "error": message })).unwrap_or_else(|_| {
        // Last-ditch fallback. We can always produce a static byte sequence.
        br#"{"ok":false,"error":"daemon failed to encode error response"}"#.to_vec()
    })
}

#[allow(dead_code)] // imported for future tests against the binary itself
fn _socket_exists(p: &Path) -> bool {
    p.exists()
}
