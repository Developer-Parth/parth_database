use parth_db::json::{parse, Json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const TOKEN: &str = "test-token-0123456789abcdef";

struct Server {
    child: Child,
    addr: SocketAddr,
    _out: BufReader<ChildStdout>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn datadir(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("parth_crash_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn start(dir: &PathBuf) -> Server {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_parth_db"))
        .env("PARTH_DB_DIR", dir)
        .env("PARTH_DB_ADDR", addr.to_string())
        .env("PARTH_DB_TOKEN", TOKEN)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    assert!(line.contains("listening"));
    Server { child, addr, _out: out }
}

fn try_sql(addr: SocketAddr, statement: &str) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(addr).ok()?;
    let body = Json::Object([("sql".to_string(), Json::String(statement.into()))].into_iter().collect()).to_string_compact();
    let request = format!(
        "POST /api/sql HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n{}",
        TOKEN,
        body.len(),
        body
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).ok()?;
    let (head, rest) = raw.split_once("\r\n\r\n")?;
    let status = head.split(' ').nth(1)?.parse().ok()?;
    Some((status, rest.to_string()))
}

fn rows(addr: SocketAddr, statement: &str) -> Vec<Vec<f64>> {
    let (status, text) = try_sql(addr, statement).unwrap();
    assert_eq!(status, 200);
    let parsed = parse(&text).unwrap();
    match parsed.get("rows") {
        Some(Json::Array(all)) => all
            .iter()
            .map(|row| match row {
                Json::Array(cells) => cells
                    .iter()
                    .map(|c| match c {
                        Json::Number(n) => *n,
                        _ => panic!("not a number"),
                    })
                    .collect(),
                _ => panic!("row is not an array"),
            })
            .collect(),
        _ => panic!("no rows"),
    }
}

fn kill_when_acked(server: &mut Server, acked: &Arc<AtomicUsize>, at_least: usize) {
    while acked.load(Ordering::SeqCst) < at_least {
        thread::sleep(Duration::from_millis(5));
    }
    server.child.kill().unwrap();
    server.child.wait().unwrap();
}

#[test]
fn acknowledged_inserts_survive_hard_kill() {
    let dir = datadir("insert");
    let mut server = start(&dir);
    let addr = server.addr;
    assert_eq!(try_sql(addr, "CREATE TABLE t (id INT PRIMARY KEY, v INT)").unwrap().0, 200);
    let acked = Arc::new(AtomicUsize::new(0));
    let counter = acked.clone();
    let writer = thread::spawn(move || {
        for id in 1..100000usize {
            match try_sql(addr, &format!("INSERT INTO t VALUES ({}, {})", id, id * 2)) {
                Some((200, _)) => counter.store(id, Ordering::SeqCst),
                _ => break,
            }
        }
    });
    kill_when_acked(&mut server, &acked, 40);
    writer.join().unwrap();
    let confirmed = acked.load(Ordering::SeqCst);

    let second = start(&dir);
    let found = rows(second.addr, "SELECT id, v FROM t ORDER BY id");
    assert!(found.len() >= confirmed);
    assert!(found.len() <= confirmed + 1);
    for (i, row) in found.iter().enumerate() {
        assert_eq!(row[0], (i + 1) as f64);
        assert_eq!(row[1], ((i + 1) * 2) as f64);
    }
    drop(second);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn multi_row_update_is_all_or_nothing_after_hard_kill() {
    let dir = datadir("update");
    let mut server = start(&dir);
    let addr = server.addr;
    assert_eq!(try_sql(addr, "CREATE TABLE t (id INT PRIMARY KEY, v INT)").unwrap().0, 200);
    for id in 1..=100 {
        assert_eq!(try_sql(addr, &format!("INSERT INTO t VALUES ({}, 0)", id)).unwrap().0, 200);
    }
    let acked = Arc::new(AtomicUsize::new(0));
    let counter = acked.clone();
    let writer = thread::spawn(move || {
        for k in 1..100000usize {
            match try_sql(addr, &format!("UPDATE t SET v = {}", k)) {
                Some((200, _)) => counter.store(k, Ordering::SeqCst),
                _ => break,
            }
        }
    });
    kill_when_acked(&mut server, &acked, 30);
    writer.join().unwrap();
    let confirmed = acked.load(Ordering::SeqCst) as f64;

    let second = start(&dir);
    let found = rows(second.addr, "SELECT id, v FROM t ORDER BY id");
    assert_eq!(found.len(), 100);
    let value = found[0][1];
    assert!(value == confirmed || value == confirmed + 1.0);
    for row in &found {
        assert_eq!(row[1], value);
    }
    drop(second);
    let _ = std::fs::remove_dir_all(&dir);
}
