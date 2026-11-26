use parth_db::server::{start, Config};
use parth_db::Database;

fn main() {
    let dir = std::env::var("PARTH_DB_DIR").unwrap_or_else(|_| "data".into());
    let addr = std::env::var("PARTH_DB_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let allow_remote = std::env::var("PARTH_DB_ALLOW_REMOTE").map(|v| v == "1").unwrap_or(false);
    let token = match std::env::var("PARTH_DB_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            eprintln!("PARTH_DB_TOKEN is required: set it to a secret of at least 16 characters");
            std::process::exit(2);
        }
    };
    let db = match Database::open(&dir) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("cannot open database in {dir}: {e}");
            std::process::exit(1);
        }
    };
    match start(db, Config { addr, token, allow_remote }) {
        Ok(server) => {
            println!("parth_db listening on http://{}", server.addr);
            loop {
                std::thread::park();
            }
        }
        Err(e) => {
            eprintln!("cannot start server: {e}");
            std::process::exit(1);
        }
    }
}