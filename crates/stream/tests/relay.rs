//! Issue 07c: the sink speaks schema/snapshot-v1.md to a relay, reconnects with the hello and
//! the chunks again, and never blocks or fails the bot when no relay is there.

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use stream::{Body, Frame, Identity, Sink};

fn identity() -> Identity {
    Identity {
        bot: "rs-001".to_owned(),
        run: "local-1".to_owned(),
        hello: Body::Hello {
            commit: "abc".to_owned(),
            mc_version: "26.1.2".to_owned(),
            seed: 7,
            lane: 0,
            start: "spawn".to_owned(),
            stage: String::new(),
            source: "real".to_owned(),
            server: "127.0.0.1:25565".to_owned(),
        },
    }
}

fn self_state(x: f64) -> Body {
    Body::SelfState {
        pos: [x, 64.0, 0.0],
        yaw: 0.0,
        pitch: 0.0,
        dim: "minecraft:overworld".to_owned(),
        on_ground: true,
        health: 20.0,
        food: 20,
        air: -1,
        held: String::new(),
        inv: BTreeMap::new(),
        stage: "in_world".to_owned(),
        tasks: Vec::new(),
        reason: String::new(),
    }
}

/// A relay that accepts `connections` connections in turn and returns every frame decoded,
/// with the path each connection asked for.
// tungstenite's handshake callback type is its own; the large error is its response.
#[allow(clippy::result_large_err)]
fn relay(connections: usize) -> (String, std::sync::mpsc::Receiver<(String, Frame)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for _ in 0..connections {
            let (stream, _) = listener.accept().unwrap();
            let path = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let p = std::sync::Arc::clone(&path);
            let mut ws = tungstenite::accept_hdr(
                stream,
                move |req: &tungstenite::handshake::server::Request, resp| {
                    *p.lock().unwrap() = req.uri().path().to_owned();
                    Ok(resp)
                },
            )
            .unwrap();
            let path = path.lock().unwrap().clone();
            while let Ok(m) = ws.read() {
                match m {
                    tungstenite::Message::Binary(b) => {
                        let f: Frame = rmp_serde::from_slice(&b).unwrap();
                        tx.send((path.clone(), f)).unwrap();
                    }
                    tungstenite::Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    });
    (format!("ws://{addr}"), rx)
}

fn take(rx: &std::sync::mpsc::Receiver<(String, Frame)>, n: usize) -> Vec<(String, Frame)> {
    (0..n)
        .map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect()
}

#[test]
fn frames_are_the_schemas_named_maps_in_order() {
    let (url, rx) = relay(1);
    let sink = Sink::start(&url, identity());
    sink.send(
        20,
        Body::Chunk {
            x: 1,
            z: -2,
            data: vec![1, 2, 3],
        },
    );
    sink.send(21, self_state(0.5));
    sink.send(
        22,
        Body::End {
            ending: "cap".to_owned(),
            killer: String::new(),
            stage_reached: "in_world".to_owned(),
            case: String::new(),
        },
    );
    let got = take(&rx, 4);
    let counts = sink.finish();
    assert_eq!(got[0].0, "/ingest/local-1-rs-001");
    let kinds: Vec<&str> = got.iter().map(|(_, f)| f.kind.as_str()).collect();
    assert_eq!(kinds, ["hello", "chunk", "self", "end"]);
    let seqs: Vec<u64> = got.iter().map(|(_, f)| f.seq).collect();
    assert_eq!(seqs, [0, 1, 2, 3]);
    assert!(
        got.iter()
            .all(|(_, f)| f.v == 1 && f.bot == "rs-001" && f.run == "local-1")
    );
    assert_eq!(
        got[1].1.body,
        Body::Chunk {
            x: 1,
            z: -2,
            data: vec![1, 2, 3]
        }
    );
    assert_eq!(counts.sent, 4);
    assert_eq!(counts.dropped, 0);
}

/// A minimal any-value for decoding a frame as a plain map.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum Any {
    Int(i64),
    Str(String),
}

#[test]
fn a_frame_is_one_flat_map_with_the_envelope() {
    let f = Frame {
        v: 1,
        kind: "block".to_owned(),
        bot: "b".to_owned(),
        run: "r".to_owned(),
        tick: 5,
        seq: 0,
        body: Body::Block {
            x: 1,
            y: 2,
            z: 3,
            state: 9,
        },
    };
    let map: BTreeMap<String, Any> = rmp_serde::from_slice(&f.encode().unwrap()).unwrap();
    let keys: Vec<&str> = map.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "bot", "kind", "run", "seq", "state", "tick", "v", "x", "y", "z"
        ]
    );
    assert!(matches!(map["kind"], Any::Str(ref k) if k == "block"));
    assert!(matches!(map["state"], Any::Int(9)));
}

#[test]
fn no_relay_never_blocks_and_counts_failures() {
    // A port nothing listens on.
    let free = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", free.local_addr().unwrap());
    drop(free);
    let sink = Sink::start(&url, identity());
    let started = Instant::now();
    for i in 0..5000u64 {
        sink.send(i, self_state(i as f64));
    }
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "sending never waits on the socket"
    );
    let counts = sink.finish();
    assert_eq!(counts.sent, 0);
    assert_eq!(counts.connects, 0);
    assert_eq!(counts.failed + counts.dropped, 5000);
}

#[test]
fn a_reconnect_sends_hello_self_and_chunks_again() {
    // The relay takes the first connection's hello and chunk, then hangs up; the second
    // connection is read to its end.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (tx, rx) = channel::<(usize, Frame)>();
    std::thread::spawn(move || {
        for conn in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            let mut n = 0;
            while let Ok(m) = ws.read() {
                if let tungstenite::Message::Binary(b) = m {
                    tx.send((conn, rmp_serde::from_slice(&b).unwrap())).unwrap();
                    n += 1;
                    if conn == 0 && n == 2 {
                        break; // hang up: the socket drops here
                    }
                }
            }
        }
    });
    let sink = Sink::start(&url, identity());
    sink.send(
        1,
        Body::Chunk {
            x: 4,
            z: 5,
            data: vec![9],
        },
    );
    let first: Vec<_> = (0..2)
        .map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect();
    assert_eq!(
        first
            .iter()
            .map(|(_, f)| f.kind.as_str())
            .collect::<Vec<_>>(),
        ["hello", "chunk"]
    );
    // Keep the bot going until the sink has noticed the hang-up and reconnected.
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut second = Vec::new();
    let mut x = 0.0;
    while Instant::now() < deadline && second.len() < 3 {
        x += 1.0;
        sink.send(2, self_state(x));
        std::thread::sleep(Duration::from_millis(50));
        while let Ok((conn, f)) = rx.try_recv() {
            if conn == 1 {
                second.push(f);
            }
        }
    }
    let counts = sink.finish();
    let kinds: Vec<&str> = second.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(&kinds[..3], ["hello", "self", "chunk"], "{kinds:?}");
    assert_eq!(second[0].seq, 0, "seq restarts per connection");
    assert_eq!(
        second[2].body,
        Body::Chunk {
            x: 4,
            z: 5,
            data: vec![9]
        }
    );
    assert_eq!(counts.connects, 2);
    assert!(
        counts.failed >= 1,
        "the frames between hang-up and reconnect: {counts:?}"
    );
}
