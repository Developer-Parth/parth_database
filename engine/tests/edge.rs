use parth_db::exec::{execute, QueryResult};
use parth_db::table::Database;
use parth_db::value::Value;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_edge_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn q(db: &Database, sql: &str) -> QueryResult {
    execute(db, sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn count(db: &Database, table: &str) -> usize {
    q(db, &format!("SELECT * FROM {table}")).rows.len()
}

#[test]
fn rows_survive_reopen_in_key_order() {
    let dir = temp_dir("reopen");
    {
        let db = Database::open(&dir).unwrap();
        q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
        for id in (0..300).rev() {
            q(&db, &format!("INSERT INTO t VALUES ({id}, 'row {id}')"));
        }
    }
    let db = Database::open(&dir).unwrap();
    let result = q(&db, "SELECT id FROM t");
    assert_eq!(result.rows.len(), 300);
    let ids: Vec<i64> = result
        .rows
        .iter()
        .map(|r| match r[0] {
            Value::Int(n) => n,
            ref other => panic!("not an int: {other:?}"),
        })
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_primary_key_is_rejected_and_keeps_old_row() {
    let dir = temp_dir("dup");
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
    q(&db, "INSERT INTO t VALUES (1, 'first')");
    assert!(execute(&db, "INSERT INTO t VALUES (1, 'second')").is_err());
    let result = q(&db, "SELECT label FROM t WHERE id = 1");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], Value::Text("first".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn null_primary_key_is_rejected() {
    let dir = temp_dir("nullpk");
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
    assert!(execute(&db, "INSERT INTO t VALUES (NULL, 'x')").is_err());
    assert_eq!(count(&db, "t"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn oversized_row_is_rejected_without_changing_table() {
    let dir = temp_dir("big");
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
    q(&db, "INSERT INTO t VALUES (1, 'small')");
    let big = "x".repeat(2000000);
    assert!(execute(&db, &format!("INSERT INTO t VALUES (2, '{big}')")).is_err());
    assert_eq!(count(&db, "t"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_statements_return_errors_and_change_nothing() {
    let dir = temp_dir("bad");
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
    q(&db, "INSERT INTO t VALUES (1, 'a')");
    assert!(execute(&db, "SELEC * FROM t").is_err());
    assert!(execute(&db, "SELECT * FROM missing").is_err());
    assert!(execute(&db, "INSERT INTO t VALUES (2)").is_err());
    assert!(execute(&db, "CREATE TABLE t (id INT PRIMARY KEY)").is_err());
    assert!(execute(&db, "CREATE TABLE u (a INT, b INT)").is_err());
    assert!(execute(&db, "").is_err());
    assert_eq!(count(&db, "t"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dropped_table_can_be_recreated_empty() {
    let dir = temp_dir("drop");
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
    q(&db, "INSERT INTO t VALUES (1, 'a'), (2, 'b')");
    q(&db, "DROP TABLE t");
    assert!(execute(&db, "SELECT * FROM t").is_err());
    q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
    assert_eq!(count(&db, "t"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn delete_then_reopen_keeps_only_survivors() {
    let dir = temp_dir("delete");
    {
        let db = Database::open(&dir).unwrap();
        q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
        for id in 0..100 {
            q(&db, &format!("INSERT INTO t VALUES ({id}, 'v')"));
        }
        q(&db, "DELETE FROM t WHERE id >= 10");
    }
    let db = Database::open(&dir).unwrap();
    assert_eq!(count(&db, "t"), 10);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn quotes_and_unicode_text_round_trip() {
    let dir = temp_dir("text");
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE t (id INT PRIMARY KEY, label TEXT)");
    q(&db, "INSERT INTO t VALUES (1, 'it''s')");
    q(&db, "INSERT INTO t VALUES (2, 'नमस्ते')");
    let result = q(&db, "SELECT label FROM t ORDER BY id");
    assert_eq!(result.rows[0][0], Value::Text("it's".to_string()));
    assert_eq!(result.rows[1][0], Value::Text("नमस्ते".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}