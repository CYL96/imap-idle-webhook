use anyhow::Result;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Client, Response};
use serde::Serialize;
use sha2::Sha256;
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;

pub fn make_payload_bytes<T: Serialize>(payload: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(payload)?)
}

pub fn build_signature(payload_bytes: &[u8], secret: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any size");
    mac.update(payload_bytes);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

pub async fn post_signed_webhook<T: Serialize>(
    url: &str,
    payload: &T,
    secret: &str,
    event: &str,
    timeout: Duration,
) -> Result<Response> {
    let payload_bytes = make_payload_bytes(payload)?;
    let response = Client::builder()
        .timeout(timeout)
        .build()?
        .post(url)
        .header("Content-Type", "application/json")
        .header(
            "X-Hub-Signature-256",
            build_signature(&payload_bytes, secret),
        )
        .header("X-GitHub-Event", event)
        .body(payload_bytes)
        .send()
        .await?
        .error_for_status()?;

    Ok(response)
}
