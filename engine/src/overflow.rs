use crate::btree::BTree;
use crate::crc::crc32;
use crate::error::{Error, Result};
use crate::store::{Batch, Op};
use std::collections::{BTreeMap, HashMap};

const META_KEY: [u8; 2] = [0, b'N'];
const CHUNK_TAG: [u8; 2] = [0, b'O'];
const RESERVED_LO: [u8; 2] = [0, b'N'];
const RESERVED_HI: [u8; 2] = [0, b'P'];
const SENTINEL: [u8; 4] = [0xFF; 4];
const HEAD_LEN: usize = 20;
const CHUNK_KEY_LEN: usize = 14;
pub const INLINE_MAX: usize = 700;
pub const CHUNK_SIZE: usize = 900;
pub const MAX_VALUE: usize = 1 << 20;

#[derive(Clone, Copy)]
struct Head {
    len: usize,
    id: u64,
    crc: u32,
}

pub fn is_reserved(key: &[u8]) -> bool {
    key.len() >= 2 && key[0] == 0 && (key[1] == b'N' || key[1] == b'O')
}

fn corrupt(msg: &str) -> Error {
    Error::Corrupt(msg.to_string())
}

fn parse_head(value: &[u8]) -> Result<Option<Head>> {
    if value.len() < 4 || value[..4] != SENTINEL {
        return Ok(None);
    }
    if value.len() != HEAD_LEN {
        return Err(corrupt("malformed overflow head"));
    }
    let len = u32::from_le_bytes(value[4..8].try_into().unwrap()) as usize;
    if len == 0 || len > MAX_VALUE {
        return Err(corrupt("overflow length out of range"));
    }
    let id = u64::from_le_bytes(value[8..16].try_into().unwrap());
    let crc = u32::from_le_bytes(value[16..20].try_into().unwrap());
    Ok(Some(Head { len, id, crc }))
}

fn encode_head(head: &Head) -> Vec<u8> {
    let mut out = SENTINEL.to_vec();
    out.extend_from_slice(&(head.len as u32).to_le_bytes());
    out.extend_from_slice(&head.id.to_le_bytes());
    out.extend_from_slice(&head.crc.to_le_bytes());
    out
}

fn chunk_key(id: u64, index: usize) -> Vec<u8> {
    let mut key = CHUNK_TAG.to_vec();
    key.extend_from_slice(&id.to_be_bytes());
    key.extend_from_slice(&(index as u32).to_be_bytes());
    key
}

fn chunk_count(len: usize) -> usize {
    len.div_ceil(CHUNK_SIZE)
}

fn read_meta(tree: &mut BTree) -> Result<(u64, u64)> {
    match tree.get(&META_KEY)? {
        None => Ok((0, 0)),
        Some(v) if v.len() == 16 => Ok((
            u64::from_le_bytes(v[..8].try_into().unwrap()),
            u64::from_le_bytes(v[8..].try_into().unwrap()),
        )),
        Some(_) => Err(corrupt("malformed overflow counter")),
    }
}

fn encode_meta(next: u64, chunks: u64) -> Vec<u8> {
    let mut out = next.to_le_bytes().to_vec();
    out.extend_from_slice(&chunks.to_le_bytes());
    out
}

pub fn read(tree: &mut BTree, physical: Vec<u8>) -> Result<Vec<u8>> {
    let head = match parse_head(&physical)? {
        None => return Ok(physical),
        Some(h) => h,
    };
    let mut out = Vec::with_capacity(head.len);
    for index in 0..chunk_count(head.len) {
        let chunk = tree.get(&chunk_key(head.id, index))?.ok_or_else(|| corrupt("missing overflow chunk"))?;
        if chunk.len() != CHUNK_SIZE.min(head.len - out.len()) {
            return Err(corrupt("overflow chunk has the wrong length"));
        }
        out.extend_from_slice(&chunk);
    }
    if crc32(&out) != head.crc {
        return Err(corrupt("overflow checksum mismatch"));
    }
    Ok(out)
}

fn old_head(tree: &mut BTree, seen: &HashMap<Vec<u8>, Option<Head>>, key: &[u8]) -> Result<Option<Head>> {
    if let Some(known) = seen.get(key) {
        return Ok(*known);
    }
    match tree.get(key)? {
        None => Ok(None),
        Some(v) => parse_head(&v),
    }
}

