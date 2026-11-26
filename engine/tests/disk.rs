use parth_db::table::{Column, ColumnType, Schema};
use parth_db::value::Value;
use parth_db::{Batch, Database, DiskStore, Error, Options, Store, SyncMode};
use std::fs;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("parth_disk_test_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&p);
    p
}

fn legacy_opts() -> Options {
    Options { sync: SyncMode::Always, checkpoint_bytes: 0 }
}

#[test]
fn batches_are_atomic_and_scans_are_ordered() {
    let d = tmpdir("batch");
    let s = DiskStore::open(&d).unwrap();
    s.put("p/2", "b").unwrap();
    s.put("p/1", "a").unwrap();
    s.put("q/1", "z").unwrap();
    let rows = s.scan_prefix(b"p/", usize::MAX).unwrap();
    assert_eq!(rows, vec![(b"p/1".to_vec(), b"a".to_vec()), (b"p/2".to_vec(), b"b".to_vec())]);
    let mut batch = Batch::new();
    batch.put("p/3", "c");
    batch.delete("p/1");
    batch.put(vec![b'k'; 900], vec![0u8; 400]);
    assert!(matches!(s.write(batch), Err(Error::InvalidInput(_))));
    assert_eq!(s.get(b"p/3").unwrap(), None);
    assert_eq!(s.get(b"p/1").unwrap(), Some(b"a".to_vec()));
    assert_eq!(s.len(), 3);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn second_open_is_locked_and_reopen_keeps_data() {
    let d = tmpdir("lock");
    let s = DiskStore::open(&d).unwrap();
    s.put("a", "1").unwrap();
    assert!(matches!(DiskStore::open(&d), Err(Error::Locked)));
    drop(s);
    let s = DiskStore::open(&d).unwrap();
    assert_eq!(s.get(b"a").unwrap(), Some(b"1".to_vec()));
    assert_eq!(s.check().unwrap().entries, 1);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn legacy_wal_and_snapshot_data_migrates_without_loss() {
    let d = tmpdir("migrate");
    {
        let s = Store::open(&d, legacy_opts()).unwrap();
        for i in 0..300u32 {
            s.put(format!("k{:04}", i), format!("v{}", i)).unwrap();
        }
        s.checkpoint().unwrap();
        for i in 300..450u32 {
            s.put(format!("k{:04}", i), format!("v{}", i)).unwrap();
        }
        s.delete("k0010").unwrap();
    }
    let s = DiskStore::open(&d).unwrap();
    assert_eq!(s.len(), 449);
    assert_eq!(s.get(b"k0010").unwrap(), None);
    assert_eq!(s.get(b"k0449").unwrap(), Some(b"v449".to_vec()));
    assert_eq!(s.get(b"k0000").unwrap(), Some(b"v0".to_vec()));
    assert_eq!(s.check().unwrap().entries, 449);
    assert!(d.join("data.pdb").exists());
    assert!(d.join("wal.log").exists());
    s.put("k9999", "new").unwrap();
    drop(s);
    let s = DiskStore::open(&d).unwrap();
    assert_eq!(s.len(), 450);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn migration_refuses_entries_that_do_not_fit_and_keeps_legacy_data() {
    let d = tmpdir("toolarge");
    {
        let s = Store::open(&d, legacy_opts()).unwrap();
        s.put("small", "1").unwrap();
        s.put("big", vec![7u8; 5000]).unwrap();
    }
    assert!(matches!(DiskStore::open(&d), Err(Error::InvalidInput(_))));
    assert!(!d.join("data.pdb").exists());
    assert!(!d.join("data.pdb.tmp").exists());
    let s = Store::open(&d, legacy_opts()).unwrap();
    assert_eq!(s.get(b"big").unwrap().len(), 5000);
    assert_eq!(s.get(b"small"), Some(b"1".to_vec()));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn leftover_partial_migration_file_is_ignored_and_redone() {
    let d = tmpdir("partial");
    {
        let s = Store::open(&d, legacy_opts()).unwrap();
        s.put("a", "1").unwrap();
    }
    fs::write(d.join("data.pdb.tmp"), vec![9u8; 5000]).unwrap();
    let s = DiskStore::open(&d).unwrap();
    assert_eq!(s.get(b"a").unwrap(), Some(b"1".to_vec()));
    assert!(!d.join("data.pdb.tmp").exists());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn database_rows_persist_and_oversized_rows_are_rejected() {
    let d = tmpdir("db");
    let schema = Schema {
        name: "notes".into(),
        columns: vec![Column { name: "id".into(), ty: ColumnType::Int }, Column { name: "body".into(), ty: ColumnType::Text }],
        primary_key: 0,
    };
    {
        let db = Database::open(&d).unwrap();
        db.create_table(schema).unwrap();
        for i in 0..800i64 {
            db.insert("notes", &[Value::Int(i), Value::Text(format!("note number {i}"))]).unwrap();
        }
        let big = Value::Text("x".repeat(3000));
        assert!(matches!(db.insert("notes", &[Value::Int(5000), big]), Err(Error::InvalidInput(_))));
        assert!(db.get("notes", &Value::Int(5000)).unwrap().is_none());
    }
    let db = Database::open(&d).unwrap();
    assert_eq!(db.scan("notes").unwrap().len(), 800);
    assert!(db.get("notes", &Value::Int(799)).unwrap().is_some());
    assert_eq!(db.store().check().unwrap().entries, 801);
    let _ = fs::remove_dir_all(&d);
}