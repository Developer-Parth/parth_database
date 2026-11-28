use parth_db::exec::execute;
use parth_db::table::Database;

fn open(name: &str) -> Database {
    let dir = std::env::temp_dir().join(format!("pdb_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    Database::open(&dir).unwrap()
}

fn seed(db: &Database) {
    execute(db, "CREATE TABLE t (id INT PRIMARY KEY, g TEXT, v INT)").unwrap();
    execute(db, "INSERT INTO t VALUES (1, 'a', 1), (2, 'a', 2), (3, 'a', 3), (4, 'b', 10), (5, 'c', NULL), (6, 'c', NULL)").unwrap();
}

fn firsts(db: &Database, sql: &str) -> Vec<String> {
    let rows = execute(db, sql).unwrap().rows;
    rows.iter().map(|row| format!("{:?}", row[0])).collect()
}
fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|name| format!("Text(\"{}\")", name)).collect()
}

#[test]
fn order_by_count_with_unequal_groups() {
    let db = open("go_count");
    seed(&db);
    let desc = firsts(&db, "SELECT g, COUNT(*) FROM t GROUP BY g ORDER BY COUNT(*) DESC");
    assert_eq!(desc, names(&["a", "c", "b"]));
    let asc = firsts(&db, "SELECT g, COUNT(*) FROM t GROUP BY g ORDER BY COUNT(*) ASC");
    assert_eq!(asc, names(&["b", "c", "a"]));
}

#[test]
fn order_by_aggregate_with_limit_and_offset() {
    let db = open("go_limit");
    seed(&db);
    let page = firsts(&db, "SELECT g, COUNT(*) FROM t GROUP BY g ORDER BY COUNT(*) DESC LIMIT 1 OFFSET 1");
    assert_eq!(page, names(&["c"]));
}

#[test]
fn order_by_sum_places_null_aggregate_first_ascending() {
    let db = open("go_sum");
    seed(&db);
    let asc = firsts(&db, "SELECT g, SUM(v) FROM t GROUP BY g ORDER BY SUM(v) ASC");
    assert_eq!(asc, names(&["c", "a", "b"]));
    let desc = firsts(&db, "SELECT g, SUM(v) FROM t GROUP BY g ORDER BY SUM(v) DESC");
    assert_eq!(desc, names(&["b", "a", "c"]));
}

#[test]
fn grouping_column_ordering_still_works() {
    let db = open("go_key");
    seed(&db);
    let desc = firsts(&db, "SELECT g, COUNT(*) FROM t GROUP BY g ORDER BY g DESC");
    assert_eq!(desc, names(&["c", "b", "a"]));
}

#[test]
fn aggregate_missing_from_select_list_is_rejected() {
    let db = open("go_missing");
    seed(&db);
    assert!(execute(&db, "SELECT g FROM t GROUP BY g ORDER BY COUNT(*) DESC").is_err());
}

#[test]
fn aggregate_order_without_group_by_is_rejected() {
    let db = open("go_plain");
    seed(&db);
    assert!(execute(&db, "SELECT id FROM t ORDER BY COUNT(*) DESC").is_err());
    assert!(execute(&db, "SELECT COUNT(*) FROM t ORDER BY COUNT(*) DESC").is_err());
}