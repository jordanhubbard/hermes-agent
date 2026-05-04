//! End-to-end test for hermes_state_daemon.
//!
//! Spawns the daemon binary, connects over UDS, sends real operations,
//! validates responses. Tracked by bead `hermes-izz.1`.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const DAEMON_BIN: &str = env!("CARGO_BIN_EXE_hermes_state_daemon");

struct Daemon {
    child: Child,
    socket: PathBuf,
}

impl Daemon {
    fn spawn(socket: PathBuf, db: PathBuf, idle_secs: u64) -> Self {
        let child = Command::new(DAEMON_BIN)
            .arg(&socket)
            .arg(&db)
            .arg(idle_secs.to_string())
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn daemon");
        // Wait for the socket to appear (daemon binds before printing).
        let deadline = Instant::now() + Duration::from_secs(5);
        while !socket.exists() {
            if Instant::now() > deadline {
                panic!("daemon socket never appeared at {}", socket.display());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Daemon { child, socket }
    }

    fn connect(&self) -> UnixStream {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match UnixStream::connect(&self.socket) {
                Ok(stream) => return stream,
                Err(err) => {
                    if Instant::now() > deadline {
                        panic!("connect: {err}");
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn send_request(stream: &mut UnixStream, ops: &[Value]) -> Value {
    let body = serde_json::to_vec(&Value::Array(ops.to_vec())).unwrap();
    let len = u32::try_from(body.len()).unwrap();
    stream.write_all(&len.to_be_bytes()).unwrap();
    stream.write_all(&body).unwrap();
    stream.flush().unwrap();

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).unwrap();
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; resp_len];
    stream.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn tmp_paths() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    // Keep the socket path short — UDS paths have ~104 byte limits on
    // macOS / 108 on Linux, and `tempdir` under macOS lives under
    // /var/folders which can be tight when combined with a long name.
    let socket = dir.path().join("d.sock");
    let db = dir.path().join("state.db");
    (dir, socket, db)
}

#[test]
fn daemon_handles_schema_version_op() {
    let (_dir, socket, db) = tmp_paths();
    let daemon = Daemon::spawn(socket.clone(), db, 60);
    let mut stream = daemon.connect();
    let resp = send_request(&mut stream, &[json!({"op": "schema_version"})]);
    assert_eq!(resp["ok"], true, "response: {resp}");
    assert!(resp["results"][0].is_number(), "response: {resp}");
}

#[test]
fn daemon_handles_create_session_and_get_session() {
    let (_dir, socket, db) = tmp_paths();
    let daemon = Daemon::spawn(socket.clone(), db, 60);
    let mut stream = daemon.connect();

    let resp = send_request(
        &mut stream,
        &[json!({
            "op": "create_session",
            "id": "s1",
            "source": "cli",
            "model": "gpt-4o",
        })],
    );
    assert_eq!(resp["ok"], true, "{resp}");
    assert_eq!(resp["results"][0], "s1");

    let resp = send_request(&mut stream, &[json!({"op": "get_session", "id": "s1"})]);
    assert_eq!(resp["ok"], true, "{resp}");
    assert_eq!(resp["results"][0]["id"], "s1");
    assert_eq!(resp["results"][0]["source"], "cli");
    assert_eq!(resp["results"][0]["model"], "gpt-4o");
}

#[test]
fn daemon_returns_error_on_unknown_op() {
    let (_dir, socket, db) = tmp_paths();
    let daemon = Daemon::spawn(socket.clone(), db, 60);
    let mut stream = daemon.connect();

    let resp = send_request(&mut stream, &[json!({"op": "nope"})]);
    assert_eq!(resp["ok"], false, "{resp}");
    assert!(resp["error"].as_str().unwrap().contains("nope"));
}

#[test]
fn daemon_serves_multiple_requests_on_one_connection() {
    let (_dir, socket, db) = tmp_paths();
    let daemon = Daemon::spawn(socket.clone(), db, 60);
    let mut stream = daemon.connect();

    for i in 0..5 {
        let id = format!("session-{i}");
        let resp = send_request(
            &mut stream,
            &[json!({"op": "create_session", "id": id, "source": "cli"})],
        );
        assert_eq!(resp["ok"], true, "iter {i}: {resp}");
        assert_eq!(resp["results"][0], id);
    }

    let resp = send_request(&mut stream, &[json!({"op": "session_count"})]);
    assert_eq!(resp["ok"], true, "{resp}");
    assert_eq!(resp["results"][0], 5);
}

#[test]
fn daemon_serves_multiple_clients_serialized() {
    let (_dir, socket, db) = tmp_paths();
    let daemon = Daemon::spawn(socket.clone(), db, 60);

    let mut a = daemon.connect();
    let resp = send_request(
        &mut a,
        &[json!({"op": "create_session", "id": "from-a", "source": "cli"})],
    );
    assert_eq!(resp["ok"], true, "{resp}");
    drop(a);

    let mut b = daemon.connect();
    let resp = send_request(&mut b, &[json!({"op": "get_session", "id": "from-a"})]);
    assert_eq!(resp["ok"], true, "{resp}");
    assert_eq!(resp["results"][0]["id"], "from-a");
}
