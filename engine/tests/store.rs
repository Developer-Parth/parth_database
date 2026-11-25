use parth_db::{Batch, Error, Options, Store, SyncMode};
use std::fs;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("parth_db_test_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&p);
    p
}

fn opts() -> Options {
    Options { sync: SyncMode::Always, checkpoint_bytes: 0 }
}

#[test]
fn put_get_delete_survive_reopen() {
    let d = tmpdir("reopen");
    {
        let s = Store::open(&d, opts()).unwrap();
        s.put("a", "1").unwrap();
        s.put("b", "2").unwrap();
        s.delete("a").unwrap();
    }
    let s = Store::open(&d, opts()).unwrap();
    assert_eq!(s.get(b"a"), None);
    assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
    assert_eq!(s.stats().recovered_batches, 3);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn batch_is_atomic_when_log_is_cut_mid_record() {
    let d = tmpdir("atomic");
    {
        let s = Store::open(&d, opts()).unwrap();
        s.put("base", "x").unwrap();
        let mut b = Batch::new();
        b.put("k1", "v1").put("k2", "v2").delete("base");
        s.write(b).unwrap();
    }
    let wal = d.join("wal.log");
    let full = fs::read(&wal).unwrap();
    // Cut the log at every byte inside the second record: the batch must be
    // either fully absent or (only when intact) fully present.
    let first_len = 8 + (8 + 4 + 1 + (4 + 4) + (4 + 1)); // header + seq,count,tag,klen+base,vlen+x
    for cut in (first_len + 1)..full.len() {
        fs::write(&wal, &full[..cut]).unwrap();
        let s = Store::open(&d, opts()).unwrap();
        assert_eq!(s.get(b"base"), Some(b"x".to_vec()), "cut {cut}");
        assert_eq!(s.get(b"k1"), None, "cut {cut}");
        assert_eq!(s.get(b"k2"), None, "cut {cut}");
        assert!(s.stats().discarded_tail_bytes > 0);
    }
    fs::write(&wal, &full).unwrap();
    let s = Store::open(&d, opts()).unwrap();
    assert_eq!(s.get(b"base"), None);
    assert_eq!(s.get(b"k1"), Some(b"v1".to_vec()));
    assert_eq!(s.get(b"k2"), Some(b"v2".to_vec()));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn checkpoint_then_more_writes_recover() {
    let d = tmpdir("checkpoint");
    {
        let s = Store::open(&d, opts()).unwrap();
        for i in 0..100 {
            s.put(format!("key{i:03}"), format!("val{i}")).unwrap();
        }
        s.checkpoint().unwrap();
        assert_eq!(s.stats().wal_bytes, 0);
        s.put("after", "cp").unwrap();
        s.delete("key000").unwrap();
    }
    let s = Store::open(&d, opts()).unwrap();
    assert_eq!(s.len(), 100);
    assert_eq!(s.get(b"after"), Some(b"cp".to_vec()));
    assert_eq!(s.get(b"key000"), None);
    assert_eq!(s.get(b"key099"), Some(b"val99".to_vec()));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn crash_between_snapshot_rename_and_wal_reset_is_safe() {
    let d = tmpdir("crash_cp");
    let saved_wal;
    {
        let s = Store::open(&d, opts()).unwrap();
        s.put("a", "1").unwrap();
        s.put("b", "2").unwrap();
        saved_wal = fs::read(d.join("wal.log")).unwrap();
        s.checkpoint().unwrap();
    }
    // Simulate a crash after the snapshot was renamed but before the WAL was emptied.
    fs::write(d.join("wal.log"), &saved_wal).unwrap();
    let s = Store::open(&d, opts()).unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(s.stats().recovered_batches, 0, "records already in the snapshot are skipped");
    s.put("c", "3").unwrap();
    drop(s);
    let s = Store::open(&d, opts()).unwrap();
    assert_eq!(s.len(), 3);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn leftover_snapshot_tmp_is_ignored() {
    let d = tmpdir("tmpsnap");
    {
        let s = Store::open(&d, opts()).unwrap();
        s.put("a", "1").unwrap();
    }
    fs::write(d.join("snapshot.tmp"), b"half written garbage").unwrap();
    let s = Store::open(&d, opts()).unwrap();
    assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
    assert!(!d.join("snapshot.tmp").exists());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn corrupt_snapshot_is_reported_not_ignored() {
    let d = tmpdir("badsnap");
    {
        let s = Store::open(&d, opts()).unwrap();
        s.put("a", "1").unwrap();
        s.checkpoint().unwrap();
    }
    let mut data = fs::read(d.join("snapshot.pdb")).unwrap();
    let mid = data.len() / 2;
    data[mid] ^= 0xFF;
    fs::write(d.join("snapshot.pdb"), data).unwrap();
    match Store::open(&d, opts()) {
        Err(Error::Corrupt(_)) => {}
        other => panic!("expected Corrupt, got {:?}", other.map(|_| ())),
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn second_open_is_rejected_while_first_is_alive() {
    let d = tmpdir("lock");
    let s = Store::open(&d, opts()).unwrap();
    match Store::open(&d, opts()) {
        Err(Error::Locked) => {}
        other => panic!("expected Locked, got {:?}", other.map(|_| ())),
    }
    drop(s);
    Store::open(&d, opts()).unwrap();
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn range_and_prefix_scans_are_ordered() {
    let d = tmpdir("scan");
    let s = Store::open(&d, opts()).unwrap();
    for k in ["user:1", "user:2", "user:3", "order:1", "zzz"] {
        s.put(k, k).unwrap();
    }
    let users = s.scan_prefix(b"user:", 100);
    let keys: Vec<_> = users.iter().map(|(k, _)| String::from_utf8(k.clone()).unwrap()).collect();
    assert_eq!(keys, ["user:1", "user:2", "user:3"]);
    assert_eq!(s.scan_prefix(b"user:", 2).len(), 2);
    let r = s.range(Some(b"order:"), Some(b"user:2"), 100);
    assert_eq!(r.len(), 2);
    assert_eq!(s.range(Some(b"b"), Some(b"a"), 10).len(), 0);
    drop(s);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn rejects_empty_and_oversized_keys() {
    let d = tmpdir("limits");
    let s = Store::open(&d, opts()).unwrap();
    assert!(matches!(s.put("", "x"), Err(Error::InvalidInput(_))));
    assert!(matches!(s.put(vec![b'k'; 70_000], "x"), Err(Error::InvalidInput(_))));
    assert_eq!(s.len(), 0);
    drop(s);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn auto_checkpoint_keeps_wal_bounded() {
    let d = tmpdir("auto");
    let o = Options { sync: SyncMode::Never, checkpoint_bytes: 4096 };
    {
        let s = Store::open(&d, o.clone()).unwrap();
        for i in 0..500 {
            s.put(format!("k{i}"), vec![b'x'; 64]).unwrap();
        }
        assert!(s.stats().wal_bytes < 4096 + 200);
    }
    let s = Store::open(&d, o).unwrap();
    assert_eq!(s.len(), 500);
    drop(s);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn concurrent_writers_do_not_lose_updates() {
    let d = tmpdir("conc");
    let o = Options { sync: SyncMode::Never, checkpoint_bytes: 0 };
    let s = Store::open(&d, o.clone()).unwrap();
    let mut hs = Vec::new();
    for t in 0..4 {
        let s = s.clone();
        hs.push(std::thread::spawn(move || {
            for i in 0..200 {
                s.put(format!("t{t}-{i}"), "v").unwrap();
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(s.len(), 800);
    drop(s);
    let s = Store::open(&d, o).unwrap();
    assert_eq!(s.len(), 800);
    assert_eq!(s.stats().sequence, 800);
    drop(s);
    let _ = fs::remove_dir_all(&d);
}
