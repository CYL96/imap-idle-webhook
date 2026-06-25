use imap_idle_webhook::config::Config;

#[test]
fn config_from_env_uses_existing_defaults_and_parses_mark_seen() {
    temp_env::with_vars(
        [
            ("IMAP_HOST", Some("imap.example.com")),
            ("IMAP_USER", Some("you@example.com")),
            ("IMAP_PASSWORD", Some("app-password")),
            ("WEBHOOK_URL", Some("https://example.com/webhook")),
            ("WEBHOOK_SECRET", Some("change-me")),
            ("MARK_SEEN", Some("yes")),
        ],
        || {
            let cfg = Config::from_env().unwrap();

            assert_eq!(cfg.imap_host, "imap.example.com");
            assert_eq!(cfg.imap_port, 993);
            assert_eq!(cfg.imap_user, "you@example.com");
            assert_eq!(cfg.imap_password, "app-password");
            assert_eq!(cfg.imap_folder, "INBOX");
            assert_eq!(cfg.folders, vec!["INBOX"]);
            assert_eq!(cfg.webhook_url, "https://example.com/webhook");
            assert_eq!(cfg.webhook_secret, "change-me");
            assert_eq!(cfg.github_event, "email.received");
            assert_eq!(cfg.idle_timeout_seconds, 1740);
            assert_eq!(cfg.reconnect_delay_seconds, 10);
            assert!(cfg.mark_seen);
            assert!(!cfg.startup_notification);
        },
    );
}

#[test]
fn startup_notification_parses_true_values() {
    for value in ["true", "yes", "1"] {
        temp_env::with_vars(
            [
                ("IMAP_HOST", Some("imap.example.com")),
                ("IMAP_USER", Some("you@example.com")),
                ("IMAP_PASSWORD", Some("app-password")),
                ("WEBHOOK_URL", Some("https://example.com/webhook")),
                ("WEBHOOK_SECRET", Some("change-me")),
                ("STARTUP_NOTIFICATION", Some(value)),
            ],
            || {
                let cfg = Config::from_env().unwrap();

                assert!(cfg.startup_notification);
            },
        );
    }
}

#[test]
fn imap_folders_takes_precedence_and_trims_empty_entries() {
    temp_env::with_vars(
        [
            ("IMAP_HOST", Some("imap.example.com")),
            ("IMAP_USER", Some("you@example.com")),
            ("IMAP_PASSWORD", Some("app-password")),
            ("WEBHOOK_URL", Some("https://example.com/webhook")),
            ("WEBHOOK_SECRET", Some("change-me")),
            ("IMAP_FOLDER", Some("Legacy")),
            ("IMAP_FOLDERS", Some(" INBOX | Alerts | | Archive/2026 | ")),
        ],
        || {
            let cfg = Config::from_env().unwrap();

            assert_eq!(cfg.folders, vec!["INBOX", "Alerts", "Archive/2026"]);
            assert_eq!(cfg.imap_folder, "INBOX");
        },
    );
}

#[test]
fn imap_folders_empty_after_parsing_falls_back_to_imap_folder() {
    temp_env::with_vars(
        [
            ("IMAP_HOST", Some("imap.example.com")),
            ("IMAP_USER", Some("you@example.com")),
            ("IMAP_PASSWORD", Some("app-password")),
            ("WEBHOOK_URL", Some("https://example.com/webhook")),
            ("WEBHOOK_SECRET", Some("change-me")),
            ("IMAP_FOLDER", Some("Primary")),
            ("IMAP_FOLDERS", Some(" |  | ")),
        ],
        || {
            let cfg = Config::from_env().unwrap();

            assert_eq!(cfg.folders, vec!["Primary"]);
            assert_eq!(cfg.imap_folder, "Primary");
        },
    );
}

#[test]
fn imap_folders_preserves_spaces_and_slashes_inside_folder_names() {
    temp_env::with_vars(
        [
            ("IMAP_HOST", Some("imap.example.com")),
            ("IMAP_USER", Some("you@example.com")),
            ("IMAP_PASSWORD", Some("app-password")),
            ("WEBHOOK_URL", Some("https://example.com/webhook")),
            ("WEBHOOK_SECRET", Some("change-me")),
            (
                "IMAP_FOLDERS",
                Some("Project Alpha/Inbox | Team Mail/Needs Review"),
            ),
        ],
        || {
            let cfg = Config::from_env().unwrap();

            assert_eq!(
                cfg.folders,
                vec!["Project Alpha/Inbox", "Team Mail/Needs Review"]
            );
            assert_eq!(cfg.imap_folder, "Project Alpha/Inbox");
        },
    );
}

#[test]
fn config_from_env_reports_all_missing_required_keys() {
    temp_env::with_vars(
        [
            ("IMAP_HOST", None::<&str>),
            ("IMAP_USER", None::<&str>),
            ("IMAP_PASSWORD", None::<&str>),
            ("WEBHOOK_URL", None::<&str>),
            ("WEBHOOK_SECRET", None::<&str>),
        ],
        || {
            let err = Config::from_env().unwrap_err().to_string();

            assert!(err.contains("IMAP_HOST"));
            assert!(err.contains("IMAP_USER"));
            assert!(err.contains("IMAP_PASSWORD"));
            assert!(err.contains("WEBHOOK_URL"));
            assert!(err.contains("WEBHOOK_SECRET"));
        },
    );
}
