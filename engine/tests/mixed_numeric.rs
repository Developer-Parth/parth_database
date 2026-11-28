use parth_db::exec::execute;
use parth_db::table::Database;

fn open(name: &str) -> Database {
    let dir = std::env::temp_dir().join(format!("pdb_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Database::open(&dir).unwrap()
}

fn count(db: &Database, sql: &str) -> usize {
    execute(db, sql).unwrap().rows.len()
}

#[test]
fn int_and_real_literals_compare_by_value() {
    let db = open("mixed_numeric");
    execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, n INT, r REAL)").unwrap();
    execute(&db, "INSERT INTO t VALUES (1, 1, 1.0), (2, 2, 2.5), (3, 3, 3.0)").unwrap();
    assert_eq!(count(&db, "SELECT id FROM t WHERE n = 1.0"), 1);
    assert_eq!(count(&db, "SELECT id FROM t WHERE r = 3"), 1);
    assert_eq!(count(&db, "SELECT id FROM t WHERE n < 2.5"), 2);
    assert_eq!(count(&db, "SELECT id FROM t WHERE r >= 3"), 1);
    assert_eq!(count(&db, "SELECT id FROM t WHERE n = 2.5"), 0);
}

#[test]
fn integer_literal_in_real_column_dedups_with_real_value() {
    let db = open("mixed_distinct");
    execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, r REAL)").unwrap();
    execute(&db, "INSERT INTO t VALUES (1, 1.0), (2, 1), (3, 2.5)").unwrap();
    assert_eq!(count(&db, "SELECT DISTINCT r FROM t"), 2);
    assert_eq!(count(&db, "SELECT id FROM t WHERE r = 1"), 2);
    assert_eq!(count(&db, "SELECT id FROM t ORDER BY r DESC LIMIT 2"), 2);
}