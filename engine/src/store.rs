
use crate::crc::crc32;
use crate::error::{Error, Result};
use crate::wal::Wal;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const SNAP_MAGIC: &[u8; 8] = b"PDBSNAP1";
pub const MAX_KEY: usize = 64 * 1024;
pub const MAX_VALUE: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncMode {
    Always,
    Never,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub sync: SyncMode,
    pub checkpoint_bytes: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options { sync: SyncMode::Always, checkpoint_bytes: 16 * 1024 * 1024 }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Put(Vec<u8>, Vec<u8>),
    Delete(Vec<u8>),
}

#[derive(Clone, Debug, Default)]
pub struct Batch {
    ops: Vec<Op>,
}

impl Batch {
    pub fn new() -> Self {
        Batch::default()
    }
    pub fn put(&mut self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> &mut Self {
        self.ops.push(Op::Put(key.into(), value.into()));
        self
    }
    pub fn delete(&mut self, key: impl Into<Vec<u8>>) -> &mut Self {
        self.ops.push(Op::Delete(key.into()));
        self
    }
    pub fn len(&self) -> usize {
        self.ops.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub keys: usize,
    pub wal_bytes: u64,
    pub sequence: u64,
    pub recovered_batches: u64,
    pub discarded_tail_bytes: u64,
}

struct State {
    map: BTreeMap<Vec<u8>, Vec<u8>>,
    wal: Wal,
    seq: u64,
    stats_recovered: u64,
    stats_discarded: u64,
}

struct Inner {
    dir: PathBuf,
    opts: Options,
    state: RwLock<State>,
    _lock: File,
}

#[derive(Clone)]
pub struct Store {
    inner: Arc<Inner>,
}

fn encode_batch(seq: u64, ops: &[Op]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(ops.len() as u32).to_le_bytes());
    for op in ops {
        match op {
            Op::Put(k, v) => {
                out.push(1);
                out.extend_from_slice(&(k.len() as u32).to_le_bytes());
                out.extend_from_slice(k);
                out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                out.extend_from_slice(v);
            }
            Op::Delete(k) => {
                out.push(2);
                out.extend_from_slice(&(k.len() as u32).to_le_bytes());
                out.extend_from_slice(k);
            }
        }
    }
    out
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.buf.len() - self.pos < n {
            return Err(Error::Corrupt("unexpected end of record".into()));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn bytes(&mut self) -> Result<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
}

fn decode_batch(payload: &[u8]) -> Result<(u64, Vec<Op>)> {
    let mut r = Reader { buf: payload, pos: 0 };
    let seq = r.u64()?;
    let count = r.u32()? as usize;
    let mut ops = Vec::with_capacity(count.min(1 << 16));
    for _ in 0..count {
        match r.u8()? {
            1 => {
                let k = r.bytes()?;
                let v = r.bytes()?;
                ops.push(Op::Put(k, v));
            }
            2 => ops.push(Op::Delete(r.bytes()?)),
            t => return Err(Error::Corrupt(format!("unknown op tag {t}"))),
        }
    }
    if r.pos != payload.len() {
        return Err(Error::Corrupt("trailing bytes in record".into()));
    }
    Ok((seq, ops))
}

fn apply(map: &mut BTreeMap<Vec<u8>, Vec<u8>>, ops: Vec<Op>) {
    for op in ops {
        match op {
            Op::Put(k, v) => {
                map.insert(k, v);
            }
            Op::Delete(k) => {
                map.remove(&k);
            }
        }
    }
}

fn load_snapshot(path: &Path) -> Result<(u64, BTreeMap<Vec<u8>, Vec<u8>>)> {
    let data = fs::read(path)?;
    if data.len() < SNAP_MAGIC.len() + 8 + 8 + 4 {
        return Err(Error::Corrupt("snapshot too short".into()));
    }
    let (body, crc_bytes) = data.split_at(data.len() - 4);
    if crc32(body) != u32::from_le_bytes(crc_bytes.try_into().unwrap()) {
        return Err(Error::Corrupt("snapshot checksum mismatch".into()));
    }
    if &body[..8] != SNAP_MAGIC {
        return Err(Error::Corrupt("bad snapshot magic".into()));
    }
    let mut r = Reader { buf: body, pos: 8 };
    let seq = r.u64()?;
    let count = r.u64()?;
    let mut map = BTreeMap::new();
    for _ in 0..count {
        let k = r.bytes()?;
        let v = r.bytes()?;
        map.insert(k, v);
    }
    if r.pos != body.len() {
        return Err(Error::Corrupt("trailing bytes in snapshot".into()));
    }
    Ok((seq, map))
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> Result<()> {
    File::open(dir)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> Result<()> {
    Ok(())
}

impl Store {
    pub fn open(dir: impl AsRef<Path>, opts: Options) -> Result<Store> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;

        let lock = OpenOptions::new().read(true).write(true).create(true).open(dir.join("LOCK"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(Error::Locked),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }

        let _ = fs::remove_file(dir.join("snapshot.tmp"));

        let snap_path = dir.join("snapshot.pdb");
        let (snap_seq, mut map) = if snap_path.exists() {
            load_snapshot(&snap_path)?
        } else {
            (0, BTreeMap::new())
        };

        let (wal, recovered) = Wal::open(&dir.join("wal.log"))?;
        let mut seq = snap_seq;
        let mut replayed = 0u64;
        for payload in &recovered.records {
            let (rseq, ops) = decode_batch(payload)?;
            if rseq <= snap_seq {
                continue;
            }
            if rseq != seq + 1 {
                return Err(Error::Corrupt(format!("sequence gap: expected {}, found {}", seq + 1, rseq)));
            }
            apply(&mut map, ops);
            seq = rseq;
            replayed += 1;
        }

        Ok(Store {
            inner: Arc::new(Inner {
                dir,
                opts,
                state: RwLock::new(State {
                    map,
                    wal,
                    seq,
                    stats_recovered: replayed,
                    stats_discarded: recovered.discarded_bytes,
                }),
                _lock: lock,
            }),
        })
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.inner.state.read().unwrap().map.get(key).cloned()
    }

    pub fn contains(&self, key: &[u8]) -> bool {
        self.inner.state.read().unwrap().map.contains_key(key)
    }

    pub fn put(&self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Result<()> {
        let mut b = Batch::new();
        b.put(key, value);
        self.write(b)
    }

    pub fn delete(&self, key: impl Into<Vec<u8>>) -> Result<()> {
        let mut b = Batch::new();
        b.delete(key);
        self.write(b)
    }

    pub fn write(&self, batch: Batch) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        for op in batch.ops() {
            match op {
                Op::Put(k, v) => {
                    if k.is_empty() || k.len() > MAX_KEY {
                        return Err(Error::InvalidInput(format!("key length must be 1..={MAX_KEY} bytes")));
                    }
                    if v.len() > MAX_VALUE {
                        return Err(Error::InvalidInput(format!("value larger than {MAX_VALUE} bytes")));
                    }
                }
                Op::Delete(k) => {
                    if k.is_empty() || k.len() > MAX_KEY {
                        return Err(Error::InvalidInput(format!("key length must be 1..={MAX_KEY} bytes")));
                    }
                }
            }
        }
        let needs_checkpoint;
        {
            let mut st = self.inner.state.write().unwrap();
            let next = st.seq + 1;
            let payload = encode_batch(next, batch.ops());
            st.wal.append(&payload, self.inner.opts.sync == SyncMode::Always)?;
            st.seq = next;
            apply(&mut st.map, batch.ops);
            let limit = self.inner.opts.checkpoint_bytes;
            needs_checkpoint = limit > 0 && st.wal.len() >= limit;
        }
        if needs_checkpoint {
            self.checkpoint()?;
        }
        Ok(())
    }

    pub fn range(&self, start: Option<&[u8]>, end: Option<&[u8]>, limit: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
        let st = self.inner.state.read().unwrap();
        let lo = match start {
            Some(s) => Bound::Included(s.to_vec()),
            None => Bound::Unbounded,
        };
        let hi = match end {
            Some(e) => Bound::Excluded(e.to_vec()),
            None => Bound::Unbounded,
        };
        if let (Bound::Included(a), Bound::Excluded(b)) = (&lo, &hi) {
            if a >= b {
                return Vec::new();
            }
        }
        st.map.range((lo, hi)).take(limit).map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    pub fn scan_prefix(&self, prefix: &[u8], limit: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
        let end = prefix_end(prefix);
        self.range(Some(prefix), end.as_deref(), limit)
    }

    pub fn len(&self) -> usize {
        self.inner.state.read().unwrap().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn stats(&self) -> Stats {
        let st = self.inner.state.read().unwrap();
        Stats {
            keys: st.map.len(),
            wal_bytes: st.wal.len(),
            sequence: st.seq,
            recovered_batches: st.stats_recovered,
            discarded_tail_bytes: st.stats_discarded,
        }
    }

    pub fn sync(&self) -> Result<()> {
        self.inner.state.write().unwrap().wal.sync()
    }

    pub fn checkpoint(&self) -> Result<()> {
        let mut st = self.inner.state.write().unwrap();
        let mut body = Vec::new();
        body.extend_from_slice(SNAP_MAGIC);
        body.extend_from_slice(&st.seq.to_le_bytes());
        body.extend_from_slice(&(st.map.len() as u64).to_le_bytes());
        for (k, v) in &st.map {
            body.extend_from_slice(&(k.len() as u32).to_le_bytes());
            body.extend_from_slice(k);
            body.extend_from_slice(&(v.len() as u32).to_le_bytes());
            body.extend_from_slice(v);
        }
        let crc = crc32(&body);
        body.extend_from_slice(&crc.to_le_bytes());

        let tmp = self.inner.dir.join("snapshot.tmp");
        let dst = self.inner.dir.join("snapshot.pdb");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&body)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &dst)?;
        sync_dir(&self.inner.dir)?;
        st.wal.reset()?;
        Ok(())
    }
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