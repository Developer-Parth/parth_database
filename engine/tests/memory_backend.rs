use parth_db::btree::BTree;

#[test]
fn memory_backend_matches_expected_after_many_writes() {
    let mut t = BTree::in_memory().unwrap();
    for i in 0..3000u32 {
        t.insert(format!("k{:05}", i).as_bytes(), &vec![(i % 251) as u8; 40]).unwrap();
    }
    for i in (0..3000u32).step_by(7) { t.remove(format!("k{:05}", i).as_bytes()).unwrap(); }
    for i in 0..3000u32 {
        let got = t.get(format!("k{:05}", i).as_bytes()).unwrap();
        if i % 7 == 0 { assert!(got.is_none()); } else { assert_eq!(got.unwrap(), vec![(i % 251) as u8; 40]); }
    }
    assert_eq!(t.check().unwrap().entries, 2571);
    let all = t.range(None, None, 10000).unwrap();
    assert_eq!(all.len(), 2571);
    assert!(all.windows(2).all(|w| w[0].0 < w[1].0));
}

#[test]
fn memory_starts_empty() {
    let mut t = BTree::in_memory().unwrap();
    assert!(t.get(b"a").unwrap().is_none());
    assert_eq!(t.check().unwrap().entries, 0);
}

#[test]
fn disk_store_in_memory_starts_empty() {
    let s = parth_db::disk::DiskStore::in_memory().unwrap();
    assert!(s.get(b"a").unwrap().is_none());
    assert!(!s.contains(b"a").unwrap());
}