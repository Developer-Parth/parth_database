use parth_db::exec::{execute, QueryResult};
use parth_db::value::Value;
use parth_db::Database;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_grp_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn q(db: &Database, sql: &str) -> QueryResult {
    execute(db, sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn failure(db: &Database, sql: &str) -> String {
    match execute(db, sql) {
        Ok(_) => panic!("expected an error for: {sql}"),
        Err(e) => format!("{e}"),
    }
}

fn seeded(tag: &str) -> (Database, PathBuf) {
    let dir = temp_dir(tag);
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE sales (id INT PRIMARY KEY, region TEXT, product TEXT, qty INT, price REAL)");
    q(
        &db,
        "INSERT INTO sales VALUES (1, 'east', 'widget', 10, 2.5), (2, 'east', 'gadget', 5, 4.0), (3, 'west', 'widget', 7, 2.5), (4, 'west', 'widget', 3, 2.5), (5, 'north', 'gadget', NULL, 4.0)",
    );
    (db, dir)
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

#[test]
fn counts_per_group_in_key_order() {
    let (db, dir) = seeded("count");
    let result = q(&db, "SELECT region, COUNT(*) FROM sales GROUP BY region");
    assert_eq!(result.columns, vec!["region".to_string(), "count(*)".to_string()]);
    assert_eq!(
        result.rows,
        vec![
            vec![text("east"), Value::Int(2)],
            vec![text("north"), Value::Int(1)],
            vec![text("west"), Value::Int(2)],
        ]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sums_and_extremes_per_group() {
    let (db, dir) = seeded("sum");
    let result = q(&db, "SELECT region, SUM(qty), MAX(price) FROM sales GROUP BY region");
    assert_eq!(
        result.rows,
        vec![
            vec![text("east"), Value::Int(15), Value::Real(4.0)],
            vec![text("north"), Value::Null, Value::Real(4.0)],
            vec![text("west"), Value::Int(10), Value::Real(2.5)],
        ]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn groups_by_two_columns() {
    let (db, dir) = seeded("two");
    let result = q(&db, "SELECT region, product, COUNT(*) FROM sales GROUP BY region, product");
    assert_eq!(result.rows.len(), 4);
    assert_eq!(
        result.rows[3],
        vec![text("west"), text("widget"), Value::Int(2)]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn filter_order_and_limit_apply_to_groups() {
    let (db, dir) = seeded("clauses");
    let result = q(
        &db,
        "SELECT region, COUNT(*) FROM sales WHERE qty > 4 GROUP BY region ORDER BY region DESC LIMIT 1",
    );
    assert_eq!(result.rows, vec![vec![text("west"), Value::Int(1)]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn aggregate_only_and_column_only_lists_work() {
    let (db, dir) = seeded("shapes");
    let counts = q(&db, "SELECT COUNT(*) FROM sales GROUP BY region");
    assert_eq!(counts.rows.len(), 3);
    let regions = q(&db, "SELECT region FROM sales GROUP BY region");
    assert_eq!(
        regions.rows,
        vec![vec![text("east")], vec![text("north")], vec![text("west")]]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn no_matching_rows_gives_no_groups() {
    let (db, dir) = seeded("empty");
    let result = q(&db, "SELECT region, COUNT(*) FROM sales WHERE qty > 100 GROUP BY region");
    assert!(result.rows.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unsupported_forms_are_rejected_explicitly() {
    let (db, dir) = seeded("errors");
    assert!(failure(&db, "SELECT region, COUNT(*) FROM sales GROUP BY region HAVING COUNT(*) > 1").contains("HAVING is not supported"));
    assert!(failure(&db, "SELECT region, product, COUNT(*) FROM sales GROUP BY region").contains("must appear in GROUP BY"));
    assert!(failure(&db, "SELECT region, COUNT(*) FROM sales").contains("GROUP BY"));
    assert!(failure(&db, "SELECT * FROM sales GROUP BY region").contains("not supported with GROUP BY"));
    assert!(failure(&db, "SELECT region, COUNT(*) FROM sales GROUP BY region ORDER BY qty").contains("grouping column"));
    assert!(failure(&db, "SELECT region, COUNT(*) FROM sales GROUP BY nope").to_lowercase().contains("nope"));
    let _ = std::fs::remove_dir_all(dir);
}
