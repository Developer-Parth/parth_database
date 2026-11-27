use parth_db::exec::{execute, select};
use parth_db::parser::{parse, Statement};
use parth_db::Database;

fn seeded(tag: &str) -> Database {
    let dir = std::env::temp_dir().join(format!("parth_gl_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = Database::open(&dir).unwrap();
    execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, v TEXT)").unwrap();
    for i in 1..40 {
        execute(&db, &format!("INSERT INTO t VALUES ({}, 'x{}')", i, i % 4)).unwrap();
    }
    db
}

fn grouped_ok(db: &Database, materialize: (usize, usize)) -> bool {
    let statement = parse("SELECT v, COUNT(*) FROM t GROUP BY v").unwrap();
    match statement {
        Statement::Select { table, projection, filter, group_by, order_by, limit, offset } => {
            select(db, &table, projection, filter, group_by, order_by, limit, offset, materialize).is_ok()
        }
        _ => false,
    }
}

#[test]
fn group_by_rejects_when_byte_bound_is_exceeded() {
    let db = seeded("bytes");
    assert!(!grouped_ok(&db, (usize::MAX, 1)));
}

#[test]
fn group_by_rejects_when_row_bound_is_exceeded() {
    let db = seeded("rows");
    assert!(!grouped_ok(&db, (1, usize::MAX)));
}

#[test]
fn group_by_succeeds_within_bounds() {
    let db = seeded("ok");
    assert!(grouped_ok(&db, (usize::MAX, usize::MAX)));
}