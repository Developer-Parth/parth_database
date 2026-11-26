use parth_db::exec::execute;
use parth_db::Database;

fn table_of(tag: &str, count: usize) -> Database {
let dir = std::env::temp_dir().join(format!("parth_sw_{}_{}", tag, std::process::id()));
let _ = std::fs::remove_dir_all(&dir);
let db = Database::open(&dir).unwrap();
execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, v TEXT)").unwrap();
for i in 1..=count {
execute(&db, &format!("INSERT INTO t VALUES ({}, 'r{}')", i, i)).unwrap();
}
db
}

fn ids(db: &Database, sql: &str) -> Vec<String> {
let result = execute(db, sql).unwrap();
result.rows.iter().map(|row| format!("{:?}", row[0])).collect()
}

#[test]
fn plain_select_window_matches_ordered_select() {
let db = table_of("window", 150);
for (limit, offset) in [(1usize, 0usize), (5, 0), (5, 10), (64, 64), (70, 100), (10, 145), (10, 149), (10, 150), (10, 400), (0, 3)] {
let plain = ids(&db, &format!("SELECT id FROM t LIMIT {} OFFSET {}", limit, offset));
let ordered = ids(&db, &format!("SELECT id FROM t ORDER BY id LIMIT {} OFFSET {}", limit, offset));
assert_eq!(plain, ordered, "limit {} offset {}", limit, offset);
}
}

#[test]
fn plain_select_without_limit_returns_every_row() {
let db = table_of("nolimit", 150);
assert_eq!(ids(&db, "SELECT id FROM t").len(), 150);
}

#[test]
fn plain_select_window_applies_the_filter_before_the_offset() {
let db = table_of("filter", 150);
let plain = ids(&db, "SELECT id FROM t WHERE id > 100 LIMIT 3 OFFSET 2");
let ordered = ids(&db, "SELECT id FROM t WHERE id > 100 ORDER BY id LIMIT 3 OFFSET 2");
assert_eq!(plain.len(), 3);
assert_eq!(plain, ordered);
}

#[test]
fn plain_select_window_still_rejects_unknown_columns() {
let db = table_of("badcol", 5);
assert!(execute(&db, "SELECT id FROM t WHERE nope = 1 LIMIT 2").is_err());
}

#[test]
fn small_windows_report_rows_without_truncation() {
let db = table_of("smallwin", 3);
let limited = execute(&db, "SELECT id FROM t LIMIT 2").unwrap();
assert_eq!(limited.rows.len(), 2);
assert!(!limited.truncated);
let exact = execute(&db, "SELECT id FROM t LIMIT 3").unwrap();
assert_eq!(exact.rows.len(), 3);
assert!(!exact.truncated);
let past = execute(&db, "SELECT id FROM t LIMIT 5 OFFSET 9").unwrap();
assert!(past.rows.is_empty());
assert!(!past.truncated);
let zero = execute(&db, "SELECT id FROM t LIMIT 0").unwrap();
assert!(zero.rows.is_empty());
assert!(!zero.truncated);
}