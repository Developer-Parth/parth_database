use parth_db::exec::{execute, QueryResult};
use parth_db::table::Database;
use parth_db::value::Value;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_sql_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn q(db: &Database, sql: &str) -> QueryResult {
    execute(db, sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn ints(result: &QueryResult, column: usize) -> Vec<i64> {
    result
        .rows
        .iter()
        .map(|r| match r[column] {
            Value::Int(n) => n,
            ref other => panic!("not an int: {other:?}"),
        })
        .collect()
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

#[test]
fn select_orders_filters_and_limits() {
    let (db, dir) = seeded("select");
    assert_eq!(ints(&q(&db, "SELECT id FROM users"), 0), vec![1, 2, 3, 4]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users ORDER BY age DESC"), 0), vec![3, 1, 4, 2]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users ORDER BY age ASC"), 0), vec![2, 1, 4, 3]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users WHERE age = 30"), 0), vec![1, 4]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users WHERE age > 30 OR score > 8"), 0), vec![2, 3]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users ORDER BY id DESC LIMIT 2"), 0), vec![4, 3]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users WHERE id * 2 + 1 >= 5"), 0), vec![2, 3, 4]);
    let all = q(&db, "SELECT * FROM users WHERE id = 3");
    assert_eq!(all.columns, vec!["id", "name", "age", "score"]);
    assert_eq!(
        all.rows,
        vec![vec![Value::Int(3), Value::Text("cara".into()), Value::Int(41), Value::Real(7.5)]]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn null_follows_three_valued_logic() {
    let (db, dir) = seeded("null");
    assert!(q(&db, "SELECT id FROM users WHERE age = NULL").rows.is_empty());
    assert_eq!(ints(&q(&db, "SELECT id FROM users WHERE age IS NULL"), 0), vec![2]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users WHERE age IS NOT NULL"), 0), vec![1, 3, 4]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users WHERE age = 30 OR score > 8"), 0), vec![1, 2, 4]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users WHERE NOT age = 30"), 0), vec![3]);
    assert!(q(&db, "SELECT id FROM users WHERE age + 1 > 0 AND score IS NULL").rows.len() == 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_and_delete_report_counts() {
    let (db, dir) = seeded("mutate");
    assert_eq!(q(&db, "UPDATE users SET age = age + 1 WHERE age = 30").affected, 2);
    assert_eq!(ints(&q(&db, "SELECT age FROM users WHERE id = 4"), 0), vec![31]);
    assert_eq!(q(&db, "UPDATE users SET id = 10 WHERE id = 1").affected, 1);
    assert_eq!(ints(&q(&db, "SELECT id FROM users"), 0), vec![2, 3, 4, 10]);
    assert_eq!(q(&db, "DELETE FROM users WHERE age IS NULL").affected, 1);
    assert_eq!(q(&db, "DELETE FROM users WHERE id = 999").affected, 0);
    assert_eq!(q(&db, "DELETE FROM users").affected, 3);
    assert!(q(&db, "SELECT * FROM users").rows.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn insert_with_column_list_fills_nulls() {
    let (db, dir) = seeded("cols");
    q(&db, "INSERT INTO users (name, id) VALUES ('eve', 5)");
    let row = q(&db, "SELECT * FROM users WHERE id = 5").rows.remove(0);
    assert_eq!(row, vec![Value::Int(5), Value::Text("eve".into()), Value::Null, Value::Null]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn multi_row_statements_are_atomic() {
    let (db, dir) = seeded("atomic");
    assert!(execute(&db, "INSERT INTO users VALUES (20, 'x', 1, 1.0), (21, 'y', 2, 2.0), (20, 'z', 3, 3.0)").is_err());
    assert!(execute(&db, "INSERT INTO users VALUES (30, 'x', 1, 1.0), (1, 'dup', 2, 2.0)").is_err());
    assert!(execute(&db, "INSERT INTO users VALUES (31, 'x', 1, 1.0), (32, 5, 2, 2.0)").is_err());
    assert_eq!(ints(&q(&db, "SELECT id FROM users"), 0), vec![1, 2, 3, 4]);
    assert!(execute(&db, "UPDATE users SET id = 1").is_err());
    assert!(execute(&db, "UPDATE users SET id = 2 WHERE id = 3").is_err());
    assert!(execute(&db, "UPDATE users SET age = age / 0").is_err());
    assert_eq!(ints(&q(&db, "SELECT id FROM users"), 0), vec![1, 2, 3, 4]);
    assert_eq!(q(&db, "UPDATE users SET id = id + 1 WHERE id <= 4").affected, 4);
    assert_eq!(ints(&q(&db, "SELECT id FROM users"), 0), vec![2, 3, 4, 5]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_failing_midway_changes_no_rows() {
    let (db, dir) = seeded("midway");
    let sql = "SELECT id FROM users WHERE age = 30";
    let all = "SELECT id, name, age, score FROM users ORDER BY id";
    let rows = format!("{:?}", q(&db, all).rows);
    let before = ints(&q(&db, sql), 0);
    assert_eq!(before, vec![1, 4]);
    assert!(execute(&db, "UPDATE users SET age = 10 / (id - 3)").is_err());
    assert_eq!(ints(&q(&db, sql), 0), before);
    assert_eq!(format!("{:?}", q(&db, all).rows), rows);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn delete_failing_midway_removes_no_rows() {
    let (db, dir) = seeded("delete_midway");
    let all = "SELECT id, name, age, score FROM users ORDER BY id";
    let rows = format!("{:?}", q(&db, all).rows);
    assert!(execute(&db, "DELETE FROM users WHERE 10 / (id - 3) < 0").is_err());
    assert_eq!(format!("{:?}", q(&db, all).rows), rows);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn errors_are_reported_not_panicked() {
    let (db, dir) = seeded("errors");
    let bad = [
        "SELECT * FROM missing",
        "SELECT nope FROM users",
        "SELECT * FROM users WHERE nope = 1",
        "SELECT * FROM users WHERE id = 'a'",
        "SELECT * FROM users WHERE name",
        "SELECT * FROM users WHERE 9223372036854775807 + 1 > 0",
        "SELECT * FROM users WHERE 1 / 0 = 1",
        "SELECT * FROM users ORDER BY nope",
        "INSERT INTO users VALUES (1)",
        "INSERT INTO users (id, id) VALUES (1, 2)",
        "INSERT INTO users (id) VALUES (id)",
        "INSERT INTO users VALUES (NULL, 'n', 1, 1.0)",
        "INSERT INTO users VALUES (9, 'n', 'old', 1.0)",
        "UPDATE users SET nope = 1",
        "UPDATE users SET age = 1, age = 2",
        "DELETE FROM users WHERE nope",
        "CREATE TABLE users (id INT PRIMARY KEY)",
        "DROP TABLE missing",
        "SELEC * FROM users",
    ];
    for sql in bad {
        assert!(execute(&db, sql).is_err(), "should fail: {sql}");
    }
    assert_eq!(ints(&q(&db, "SELECT id FROM users"), 0), vec![1, 2, 3, 4]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn text_keys_and_drop_table() {
    let dir = temp_dir("text");
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE tags (label TEXT PRIMARY KEY, uses INT)");
    q(&db, "INSERT INTO tags VALUES ('rust', 3), ('db', 5), ('sql', 1)");
    let labels: Vec<String> = q(&db, "SELECT label FROM tags")
        .rows
        .into_iter()
        .map(|r| match &r[0] {
            Value::Text(s) => s.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(labels, vec!["db", "rust", "sql"]);
    q(&db, "DROP TABLE tags");
    assert!(execute(&db, "SELECT * FROM tags").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sql_data_survives_reopen() {
    let dir = temp_dir("reopen");
    {
        let db = Database::open(&dir).unwrap();
        q(&db, "CREATE TABLE t (id INT PRIMARY KEY, v TEXT)");
        q(&db, "INSERT INTO t VALUES (1, 'a'), (2, 'b')");
        q(&db, "UPDATE t SET v = 'z' WHERE id = 2");
        q(&db, "DELETE FROM t WHERE id = 1");
    }
    let db = Database::open(&dir).unwrap();
    let rows = q(&db, "SELECT * FROM t").rows;
    assert_eq!(rows, vec![vec![Value::Int(2), Value::Text("z".into())]]);
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn offset_pages_through_ordered_rows() {
    let (db, dir) = seeded("offset");
    assert_eq!(ints(&q(&db, "SELECT id FROM users ORDER BY id LIMIT 2 OFFSET 1"), 0), vec![2, 3]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users ORDER BY id LIMIT 2 OFFSET 3"), 0), vec![4]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users ORDER BY id DESC LIMIT 2 OFFSET 2"), 0), vec![2, 1]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users ORDER BY id OFFSET 2"), 0), vec![3, 4]);
    assert_eq!(ints(&q(&db, "SELECT id FROM users ORDER BY id LIMIT 10 OFFSET 0"), 0), vec![1, 2, 3, 4]);
    assert!(q(&db, "SELECT id FROM users ORDER BY id LIMIT 2 OFFSET 10").rows.is_empty());
    assert!(q(&db, "SELECT id FROM users LIMIT 0 OFFSET 1").rows.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn offset_applies_to_groups_and_aggregates() {
    let (db, dir) = seeded("offset_groups");
    assert_eq!(ints(&q(&db, "SELECT age FROM users WHERE age > 0 GROUP BY age ORDER BY age LIMIT 1 OFFSET 1"), 0), vec![41]);
    assert_eq!(q(&db, "SELECT COUNT(*) FROM users OFFSET 0").rows.len(), 1);
    assert!(q(&db, "SELECT COUNT(*) FROM users OFFSET 1").rows.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn offset_rejects_invalid_values() {
    let (db, dir) = seeded("offset_bad");
    assert!(execute(&db, "SELECT id FROM users LIMIT 1 OFFSET -1").is_err());
    assert!(execute(&db, "SELECT id FROM users LIMIT 1 OFFSET 'x'").is_err());
    assert!(execute(&db, "SELECT id FROM users LIMIT 1 OFFSET").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
