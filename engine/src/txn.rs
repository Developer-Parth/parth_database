use crate::disk::DiskStore;
use crate::error::Result;
use crate::store::{Batch, Op};
use crate::table::Database;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

type Overlay = BTreeMap<Vec<u8>, Option<Vec<u8>>>;
type Pairs = Vec<(Vec<u8>, Vec<u8>)>;

pub struct Kv {
    disk: DiskStore,
    overlay: Option<Mutex<Overlay>>,
}

impl Kv {
    pub fn new(disk: DiskStore) -> Kv {
        Kv { disk, overlay: None }
    }

    pub fn disk(&self) -> &DiskStore {
        &self.disk
    }

    pub fn view(&self) -> Kv {
        Kv { disk: self.disk.clone(), overlay: Some(Mutex::new(BTreeMap::new())) }
    }

    fn pending(&self) -> Option<MutexGuard<'_, Overlay>> {
        self.overlay.as_ref().map(|o| o.lock().unwrap_or_else(|p| p.into_inner()))
    }

    fn pending_batch(&self) -> Batch {
        let mut batch = Batch::new();
        if let Some(pending) = self.pending() {
            for (key, value) in pending.iter() {
                match value {
                    Some(v) => batch.put(key.clone(), v.clone()),
                    None => batch.delete(key.clone()),
                };
            }
        }
        batch
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if let Some(pending) = self.pending() {
            if let Some(entry) = pending.get(key) {
                return Ok(entry.clone());
            }
        }
        self.disk.get(key)
    }

    pub fn contains(&self, key: &[u8]) -> Result<bool> {
        Ok(self.get(key)?.is_some())
    }

    pub fn put(&self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Result<()> {
        match self.pending() {
            Some(mut pending) => {
                pending.insert(key.into(), Some(value.into()));
                Ok(())
            }
            None => self.disk.put(key, value),
        }
    }

    pub fn delete(&self, key: impl Into<Vec<u8>>) -> Result<()> {
        match self.pending() {
            Some(mut pending) => {
                pending.insert(key.into(), None);
                Ok(())
            }
            None => self.disk.delete(key),
        }
    }

    pub fn write(&self, batch: Batch) -> Result<()> {
        match self.pending() {
            Some(mut pending) => {
                for op in batch.ops() {
                    match op {
                        Op::Put(k, v) => pending.insert(k.clone(), Some(v.clone())),
                        Op::Delete(k) => pending.insert(k.clone(), None),
                    };
                }
                Ok(())
            }
            None => self.disk.write(batch),
        }
    }

    pub fn range(&self, start: Option<&[u8]>, end: Option<&[u8]>, limit: usize) -> Result<Pairs> {
        let Some(pending) = self.pending() else {
            return self.disk.range(start, end, limit);
        };
        let mut merged: BTreeMap<Vec<u8>, Vec<u8>> = self.disk.range(start, end, usize::MAX)?.into_iter().collect();
        for (key, value) in pending.iter() {
            if start.is_some_and(|s| key.as_slice() < s) || end.is_some_and(|e| key.as_slice() >= e) {
                continue;
            }
            match value {
                Some(v) => merged.insert(key.clone(), v.clone()),
                None => merged.remove(key),
            };
        }
        Ok(merged.into_iter().take(limit).collect())
    }

    pub fn scan_prefix(&self, prefix: &[u8], limit: usize) -> Result<Pairs> {
        let Some(pending) = self.pending() else {
            return self.disk.scan_prefix(prefix, limit);
        };
        let mut merged: BTreeMap<Vec<u8>, Vec<u8>> = self.disk.scan_prefix(prefix, usize::MAX)?.into_iter().collect();
        for (key, value) in pending.iter().filter(|(k, _)| k.starts_with(prefix)) {
            match value {
                Some(v) => merged.insert(key.clone(), v.clone()),
                None => merged.remove(key),
            };
        }
        Ok(merged.into_iter().take(limit).collect())
    }

pub fn scan_prefix_each<F: FnMut(&[u8], &[u8]) -> Result<bool>>(&self, prefix: &[u8], chunk: usize, mut visit: F) -> Result<()> {
if self.pending().is_some() {
for (key, value) in self.scan_prefix(prefix, usize::MAX)? {
if !visit(&key, &value)? { return Ok(()); }
}
return Ok(());
}
self.disk.scan_prefix_each(prefix, chunk, visit)
}
}

impl Database {
    pub fn transaction<T>(&self, commit: bool, run: impl FnOnce(&Database) -> Result<T>) -> Result<T> {
        let view = Database { store: self.store.view() };
        let out = run(&view)?;
        let batch = view.store.pending_batch();
        if commit && !batch.is_empty() {
            self.store.write(batch)?;
        }
        Ok(out)
    }
}
