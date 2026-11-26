use parth_db::exec::{bounded_rows, execute};
use parth_db::Database;
use std::path::PathBuf;
fn temp_dir(tag: &str) -> PathBuf {
let dir = std::env::temp_dir().join(format!("parth_mb_{}_{}", tag, std::process::id()));
let _ = std::fs::remove_dir_all(&dir);
dir
}
fn table_with(tag: &str, count: usize, width: usize) -> Database {
let db = Database::open(&temp_dir(tag)).unwrap();
execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, v TEXT)").unwrap();
let pad = "x".repeat(width);
for i in 1..=count {
execute(&db, &format!("INSERT INTO t VALUES ({}, '{}{}')", i, pad, i)).unwrap();
}
db
}
#[test]
fn row_bound_passes_at_limit_and_rejects_above() {
let db = table_with("rows", 50, 4);
assert_eq!(bounded_rows(&db, "t", 50, usize::MAX).unwrap().len(), 50);
assert!(bounded_rows(&db, "t", 49, usize::MAX).is_err());
}
#[test]
fn byte_bound_rejects_before_all_rows_are_held() {
let db = table_with("bytes", 50, 100);
assert!(bounded_rows(&db, "t", usize::MAX, 500).is_err());
assert_eq!(bounded_rows(&db, "t", usize::MAX, 1_000_000).unwrap().len(), 50);
}
#[test]
fn overflow_values_count_toward_the_byte_bound() {
let db = table_with("overflow", 5, 3000);
assert!(bounded_rows(&db, "t", usize::MAX, 6000).is_err());
assert_eq!(bounded_rows(&db, "t", usize::MAX, 100_000).unwrap().len(), 5);
}
#[test]
fn chunked_scan_visits_every_row_once_in_order() {
let db = table_with("scan", 47, 2);
let mut ids = Vec::new();
db.scan_each("t", 5, |row, _| {
ids.push(format!("{:?}", row[0]));
Ok(true)
})
.unwrap();
assert_eq!(ids.len(), 47);
let mut unique = ids.clone();
unique.sort();
unique.dedup();
assert_eq!(unique.len(), 47);
}
#[test]
fn chunked_scan_stops_when_the_visitor_says_so() {
let db = table_with("stop", 30, 2);
let mut seen = 0;
db.scan_each("t", 4, |_, _| {
seen += 1;
Ok(seen < 9)
})
.unwrap();
assert_eq!(seen, 9);
}
#[test]
fn zero_chunk_is_treated_as_one_row() {
let db = table_with("zero", 6, 2);
let mut seen = 0;
db.scan_each("t", 0, |_, _| {
seen += 1;
Ok(true)
})
.unwrap();
assert_eq!(seen, 6);
}
#[test]
fn scan_blocks_a_concurrent_writer_until_the_scan_finishes() {
use std::sync::atomic::{AtomicBool, Ordering};
let db = table_with("interleave", 200, 4);
let go = AtomicBool::new(false);
let attempted = AtomicBool::new(false);
let finished = AtomicBool::new(false);
let mut seen = 0;
let mut finished_at_last_row = true;
std::thread::scope(|s| {
let writer = s.spawn(|| {
while !go.load(Ordering::SeqCst) {
std::thread::yield_now();
}
attempted.store(true, Ordering::SeqCst);
execute(&db, "INSERT INTO t VALUES (100000, 'late')").unwrap();
finished.store(true, Ordering::SeqCst);
});
db.scan_each("t", 16, |_, _| {
seen += 1;
if seen == 1 {
go.store(true, Ordering::SeqCst);
while !attempted.load(Ordering::SeqCst) {
std::thread::yield_now();
}
}
if seen == 200 {
for _ in 0..20000 {
std::thread::yield_now();
}
finished_at_last_row = finished.load(Ordering::SeqCst);
}
Ok(true)
})
.unwrap();
writer.join().unwrap();
});
assert_eq!(seen, 200);
assert!(!finished_at_last_row);
assert!(finished.load(Ordering::SeqCst));
assert_eq!(db.scan("t").unwrap().len(),201);
}
#[test]
fn control_paging_with_separate_scans_lets_the_writer_in() {
use std::sync::atomic::{AtomicBool, Ordering};
let db = table_with("control", 200, 4);
let go = AtomicBool::new(false);
let attempted = AtomicBool::new(false);
let finished = AtomicBool::new(false);
let mut interleaved = false;
std::thread::scope(|s| {
let writer = s.spawn(|| {
while !go.load(Ordering::SeqCst) {
std::thread::yield_now();
}
attempted.store(true, Ordering::SeqCst);
execute(&db, "INSERT INTO t VALUES (100000, 'late')").unwrap();
finished.store(true, Ordering::SeqCst);
});
for page in 0..12usize {
let mut index = 0;
db.scan_each("t", 16, |_, _| {
index += 1;
Ok(index < (page + 1) * 16)
})
.unwrap();
if page == 0 {
go.store(true, Ordering::SeqCst);
while !attempted.load(Ordering::SeqCst) {
std::thread::yield_now();
}
}
for _ in 0..200000 {
if finished.load(Ordering::SeqCst) {
interleaved = true;
break;
}
std::thread::yield_now();
}
}
writer.join().unwrap();
});
assert!(interleaved);
}