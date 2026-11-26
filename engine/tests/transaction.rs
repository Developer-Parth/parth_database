use parth_db::exec::{execute, execute_script, Outcome, QueryResult};
use parth_db::value::Value;
use parth_db::Database;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_txn_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn q(db: &Database, sql: &str) -> QueryResult {
    execute(db, sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn seeded(tag: &str) -> (Database, PathBuf) {
    let dir = temp_dir(tag);
    let db = Database::open(&dir).unwrap();
    q(&db, "CREATE TABLE users (id INT PRIMARY KEY, name TEXT)");
    q(&db, "INSERT INTO users VALUES (1, 'alan'), (2, 'bea'), (3, 'cara')");
    (db, dir)
}

fn count(db: &Database, table: &str) -> Value {
    q(db, &format!("SELECT COUNT(*) FROM {table}")).rows[0][0].clone()
}

fn results(outcome: Outcome) -> (bool, Vec<QueryResult>) {
    match outcome {
        Outcome::Transaction { committed, results } => (committed, results),
        Outcome::Single(_) => panic!("expected a transaction"),
    }
}

#[test]
fn commit_applies_every_statement() {
    let (db, dir) = seeded("commit");
    let outcome = execute_script(
        &db,
        "BEGIN; INSERT INTO users VALUES (4, 'dev'); UPDATE users SET name = 'bee' WHERE id = 2; DELETE FROM users WHERE id = 1; COMMIT;",
    )
    .unwrap();
    let (committed, done) = results(outcome);
    assert!(committed);
    assert_eq!(done.len(), 3);
    assert_eq!(count(&db, "users"), Value::Int(3));
    assert_eq!(q(&db, "SELECT name FROM users WHERE id = 2").rows[0][0], Value::Text("bee".into()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rollback_discards_everything() {
    let (db, dir) = seeded("rollback");
    let outcome = execute_script(&db, "BEGIN; INSERT INTO users VALUES (4, 'dev'); DELETE FROM users WHERE id = 1; ROLLBACK").unwrap();
    let (committed, _) = results(outcome);
    assert!(!committed);
    assert_eq!(count(&db, "users"), Value::Int(3));
    assert_eq!(q(&db, "SELECT name FROM users WHERE id = 1").rows.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn missing_commit_persists_nothing() {
    let (db, dir) = seeded("nocommit");
    assert!(execute_script(&db, "BEGIN; INSERT INTO users VALUES (4, 'dev')").is_err());
    assert!(execute_script(&db, "BEGIN").is_err());
    assert_eq!(count(&db, "users"), Value::Int(3));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn failing_statement_leaves_no_partial_writes() {
    let (db, dir) = seeded("fail");
    let outcome = execute_script(&db, "BEGIN; INSERT INTO users VALUES (4, 'dev'); INSERT INTO users VALUES (1, 'dup'); COMMIT");
    assert!(outcome.is_err());
    let outcome = execute_script(&db, "BEGIN; INSERT INTO users VALUES (5, 'eve'); SELECT * FROM missing; COMMIT");
    assert!(outcome.is_err());
    assert_eq!(count(&db, "users"), Value::Int(3));
    assert_eq!(q(&db, "SELECT id FROM users WHERE id = 4").rows.len(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn statements_see_their_own_uncommitted_writes() {
    let (db, dir) = seeded("ryw");
    let outcome = execute_script(
        &db,
        "BEGIN; INSERT INTO users VALUES (4, 'dev'); SELECT COUNT(*) FROM users; DELETE FROM users WHERE id = 1; SELECT id FROM users ORDER BY id; UPDATE users SET name = 'zed' WHERE id = 4; SELECT name FROM users WHERE id = 4; ROLLBACK",
    )
    .unwrap();
    let (_, done) = results(outcome);
    assert_eq!(done[1].rows[0][0], Value::Int(4));
    let ids: Vec<Value> = done[3].rows.iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec![Value::Int(2), Value::Int(3), Value::Int(4)]);
    assert_eq!(done[5].rows[0][0], Value::Text("zed".into()));
    assert_eq!(count(&db, "users"), Value::Int(3));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn schema_changes_are_transactional() {
    let (db, dir) = seeded("schema");
    let outcome = execute_script(
        &db,
        "BEGIN; CREATE TABLE notes (id INT PRIMARY KEY, body TEXT); INSERT INTO notes VALUES (1, 'a'); SELECT COUNT(*) FROM notes; COMMIT",
    )
    .unwrap();
    let (_, done) = results(outcome);
    assert_eq!(done[2].rows[0][0], Value::Int(1));
    assert_eq!(count(&db, "notes"), Value::Int(1));
    execute_script(&db, "BEGIN; CREATE TABLE temp (id INT PRIMARY KEY); DROP TABLE notes; ROLLBACK").unwrap();
    assert!(execute(&db, "SELECT * FROM temp").is_err());
    assert_eq!(count(&db, "notes"), Value::Int(1));
    execute_script(&db, "BEGIN; DROP TABLE notes; COMMIT").unwrap();
    assert!(execute(&db, "SELECT * FROM notes").is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unsupported_forms_are_rejected_without_effects() {
    let (db, dir) = seeded("forms");
    let bad = [
        "BEGIN; BEGIN; INSERT INTO users VALUES (9, 'x'); COMMIT",
        "BEGIN; INSERT INTO users VALUES (9, 'x'); COMMIT; INSERT INTO users VALUES (10, 'y')",
        "BEGIN; INSERT INTO users VALUES (9, 'x'); COMMIT; COMMIT",
        "COMMIT",
        "ROLLBACK",
        "INSERT INTO users VALUES (9, 'x'); INSERT INTO users VALUES (10, 'y')",
        "BEGIN; INSERT INTO users VALUES (9, 'x') INSERT INTO users VALUES (10, 'y'); COMMIT",
    ];
    for sql in bad {
        assert!(execute_script(&db, sql).is_err(), "{sql}");
    }
    assert_eq!(count(&db, "users"), Value::Int(3));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn single_statement_still_runs_directly() {
    let (db, dir) = seeded("single");
    match execute_script(&db, "INSERT INTO users VALUES (4, 'dev');").unwrap() {
        Outcome::Single(result) => assert_eq!(result.affected, 1),
        Outcome::Transaction { .. } => panic!("expected a single result"),
    }
    assert_eq!(count(&db, "users"), Value::Int(4));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn committed_work_survives_reopen_and_rolled_back_work_does_not() {
    let (db, dir) = seeded("reopen");
    execute_script(&db, "BEGIN; INSERT INTO users VALUES (4, 'dev'); COMMIT").unwrap();
    execute_script(&db, "BEGIN; INSERT INTO users VALUES (5, 'eve'); ROLLBACK").unwrap();
    assert!(execute_script(&db, "BEGIN; INSERT INTO users VALUES (6, 'fay')").is_err());
    drop(db);
    let db = Database::open(&dir).unwrap();
    assert_eq!(count(&db, "users"), Value::Int(4));
    assert_eq!(q(&db, "SELECT id FROM users WHERE id > 4").rows.len(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn semicolons_inside_quoted_text_do_not_split_statements() {
    let (db, dir) = seeded("semi");
    let outcome = execute_script(&db, "BEGIN; INSERT INTO users VALUES (4, 'a;b; COMMIT; c'); SELECT name FROM users WHERE id = 4; COMMIT").unwrap();
    let (committed, done) = results(outcome);
    assert!(committed);
    assert_eq!(done.len(), 2);
    assert_eq!(done[1].rows[0][0], Value::Text("a;b; COMMIT; c".into()));
    let _ = std::fs::remove_dir_all(dir);
}
