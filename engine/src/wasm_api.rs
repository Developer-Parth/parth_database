use crate::exec::{execute_script, Outcome};
use crate::json::Json;
use crate::server::result_json;
use crate::table::Database;
use std::cell::RefCell;
use std::collections::BTreeMap;

const MAX_INPUT: usize = 1 << 20;

thread_local! {
    static DB: RefCell<Option<Database>> = RefCell::new(None);
    static INPUT: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static OUTPUT: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

fn error_json(message: String) -> Json {
    let mut map = BTreeMap::new();
    map.insert("error".to_string(), Json::String(message));
    Json::Object(map)
}

fn run_text(sql: &str) -> Json {
    DB.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            match Database::in_memory() {
                Ok(db) => *slot = Some(db),
                Err(e) => return error_json(e.to_string()),
            }
        }
        let db = slot.as_ref().unwrap();
        match execute_script(db, sql) {
            Ok(Outcome::Single(result)) => result_json(&result),
            Ok(Outcome::Transaction { committed, results }) => {
                let mut map = BTreeMap::new();
                map.insert("committed".to_string(), Json::Bool(committed));
                map.insert("results".to_string(), Json::Array(results.iter().map(result_json).collect()));
                Json::Object(map)
            }
            Err(e) => error_json(e.to_string()),
        }
    })
}

#[no_mangle]
pub extern "C" fn input_buffer(len: usize) -> *mut u8 {
    if len > MAX_INPUT {
        return std::ptr::null_mut();
    }
    INPUT.with(|cell| {
        let mut buf = cell.borrow_mut();
        buf.clear();
        buf.resize(len, 0);
        buf.as_mut_ptr()
    })
}

#[no_mangle]
pub extern "C" fn run(len: usize) -> usize {
    let text = INPUT.with(|cell| {
        let buf = cell.borrow();
        if len > buf.len() {
            return None;
        }
        std::str::from_utf8(&buf[..len]).ok().map(|s| s.to_string())
    });
    let json = match text {
        Some(sql) => run_text(&sql),
        None => error_json("input is not valid UTF-8 or exceeds the buffer".to_string()),
    };
    let bytes = json.to_string_compact().into_bytes();
    OUTPUT.with(|cell| {
        *cell.borrow_mut() = bytes;
        cell.borrow().len()
    })
}

#[no_mangle]
pub extern "C" fn output_buffer() -> *const u8 {
    OUTPUT.with(|cell| cell.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn reset() {
    DB.with(|cell| *cell.borrow_mut() = None);
    INPUT.with(|cell| *cell.borrow_mut() = Vec::new());
    OUTPUT.with(|cell| *cell.borrow_mut() = Vec::new());
}