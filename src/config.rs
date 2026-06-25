use anyhow::{anyhow, Context, Result};
use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_user: String,
    pub imap_password: String,
    pub imap_folder: String,
    pub folders: Vec<String>,
    pub webhook_url: String,
    pub webhook_secret: String,
    pub github_event: String,
    pub idle_timeout_seconds: u64,
    pub reconnect_delay_seconds: u64,
    pub mark_seen: bool,
    pub startup_notification: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let missing: Vec<&str> = REQUIRED_ENV
            .iter()
            .copied()
            .filter(|key| {
                env::var(key)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            })
            .collect();

        if !missing.is_empty() {
            return Err(anyhow!(
                "Missing required environment variables: {}",
                missing.join(", ")
            ));
        }

        let fallback_folder = optional_env("IMAP_FOLDER", "INBOX");
        let folders = configured_folders(&fallback_folder);
        let imap_folder = folders[0].clone();

        Ok(Self {
            imap_host: required_env("IMAP_HOST")?,
            imap_port: optional_env("IMAP_PORT", "993")
                .parse()
                .context("IMAP_PORT must be an integer")?,
            imap_user: required_env("IMAP_USER")?,
            imap_password: required_env("IMAP_PASSWORD")?,
            imap_folder,
            folders,
            webhook_url: required_env("WEBHOOK_URL")?,
            webhook_secret: required_env("WEBHOOK_SECRET")?,
            github_event: optional_env("GITHUB_EVENT", "email.received"),
            idle_timeout_seconds: optional_env("IDLE_TIMEOUT_SECONDS", "1740")
                .parse()
                .context("IDLE_TIMEOUT_SECONDS must be an integer")?,
            reconnect_delay_seconds: optional_env("RECONNECT_DELAY_SECONDS", "10")
                .parse()
                .context("RECONNECT_DELAY_SECONDS must be an integer")?,
            mark_seen: parse_bool(&optional_env("MARK_SEEN", "false")),
            startup_notification: parse_bool(&optional_env("STARTUP_NOTIFICATION", "false")),
        })
    }
}

const REQUIRED_ENV: &[&str] = &[
    "IMAP_HOST",
    "IMAP_USER",
    "IMAP_PASSWORD",
    "WEBHOOK_URL",
    "WEBHOOK_SECRET",
];

fn required_env(key: &str) -> Result<String> {
    env::var(key).with_context(|| format!("{key} is required"))
}

fn optional_env(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn configured_folders(fallback_folder: &str) -> Vec<String> {
    let folders = env::var("IMAP_FOLDERS")
        .ok()
        .map(|value| parse_folder_list(&value))
        .unwrap_or_default();

    if folders.is_empty() {
        vec![fallback_folder.to_owned()]
    } else {
        folders
    }
}

fn parse_folder_list(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|folder| !folder.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}
