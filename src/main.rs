//! Binary entry point — delegates to the library's run loop. Env: MC_HOST, MC_PORT,
//! MC_USERNAME, STEVE_DATA (registry dir, default `data`). See `app::run`.
#[tokio::main]
async fn main() -> std::io::Result<()> {
    ruststeve::app::run().await
}
