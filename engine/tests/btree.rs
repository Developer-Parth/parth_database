use parth_db::btree::{BTree, PAGE_SIZE};
use parth_db::Error;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;

fn tmpfile(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("parth_btree_test_{}_{}.db", name, std::process::id()));
    let _ = fs::remove_file(&p);
    p
}

fn key(i: u64) -> Vec<u8> {
    format!("key{:08}", i).into_bytes()
}

fn val(i: u64) -> Vec<u8> {
    format!("value-{}-{}", i, "x".repeat((i % 40) as usize)).into_bytes()
}

fn flip_byte(path: &PathBuf, offset: u64) {
    let mut f = OpenOptions::new().read(true).write(true).open(path).unwrap();
    f.seek(SeekFrom::Start(offset)).unwrap();
    let mut b = [0u8; 1];
    std::io::Read::read_exact(&mut f, &mut b).unwrap();
    b[0] ^= 0xFF;
    f.seek(SeekFrom::Start(offset)).unwrap();
    f.write_all(&b).unwrap();
}

#[test]
fn insert_get_overwrite() {
    let p = tmpfile("basic");
    let mut t = BTree::open(&p).unwrap();
    t.insert(b"a", b"1").unwrap();
    t.insert(b"b", b"2").unwrap();
    t.insert(b"a", b"3").unwrap();
    assert_eq!(t.get(b"a").unwrap(), Some(b"3".to_vec()));
    assert_eq!(t.get(b"b").unwrap(), Some(b"2".to_vec()));
    assert_eq!(t.get(b"c").unwrap(), None);
    assert_eq!(t.len(), 2);
    let _ = fs::remove_file(&p);
}

#[test]
fn many_inserts_split_pages_and_stay_ordered() {
    let p = tmpfile("split");
    let mut t = BTree::open(&p).unwrap();
    let ops: Vec<_> = (0..3000u64).map(|i| ((i * 7919 % 3000), i)).map(|(k, i)| (key(k), Some(val(i)))).collect();
    for chunk in ops.chunks(100) {
        t.apply(chunk).unwrap();
    }
    let report = t.check().unwrap();
    assert_eq!(report.entries, 3000);
    assert!(report.height >= 2);
    let all = t.range(None, None, usize::MAX).unwrap();
    assert_eq!(all.len(), 3000);
    assert!(all.windows(2).all(|w| w[0].0 < w[1].0));
    let _ = fs::remove_file(&p);
}

#[test]
fn range_bounds_and_limit() {
    let p = tmpfile("range");
    let mut t = BTree::open(&p).unwrap();
    let ops: Vec<_> = (0..1000u64).map(|i| (key(i), Some(val(i)))).collect();
    t.apply(&ops).unwrap();
    let r = t.range(Some(&key(100)), Some(&key(110)), usize::MAX).unwrap();
    assert_eq!(r.len(), 10);
    assert_eq!(r[0].0, key(100));
    assert_eq!(r[9].0, key(109));
    let r = t.range(Some(&key(990)), None, 5).unwrap();
    assert_eq!(r.len(), 5);
    assert_eq!(r[0].0, key(990));
    let r = t.range(Some(&key(5000)), None, 5).unwrap();
    assert!(r.is_empty());
    let _ = fs::remove_file(&p);
}

#[test]
fn remove_everything_leaves_empty_tree() {
    let p = tmpfile("remove");
    let mut t = BTree::open(&p).unwrap();
    let ops: Vec<_> = (0..2000u64).map(|i| (key(i), Some(val(i)))).collect();
    t.apply(&ops).unwrap();
    for i in (0..2000u64).rev().step_by(2) {
        t.remove(&key(i)).unwrap();
    }
    assert_eq!(t.check().unwrap().entries, 1000);
    for i in 0..2000u64 {
        t.remove(&key(i)).unwrap();
    }
    assert_eq!(t.len(), 0);
    assert_eq!(t.check().unwrap().height, 0);
    assert_eq!(t.get(&key(1)).unwrap(), None);
    t.insert(b"again", b"yes").unwrap();
    assert_eq!(t.get(b"again").unwrap(), Some(b"yes".to_vec()));
    let _ = fs::remove_file(&p);
}

