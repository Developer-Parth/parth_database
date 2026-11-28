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