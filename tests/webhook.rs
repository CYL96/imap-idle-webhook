use imap_idle_webhook::webhook::{build_signature, make_payload_bytes};
use serde_json::json;

#[test]
fn build_signature_matches_openssl_github_style_hmac_sha256() {
    let payload = br#"{"hello":"world"}"#;

    assert_eq!(
        build_signature(payload, "secret"),
        "sha256=2677ad3e7c090b2fa2c0fb13020d66d5420879b8316eb356a2d60fb9073bc778"
    );
}

#[test]
fn make_payload_bytes_is_stable_compact_json() {
    let payload = make_payload_bytes(&json!({
        "subject": "hi",
        "uid": 42,
        "to": ["a@example.com"]
    }))
    .unwrap();

    assert_eq!(
        payload,
        br#"{"subject":"hi","to":["a@example.com"],"uid":42}"#
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&payload).unwrap(),
        json!({"subject": "hi", "uid": 42, "to": ["a@example.com"]})
    );
}
