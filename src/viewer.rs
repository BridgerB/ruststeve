//! Optional live 3D-viewer feed (enabled by the `RUST_VIEW` env var).
//!
//! Each bot stands up a tiny hand-rolled SSE server (over a tokio TCP listener) that
//! streams its world to steve's ported Babylon `viewer.js` running in the dashboard:
//! `init` + `assets` + `chunk` dumps + `position`/`time`. The bot fills a shared
//! snapshot each throttled tick (see `Bot::update_viewer`); the server drains it to
//! any connected browser. Reusing steve's viewer means we render the SAME textured
//! first-person world — the chunk buffers are `ChunkColumn::dump(true)`, the exact
//! format the viewer's `loadChunkColumn` decodes, and the asset pack is dumped once
//! from steve (both run MC 26.1.2, so block-state ids line up).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The bot-populated snapshot the SSE server streams. Holds only owned data (dumped
/// chunk bytes + pose), so `Arc<Mutex<ViewerShared>>` is `'static + Send` and can be
/// shared with a spawned task even though `Bot` itself is lifetime-bound.
#[derive(Default)]
pub struct ViewerShared {
    pub version: String,
    pub min_y: i32,
    pub height: i32,
    /// (x, y, z, yaw, pitch)
    pub pose: (f64, f64, f64, f64, f64),
    pub time: i64,
    /// Dumped chunk columns keyed by (chunk_x, chunk_z).
    pub chunks: HashMap<(i32, i32), Vec<u8>>,
    /// Bumped whenever `chunks` gains an entry, so open connections know to send more.
    pub seq: u64,
}

pub type ViewerHandle = Arc<Mutex<ViewerShared>>;

/// Stable per-bot port from the trailing digits of the username: rust-race-003 → 4603,
/// a name with no digits → 4600. The dashboard mirrors this to know where each bot streams.
pub fn port_for(username: &str) -> u16 {
    let digits: String = username.chars().filter(|c| c.is_ascii_digit()).collect();
    4600 + digits.parse::<u16>().unwrap_or(0) % 100
}

/// Spawn the SSE server for one bot on a background tokio task.
pub fn spawn_server(port: u16, shared: ViewerHandle, assets_path: String) {
    tokio::spawn(async move {
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[viewer] bind :{port} failed: {e}");
                return;
            }
        };
        // The assets message is a single pre-serialized `{"type":"assets",...}` line.
        let assets = Arc::new(std::fs::read_to_string(&assets_path).unwrap_or_default());
        if assets.is_empty() {
            eprintln!("[viewer] WARNING: no assets at {assets_path} — viewer will be blank");
        }
        eprintln!("[viewer] streaming on http://127.0.0.1:{port}");
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let shared = shared.clone();
                    let assets = assets.clone();
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, shared, assets).await;
                    });
                }
                Err(_) => continue,
            }
        }
    });
}

async fn send_event(stream: &mut TcpStream, data: &str) -> std::io::Result<()> {
    stream.write_all(b"data: ").await?;
    stream.write_all(data.as_bytes()).await?;
    stream.write_all(b"\n\n").await
}

fn pose_msg(p: (f64, f64, f64, f64, f64)) -> String {
    format!(
        r#"{{"type":"position","x":{},"y":{},"z":{},"yaw":{},"pitch":{}}}"#,
        p.0, p.1, p.2, p.3, p.4
    )
}

fn chunk_msg(key: (i32, i32), bytes: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!(r#"{{"type":"chunk","x":{},"z":{},"buf":"{}"}}"#, key.0, key.1, b64)
}

async fn serve_connection(
    mut stream: TcpStream,
    shared: ViewerHandle,
    assets: Arc<String>,
) -> std::io::Result<()> {
    // Drain the HTTP request line + headers (we serve the same SSE for any path).
    let mut buf = [0u8; 2048];
    let _ = stream.read(&mut buf).await;

    let head = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream\r\n\
        Cache-Control: no-cache\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Connection: keep-alive\r\n\r\n";
    stream.write_all(head.as_bytes()).await?;

    // Replay: init → assets → position → time → every loaded chunk. Matches the order
    // steve's server replays on each (re)connect (the client resets state on open).
    let (init, pose, time, mut snapshot, mut last_seq) = {
        let s = shared.lock().unwrap();
        let init = format!(
            r#"{{"type":"init","version":"{}","minY":{},"height":{}}}"#,
            s.version, s.min_y, s.height
        );
        let snapshot: Vec<((i32, i32), Vec<u8>)> =
            s.chunks.iter().map(|(k, v)| (*k, v.clone())).collect();
        (init, s.pose, s.time, snapshot, s.seq)
    };
    send_event(&mut stream, &init).await?;
    if !assets.is_empty() {
        send_event(&mut stream, &assets).await?;
    }
    send_event(&mut stream, &pose_msg(pose)).await?;
    send_event(&mut stream, &format!(r#"{{"type":"time","time":{time}}}"#)).await?;

    let mut sent: HashSet<(i32, i32)> = HashSet::new();
    for (key, bytes) in snapshot.drain(..) {
        send_event(&mut stream, &chunk_msg(key, &bytes)).await?;
        sent.insert(key);
    }

    // Live loop: stream POSE at ~30 Hz so the camera is smooth (the bot refreshes pose every
    // tick). Chunk-scan + time are far cheaper to send occasionally, so only every ~1s.
    let mut i: u32 = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(33)).await;
        i = i.wrapping_add(1);
        let slow = i % 30 == 0; // ~1s: chunks + time
        let (pose, time, new_chunks) = {
            let s = shared.lock().unwrap();
            let new_chunks: Vec<((i32, i32), Vec<u8>)> = if slow && s.seq != last_seq {
                s.chunks
                    .iter()
                    .filter(|(k, _)| !sent.contains(*k))
                    .map(|(k, v)| (*k, v.clone()))
                    .collect()
            } else {
                Vec::new()
            };
            if slow {
                last_seq = s.seq;
            }
            (s.pose, s.time, new_chunks)
        };
        send_event(&mut stream, &pose_msg(pose)).await?;
        if slow {
            send_event(&mut stream, &format!(r#"{{"type":"time","time":{time}}}"#)).await?;
        }
        for (key, bytes) in new_chunks {
            send_event(&mut stream, &chunk_msg(key, &bytes)).await?;
            sent.insert(key);
        }
    }
}
