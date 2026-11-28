use parth_db::server::{start, Config};
use parth_db::Database;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};


const TOKEN: &str = "test-token-0123456789abcdef";


fn launch(name: &str) -> SocketAddr {
    let mut dir = std::env::temp_dir();
    let _ = std::fs::remove_dir_all(&dir);
    dir.push(format!("parth_assets_test_{}_{}", name, std::process::id()));
    let config = Config { addr: "127.0.0.1:0".into(), token: TOKEN.into(), allow_remote: false };
    let db = Database::open(dir).unwrap();
    start(db, config).unwrap().addr
}
fn raw(addr: SocketAddr, request: &str) -> (u16, String, String) {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut out = Vec::new();
    stream.read_to_end(&mut out).unwrap();
    let text = String::from_utf8_lossy(&out).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    let status: u16 = head[9..12].parse().unwrap();
    (status, head.to_string(), body.to_string())
}
fn get(addr: SocketAddr, path: &str, token: Option<&str>) -> (u16, String, String) {
    let auth = match token { Some(t) => format!("Authorization: Bearer {}\r\n", t), None => String::new() };
    raw(addr, &format!("GET {} HTTP/1.1\r\nHost: localhost\r\n{}\r\n", path, auth))
}
const ASSETS: [(&str, &str); 10] = [
    ("/", "text/html"), ("/index.html", "text/html"), ("/style.css", "text/css"),
    ("/app.js", "text/javascript"), ("/csv.js", "text/javascript"), ("/guarded.js", "text/javascript"),
    ("/history.js", "text/javascript"), ("/latest.js", "text/javascript"), ("/paging.js", "text/javascript"),
    ("/resultpager.js", "text/javascript"),
];
#[test]
fn assets_are_served_without_a_token() {
    let addr = launch("serve");
    for (path, kind) in ASSETS.iter() {
        let (status, head, body) = get(addr, path, None);
        assert_eq!(status, 200, "{}", path);
        assert!(head.contains(kind), "{}", path);
        assert!(!body.is_empty(), "{}", path);
    }
}
#[test]
fn unknown_paths_never_serve_files() {
    let addr = launch("unknown");
    let paths = ["/Cargo.toml", "/../Cargo.toml", "/%2e%2e/Cargo.toml", "/engine/Cargo.toml", "/web/index.html", "/data"];
    for path in paths.iter() {
        for token in [None, Some(TOKEN)] {
            let (status, _, body) = get(addr, path, token);
            println!("{} {:?} {}", path, token.is_some(), status);
            assert_ne!(status, 200, "{}", path);
            assert!(!body.contains("[package]"), "{}", path);
        }
    }
}
#[test]
fn api_still_needs_the_token() {
    let addr = launch("api");
    assert_eq!(get(addr, "/api/tables", None).0, 401);
    assert_eq!(get(addr, "/api/tables", Some(TOKEN)).0, 200);
}
#[test]
fn post_to_root_is_not_served_as_an_asset() {
    let addr = launch("post");
    let (status, _, body) = raw(addr, "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n");
    assert_ne!(status, 200);
    assert!(!body.contains("<html"));
}