use parth_db::exec::{execute, QueryResult, MAX_RESULT_ROWS};
use parth_db::value::Value;
use parth_db::Database;
use std::path::PathBuf;

const TOTAL: usize = 10050;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_cap_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn q(db: &Database, sql: &str) -> QueryResult {
    execute(db, sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn big_table(tag: &str) -> Database {
    let dir = temp_dir(tag);
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE t (id INT PRIMARY KEY, v TEXT)");
    let mut sql = String::from("INSERT INTO t VALUES ");
    for i in 1..=TOTAL {
        if i > 1 {
            sql.push_str(", ");
        }
        sql.push_str(&format!("({}, 'row{}')", i, i));
    }
    q(&db, &sql);
    db
}

#[test]
fn unlimited_select_is_capped_and_flagged() {
    let db = big_table("unlimited");
    let r = q(&db, "SELECT id FROM t");
    assert_eq!(r.rows.len(), MAX_RESULT_ROWS);
    assert!(r.truncated);
}

#[test]
fn explicit_limit_under_cap_is_not_flagged() {
    let db = big_table("explicit");
    let r = q(&db, "SELECT id FROM t LIMIT 10");
    assert_eq!(r.rows.len(), 10);
    assert!(!r.truncated);
}

#[test]
fn explicit_limit_over_cap_is_capped_and_flagged() {
    let db = big_table("over");
    let r = q(&db, "SELECT id FROM t LIMIT 20000");
    assert_eq!(r.rows.len(), MAX_RESULT_ROWS);
    assert!(r.truncated);
}

#[test]
fn order_by_applies_before_the_cap() {
    let db = big_table("order");
    let r = q(&db, "SELECT id FROM t ORDER BY id DESC");
    assert_eq!(r.rows.len(), MAX_RESULT_ROWS);
    assert_eq!(r.rows[0][0], Value::Int(TOTAL as i64));
    assert!(r.truncated);
}

#[test]
fn filter_and_offset_apply_before_the_cap() {
    let db = big_table("filter");
    let r = q(&db, "SELECT id FROM t WHERE id <= 5");
    assert_eq!(r.rows.len(), 5);
    assert!(!r.truncated);
    let r = q(&db, "SELECT id FROM t LIMIT 100 OFFSET 10040");
    assert_eq!(r.rows.len(), 10);
    assert!(!r.truncated);
}

#[test]
fn plain_select_window_boundary_at_the_cap() {
let db = big_table("boundary");
let exact = q(&db, "SELECT id FROM t LIMIT 10000");
assert_eq!(exact.rows.len(), MAX_RESULT_ROWS);
assert!(!exact.truncated);
let over = q(&db, "SELECT id FROM t LIMIT 10001");
assert_eq!(over.rows.len(), MAX_RESULT_ROWS);
assert!(over.truncated);
let fits = q(&db, "SELECT id FROM t OFFSET 50");
assert_eq!(fits.rows.len(), MAX_RESULT_ROWS);
assert!(!fits.truncated);
let spills = q(&db, "SELECT id FROM t OFFSET 49");
assert_eq!(spills.rows.len(), MAX_RESULT_ROWS);
assert!(spills.truncated);
}
#[test]
fn grouped_result_is_capped_and_flagged() {
    let db = big_table("group");
    let r = q(&db, "SELECT id, COUNT(*) FROM t GROUP BY id");
    assert_eq!(r.rows.len(), MAX_RESULT_ROWS);
    assert!(r.truncated);
}

#[test]
fn aggregate_sees_every_row_and_is_not_flagged() {
    let db = big_table("agg");
    let r = q(&db, "SELECT COUNT(*) FROM t");
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0][0], Value::Int(TOTAL as i64));
    assert!(!r.truncated);
}

#[test]
fn ordered_limit_at_cap_is_not_flagged() {
let db = big_table("ord_at_cap");
let r = q(&db, "SELECT id FROM t ORDER BY id DESC LIMIT 10000");
assert_eq!(r.rows.len(), MAX_RESULT_ROWS);
assert!(!r.truncated);
assert_eq!(r.rows[0][0], Value::Int(TOTAL as i64));
}

#[test]
fn ordered_limit_over_cap_is_flagged_and_matches_full_sort() {
let db = big_table("ord_over_cap");
let full = q(&db, "SELECT id FROM t ORDER BY id DESC");
assert_eq!(full.rows.len(), MAX_RESULT_ROWS);
assert!(full.truncated);
for limit in [10001, 20000] {
let r = q(&db, &format!("SELECT id FROM t ORDER BY id DESC LIMIT {limit}"));
assert_eq!(r.rows.len(), MAX_RESULT_ROWS);
assert!(r.truncated);
assert_eq!(r.rows, full.rows);
}
}

#[test]
fn unlimited_order_by_keeps_the_largest_window() {
let db = big_table("unlimited_order");
let r = q(&db, "SELECT id FROM t ORDER BY id DESC");
assert_eq!(r.rows.len(), MAX_RESULT_ROWS);
assert_eq!(r.rows[0][0], Value::Int(TOTAL as i64));
assert_eq!(r.rows[MAX_RESULT_ROWS - 1][0], Value::Int((TOTAL - MAX_RESULT_ROWS + 1) as i64));
assert!(r.truncated);
let skipped = q(&db, "SELECT id FROM t ORDER BY id DESC OFFSET 10");
assert_eq!(skipped.rows.len(), MAX_RESULT_ROWS);
assert_eq!(skipped.rows[0][0], Value::Int((TOTAL - 10) as i64));
assert!(skipped.truncated);
}

#[test]
fn unlimited_order_by_offset_uses_the_window_up_to_the_bound() {
let db = big_table("offset_window");
let near = q(&db, "SELECT id FROM t ORDER BY id DESC OFFSET 10000");
assert_eq!(near.rows.len(), TOTAL - 10000);
assert_eq!(near.rows[0][0], Value::Int((TOTAL - 10000) as i64));
assert_eq!(near.rows[near.rows.len() - 1][0], Value::Int(1));
assert!(!near.truncated);
let past = q(&db, "SELECT id FROM t ORDER BY id DESC OFFSET 10005");
assert_eq!(past.rows.len(), TOTAL - 10005);
assert_eq!(past.rows[0][0], Value::Int((TOTAL - 10005) as i64));
assert!(!past.truncated);
let mid = q(&db, "SELECT id FROM t ORDER BY id ASC OFFSET 40");
assert_eq!(mid.rows.len(), MAX_RESULT_ROWS);
assert_eq!(mid.rows[0][0], Value::Int(41));
assert!(mid.truncated);
}