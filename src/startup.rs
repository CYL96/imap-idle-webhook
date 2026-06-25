use crate::config::Config;
use crate::email_parse::ParsedEmail;
use crate::webhook::post_signed_webhook;
use anyhow::Result;
use log::{info, warn};
use std::future::Future;
use std::time::Duration;

pub fn log_config_summary(cfg: &Config) {
    info!(
        "startup config host={} account={} folder_count={} folders={} idle_timeout_seconds={} mark_seen={} github_event={}",
        cfg.imap_host,
        cfg.imap_user,
        cfg.folders.len(),
        cfg.folders.join(" | "),
        cfg.idle_timeout_seconds,
        cfg.mark_seen,
        cfg.github_event
    );
}

pub async fn send_startup_notification(cfg: &Config) -> Result<()> {
    let webhook_url = cfg.webhook_url.clone();
    let webhook_secret = cfg.webhook_secret.clone();
    let github_event = cfg.github_event.clone();
    send_startup_notification_with(cfg, move |marker| async move {
        post_signed_webhook(
            &webhook_url,
            &marker,
            &webhook_secret,
            &github_event,
            Duration::from_secs(20),
        )
        .await
        .map(|_| ())
    })
    .await
}

pub async fn send_startup_notification_with<F, Fut>(cfg: &Config, send: F) -> Result<()>
where
    F: FnOnce(ParsedEmail) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    if !cfg.startup_notification {
        return Ok(());
    }

    let marker = build_startup_marker(cfg);
    if let Err(err) = send(marker).await {
        warn!("startup notification webhook failed; continuing listener startup: {err:#}");
    }

    Ok(())
}

pub fn build_startup_marker(cfg: &Config) -> ParsedEmail {
    ParsedEmail {
        event: cfg.github_event.clone(),
        account: cfg.imap_user.clone(),
        folder: "startup".to_owned(),
        uid: 0,
        message_id: None,
        from: Some("imap-idle-webhook <startup@localhost>".to_owned()),
        to: vec![cfg.imap_user.clone()],
        cc: Vec::new(),
        subject: Some("imap-idle-webhook started".to_owned()),
        date: None,
        text: Some(format!(
            "imap-idle-webhook started\nfolder_count: {}\nfolders: {}\nidle_timeout_seconds: {}",
            cfg.folders.len(),
            cfg.folders.join(" | "),
            cfg.idle_timeout_seconds
        )),
        raw_rfc822_base64: String::new(),
    }
}
