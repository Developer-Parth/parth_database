use parth_db::disk::DiskStore;
use parth_db::txn::Kv;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_scan_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn keys(rows: &[(Vec<u8>, Vec<u8>)]) -> Vec<String> {
    rows.iter().map(|(k, _)| String::from_utf8(k.clone()).unwrap()).collect()
}

#[test]
fn finite_limit_scan_matches_unlimited_merge_prefix() {
    let dir = temp_dir("merge");
    let kv = Kv::new(DiskStore::open(&dir).unwrap());
    for id in 1..=5 {
        kv.put(format!("p/{:02}", id), format!("disk{}", id)).unwrap();
    }
    kv.put("q/1", "other").unwrap();
    let view = kv.view();
    view.put("p/03", "new").unwrap();
    view.delete("p/02").unwrap();
    view.put("p/035", "ins").unwrap();
    let full = view.scan_prefix(b"p/", usize::MAX).unwrap();
    assert_eq!(keys(&full), vec!["p/01", "p/03", "p/035", "p/04", "p/05"]);
    assert_eq!(full[1].1, b"new".to_vec());
    for limit in [0usize, 1, 2, 3, 4, 5, 6, 100] {
        let part = view.scan_prefix(b"p/", limit).unwrap();
        let want = limit.min(full.len());
        assert_eq!(part, full[..want].to_vec(), "limit {}", limit);
    }
    let _ = std::fs::remove_dir_all(&dir);
}