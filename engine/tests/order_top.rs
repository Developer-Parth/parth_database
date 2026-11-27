use parth_db::exec::execute;
use parth_db::Database;

fn table_of(tag: &str, count: usize) -> Database {
let dir = std::env::temp_dir().join(format!("parth_ot_{}_{}", tag, std::process::id()));
let _ = std::fs::remove_dir_all(&dir);
let db = Database::open(&dir).unwrap();
execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, k INT)").unwrap();
for i in 1..=count {
execute(&db, &format!("INSERT INTO t VALUES ({}, {})", i, (i * 7) % 13)).unwrap();
}
db
}

fn ids(db: &Database, sql: &str) -> Vec<String> {
let result = execute(db, sql).unwrap();
result.rows.iter().map(|row| format!("{:?}", row[0])).collect()
}

#[test]
fn top_window_matches_full_sort_for_both_directions() {
let db = table_of("both", 400);
for direction in ["", " DESC"] {
let full = ids(&db, &format!("SELECT id, k FROM t ORDER BY k{}", direction));
for (limit, offset) in [(1, 0), (5, 0), (10, 3), (50, 70), (100, 250), (30, 380), (10, 400)] {
let sql = format!("SELECT id, k FROM t ORDER BY k{} LIMIT {} OFFSET {}", direction, limit, offset);
let expected: Vec<String> = full.iter().skip(offset).take(limit).cloned().collect();
assert_eq!(ids(&db, &sql), expected, "{}", sql);
}
}
}

#[test]
fn top_window_keeps_insertion_order_among_equal_keys() {
let db = table_of("ties", 400);
let first = ids(&db, "SELECT id, k FROM t ORDER BY k LIMIT 40");
let full = ids(&db, "SELECT id, k FROM t ORDER BY k");
assert_eq!(first, full[..40].to_vec());
}

#[test]
fn top_window_applies_the_filter_before_ordering() {
let db = table_of("filtered", 400);
let full = ids(&db, "SELECT id, k FROM t WHERE id > 120 ORDER BY k DESC");
let top = ids(&db, "SELECT id, k FROM t WHERE id > 120 ORDER BY k DESC LIMIT 25 OFFSET 10");
assert_eq!(top, full[10..35].to_vec());
}

#[test]
fn top_window_still_reports_filter_errors() {
let db = table_of("errors", 400);
assert!(execute(&db, "SELECT id FROM t WHERE missing = 1 ORDER BY k LIMIT 3").is_err());
}

#[test]
fn large_offset_falls_back_to_the_full_sort() {
let db = table_of("fallback", 400);
let full = ids(&db, "SELECT id, k FROM t ORDER BY k");
let far = ids(&db, "SELECT id, k FROM t ORDER BY k LIMIT 5 OFFSET 10000");
assert!(far.is_empty());
let near = ids(&db, "SELECT id, k FROM t ORDER BY k LIMIT 10 OFFSET 395");
assert_eq!(near, full[395..].to_vec());
}

#[test]
fn top_window_handles_zero_and_oversized_limits() {
let db = table_of("limits", 400);
let full = ids(&db, "SELECT id, k FROM t ORDER BY k DESC");
assert!(ids(&db, "SELECT id, k FROM t ORDER BY k LIMIT 0").is_empty());
assert!(ids(&db, "SELECT id, k FROM t ORDER BY k LIMIT 0 OFFSET 7").is_empty());
assert_eq!(ids(&db, "SELECT id, k FROM t ORDER BY k DESC LIMIT 20000"), full);
let huge = ids(&db, "SELECT id, k FROM t ORDER BY k DESC LIMIT 9223372036854775807 OFFSET 5");
assert_eq!(huge, full[5..].to_vec());
}

#[test]
fn top_window_orders_null_keys_like_the_full_sort() {
let db = table_of("nulls", 100);
for id in [101, 102, 103] {
execute(&db, &format!("INSERT INTO t VALUES ({}, NULL)", id)).unwrap();
}
for direction in ["", " DESC"] {
let full = ids(&db, &format!("SELECT id, k FROM t ORDER BY k{}", direction));
let top = ids(&db, &format!("SELECT id, k FROM t ORDER BY k{} LIMIT 12 OFFSET 2", direction));
assert_eq!(top, full[2..14].to_vec());
}
}