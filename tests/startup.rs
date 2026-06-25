use anyhow::anyhow;
use imap_idle_webhook::config::Config;
use imap_idle_webhook::startup::{build_startup_marker, send_startup_notification_with};

#[test]
fn startup_marker_payload_preserves_folders_and_omits_secrets() {
    let cfg = Config {
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
        startup_notification: true,
    };

    let marker = build_startup_marker(&cfg);

    assert_eq!(marker.event, "email.received");
    assert_eq!(marker.account, "you@example.com");
    assert_eq!(marker.folder, "startup");
    assert_eq!(marker.uid, 0);
    assert_eq!(
        marker.from.as_deref(),
        Some("imap-idle-webhook <startup@localhost>")
    );
    assert_eq!(marker.to, vec!["you@example.com"]);
    assert_eq!(marker.subject.as_deref(), Some("imap-idle-webhook started"));
    assert_eq!(marker.raw_rfc822_base64, "");

    let text = marker.text.as_deref().unwrap();
    assert!(text.contains("folders: INBOX | Junk Mail"));
    assert!(text.contains("folder_count: 2"));
    assert!(text.contains("idle_timeout_seconds: 1740"));
    assert!(!text.contains("app-password"));
    assert!(!text.contains("change-me"));
}

#[tokio::test]
async fn startup_notification_send_failure_is_non_fatal() {
    let cfg = Config {
        imap_host: "imap.example.com".to_owned(),
        imap_port: 993,
        imap_user: "you@example.com".to_owned(),
        imap_password: "app-password".to_owned(),
        imap_folder: "INBOX".to_owned(),
        folders: vec!["INBOX".to_owned()],
        webhook_url: "https://example.com/webhook".to_owned(),
        webhook_secret: "change-me".to_owned(),
        github_event: "email.received".to_owned(),
        idle_timeout_seconds: 1740,
        reconnect_delay_seconds: 10,
        mark_seen: false,
        startup_notification: true,
    };

    let result =
        send_startup_notification_with(&cfg, |_| async { Err(anyhow!("webhook unavailable")) })
            .await;

    assert!(result.is_ok());
}
