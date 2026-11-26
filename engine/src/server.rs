use crate::error::{Error, Result};
use crate::exec::{execute, execute_script, Outcome, QueryResult};
use crate::json::{parse, Json};
use crate::table::{ColumnType, Database};
use crate::value::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const MIN_TOKEN_LEN: usize = 16;
const WORKERS: usize = 8;
const ROW_LIMIT: usize = 1000;
const SAFE_INTEGER: i64 = 1 << 53;

pub struct Config {
    pub addr: String,
    pub token: String,
    pub allow_remote: bool,
}

pub struct Server {
    pub addr: SocketAddr,
}

struct Context {
    db: Mutex<Database>,
    token: String,
}

struct Request {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Vec<u8>,
}

struct Response {
    status: u16,
    body: String,
}

pub fn start(db: Database, config: Config) -> Result<Server> {
    if config.token.len() < MIN_TOKEN_LEN {
        return Err(Error::InvalidInput(format!("token must be at least {MIN_TOKEN_LEN} characters")));
    }
    let wanted: SocketAddr = config.addr.parse().map_err(|_| Error::InvalidInput("invalid listen address".into()))?;
    if !wanted.ip().is_loopback() && !config.allow_remote {
        return Err(Error::InvalidInput("refusing a non-loopback address without allow_remote".into()));
    }
    let listener = Arc::new(TcpListener::bind(wanted)?);
    let addr = listener.local_addr()?;
    let ctx = Arc::new(Context { db: Mutex::new(db), token: config.token });
    for _ in 0..WORKERS {
        let listener = Arc::clone(&listener);
        let ctx = Arc::clone(&ctx);
        std::thread::spawn(move || loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = handle(stream, &ctx);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        });
    }
    Ok(Server { addr })
}

fn handle(mut stream: TcpStream, ctx: &Context) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let response = match read_request(&mut stream) {
        Ok(request) => route(&request, ctx),
        Err(response) => response,
    };
    write_response(&mut stream, &response)
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn read_request(stream: &mut TcpStream) -> std::result::Result<Request, Response> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let header_end = loop {
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(error_response(431, "headers too large"));
        }
        let n = stream.read(&mut chunk).map_err(|_| error_response(408, "read failed"))?;
        if n == 0 {
            return Err(error_response(400, "incomplete request"));
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8(buf[..header_end].to_vec()).map_err(|_| error_response(400, "invalid header encoding"))?;
    let mut lines = head.split("\r\n");
    let mut parts = lines.next().unwrap_or("").split(' ');
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut authorization = None;
    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "authorization" => authorization = Some(value.to_string()),
                "content-length" => {
                    content_length = value.parse().map_err(|_| error_response(400, "bad content length"))?;
                }
                "transfer-encoding" => return Err(error_response(400, "transfer encoding is not supported")),
                _ => {}
            }
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Err(error_response(413, "body too large"));
    }
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).map_err(|_| error_response(408, "read failed"))?;
        if n == 0 {
            return Err(error_response(400, "incomplete body"));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);
    Ok(Request { method, path, authorization, body })
}

fn object(fields: Vec<(&str, Json)>) -> Json {
    Json::Object(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<BTreeMap<_, _>>())
}

fn json_response(status: u16, body: Json) -> Response {
    Response { status, body: body.to_string_compact() }
}

fn error_response(status: u16, message: &str) -> Response {
    json_response(status, object(vec![("error", Json::String(message.into()))]))
}

fn engine_error(error: Error) -> Response {
    match error {
        Error::InvalidInput(message) => error_response(400, &message),
        Error::Locked => error_response(503, "database is busy"),
        _ => error_response(500, "storage error"),
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (*a.get(i).unwrap_or(&0) ^ *b.get(i).unwrap_or(&0)) as usize;
    }
    diff == 0
}

fn authorized(request: &Request, token: &str) -> bool {
    let Some(header) = &request.authorization else { return false };
    let Some(given) = header.strip_prefix("Bearer ") else { return false };
    constant_eq(given.as_bytes(), token.as_bytes())
}

