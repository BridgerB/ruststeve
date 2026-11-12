//! relay-stub: a stand-in for steve's local relay, for checking the stream without steve. It
//! accepts bots at `/ingest/<bot-id>`, decodes every frame against schema/snapshot-v1.md and
//! prints one line per frame kind change plus a tally when a bot disconnects.
//!
//! ```text
//! cargo run -p stream --bin relay-stub -- [--port 8787]
//! STEVE_RELAY=ws://localhost:8787 cargo run -p bot -- --seconds 30
//! ```

use std::collections::BTreeMap;
use std::net::TcpListener;

use stream::Frame;

fn main() {
    let port = std::env::args()
        .skip_while(|a| a != "--port")
        .nth(1)
        .unwrap_or_else(|| "8787".to_owned());
    let listener = match TcpListener::bind(format!("127.0.0.1:{port}")) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("relay-stub: bind {port}: {e}");
            std::process::exit(1);
        }
    };
    println!("relay-stub: listening on ws://localhost:{port}");
    for conn in listener.incoming().flatten() {
        std::thread::spawn(move || {
            let Ok(mut ws) = tungstenite::accept(conn) else {
                return;
            };
            let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
            let mut who = String::new();
            let mut last_seq = None;
            let mut bad = 0u64;
            while let Ok(m) = ws.read() {
                let tungstenite::Message::Binary(b) = m else {
                    continue;
                };
                match rmp_serde::from_slice::<Frame>(&b) {
                    Ok(f) => {
                        if f.kind == "hello" {
                            who = format!("{}-{}", f.run, f.bot);
                            println!("relay-stub: hello from {who}: {:?}", f.body);
                        }
                        if f.kind == "end" {
                            println!("relay-stub: end from {who}: {:?}", f.body);
                        }
                        if last_seq.is_some_and(|s: u64| f.seq != s + 1) {
                            bad += 1;
                        }
                        last_seq = Some(f.seq);
                        *kinds.entry(f.kind).or_default() += 1;
                    }
                    Err(e) => {
                        bad += 1;
                        eprintln!("relay-stub: a frame that is not v1: {e}");
                    }
                }
            }
            println!("relay-stub: {who} closed; frames by kind {kinds:?}; bad {bad}");
        });
    }
}
