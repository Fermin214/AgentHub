use agenthub_core::{
    network::{client_builder, dispatch, proxy_url},
    store::Store,
};
use serde_json::json;

#[test]
fn proxy_roundtrip_reset_and_input_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        dispatch(&store, "network.get", &json!({})).unwrap(),
        json!({"proxyUrl":""})
    );
    let before_path = std::env::var_os("PATH");
    let before_proxy = std::env::var_os("HTTPS_PROXY");
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    let git_config = home.map(|h| std::path::PathBuf::from(h).join(".gitconfig"));
    let before_git = git_config.as_ref().and_then(|p| std::fs::read(p).ok());
    assert_eq!(
        dispatch(
            &store,
            "network.save",
            &json!({"proxyUrl":" http://127.0.0.1:7897/ ","token":"do-not-store"})
        )
        .unwrap(),
        json!({"proxyUrl":"http://127.0.0.1:7897"})
    );
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(proxy_url(&store).unwrap(), "http://127.0.0.1:7897");
    for invalid in [
        "socks5://127.0.0.1:1080",
        "http://user:secret@localhost:8080",
        "http://localhost/path",
        "http://localhost?q=secret",
        "http://localhost#fragment",
        "garbage",
    ] {
        assert!(dispatch(&store, "network.save", &json!({"proxyUrl":invalid})).is_err());
        assert!(client_builder(Some(invalid)).is_err());
        assert_eq!(proxy_url(&store).unwrap(), "http://127.0.0.1:7897");
    }
    assert!(dispatch(&store, "network.save", &json!({})).is_err());
    dispatch(&store, "network.save", &json!({"proxyUrl":""})).unwrap();
    assert_eq!(proxy_url(&store).unwrap(), "");
    assert_eq!(std::env::var_os("PATH"), before_path);
    assert_eq!(std::env::var_os("HTTPS_PROXY"), before_proxy);
    assert_eq!(
        git_config.as_ref().and_then(|p| std::fs::read(p).ok()),
        before_git
    );
    let conn = rusqlite::Connection::open(dir.path().join("agenthub.sqlite3")).unwrap();
    let saved: String = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key='network_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!saved.contains("token"));
    assert!(client_builder(None).unwrap().build().is_ok());
    assert!(client_builder(Some("https://127.0.0.1:7897"))
        .unwrap()
        .build()
        .is_ok());
}

#[test]
fn explicit_proxy_receives_https_connect_without_external_network() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = [0; 4096];
                    let size = stream.read(&mut bytes).unwrap();
                    stream
                        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                        .unwrap();
                    return String::from_utf8_lossy(&bytes[..size]).into_owned();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "No connection to configured proxy"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => panic!("{e}"),
            }
        }
    });
    let client = client_builder(Some(&proxy))
        .unwrap()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();
    assert!(client
        .get("https://fixture.invalid/skills.zip")
        .send()
        .is_err());
    assert!(worker
        .join()
        .unwrap()
        .starts_with("CONNECT fixture.invalid:443 HTTP/1.1\r\n"));
}