pub fn plan(tree: &mut BTree, batch: &Batch) -> Result<Vec<(Vec<u8>, Option<Vec<u8>>)>> {
    let (mut next, mut chunks) = read_meta(tree)?;
    let before = (next, chunks);
    let mut out: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
    let mut seen: HashMap<Vec<u8>, Option<Head>> = HashMap::new();
    for op in batch.ops() {
        let key = match op {
            Op::Put(k, _) | Op::Delete(k) => k,
        };
        if is_reserved(key) {
            return Err(Error::InvalidInput("keys beginning 0x00 'N' or 0x00 'O' are reserved".into()));
        }
        if let Some(old) = old_head(tree, &seen, key)? {
            for index in 0..chunk_count(old.len) {
                out.insert(chunk_key(old.id, index), None);
            }
            chunks = chunks.saturating_sub(chunk_count(old.len) as u64);
        }
        match op {
            Op::Delete(_) => {
                out.insert(key.clone(), None);
                seen.insert(key.clone(), None);
            }
            Op::Put(_, value) => {
                if value.len() > MAX_VALUE {
                    return Err(Error::InvalidInput("value exceeds the 1 MiB storage limit".into()));
                }
                if value.len() > INLINE_MAX || value.starts_with(&SENTINEL) {
                    let head = Head { len: value.len(), id: next, crc: crc32(value) };
                    next += 1;
                    for (index, piece) in value.chunks(CHUNK_SIZE).enumerate() {
                        out.insert(chunk_key(head.id, index), Some(piece.to_vec()));
                    }
                    chunks += chunk_count(value.len()) as u64;
                    out.insert(key.clone(), Some(encode_head(&head)));
                    seen.insert(key.clone(), Some(head));
                } else {
                    out.insert(key.clone(), Some(value.clone()));
                    seen.insert(key.clone(), None);
                }
            }
        }
    }
    if (next, chunks) != before {
        out.insert(META_KEY.to_vec(), Some(encode_meta(next, chunks)));
    }
    Ok(out.into_iter().collect())
}

pub fn logical_len(tree: &mut BTree) -> Result<u64> {
    let (_, chunks) = read_meta(tree)?;
    let meta = tree.get(&META_KEY)?.is_some() as u64;
    Ok(tree.len().saturating_sub(chunks + meta))
}

pub fn visible_range(
    tree: &mut BTree,
    start: Option<&[u8]>,
    end: Option<&[u8]>,
    limit: usize,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let lo: &[u8] = &RESERVED_LO;
    let hi: &[u8] = &RESERVED_HI;
    let mut rows = Vec::new();
    if start.map_or(true, |s| s < lo) {
        let seg_end = match end {
            Some(e) if e < lo => Some(e),
            _ => Some(lo),
        };
        rows.extend(tree.range(start, seg_end, limit)?);
    }
    if rows.len() < limit && end.map_or(true, |e| e > hi) {
        let seg_start = match start {
            Some(s) if s > hi => Some(s),
            _ => Some(hi),
        };
        rows.extend(tree.range(seg_start, end, limit - rows.len())?);
    }
    for row in rows.iter_mut() {
        let value = std::mem::take(&mut row.1);
        row.1 = read(tree, value)?;
    }
    Ok(rows)
}

pub fn verify(tree: &mut BTree) -> Result<()> {
    let (_, recorded) = read_meta(tree)?;
    let all = tree.range(None, None, usize::MAX)?;
    let mut stored = 0u64;
    let mut expected = 0u64;
    for (key, value) in all {
        if key == META_KEY {
            continue;
        }
        if key.starts_with(&CHUNK_TAG) {
            if key.len() != CHUNK_KEY_LEN {
                return Err(corrupt("malformed overflow chunk key"));
            }
            stored += 1;
            continue;
        }
        if let Some(head) = parse_head(&value)? {
            expected += chunk_count(head.len) as u64;
            read(tree, value)?;
        }
    }
    if stored != expected || stored != recorded {
        return Err(corrupt("overflow chunk count does not match the stored heads"));
    }
    Ok(())
}
