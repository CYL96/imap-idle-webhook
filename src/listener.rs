use crate::config::Config;
use crate::email_parse::parse_message;
use crate::webhook::post_signed_webhook;
use anyhow::{anyhow, bail, Context, Result};
use log::{info, warn};
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::ServerName;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::task::JoinSet;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderWorkerSpec {
    pub folder: String,
}

pub fn folder_worker_specs(cfg: &Config) -> Vec<FolderWorkerSpec> {
    cfg.folders
        .iter()
        .map(|folder| FolderWorkerSpec {
            folder: folder.clone(),
        })
        .collect()
}

pub async fn run_forever(cfg: &Config) -> Result<()> {
    let mut workers = JoinSet::new();

    for spec in folder_worker_specs(cfg) {
        let worker_cfg = cfg.clone();
        workers.spawn(async move { run_folder_worker(worker_cfg, spec.folder).await });
    }

    while let Some(result) = workers.join_next().await {
        result.context("folder worker task failed")??;
    }

    Ok(())
}

async fn run_folder_worker(cfg: Config, folder: String) -> Result<()> {
    info!("starting folder worker folder={}", folder);
    loop {
        if let Err(err) = run_folder_connection(&cfg, &folder).await {
            warn!(
                "folder worker crashed folder={} reconnecting in {}s: {err:#}",
                folder, cfg.reconnect_delay_seconds
            );
            sleep(Duration::from_secs(cfg.reconnect_delay_seconds)).await;
        }
    }
}

async fn run_folder_connection(cfg: &Config, folder: &str) -> Result<()> {
    let mut client = ImapConnection::connect(&cfg.imap_host, cfg.imap_port).await?;
    client.login(&cfg.imap_user, &cfg.imap_password).await?;
    client.select(folder).await?;
    info!("connected to {} folder={}", cfg.imap_host, folder);

    let unseen = client.search_unseen().await?;
    info!("search unseen folder={} uids={:?}", folder, unseen);
    fetch_and_send(&mut client, cfg, folder, &unseen).await?;

    loop {
        info!("entering IDLE folder={}", folder);
        let idle_outcome = client
            .idle(Duration::from_secs(cfg.idle_timeout_seconds))
            .await?;
        for line in idle_outcome
            .responses
            .iter()
            .filter(|line| line.contains("EXISTS") || line.contains("RECENT"))
        {
            info!("idle response folder={} line={:?}", folder, line);
        }
        info!(
            "idle done folder={} reason={}",
            folder,
            idle_outcome.reason.as_str()
        );

        let unseen = client.search_unseen().await?;
        info!("search unseen folder={} uids={:?}", folder, unseen);
        fetch_and_send(&mut client, cfg, folder, &unseen).await?;
    }
}

async fn fetch_and_send(
    client: &mut ImapConnection,
    cfg: &Config,
    folder: &str,
    uids: &[u32],
) -> Result<()> {
    let unique_uids: Vec<u32> = uids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for uid in unique_uids {
        let raw = client.fetch_raw_message(uid).await?;
        let payload = parse_message(&raw, &cfg.imap_user, folder, uid)?;
        info!(
            "posting email folder={} uid={} subject={:?}",
            folder, uid, payload.subject
        );
        post_signed_webhook(
            &cfg.webhook_url,
            &payload,
            &cfg.webhook_secret,
            &cfg.github_event,
            Duration::from_secs(20),
        )
        .await?;
        if cfg.mark_seen {
            client.mark_seen(uid).await?;
        }
    }
    Ok(())
}

struct ImapConnection {
    reader: BufReader<TlsStream<TcpStream>>,
    tag_counter: u32,
}

impl ImapConnection {
    async fn connect(host: &str, port: u16) -> Result<Self> {
        let tcp = TcpStream::connect((host, port))
            .await
            .with_context(|| format!("failed to connect to {host}:{port}"))?;
        let tls = TlsConnector::from(Arc::new(tls_config()?))
            .connect(tls_server_name(host)?, tcp)
            .await?;
        let mut connection = Self {
            reader: BufReader::new(tls),
            tag_counter: 0,
        };
        connection.read_greeting().await?;
        Ok(connection)
    }

