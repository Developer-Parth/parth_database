use parth_db::exec::execute;
use parth_db::table::Database;

#[test]
fn unsupported_constructs_are_errors_not_silent_results() {
    let dir = std::env::temp_dir().join(format!("pd_unsupported_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = Database::open(&dir).unwrap();
    execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, n INT)").unwrap();
    execute(&db, "INSERT INTO t VALUES (1, 5), (2, 7)").unwrap();
    execute(&db, "CREATE TABLE u (id INT PRIMARY KEY, s TEXT)").unwrap();
    execute(&db, "INSERT INTO u VALUES (1, 'abc')").unwrap();
    let unsupported = [
        "SELECT * FROM t JOIN u ON t.id = u.id",
        "SELECT * FROM t UNION SELECT * FROM t",
        "SELECT n FROM t GROUP BY n HAVING n > 1",
    ];
    for sql in unsupported {
        assert!(execute(&db, sql).is_err(), "{sql}");
    }
    assert_eq!(execute(&db, "SELECT n FROM t").unwrap().rows.len(), 2);
    assert_eq!(execute(&db, "SELECT s FROM u").unwrap().rows.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}