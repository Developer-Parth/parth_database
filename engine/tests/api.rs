use parth_db::json::{parse, Json};
use parth_db::server::{start, Config};
use parth_db::Database;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;

const TOKEN: &str = "test-token-0123456789abcdef";

fn tmpdir(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("parth_api_test_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn launch(name: &str) -> SocketAddr {
    let db = Database::open(tmpdir(name)).unwrap();
    let config = Config { addr: "127.0.0.1:0".into(), token: TOKEN.into(), allow_remote: false };
    start(db, config).unwrap().addr
}

fn send(addr: SocketAddr, method: &str, path: &str, token: Option<&str>, body: &str) -> (u16, String, String) {
    let mut stream = TcpStream::connect(addr).unwrap();
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let request = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n{auth}Content-Length: {}\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, head.to_string(), body.to_string())
}

fn sql(addr: SocketAddr, statement: &str) -> (u16, Json) {
    let body = Json::Object([("sql".to_string(), Json::String(statement.into()))].into_iter().collect()).to_string_compact();
    let (status, _, text) = send(addr, "POST", "/api/sql", Some(TOKEN), &body);
    (status, parse(&text).unwrap())
}

#[test]
fn health_needs_no_token() {
    let addr = launch("health");
    let (status, _, body) = send(addr, "GET", "/api/health", None, "");
    assert_eq!(status, 200);
    assert_eq!(parse(&body).unwrap().get("status").unwrap().as_str(), Some("ok"));
}

#[test]
fn data_endpoints_reject_missing_and_wrong_tokens() {
    let addr = launch("auth");
    for path in ["/api/tables", "/api/tables/t/rows"] {
        assert_eq!(send(addr, "GET", path, None, "").0, 401);
        assert_eq!(send(addr, "GET", path, Some("wrong-token-wrong-token"), "").0, 401);
    }
    let body = "{\"sql\":\"CREATE TABLE t (id INT PRIMARY KEY)\"}";
    assert_eq!(send(addr, "POST", "/api/sql", None, body).0, 401);
    assert_eq!(send(addr, "POST", "/api/sql", Some("short"), body).0, 401);
    assert_eq!(send(addr, "GET", "/api/tables", Some(TOKEN), "").0, 200);
}

#[test]
fn sql_round_trip_returns_typed_values() {
    let addr = launch("sql");
    assert_eq!(sql(addr, "CREATE TABLE t (id INT PRIMARY KEY, name TEXT, score REAL, ok BOOL)").0, 200);
    let (status, result) = sql(addr, "INSERT INTO t VALUES (1, 'ann', 2.5, TRUE), (2, NULL, 3, FALSE)");
    assert_eq!(status, 200);
    assert_eq!(result.get("affected"), Some(&Json::Number(2.0)));
    let (status, result) = sql(addr, "SELECT id, name, score, ok FROM t ORDER BY id");
    assert_eq!(status, 200);
    let expected = parse("[[1,\"ann\",2.5,true],[2,null,3,false]]").unwrap();
    assert_eq!(result.get("rows"), Some(&expected));
    assert_eq!(result.get("columns"), Some(&parse("[\"id\",\"name\",\"score\",\"ok\"]").unwrap()));
}

#[test]
fn table_listing_and_row_endpoint() {
    let addr = launch("tables");
    sql(addr, "CREATE TABLE notes (id INT PRIMARY KEY, body TEXT)");
    sql(addr, "INSERT INTO notes VALUES (1, 'a'), (2, 'b')");
    let (status, _, body) = send(addr, "GET", "/api/tables", Some(TOKEN), "");
    assert_eq!(status, 200);
    let listing = parse(&body).unwrap();
    let expected = parse("{\"tables\":[{\"name\":\"notes\",\"columns\":[{\"name\":\"id\",\"type\":\"INT\",\"primary_key\":true},{\"name\":\"body\",\"type\":\"TEXT\",\"primary_key\":false}]}]}").unwrap();
    assert_eq!(listing, expected);
    let (status, _, body) = send(addr, "GET", "/api/tables/notes/rows", Some(TOKEN), "");
    assert_eq!(status, 200);
    assert_eq!(parse(&body).unwrap().get("rows"), Some(&parse("[[1,\"a\"],[2,\"b\"]]").unwrap()));
    assert_eq!(send(addr, "GET", "/api/tables/missing/rows", Some(TOKEN), "").0, 400);
    assert_eq!(send(addr, "GET", "/api/tables/notes;DROP/rows", Some(TOKEN), "").0, 400);
}

#[test]
fn bad_requests_get_clear_errors() {
    let addr = launch("bad");
    let (status, result) = sql(addr, "SELEKT nothing");
    assert_eq!(status, 400);
    assert!(result.get("error").unwrap().as_str().unwrap().len() > 3);
    assert_eq!(send(addr, "POST", "/api/sql", Some(TOKEN), "not json").0, 400);
    assert_eq!(send(addr, "POST", "/api/sql", Some(TOKEN), "{\"query\":\"x\"}").0, 400);
    assert_eq!(send(addr, "GET", "/api/sql", Some(TOKEN), "").0, 405);
    assert_eq!(send(addr, "GET", "/api/unknown", Some(TOKEN), "").0, 404);
}

#[test]
fn oversized_body_is_refused_before_reading_it() {
    let addr = launch("big");
    let mut stream = TcpStream::connect(addr).unwrap();
    let head = format!("POST /api/sql HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: 5000000\r\n\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    assert!(raw.starts_with("HTTP/1.1 413"));
}

#[test]
fn preflight_is_answered_without_a_token() {
    let addr = launch("cors");
    let (status, head, _) = send(addr, "OPTIONS", "/api/sql", None, "");
    assert_eq!(status, 204);
    assert!(head.contains("Access-Control-Allow-Headers: Authorization"));
    assert!(head.contains("Access-Control-Allow-Private-Network: true"));
}

#[test]
fn unsafe_configurations_are_refused() {
    let open = |addr: &str, token: &str, remote: bool, name: &str| {
        let db = Database::open(tmpdir(name)).unwrap();
        start(db, Config { addr: addr.into(), token: token.into(), allow_remote: remote })
    };
    assert!(open("0.0.0.0:0", TOKEN, false, "remote").is_err());
    assert!(open("127.0.0.1:0", "short", false, "short").is_err());
    assert!(open("not an address", TOKEN, false, "badaddr").is_err());
    assert!(open("0.0.0.0:0", TOKEN, true, "allowed").is_ok());
}

#[test]
fn concurrent_clients_do_not_lose_writes() {
    let addr = launch("concurrent");
    sql(addr, "CREATE TABLE c (id INT PRIMARY KEY, worker INT)");
    let handles: Vec<_> = (0..8i64)
        .map(|w| {
            std::thread::spawn(move || {
                for i in 0..15i64 {
                    let (status, _) = sql(addr, &format!("INSERT INTO c VALUES ({}, {w})", w * 100 + i));
                    assert_eq!(status, 200);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let (_, result) = sql(addr, "SELECT id FROM c");
    match result.get("rows") {
        Some(Json::Array(rows)) => assert_eq!(rows.len(), 120),
        _ => panic!("no rows"),
    }
}