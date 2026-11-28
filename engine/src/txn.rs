use crate::disk::DiskStore;
use crate::error::{Error, Result};
use crate::store::{Batch, Op};
use crate::table::Database;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

type Overlay = BTreeMap<Vec<u8>, Option<Vec<u8>>>;
type Pairs = Vec<(Vec<u8>, Vec<u8>)>;

const MAX_PENDING_ENTRIES: usize = 100_000;const MAX_PENDING_BYTES: usize = 32 * 1024 * 1024;

pub struct Kv {
    disk: DiskStore,
    overlay: Option<Mutex<Overlay>>,
    pending_bytes: std::sync::atomic::AtomicUsize,
}

impl Kv {
    pub fn new(disk: DiskStore) -> Kv {
        Kv { disk, overlay: None, pending_bytes: Default::default() }
    }

    pub fn disk(&self) -> &DiskStore {
        &self.disk
    }

    pub fn view(&self) -> Kv {
        Kv { disk: self.disk.clone(), overlay: Some(Mutex::new(BTreeMap::new())), pending_bytes: Default::default() }
    }

    fn pending(&self) -> Option<MutexGuard<'_, Overlay>> {
        self.overlay.as_ref().map(|o| o.lock().unwrap_or_else(|p| p.into_inner()))
    }

fn charge(&self, entries: usize, bytes: usize) -> Result<()> { let total = self.pending_bytes.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed) + bytes; if entries > MAX_PENDING_ENTRIES || total > MAX_PENDING_BYTES { return Err(Error::InvalidInput(format!("transaction too large: over {} written entries or {} bytes", MAX_PENDING_ENTRIES, MAX_PENDING_BYTES))); } Ok(()) }

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
                let key = key.into(); let value = value.into(); let bytes = key.len() + value.len(); pending.insert(key, Some(value)); self.charge(pending.len(), bytes)?;
                Ok(())
            }
            None => self.disk.put(key, value),
        }
    }

    pub fn delete(&self, key: impl Into<Vec<u8>>) -> Result<()> {
        match self.pending() {
            Some(mut pending) => {
                let key = key.into(); let bytes = key.len(); pending.insert(key, None); self.charge(pending.len(), bytes)?;
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
let added: usize = batch.ops().iter().map(|op| match op { Op::Put(k, v) => k.len() + v.len(), Op::Delete(k) => k.len() }).sum(); self.charge(pending.len(), added)?;
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
        if self.pending().is_none() {
            return self.disk.scan_prefix(prefix, limit);
        }
        let mut out = Vec::new();
        if limit == 0 {
            return Ok(out);
        }
        self.scan_prefix_each(prefix, 64, |key, value| {
            out.push((key.to_vec(), value.to_vec()));
            Ok(out.len() < limit)
        })?;
        Ok(out)
    }

pub fn scan_prefix_each<F: FnMut(&[u8], &[u8]) -> Result<bool>>(&self, prefix: &[u8], chunk: usize, mut visit: F) -> Result<()> {
if self.pending().is_some() {
        let overlay: Vec<(Vec<u8>, Option<Vec<u8>>)> = match self.pending() {
            Some(pending) => pending
                .iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            None => Vec::new(),
        };
        let mut next = 0;
        let mut stopped = false;
        self.disk.scan_prefix_each(prefix, chunk, |key, value| {
            while next < overlay.len() && overlay[next].0.as_slice() < key {
                let (pending_key, pending_value) = &overlay[next];
                next += 1;
                if let Some(v) = pending_value {
                    if !visit(pending_key, v)? {
                        stopped = true;
                        return Ok(false);
                    }
                }
            }
            if next < overlay.len() && overlay[next].0.as_slice() == key {
                let pending_value = &overlay[next].1;
                next += 1;
                if let Some(v) = pending_value {
                    let go = visit(key, v)?;
                    stopped = !go;
                    return Ok(go);
                }
                return Ok(true);
            }
            let go = visit(key, value)?;
            stopped = !go;
            Ok(go)
        })?;
        if !stopped {
            for (pending_key, pending_value) in &overlay[next..] {
                if let Some(v) = pending_value {
                    if !visit(pending_key, v)? {
                        break;
                    }
                }
            }
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