    async fn login(&mut self, user: &str, password: &str) -> Result<()> {
        self.command(&format!(
            "LOGIN {} {}",
            quote_imap_string(user),
            quote_imap_string(password)
        ))
        .await?;
        Ok(())
    }

    async fn select(&mut self, folder: &str) -> Result<()> {
        self.command(&select_folder_command(folder)).await?;
        Ok(())
    }

    async fn search_unseen(&mut self) -> Result<Vec<u32>> {
        let lines = self.command("UID SEARCH UNSEEN").await?;
        Ok(lines
            .iter()
            .find_map(|line| line.strip_prefix("* SEARCH "))
            .map(|uids| {
                uids.split_whitespace()
                    .filter_map(|value| value.parse::<u32>().ok())
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn fetch_raw_message(&mut self, uid: u32) -> Result<Vec<u8>> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} {}", fetch_message_command(uid)))
            .await?;

        let mut raw = Vec::new();
        loop {
            let line = self.read_line().await?;
            if let Some(byte_count) = literal_size(&line) {
                let mut bytes = vec![0; byte_count];
                self.reader.read_exact(&mut bytes).await?;
                raw = bytes;
                let _ = self.read_line().await?;
                continue;
            }
            if is_tagged_completion(&line, &tag) {
                ensure_ok(&line)?;
                break;
            }
        }

        if raw.is_empty() {
            bail!("UID FETCH {uid} returned no message body");
        }
        Ok(raw)
    }

    async fn mark_seen(&mut self, uid: u32) -> Result<()> {
        self.command(&format!("UID STORE {uid} +FLAGS.SILENT (\\Seen)"))
            .await?;
        Ok(())
    }

    async fn idle(&mut self, idle_timeout: Duration) -> Result<IdleOutcome> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} IDLE")).await?;
        let continuation = self.read_line().await?;
        if !continuation.starts_with('+') {
            bail!("server rejected IDLE: {continuation}");
        }

        let mut responses = Vec::new();
        let mut reason = IdleDoneReason::Timeout;
        loop {
            match timeout(idle_timeout, self.read_line()).await {
                Ok(Ok(line)) => {
                    let has_exists = line.contains("EXISTS");
                    responses.push(line);
                    if has_exists {
                        reason = IdleDoneReason::Exists;
                        break;
                    }
                }
                Ok(Err(err)) => return Err(err),
                Err(_) => {
                    break;
                }
            }
        }

        self.write_line("DONE").await?;
        loop {
            let line = self.read_line().await?;
            if is_tagged_completion(&line, &tag) {
                ensure_ok(&line)?;
                break;
            }
            responses.push(line);
        }
        Ok(IdleOutcome { reason, responses })
    }

    async fn command(&mut self, command: &str) -> Result<Vec<String>> {
        let tag = self.next_tag();
        self.write_line(&format!("{tag} {command}")).await?;

        let mut lines = Vec::new();
        loop {
            let line = self.read_line().await?;
            let done = is_tagged_completion(&line, &tag);
            if done {
                ensure_ok(&line)?;
                break;
            }
            lines.push(line);
        }
        Ok(lines)
    }

    async fn read_greeting(&mut self) -> Result<()> {
        let greeting = self.read_line().await?;
        if !greeting.starts_with("* OK") && !greeting.starts_with("* PREAUTH") {
            bail!("unexpected IMAP greeting: {greeting}");
        }
        Ok(())
    }

    fn next_tag(&mut self) -> String {
        self.tag_counter += 1;
        format!("A{:04}", self.tag_counter)
    }

