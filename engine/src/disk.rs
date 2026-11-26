use crate::btree::{BTree, TreeReport};
use crate::error::{Error, Result};
use crate::store::{Batch, Op, Options, Store};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const DATA_FILE: &str = "data.pdb";
const MIGRATION_FILE: &str = "data.pdb.tmp";
const MIGRATION_CHUNK: usize = 400;

struct Inner {
    tree: Mutex<BTree>,
    _lock: File,
}

#[derive(Clone)]
pub struct DiskStore {
    inner: Arc<Inner>,
}

fn prefix_end(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.pop() {
        if last < 0xFF {
            end.push(last + 1);
            return Some(end);
        }
    }
    None
}

fn has_legacy_files(dir: &Path) -> bool {
    dir.join("wal.log").exists() || dir.join("snapshot.pdb").exists()
}

fn migrate_legacy(dir: &Path) -> Result<()> {
    let tmp: PathBuf = dir.join(MIGRATION_FILE);
    let _ = fs::remove_file(&tmp);
    let result = copy_legacy_entries(dir, &tmp);
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(Error::InvalidInput(format!("cannot migrate existing data: {e}")));
    }
    fs::rename(&tmp, dir.join(DATA_FILE))?;
    Ok(())
}

fn copy_legacy_entries(dir: &Path, tmp: &Path) -> Result<()> {
    let legacy = Store::open(dir, Options::default())?;
    let mut tree = BTree::open(tmp)?;
    let entries = legacy.range(None, None, usize::MAX);
    for chunk in entries.chunks(MIGRATION_CHUNK) {
        let ops: Vec<(Vec<u8>, Option<Vec<u8>>)> = chunk.iter().map(|(k, v)| (k.clone(), Some(v.clone()))).collect();
        tree.apply(&ops)?;
    }
    if tree.len() != entries.len() as u64 {
        return Err(Error::Corrupt("migrated entry count differs".into()));
    }
    Ok(())
}

impl DiskStore {
    pub fn open(dir: impl AsRef<Path>) -> Result<DiskStore> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let data = dir.join(DATA_FILE);
        if !data.exists() && has_legacy_files(&dir) {
            migrate_legacy(&dir)?;
        }
        let lock = OpenOptions::new().read(true).write(true).create(true).open(dir.join("LOCK"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(Error::Locked),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        let _ = fs::remove_file(dir.join(MIGRATION_FILE));
        let tree = BTree::open(&data)?;
        Ok(DiskStore { inner: Arc::new(Inner { tree: Mutex::new(tree), _lock: lock }) })
    }

    fn tree(&self) -> std::sync::MutexGuard<'_, BTree> {
        self.inner.tree.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.tree().get(key)
    }

    pub fn contains(&self, key: &[u8]) -> Result<bool> {
        Ok(self.tree().get(key)?.is_some())
    }

    pub fn put(&self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Result<()> {
        let mut batch = Batch::new();
        batch.put(key, value);
        self.write(batch)
    }

    pub fn delete(&self, key: impl Into<Vec<u8>>) -> Result<()> {
        let mut batch = Batch::new();
        batch.delete(key);
        self.write(batch)
    }

    pub fn write(&self, batch: Batch) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let ops: Vec<(Vec<u8>, Option<Vec<u8>>)> = batch
            .ops()
            .iter()
            .map(|op| match op {
                Op::Put(k, v) => (k.clone(), Some(v.clone())),
                Op::Delete(k) => (k.clone(), None),
            })
            .collect();
        self.tree().apply(&ops)
    }

    pub fn range(&self, start: Option<&[u8]>, end: Option<&[u8]>, limit: usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.tree().range(start, end, limit)
    }

    pub fn scan_prefix(&self, prefix: &[u8], limit: usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let end = prefix_end(prefix);
        self.range(Some(prefix), end.as_deref(), limit)
    }

    pub fn len(&self) -> u64 {
        self.tree().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn check(&self) -> Result<TreeReport> {
        self.tree().check()
    }
}