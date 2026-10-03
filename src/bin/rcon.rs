//! Tiny RCON CLI over the crate's own client — no ssh round-trip, no mcrcon install.
//!
//!   rcon "execute if block 898 -52 585 minecraft:obsidian" "list"
//!
//! Env: RCON_HOST (localhost), RCON_PORT (25576 = Server B via the ssh tunnel),
//! RCON_PASS (minecraft-test-rcon). Prints one response per command, in order.

use ruststeve::rcon::{RconClient, RconOptions};

#[tokio::main]
async fn main() {
    let cmds: Vec<String> = std::env::args().skip(1).collect();
    if cmds.is_empty() {
        eprintln!("usage: rcon \"<command>\" [\"<command>\" ...]");
        std::process::exit(2);
    }
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let mut c = match RconClient::connect(RconOptions {
        host: env("RCON_HOST", "localhost"),
        port: env("RCON_PORT", "25576").parse().unwrap_or(25576),
        password: env("RCON_PASS", "minecraft-test-rcon"),
        ..Default::default()
    })
    .await
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("rcon connect failed: {e} (tunnel: ssh -fN -L 25576:127.0.0.1:25576 bridger@144.24.32.76)");
            std::process::exit(1);
        }
    };
    for cmd in cmds {
        match c.command(&cmd).await {
            Ok(r) => println!("{}", r.trim_end()),
            Err(e) => println!("ERR {e}"),
        }
    }
}
