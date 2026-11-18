//! Send commands to the race's own server over RCON, one argument each, printing each reply:
//! for probes on a runner (issue 15: `locate` a fortress on the race seed to place the `nether`
//! start). The password is the run's own (`RUSTSTEVE_RCON_PASSWORD`); never a shared server.
//!
//! ```text
//! cargo run -q -p rcon -- [--addr 127.0.0.1:25575] "<command>" ...
//! ```

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut addr = "127.0.0.1:25575".to_owned();
    if args.first().is_some_and(|a| a == "--addr") && args.len() > 1 {
        addr = args.remove(1);
        args.remove(0);
    }
    let Ok(password) = std::env::var("RUSTSTEVE_RCON_PASSWORD") else {
        eprintln!("rcon: RUSTSTEVE_RCON_PASSWORD is not set");
        return ExitCode::FAILURE;
    };
    let mut rcon = match rcon::Rcon::connect(&addr, &password) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("rcon: {e}");
            return ExitCode::FAILURE;
        }
    };
    for command in &args {
        match rcon.command(command) {
            Ok(reply) => println!("> {command}\n{reply}"),
            Err(e) => {
                eprintln!("rcon: {command}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
