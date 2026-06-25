use anyhow::Result;
use imap_idle_webhook::config::Config;
use imap_idle_webhook::listener::run_forever;
use imap_idle_webhook::startup::{log_config_summary, send_startup_notification};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cfg = Config::from_env()?;
    log_config_summary(&cfg);
    send_startup_notification(&cfg).await?;
    run_forever(&cfg).await
}
