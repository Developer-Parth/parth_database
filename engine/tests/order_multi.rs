use parth_db::exec::execute;
use parth_db::table::Database;

fn open(name: &str) -> Database { let dir = std::env::temp_dir().join(format!("pd_omulti_{}_{}", name, std::process::id())); let _ = std::fs::remove_dir_all(&dir); Database::open(&dir).unwrap() }
fn ids(db: &Database, sql: &str) -> String { execute(db, sql).unwrap().rows.iter().map(|r| format!("{:?}", r[0])).collect::<Vec<_>>().join(",") }
fn seeded(name: &str) -> Database { let db = open(name); execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, a INT, b INT)").unwrap(); execute(&db, "INSERT INTO t VALUES (1, 1, 5), (2, 2, 3), (3, 1, 7), (4, 2, 4), (5, 1, 6)").unwrap(); db }

#[test]
fn second_key_breaks_ties_in_the_first() { let db = seeded("tie"); assert_eq!(ids(&db, "SELECT id FROM t ORDER BY a, b"), "Int(1),Int(5),Int(3),Int(2),Int(4)"); }
#[test]
fn directions_apply_per_column() { let db = seeded("dir"); assert_eq!(ids(&db, "SELECT id FROM t ORDER BY a DESC, b ASC"), "Int(2),Int(4),Int(1),Int(5),Int(3)"); assert_eq!(ids(&db, "SELECT id FROM t ORDER BY a ASC, b DESC"), "Int(3),Int(5),Int(1),Int(4),Int(2)"); }
#[test]
fn limit_and_offset_follow_the_full_ordering() { let db = seeded("lim"); assert_eq!(ids(&db, "SELECT id FROM t ORDER BY a, b LIMIT 2 OFFSET 1"), "Int(5),Int(3)"); assert_eq!(ids(&db, "SELECT id FROM t ORDER BY a DESC, b DESC LIMIT 1"), "Int(4)"); }
#[test]
fn single_column_order_is_unchanged() { let db = seeded("one"); assert_eq!(ids(&db, "SELECT id FROM t ORDER BY b DESC"), "Int(3),Int(5),Int(1),Int(4),Int(2)"); assert_eq!(ids(&db, "SELECT id FROM t ORDER BY id DESC LIMIT 2"), "Int(5),Int(4)"); }
#[test]
fn multiple_keys_are_rejected_with_group_by_and_distinct() { let db = seeded("rej"); assert!(execute(&db, "SELECT a, COUNT(*) FROM t GROUP BY a ORDER BY a, COUNT(*)").is_err()); assert!(execute(&db, "SELECT DISTINCT a, b FROM t ORDER BY a, b").is_err()); assert!(execute(&db, "SELECT COUNT(*) FROM t ORDER BY a, b").is_err()); }
