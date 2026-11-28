use parth_db::exec::execute;
use parth_db::table::Database;

fn open(name: &str) -> Database { let dir = std::env::temp_dir().join(format!("pdb_{}_{}", name, std::process::id())); let _ = std::fs::remove_dir_all(&dir); Database::open(&dir).unwrap() }

fn seed(db: &Database) { execute(db, "CREATE TABLE t (id INT PRIMARY KEY, g TEXT, v INT)").unwrap(); execute(db, "INSERT INTO t VALUES (1, 'a', 1), (2, 'a', 2), (3, 'a', 3), (4, 'b', 10), (5, 'c', NULL), (6, 'c', NULL)").unwrap(); }

fn firsts(db: &Database, sql: &str) -> Vec<String> { let rows = execute(db, sql).unwrap().rows; rows.iter().map(|row| format!("{:?}", row[0])).collect() }

fn names(items: &[&str]) -> Vec<String> { items.iter().map(|name| format!("Text({:?})", name)).collect() }

#[test]
fn having_filters_groups_by_count() { let db = open("gh_count"); seed(&db); assert_eq!(firsts(&db, "SELECT g, COUNT(*) FROM t GROUP BY g HAVING COUNT(*) > 1 ORDER BY COUNT(*) DESC"), names(&["a", "c"])); assert_eq!(firsts(&db, "SELECT g, COUNT(*) FROM t GROUP BY g HAVING COUNT(*) = 1"), names(&["b"])); }

#[test]
fn having_null_aggregate_fails_the_condition() { let db = open("gh_sum"); seed(&db); assert_eq!(firsts(&db, "SELECT g, SUM(v) FROM t GROUP BY g HAVING SUM(v) > 5 ORDER BY SUM(v) DESC"), names(&["b", "a"])); }

#[test]
fn having_applies_before_limit() { let db = open("gh_limit"); seed(&db); assert_eq!(firsts(&db, "SELECT g, COUNT(*) FROM t GROUP BY g HAVING COUNT(*) != 1 ORDER BY COUNT(*) DESC LIMIT 1"), names(&["a"])); }

#[test]
fn having_combines_conditions_with_and() { let db = open("gh_and"); seed(&db); assert_eq!(firsts(&db, "SELECT g, COUNT(*), SUM(v) FROM t GROUP BY g HAVING COUNT(*) > 1 AND SUM(v) > 5"), names(&["a"])); assert_eq!(firsts(&db, "SELECT g, COUNT(*) FROM t GROUP BY g HAVING COUNT(*) > 1 AND COUNT(*) < 3"), names(&["c"])); assert_eq!(firsts(&db, "SELECT g, COUNT(*), SUM(v) FROM t GROUP BY g HAVING COUNT(*) > 0 AND COUNT(*) < 3 AND SUM(v) > 5"), names(&["b"])); assert!(firsts(&db, "SELECT g, COUNT(*), SUM(v) FROM t GROUP BY g HAVING COUNT(*) > 5 AND SUM(v) > 0").is_empty()); }

