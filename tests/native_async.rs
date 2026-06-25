use std::fs;

#[test]
fn live_io_uses_native_async_apis_not_blocking_adapters() {
    let listener = fs::read_to_string("src/listener.rs").unwrap();
    let webhook = fs::read_to_string("src/webhook.rs").unwrap();
    let cargo = fs::read_to_string("Cargo.toml").unwrap();

    for forbidden in [
        "spawn_blocking",
        "std::net::TcpStream",
        "std::thread",
        "std::io::{BufRead, BufReader, Read, Write}",
        "set_read_timeout",
    ] {
        assert!(
            !listener.contains(forbidden),
            "listener must not use blocking live I/O token `{forbidden}`"
        );
    }

    assert!(
        listener.contains("tokio::net::TcpStream"),
        "listener should use tokio TCP"
    );
    assert!(
        listener.contains("tokio_rustls::TlsConnector"),
        "listener should use tokio-rustls TLS"
    );
    assert!(
        listener.contains("tokio::time::timeout"),
        "listener should use tokio timeout for IDLE windows"
    );
    assert!(
        listener.contains("tokio::time::sleep"),
        "listener should use async reconnect sleep"
    );
    assert!(
        !listener.contains("Ok(()) => debug!"),
        "successful IDLE cycles must continue on the same connection instead of reconnecting"
    );
    assert!(
        listener.contains("loop {\n        info!(\"entering IDLE folder={}\", folder);\n        let idle_outcome = client"),
        "folder worker should loop over IDLE cycles inside one selected connection"
    );
    assert!(
        listener.contains("entering IDLE folder={}"),
        "listener should log INFO-level IDLE entry diagnostics"
    );
    assert!(
        listener.contains("idle done folder={} reason={}"),
        "listener should log why each IDLE cycle ended"
    );
    assert!(
        !webhook.contains("reqwest::blocking"),
        "webhook must not use reqwest blocking client"
    );
    assert!(
        !cargo.contains("\"blocking\""),
        "reqwest blocking feature must be disabled"
    );
}