    async fn write_line(&mut self, line: &str) -> Result<()> {
        let stream = self.reader.get_mut();
        stream.write_all(line.as_bytes()).await?;
        stream.write_all(b"\r\n").await?;
        stream.flush().await?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<String> {
        let mut line = String::new();
        let bytes = self.reader.read_line(&mut line).await?;
        if bytes == 0 {
            bail!("IMAP connection closed");
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdleOutcome {
    reason: IdleDoneReason,
    responses: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleDoneReason {
    Exists,
    Timeout,
}

impl IdleDoneReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exists => "exists",
            Self::Timeout => "timeout",
        }
    }
}

fn quote_imap_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn select_folder_command(folder: &str) -> String {
    format!("SELECT {}", quote_imap_string(folder))
}

fn is_tagged_completion(line: &str, tag: &str) -> bool {
    line.starts_with(tag) && line[tag.len()..].starts_with(' ')
}

fn ensure_ok(line: &str) -> Result<()> {
    if line.contains(" OK") {
        Ok(())
    } else {
        Err(anyhow!("IMAP command failed: {line}"))
    }
}

fn literal_size(line: &str) -> Option<usize> {
    let start = line.rfind('{')? + 1;
    let end = line[start..].find('}')? + start;
    line[start..end].parse().ok()
}

fn fetch_message_command(uid: u32) -> String {
    format!("UID FETCH {uid} (BODY.PEEK[])")
}

fn tls_config() -> Result<ClientConfig> {
    let mut roots = RootCertStore::empty();
    let certs = rustls_native_certs::load_native_certs();
    if !certs.errors.is_empty() {
        warn!(
            "native root CA certificate loading errors: {:?}",
            certs.errors
        );
    }
    if certs.certs.is_empty() {
        bail!("no native root CA certificates found: {:?}", certs.errors);
    }

    let (accepted, rejected) = roots.add_parsable_certificates(certs.certs);
    if accepted == 0 {
        bail!("no usable native CA certificates found");
    }
    if rejected > 0 {
        warn!("ignored {rejected} unparsable native CA certificates while preparing TLS roots");
    }

    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

fn tls_server_name(host: &str) -> Result<ServerName<'static>> {
    if host.parse::<std::net::IpAddr>().is_ok() {
        bail!("IMAP host must be a DNS name for TLS certificate verification, not an IP address");
    }
    ServerName::try_from(host.to_owned()).context("IMAP host is not a valid TLS DNS name")
}

#[cfg(test)]
mod tests {
    #[test]
    fn fetch_command_peeks_body_without_implicitly_marking_seen() {
        let command = super::fetch_message_command(123);

        assert_eq!(command, "UID FETCH 123 (BODY.PEEK[])");
        assert!(!command.contains("RFC822"));
    }

    #[test]
    fn select_command_quotes_folder_names_with_spaces_and_slashes() {
        let command = super::select_folder_command("Project Alpha/Inbox");

        assert_eq!(command, "SELECT \"Project Alpha/Inbox\"");
    }

    #[test]
    fn folder_worker_specs_preserve_one_worker_per_configured_folder() {
        let cfg = crate::config::Config {
            imap_host: "imap.example.com".to_owned(),
            imap_port: 993,
            imap_user: "you@example.com".to_owned(),
            imap_password: "app-password".to_owned(),
            imap_folder: "INBOX".to_owned(),
            folders: vec!["INBOX".to_owned(), "Junk Mail".to_owned()],
            webhook_url: "https://example.com/webhook".to_owned(),
            webhook_secret: "change-me".to_owned(),
            github_event: "email.received".to_owned(),
            idle_timeout_seconds: 1740,
            reconnect_delay_seconds: 10,
            mark_seen: false,
            startup_notification: false,
        };

        let specs = super::folder_worker_specs(&cfg);

        let folders: Vec<&str> = specs.iter().map(|spec| spec.folder.as_str()).collect();
        assert_eq!(folders, vec!["INBOX", "Junk Mail"]);
        assert_eq!(specs.len(), cfg.folders.len());
    }

    #[test]
    fn idle_done_reason_explains_successful_cycle_without_reconnect() {
        assert_eq!(super::IdleDoneReason::Exists.as_str(), "exists");
        assert_eq!(super::IdleDoneReason::Timeout.as_str(), "timeout");
    }

    #[test]
    fn tls_server_name_rejects_ip_literals() {
        assert!(super::tls_server_name("imap.example.com").is_ok());
        assert!(super::tls_server_name("127.0.0.1").is_err());
    }

    #[test]
    fn tls_config_builds_with_single_crypto_provider() {
        super::tls_config().expect("TLS config should build with native roots");
    }
}