#[test] fn having_on_aggregate_outside_select_list() { let db = open("gh_hidden"); seed(&db); let r = execute(&db, "SELECT g FROM t GROUP BY g HAVING SUM(v) > 5 ORDER BY g").unwrap(); assert_eq!(r.columns, vec!["g".to_string()]); assert!(r.rows.iter().all(|row| row.len() == 1)); assert_eq!(firsts(&db, "SELECT g FROM t GROUP BY g HAVING SUM(v) > 5 ORDER BY g"), names(&["a", "b"])); assert_eq!(firsts(&db, "SELECT g FROM t GROUP BY g HAVING COUNT(v) = 0"), names(&["c"])); let r = execute(&db, "SELECT g, COUNT(*) FROM t GROUP BY g HAVING SUM(v) > 5 ORDER BY g").unwrap(); assert!(r.rows.iter().all(|row| row.len() == 2)); assert_eq!(r.rows.len(), 2); assert_eq!(firsts(&db, "SELECT g, SUM(v) FROM t GROUP BY g HAVING COUNT(*) >= 2 ORDER BY g"), names(&["a", "c"])); assert!(execute(&db, "SELECT g FROM t GROUP BY g HAVING SUM(v) > 100").unwrap().rows.is_empty()); assert!(execute(&db, "SELECT g FROM t WHERE id > 100 GROUP BY g HAVING SUM(nope) > 1").is_err()); assert!(execute(&db, "SELECT g FROM t GROUP BY g HAVING SUM(g) > 1").is_err()); }
#[test] fn hidden_count_distinct_and_text_min_max() { let db = open("gh_distinct"); seed(&db); let r = execute(&db, "SELECT g FROM t GROUP BY g HAVING COUNT(DISTINCT v) > 1").unwrap(); assert_eq!(r.columns, vec!["g".to_string()]); assert!(r.rows.iter().all(|row| row.len() == 1)); assert_eq!(firsts(&db, "SELECT g FROM t GROUP BY g HAVING COUNT(DISTINCT v) > 1"), names(&["a"])); assert_eq!(firsts(&db, "SELECT g FROM t GROUP BY g HAVING COUNT(DISTINCT v) = 0"), names(&["c"])); assert_eq!(firsts(&db, "SELECT g FROM t GROUP BY g HAVING COUNT(DISTINCT v) >= 1 ORDER BY g"), names(&["a", "b"])); assert_eq!(firsts(&db, "SELECT g FROM t GROUP BY g HAVING MAX(g) = 'b'"), names(&["b"])); assert_eq!(firsts(&db, "SELECT g FROM t GROUP BY g HAVING MIN(g) < 'c' ORDER BY g"), names(&["a", "b"])); assert_eq!(firsts(&db, "SELECT g FROM t GROUP BY g HAVING MIN(v) < 100 ORDER BY g"), names(&["a", "b"])); assert!(execute(&db, "SELECT g FROM t WHERE id > 100 GROUP BY g HAVING COUNT(DISTINCT v) > 1").unwrap().rows.is_empty()); }
#[test]fn hidden_aggregate_types_are_checked_without_rows() { let db = open("gh_types"); seed(&db); assert!(execute(&db, "SELECT g, COUNT(*) FROM t WHERE id > 100 GROUP BY g HAVING SUM(g) > 1").is_err()); assert!(execute(&db, "SELECT g, COUNT(*) FROM t WHERE id > 100 GROUP BY g HAVING AVG(g) > 1").is_err()); assert!(execute(&db, "SELECT g, COUNT(*) FROM t WHERE id > 100 GROUP BY g HAVING SUM(v) > 1").unwrap().rows.is_empty()); }
#[test]
fn unsupported_having_forms_are_rejected() { let db = open("gh_reject"); seed(&db); assert!(execute(&db, "SELECT g, COUNT(*) FROM t HAVING COUNT(*) > 1").is_err()); assert!(execute(&db, "SELECT g, COUNT(*) FROM t GROUP BY g HAVING COUNT(*) > 1 OR").is_err()); assert!(execute(&db, "SELECT g, COUNT(*) FROM t GROUP BY g HAVING g > 1").is_err()); assert!(execute(&db, "SELECT g FROM t GROUP BY g HAVING g > 1").is_err()); }

#[test]
fn having_or_matches_the_union_of_its_branches() {
    let db = open("gh_or");
    seed(&db);
    let q = |cond: &str| {
        firsts(&db, &format!("SELECT g, COUNT(*) FROM t GROUP BY g HAVING {} ORDER BY g", cond))
    };
    let mut expected = q("COUNT(*) = 1");
    expected.extend(q("COUNT(*) > 1 AND SUM(v) > 5"));
    expected.sort();
    assert_eq!(q("COUNT(*) = 1 OR COUNT(*) > 1 AND SUM(v) > 5"), expected);
    assert_eq!(q("COUNT(*) > 1 AND SUM(v) > 5 OR COUNT(*) = 1"), expected);
}

#[test]
fn having_or_with_a_hidden_aggregate_and_nulls() {
    let db = open("gh_or_hidden");
    seed(&db);
    let q = |cond: &str| firsts(&db, &format!("SELECT g FROM t GROUP BY g HAVING {} ORDER BY g", cond));
    let mut expected = q("COUNT(*) = 1");
    expected.extend(q("SUM(v) > 5"));
    expected.sort();
    expected.dedup();
    assert_eq!(q("COUNT(*) = 1 OR SUM(v) > 5"), expected);
    assert!(execute(&db, "SELECT g FROM t GROUP BY g HAVING COUNT(*) > 1 OR").is_err());
}