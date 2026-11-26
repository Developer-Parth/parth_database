use parth_db::btree::BTree;
use parth_db::{DiskStore, Error};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_ovc_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn blob(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(7).wrapping_add(3)).collect()
}

fn seed(dir: &PathBuf) {
    let store = DiskStore::open(dir).unwrap();
    store.put(b"big".to_vec(), blob(5000)).unwrap();
    store.check().unwrap();
}

fn raw_chunks(dir: &PathBuf) -> (BTree, Vec<(Vec<u8>, Vec<u8>)>) {
    let mut tree = BTree::open(dir.join("data.pdb")).unwrap();
    let rows = tree.range(Some(&[0u8, b'O']), Some(&[0u8, b'P']), 100).unwrap();
    (tree, rows)
}

fn assert_corrupt(dir: &PathBuf, reads_fail: bool) {
    let store = DiskStore::open(dir).unwrap();
    assert!(matches!(store.check(), Err(Error::Corrupt(_))));
    if reads_fail {
        assert!(matches!(store.get(b"big"), Err(Error::Corrupt(_))));
    }
}

#[test]
fn missing_chunk_is_detected() {
    let dir = temp_dir("missing");
    seed(&dir);
    {
        let (mut tree, rows) = raw_chunks(&dir);
        assert_eq!(rows.len(), 6);
        tree.remove(&rows[2].0).unwrap();
    }
    assert_corrupt(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn flipped_byte_fails_the_checksum() {
    let dir = temp_dir("crc");
    seed(&dir);
    {
        let (mut tree, rows) = raw_chunks(&dir);
        let mut value = rows[1].1.clone();
        value[10] ^= 0xFF;
        tree.insert(&rows[1].0, &value).unwrap();
    }
    assert_corrupt(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn short_chunk_is_detected() {
    let dir = temp_dir("short");
    seed(&dir);
    {
        let (mut tree, rows) = raw_chunks(&dir);
        let value = rows[0].1[..100].to_vec();
        tree.insert(&rows[0].0, &value).unwrap();
    }
    assert_corrupt(&dir, true);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn orphan_chunk_is_detected() {
    let dir = temp_dir("orphan");
    seed(&dir);
    {
        let (mut tree, _) = raw_chunks(&dir);
        let mut key = vec![0u8, b'O'];
        key.extend_from_slice(&9999u64.to_be_bytes());
        key.extend_from_slice(&0u32.to_be_bytes());
        tree.insert(&key, &blob(900)).unwrap();
    }
    assert_corrupt(&dir, false);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_chunk_key_is_detected() {
    let dir = temp_dir("badkey");
    seed(&dir);
    {
        let (mut tree, _) = raw_chunks(&dir);
        tree.insert(&[0u8, b'O', 1, 2, 3], &blob(10)).unwrap();
    }
    assert_corrupt(&dir, false);
    let _ = std::fs::remove_dir_all(&dir);
}
