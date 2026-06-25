use imap_idle_webhook::email_parse::parse_message;

const MAX_TEXT_CHARS: usize = 50_000;
const TRUNCATION_SUFFIX: &str = "\n\n[message truncated: original text exceeded 50000 chars]";

const RAW_EMAIL: &[u8] = b"From: Alice <alice@example.com>\r\n\
To: Bob <bob@example.com>\r\n\
Subject: Hello\r\n\
Message-ID: <m1@example.com>\r\n\
Date: Tue, 12 May 2026 08:00:00 +0800\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Hi Bob\r\n";

#[test]
fn parse_message_extracts_basic_fields_without_raw_base64() {
    let parsed = parse_message(RAW_EMAIL, "me@example.com", "INBOX", 123).unwrap();

    assert_eq!(parsed.event, "email.received");
    assert_eq!(parsed.account, "me@example.com");
    assert_eq!(parsed.folder, "INBOX");
    assert_eq!(parsed.uid, 123);
    assert_eq!(parsed.from.as_deref(), Some("Alice <alice@example.com>"));
    assert_eq!(parsed.to, vec!["Bob <bob@example.com>"]);
    assert_eq!(parsed.subject.as_deref(), Some("Hello"));
    assert_eq!(parsed.message_id.as_deref(), Some("<m1@example.com>"));
    assert_eq!(parsed.text.as_deref(), Some("Hi Bob\r\n"));
    assert_eq!(parsed.raw_rfc822_base64, "");
}

#[test]
fn parse_message_raw_base64_does_not_grow_with_raw_body() {
    let mut raw = b"From: Alice <alice@example.com>\r\n\
To: Bob <bob@example.com>\r\n\
Subject: Large body\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n"
        .to_vec();
    raw.extend(std::iter::repeat_n(b'x', 256 * 1024));

    let parsed = parse_message(&raw, "me@example.com", "INBOX", 124).unwrap();

    assert_eq!(parsed.raw_rfc822_base64, "");
}

#[test]
fn parse_message_truncates_large_text_without_splitting_utf8_and_keeps_raw_empty() {
    let mut raw = b"From: Alice <alice@example.com>\r\n\
To: Bob <bob@example.com>\r\n\
Subject: Large unicode body\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n"
        .to_vec();
    let body = "文".repeat(MAX_TEXT_CHARS + 2);
    raw.extend_from_slice(body.as_bytes());

    let parsed = parse_message(&raw, "me@example.com", "INBOX", 126).unwrap();
    let text = parsed.text.unwrap();

    assert_eq!(parsed.raw_rfc822_base64, "");
    assert_eq!(
        text,
        format!("{}{}", "文".repeat(MAX_TEXT_CHARS), TRUNCATION_SUFFIX)
    );
}

#[test]
fn parse_message_extracts_readable_text_from_html_only_email() {
    let raw = b"From: HSBC <alerts@example.com>\r\n\
To: Customer <customer@example.com>\r\n\
Subject: HSBC Notification\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><body><h1>Payment received</h1><p>Your balance is&nbsp;&pound;1,234.56 &amp; available.</p></body></html>";

    let parsed = parse_message(raw, "me@example.com", "INBOX", 124).unwrap();

    assert_eq!(
        parsed.text.as_deref(),
        Some("Payment received Your balance is £1,234.56 & available.")
    );
}

#[test]
fn parse_message_html_fallback_preserves_escaped_angle_brackets() {
    let raw = b"From: Alerts <alerts@example.com>\r\n\
To: Customer <customer@example.com>\r\n\
Subject: HTML Escapes\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><body><p>Comparison: 2 &lt; 3 &gt; 1 &amp; stable.</p></body></html>";

    let parsed = parse_message(raw, "me@example.com", "INBOX", 127).unwrap();

    assert_eq!(
        parsed.text.as_deref(),
        Some("Comparison: 2 < 3 > 1 & stable.")
    );
}

#[test]
fn parse_message_falls_back_to_html_when_text_plain_part_is_blank() {
    let raw = b"From: Alerts <alerts@example.com>\r\n\
To: Customer <customer@example.com>\r\n\
Subject: Multipart Notification\r\n\
Content-Type: multipart/alternative; boundary=boundary\r\n\
\r\n\
--boundary\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
   \r\n\
\r\n\
--boundary\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><body><p>Use this secure message for your account summary.</p></body></html>\r\n\
--boundary--\r\n";

    let parsed = parse_message(raw, "me@example.com", "INBOX", 125).unwrap();

    assert_eq!(
        parsed.text.as_deref(),
        Some("Use this secure message for your account summary.")
    );
}
