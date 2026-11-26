use parth_db::exec::{execute, QueryResult};
use parth_db::value::Value;
use parth_db::Database;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_agg_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn q(db: &Database, sql: &str) -> QueryResult {
    execute(db, sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn seeded(tag: &str) -> (Database, PathBuf) {
    let dir = temp_dir(tag);
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT, score REAL)");
    q(
        &db,
        "INSERT INTO users VALUES (3, 'cara', 41, 7.5), (1, 'alan', 30, NULL), (2, 'bea', NULL, 9.0), (4, 'dev', 30, 1.5)",
    );
    (db, dir)
}

fn single(db: &Database, sql: &str) -> Value {
    let result = q(db, sql);
    assert_eq!(result.rows.len(), 1);
    result.rows[0][0].clone()
}

#[test]
fn counts_rows_and_non_null_values() {
    let (db, dir) = seeded("count");
    assert_eq!(single(&db, "SELECT COUNT(*) FROM users"), Value::Int(4));
    assert_eq!(single(&db, "SELECT COUNT(age) FROM users"), Value::Int(3));
    assert_eq!(single(&db, "SELECT COUNT(*) FROM users WHERE age = 30"), Value::Int(2));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sums_minimums_maximums_and_averages() {
    let (db, dir) = seeded("math");
    assert_eq!(single(&db, "SELECT SUM(age) FROM users"), Value::Int(101));
    assert_eq!(single(&db, "SELECT MIN(age) FROM users"), Value::Int(30));
    assert_eq!(single(&db, "SELECT MAX(age) FROM users"), Value::Int(41));
    assert_eq!(single(&db, "SELECT SUM(score) FROM users"), Value::Real(18.0));
    assert_eq!(single(&db, "SELECT MAX(name) FROM users"), Value::Text("dev".into()));
    assert_eq!(single(&db, "SELECT AVG(age) FROM users"), Value::Real(101.0 / 3.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn several_aggregates_and_labels() {
    let (db, dir) = seeded("multi");
    let result = q(&db, "SELECT COUNT(*), min(age), MAX(age) FROM users");
    assert_eq!(result.columns, vec!["count(*)", "min(age)", "max(age)"]);
    assert_eq!(result.rows, vec![vec![Value::Int(4), Value::Int(30), Value::Int(41)]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn empty_input_gives_zero_count_and_null_others() {
    let (db, dir) = seeded("empty");
    assert_eq!(single(&db, "SELECT COUNT(*) FROM users WHERE age = 99"), Value::Int(0));
    assert_eq!(single(&db, "SELECT SUM(age) FROM users WHERE age = 99"), Value::Null);
    assert_eq!(single(&db, "SELECT MAX(age) FROM users WHERE age = 99"), Value::Null);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rejects_invalid_aggregates() {
    let (db, dir) = seeded("bad");
    assert!(execute(&db, "SELECT SUM(name) FROM users").is_err());
    assert!(execute(&db, "SELECT SUM(*) FROM users").is_err());
    assert!(execute(&db, "SELECT COUNT(missing) FROM users").is_err());
    assert!(execute(&db, "SELECT COUNT(age FROM users").is_err());
    let _ = std::fs::remove_dir_all(dir);
}