#[test]
fn matches_model_under_random_operations() {
    let p = tmpfile("model");
    let mut t = BTree::open(&p).unwrap();
    let mut model: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    let mut state = 12345u64;
    for _ in 0..200 {
        let mut batch = Vec::new();
        for _ in 0..25 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let k = key((state >> 33) % 600);
            if (state >> 20) % 3 == 0 {
                batch.push((k.clone(), None));
                model.remove(&k);
            } else {
                let v = val(state >> 40);
                batch.push((k.clone(), Some(v.clone())));
                model.insert(k, v);
            }
        }
        t.apply(&batch).unwrap();
    }
    let all = t.range(None, None, usize::MAX).unwrap();
    let expected: Vec<_> = model.into_iter().collect();
    assert_eq!(all, expected);
    assert_eq!(t.check().unwrap().entries, expected.len() as u64);
    let _ = fs::remove_file(&p);
}

#[test]
fn data_survives_reopen() {
    let p = tmpfile("reopen");
    {
        let mut t = BTree::open(&p).unwrap();
        let ops: Vec<_> = (0..1500u64).map(|i| (key(i), Some(val(i)))).collect();
        t.apply(&ops).unwrap();
    }
    let mut t = BTree::open(&p).unwrap();
    assert_eq!(t.len(), 1500);
    assert_eq!(t.get(&key(777)).unwrap(), Some(val(777)));
    assert_eq!(t.check().unwrap().entries, 1500);
    let _ = fs::remove_file(&p);
}

#[test]
fn invalid_batch_changes_nothing() {
    let p = tmpfile("invalid");
    let mut t = BTree::open(&p).unwrap();
    t.insert(b"keep", b"me").unwrap();
    let batch = vec![(b"new".to_vec(), Some(b"v".to_vec())), (vec![b'k'; 600], Some(b"v".to_vec()))];
    assert!(matches!(t.apply(&batch), Err(Error::InvalidInput(_))));
    assert_eq!(t.get(b"new").unwrap(), None);
    assert_eq!(t.len(), 1);
    assert!(t.insert(b"", b"v").is_err());
    assert!(t.insert(b"k", &vec![0u8; 1200]).is_err());
    let _ = fs::remove_file(&p);
}

#[test]
fn torn_latest_meta_falls_back_to_previous_commit() {
    let p = tmpfile("torn");
    let txid;
    {
        let mut t = BTree::open(&p).unwrap();
        t.insert(b"first", b"1").unwrap();
        t.insert(b"second", b"2").unwrap();
        txid = t.txid();
    }
    flip_byte(&p, (txid % 2) * PAGE_SIZE as u64 + 20);
    let mut t = BTree::open(&p).unwrap();
    assert_eq!(t.txid(), txid - 1);
    assert_eq!(t.get(b"first").unwrap(), Some(b"1".to_vec()));
    assert_eq!(t.get(b"second").unwrap(), None);
    t.check().unwrap();
    let _ = fs::remove_file(&p);
}

#[test]
fn uncommitted_trailing_pages_are_ignored() {
    let p = tmpfile("trailing");
    {
        let mut t = BTree::open(&p).unwrap();
        let ops: Vec<_> = (0..500u64).map(|i| (key(i), Some(val(i)))).collect();
        t.apply(&ops).unwrap();
    }
    let mut f = OpenOptions::new().append(true).open(&p).unwrap();
    f.write_all(&vec![0xAB; PAGE_SIZE * 3]).unwrap();
    drop(f);
    let mut t = BTree::open(&p).unwrap();
    assert_eq!(t.check().unwrap().entries, 500);
    t.insert(b"after", b"crash").unwrap();
    assert_eq!(t.get(b"after").unwrap(), Some(b"crash".to_vec()));
    assert_eq!(t.check().unwrap().entries, 501);
    let _ = fs::remove_file(&p);
}

#[test]
fn corrupted_data_page_is_detected() {
    let p = tmpfile("corrupt");
    {
        let mut t = BTree::open(&p).unwrap();
        let ops: Vec<_> = (0..400u64).map(|i| (key(i), Some(val(i)))).collect();
        t.apply(&ops).unwrap();
    }
    flip_byte(&p, 2 * PAGE_SIZE as u64 + 100);
    let mut t = BTree::open(&p).unwrap();
    assert!(matches!(t.check(), Err(Error::Corrupt(_))));
    let _ = fs::remove_file(&p);
}

#[test]
fn file_without_valid_meta_is_rejected() {
    let p = tmpfile("nometa");
    fs::write(&p, vec![7u8; PAGE_SIZE * 2]).unwrap();
    assert!(matches!(BTree::open(&p), Err(Error::Corrupt(_))));
    let _ = fs::remove_file(&p);
}