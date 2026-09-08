use hhm_mash_web::{config::Config, flags, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(output) = flags::process_control().map_err(anyhow::Error::msg)? {
        print!("{output}");
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_new(
            flags::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()),
        )?)
        .init();
    let config = Config::from_runtime().map_err(anyhow::Error::msg)?;
    serve(config).await
}