fn route(request: &Request, ctx: &Context) -> Response {
    let path = request.path.split('?').next().unwrap_or("");
    if request.method == "OPTIONS" {
        return Response { status: 204, body: String::new() };
    }
    if path == "/api/health" {
        if request.method != "GET" {
            return error_response(405, "method not allowed");
        }
        return json_response(200, object(vec![("status", Json::String("ok".into()))]));
    }
    if !authorized(request, &ctx.token) {
        return error_response(401, "missing or invalid token");
    }
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    match (request.method.as_str(), segments.as_slice()) {
        ("GET", ["api", "tables"]) => list_tables(ctx),
        ("GET", ["api", "tables", name, "rows"]) => table_rows(ctx, name),
        ("POST", ["api", "sql"]) => run_sql(ctx, &request.body),
        (_, ["api", "tables"]) | (_, ["api", "sql"]) | (_, ["api", "tables", _, "rows"]) => error_response(405, "method not allowed"),
        _ => error_response(404, "not found"),
    }
}

fn type_name(ty: ColumnType) -> &'static str {
    match ty {
        ColumnType::Int => "INT",
        ColumnType::Real => "REAL",
        ColumnType::Text => "TEXT",
        ColumnType::Bool => "BOOL",
    }
}

fn value_json(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(n) if n.abs() < SAFE_INTEGER => Json::Number(*n as f64),
        Value::Int(n) => Json::String(n.to_string()),
        Value::Real(r) if r.is_finite() => Json::Number(*r),
        Value::Real(_) => Json::Null,
        Value::Text(s) => Json::String(s.clone()),
    }
}

fn result_json(result: &QueryResult) -> Json {
    let rows = result.rows.iter().map(|row| Json::Array(row.iter().map(value_json).collect())).collect();
    object(vec![
        ("columns", Json::Array(result.columns.iter().map(|c| Json::String(c.clone())).collect())),
        ("rows", Json::Array(rows)),
        ("affected", Json::Number(result.affected as f64)),
    ])
}

fn list_tables(ctx: &Context) -> Response {
    let db = ctx.db.lock().unwrap_or_else(|p| p.into_inner());
    match db.tables() {
        Ok(schemas) => {
            let items = schemas
                .iter()
                .map(|s| {
                    let columns = s
                        .columns
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            object(vec![
                                ("name", Json::String(c.name.clone())),
                                ("type", Json::String(type_name(c.ty).into())),
                                ("primary_key", Json::Bool(i == s.primary_key)),
                            ])
                        })
                        .collect();
                    object(vec![("name", Json::String(s.name.clone())), ("columns", Json::Array(columns))])
                })
                .collect();
            json_response(200, object(vec![("tables", Json::Array(items))]))
        }
        Err(e) => engine_error(e),
    }
}

fn table_rows(ctx: &Context, name: &str) -> Response {
    let db = ctx.db.lock().unwrap_or_else(|p| p.into_inner());
    if let Err(e) = db.schema(name) {
        return engine_error(e);
    }
    match execute(&db, &format!("SELECT * FROM {name} LIMIT {ROW_LIMIT}")) {
        Ok(result) => json_response(200, result_json(&result)),
        Err(e) => engine_error(e),
    }
}

fn run_sql(ctx: &Context, body: &[u8]) -> Response {
    let Ok(text) = std::str::from_utf8(body) else { return error_response(400, "body must be UTF-8") };
    let Ok(json) = parse(text) else { return error_response(400, "body must be valid JSON") };
    let Some(sql) = json.get("sql").and_then(|v| v.as_str()) else {
        return error_response(400, "body must be an object with a string field named sql");
    };
    let db = ctx.db.lock().unwrap_or_else(|p| p.into_inner());
    match execute_script(&db, sql) {
        Ok(Outcome::Single(result)) => json_response(200, result_json(&result)),
        Ok(Outcome::Transaction { committed, results }) => {
            let mut map = std::collections::BTreeMap::new();
            map.insert("committed".to_string(), Json::Bool(committed));
            map.insert("results".to_string(), Json::Array(results.iter().map(result_json).collect()));
            json_response(200, Json::Object(map))
        }
        Err(e) => engine_error(e),
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}

fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Private-Network: true\r\n\r\n",
        response.status,
        status_text(response.status),
        response.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(response.body.as_bytes())?;
    stream.flush()
}