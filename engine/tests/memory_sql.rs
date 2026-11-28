use parth_db::exec::execute;
use parth_db::table::Database;

#[test]
fn sql_runs_against_memory_database() {
    let db = Database::in_memory().unwrap();
    execute(&db, "CREATE TABLE u (id INT PRIMARY KEY, s TEXT)").unwrap();
    execute(&db, "INSERT INTO u VALUES (1, 'a'), (2, 'b'), (3, 'c')").unwrap();
    assert_eq!(execute(&db, "SELECT * FROM u").unwrap().rows.len(), 3);
    execute(&db, "DELETE FROM u WHERE id = 2").unwrap();
    assert_eq!(execute(&db, "SELECT * FROM u").unwrap().rows.len(), 2);
    assert_eq!(execute(&db, "SELECT * FROM u WHERE s LIKE 'a%'").unwrap().rows.len(), 1);
}

#[test]
fn memory_database_creates_no_files() {
    let list = || { let mut v: Vec<_> = std::fs::read_dir(".").unwrap().map(|e| e.unwrap().file_name()).collect(); v.sort(); v };
    let before = list();
    let db = Database::in_memory().unwrap();
    execute(&db, "CREATE TABLE t (id INT PRIMARY KEY)").unwrap();
    execute(&db, "INSERT INTO t VALUES (1)").unwrap();
    assert_eq!(list(), before);
}