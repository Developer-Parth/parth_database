use parth_db::exec::execute;
use parth_db::table::Database;

#[test]
fn select_distinct_is_rejected_explicitly() {
    let dir = std::env::temp_dir().join(format!("pd_distinct_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = Database::open(&dir).unwrap();
    execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, n INT)").unwrap();
    execute(&db, "INSERT INTO t VALUES (1, 5), (2, 5)").unwrap();
    for sql in ["SELECT DISTINCT n FROM t", "SELECT DISTINCT * FROM t"] {
        let message = format!("{}", execute(&db, sql).unwrap_err());
        assert!(message.contains("DISTINCT"), "{message}");
    }
    assert_eq!(execute(&db, "SELECT n FROM t").unwrap().rows.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}