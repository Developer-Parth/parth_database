use parth_db::{Batch, DiskStore};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pd_ovf_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    dir
}

fn blob(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
}


#[test]
fn values_of_many_sizes_round_trip_and_survive_reopen() {
    let dir = temp_dir("sizes");
    let sizes = [1usize, 400, 999, 1000, 1001, 4096, 5000, 70000, 200000];
    {

        let store = DiskStore::open(&dir).unwrap();
        for (i, size) in sizes.iter().enumerate() {
            store.put(format!("k{i}").into_bytes(), blob(*size, i as u8)).unwrap();
        }
    }
    let store = DiskStore::open(&dir).unwrap();
    for (i, size) in sizes.iter().enumerate() {
        let got = store.get(format!("k{i}").as_bytes()).unwrap();
        assert_eq!(got, Some(blob(*size, i as u8)));
    }
    assert_eq!(store.len(), sizes.len() as u64);
    store.check().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replace_shrink_and_delete_reuse_space() {
    let dir = temp_dir("reuse");
    let store = DiskStore::open(&dir).unwrap();
    for round in 0..30u8 {
        store.put(b"row".to_vec(), blob(50000, round)).unwrap();
    }
    store.put(b"row".to_vec(), blob(10, 1)).unwrap();
    assert_eq!(store.get(b"row").unwrap(), Some(blob(10, 1)));
    store.put(b"row".to_vec(), blob(30000, 2)).unwrap();
    store.delete(b"row".to_vec()).unwrap();
    assert_eq!(store.get(b"row").unwrap(), None);
    assert_eq!(store.len(), 0);
    store.check().unwrap();
    let size = std::fs::metadata(dir.join("data.pdb")).unwrap().len();
    assert!(size < 2_000_000, "data file grew to {size} bytes");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scans_and_len_hide_chunk_keys() {
    let dir = temp_dir("scan");
    let store = DiskStore::open(&dir).unwrap();
    store.put(b"p1".to_vec(), blob(9000, 3)).unwrap();
    store.put(b"p2".to_vec(), blob(20, 4)).unwrap();
    let found = store.scan_prefix(b"p", usize::MAX).unwrap();
    assert_eq!(found.len(), 2);

    assert_eq!(found[0], (b"p1".to_vec(), blob(9000, 3)));
    assert_eq!(found[1], (b"p2".to_vec(), blob(20, 4)));
    assert_eq!(store.range(None, None, usize::MAX).unwrap().len(), 2);
    assert_eq!(store.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}


#[test]
fn batch_with_large_values_is_applied_as_one_unit() {
    let dir = temp_dir("batch");
    {
        let store = DiskStore::open(&dir).unwrap();

        store.put(b"old".to_vec(), blob(8000, 5)).unwrap();
        let mut batch = Batch::new();
        batch.put(b"big".to_vec(), blob(60000, 6));
        batch.delete(b"old".to_vec());
        batch.put(b"small".to_vec(), blob(12, 7));
        store.write(batch).unwrap();

    }
    let store = DiskStore::open(&dir).unwrap();
    assert_eq!(store.get(b"old").unwrap(), None);
    assert_eq!(store.get(b"big").unwrap(), Some(blob(60000, 6)));
    assert_eq!(store.get(b"small").unwrap(), Some(blob(12, 7)));
    assert_eq!(store.len(), 2);

    store.check().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn limits_and_reserved_names_are_enforced() {

    let dir = temp_dir("limits");
    let store = DiskStore::open(&dir).unwrap();
    store.put(b"max".to_vec(), blob(1 << 20, 8)).unwrap();
    assert_eq!(store.get(b"max").unwrap().unwrap().len(), 1 << 20);
    assert!(store.put(b"over".to_vec(), blob((1 << 20) + 1, 9)).is_err());
    assert!(store.put(vec![0, b'O', 1], blob(5, 1)).is_err());

    let tricky = vec![0xFF, 0xFF, 0xFF, 0xFF, 1, 2, 3];
    store.put(b"tricky".to_vec(), tricky.clone()).unwrap();
    assert_eq!(store.get(b"tricky").unwrap(), Some(tricky));
    assert_eq!(store.get(b"over").unwrap(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

