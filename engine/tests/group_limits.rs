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

#[test]
fn order_by_beyond_top_n_rejects_when_byte_bound_is_exceeded() {
    let db = seeded("fallback");
    let statement = parse("SELECT v FROM t ORDER BY v LIMIT 10 OFFSET 20000").unwrap();
    match statement {
        Statement::Select { table, projection, filter, group_by, order_by, limit, offset } => {
            assert!(select(&db, &table, projection.clone(), filter.clone(), group_by.clone(), order_by.clone(), limit, offset, (usize::MAX, 1)).is_err());
            assert!(select(&db, &table, projection, filter, group_by, order_by, limit, offset, (usize::MAX, usize::MAX)).is_ok());
        }
        _ => panic!(),
    }
}

#[test]
fn order_by_beyond_top_n_rejects_inside_transaction_with_pending_write() {
    let db = seeded("txn");
    let statement = parse("SELECT v FROM t ORDER BY v LIMIT 10 OFFSET 20000").unwrap();
    let rejected = db.transaction(false, |tx| {
        execute(tx, "INSERT INTO t VALUES (100, 'p')")?;
        match statement {
            Statement::Select { table, projection, filter, group_by, order_by, limit, offset } => {
                Ok(select(tx, &table, projection, filter, group_by, order_by, limit, offset, (usize::MAX, 1)).is_err())
            }
            _ => Ok(false),
        }
    });
    assert!(rejected.unwrap());
}

#[test]
fn transaction_scan_merges_pending_inserts_updates_and_deletes_in_order() {
    let db = seeded("merge");
    let queries = ["SELECT id, v FROM t", "SELECT id, v FROM t LIMIT 5 OFFSET 3", "SELECT id FROM t LIMIT 2"];
    let inside = db.transaction(true, |tx| {
        execute(tx, "INSERT INTO t VALUES (0, 'first')")?;
        execute(tx, "INSERT INTO t VALUES (100, 'last')")?;
        execute(tx, "DELETE FROM t WHERE id = 5")?;
        execute(tx, "UPDATE t SET v = 'zz' WHERE id = 7")?;
        let mut seen = Vec::new();
        for q in queries {
            seen.push(format!("{:?}", execute(tx, q)?.rows));
        }
        Ok(seen)
    }).unwrap();
    for (q, before) in queries.iter().zip(inside.iter()) {
        assert_eq!(&format!("{:?}", execute(&db, q).unwrap().rows), before);
    }
    assert!(inside[0].contains("zz") && inside[0].contains("first") && inside[0].contains("last"));
    assert_eq!(execute(&db, "SELECT id FROM t").unwrap().rows.len(), 40);
}