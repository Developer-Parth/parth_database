use parth_db::exec::{bounded_rows, delete_limited, execute, update_limited};
use parth_db::parser::Expr;
use parth_db::value::Value;
use parth_db::Database;

fn fresh(tag: &str, count: usize) -> Database {
let dir = std::env::temp_dir().join(format!("parth_wl_{}_{}", tag, std::process::id()));
let _ = std::fs::remove_dir_all(&dir);
let db = Database::open(&dir).unwrap();
execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, v TEXT)").unwrap();
for i in 1..=count {
execute(&db, &format!("INSERT INTO t VALUES ({}, 'x{}')", i, i)).unwrap();
}
db
}

fn all_rows(db: &Database) -> Vec<Vec<Value>> {
bounded_rows(db, "t", usize::MAX, usize::MAX).unwrap()
}

fn null_sets() -> Vec<(String, Expr)> {
vec![("v".to_string(), Expr::Literal(Value::Null))]
}

#[test]
fn delete_at_limit_succeeds() {
let db = fresh("d1", 5);
assert!(delete_limited(&db, "t", None, (5, usize::MAX)).is_ok());
assert_eq!(all_rows(&db).len(), 0);
}

#[test]
fn delete_above_limit_errors_and_keeps_rows() {
let db = fresh("d2", 5);
assert!(delete_limited(&db, "t", None, (4, usize::MAX)).is_err());
assert_eq!(all_rows(&db).len(), 5);
}

#[test]
fn update_at_limit_succeeds() {
let db = fresh("u1", 5);
assert!(update_limited(&db, "t", null_sets(), None, (5, usize::MAX)).is_ok());
assert_eq!(all_rows(&db).len(), 5);
}

#[test]
fn update_above_limit_errors_and_keeps_rows() {
let db = fresh("u2", 5);
let before = all_rows(&db);
assert!(update_limited(&db, "t", null_sets(), None, (4, usize::MAX)).is_err());
assert_eq!(all_rows(&db), before);
}