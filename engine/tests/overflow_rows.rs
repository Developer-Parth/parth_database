use parth_db::btree::BTree;
use parth_db::exec::{execute, execute_script};
use parth_db::table::Database;
use parth_db::value::Value;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_ovr_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn chunk_keys(dir: &PathBuf) -> usize {
    let mut tree = BTree::open(dir.join("data.pdb")).unwrap();
    tree.range(Some(&[0u8, b'O']), Some(&[0u8, b'P']), 100000).unwrap().len()
}

fn body_of(db: &Database, id: i64) -> Option<String> {
    let result = execute(db, &format!("SELECT body FROM t WHERE id = {id}")).unwrap();
    match result.rows.first() {
        Some(row) => match &row[0] {
            Value::Text(s) => Some(s.clone()),
            other => panic!("not text: {other:?}"),
        },
        None => None,
    }
}

#[test]
fn large_text_row_update_delete_and_reopen() {
    let dir = temp_dir("life");
    let big = "x".repeat(5000);
    let bigger = "y".repeat(20000);
    {
        let db = Database::open(&dir).unwrap();
        execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, body TEXT)").unwrap();
        execute(&db, &format!("INSERT INTO t VALUES (1, '{big}')")).unwrap();
        assert_eq!(body_of(&db, 1), Some(big.clone()));
    }
    assert_eq!(chunk_keys(&dir), 6);
    {
        let db = Database::open(&dir).unwrap();
        assert_eq!(body_of(&db, 1), Some(big.clone()));
        execute(&db, &format!("UPDATE t SET body = '{bigger}' WHERE id = 1")).unwrap();
        assert_eq!(body_of(&db, 1), Some(bigger.clone()));
    }
    assert_eq!(chunk_keys(&dir), 23);
    {
        let db = Database::open(&dir).unwrap();
        execute(&db, "UPDATE t SET body = 'small' WHERE id = 1").unwrap();
        assert_eq!(body_of(&db, 1), Some("small".to_string()));
    }
    assert_eq!(chunk_keys(&dir), 0);
    {
        let db = Database::open(&dir).unwrap();
        execute(&db, &format!("UPDATE t SET body = '{big}' WHERE id = 1")).unwrap();
        execute(&db, "DELETE FROM t WHERE id = 1").unwrap();
        assert_eq!(body_of(&db, 1), None);
    }
    assert_eq!(chunk_keys(&dir), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rolled_back_transaction_leaves_no_chunks_and_commit_keeps_them() {
    let dir = temp_dir("txn");
    let big = "z".repeat(7000);
    {
        let db = Database::open(&dir).unwrap();
        execute(&db, "CREATE TABLE t (id INT PRIMARY KEY, body TEXT)").unwrap();
        execute_script(&db, &format!("BEGIN; INSERT INTO t VALUES (1, '{big}'); ROLLBACK;")).unwrap();
        assert_eq!(body_of(&db, 1), None);
    }
    assert_eq!(chunk_keys(&dir), 0);
    {
        let db = Database::open(&dir).unwrap();
        execute_script(&db, &format!("BEGIN; INSERT INTO t VALUES (2, '{big}'); COMMIT;")).unwrap();
        assert_eq!(body_of(&db, 2), Some(big.clone()));
    }
    assert_eq!(chunk_keys(&dir), 8);
    let db = Database::open(&dir).unwrap();
    assert_eq!(body_of(&db, 2), Some(big));
    let _ = std::fs::remove_dir_all(&dir);
}
