use parth_db::exec::execute;
use parth_db::table::Database;

fn count(db: &Database, sql: &str) -> usize {
    execute(db, sql).unwrap().rows.len()
}

#[test]
fn like_matches_text_with_wildcards() {
    let dir = std::env::temp_dir().join(format!("pd_like_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = Database::open(&dir).unwrap();
    execute(&db, "CREATE TABLE u (id INT PRIMARY KEY, s TEXT)").unwrap();
    execute(&db, "INSERT INTO u VALUES (1, 'abc'), (2, 'abd'), (3, 'xbc'), (4, NULL), (5, '')").unwrap();
    assert_eq!(count(&db, "SELECT * FROM u WHERE s LIKE 'a%'"), 2);
    assert_eq!(count(&db, "SELECT * FROM u WHERE s LIKE '%bc'"), 2);
    assert_eq!(count(&db, "SELECT * FROM u WHERE s LIKE 'a_c'"), 1);
    assert_eq!(count(&db, "SELECT * FROM u WHERE s LIKE '%'"), 4);
    assert_eq!(count(&db, "SELECT * FROM u WHERE s LIKE 'abc'"), 1);
    assert_eq!(count(&db, "SELECT * FROM u WHERE s LIKE 'z%'"), 0);
    assert_eq!(count(&db, "SELECT * FROM u WHERE s LIKE 'a__'"), 2);
    assert_eq!(count(&db, "SELECT * FROM u WHERE s LIKE '_'"), 0);
    assert_eq!(count(&db, "SELECT * FROM u WHERE NOT s LIKE 'a%'"), 2);
    assert!(execute(&db, "SELECT * FROM u WHERE id LIKE '1'").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}