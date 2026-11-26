use crate::crc::crc32;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const PAGE_SIZE: usize = 4096;
pub const MAX_KEY_LEN: usize = 500;
pub const MAX_ENTRY_LEN: usize = 1000;
const HEADER: usize = 7;
const META_MAGIC: &[u8; 8] = b"PDBTREE1";
const KIND_LEAF: u8 = 1;
const KIND_INTERNAL: u8 = 2;

#[derive(Clone, Debug)]
enum Node {
    Leaf(Vec<(Vec<u8>, Vec<u8>)>),
    Internal(Vec<Vec<u8>>, Vec<u64>),
}

impl Node {
    fn size(&self) -> usize {
        match self {
            Node::Leaf(entries) => HEADER + entries.iter().map(|(k, v)| 4 + k.len() + v.len()).sum::<usize>(),
            Node::Internal(keys, _) => HEADER + 8 + keys.iter().map(|k| 10 + k.len()).sum::<usize>(),
        }
    }

    fn encode(&self) -> Result<Vec<u8>> {
        if self.size() > PAGE_SIZE {
            return Err(Error::Corrupt("node does not fit in a page".into()));
        }
        let mut page = vec![0u8; PAGE_SIZE];
        let mut pos = HEADER;
        match self {
            Node::Leaf(entries) => {
                page[4] = KIND_LEAF;
                page[5..7].copy_from_slice(&(entries.len() as u16).to_le_bytes());
                for (k, v) in entries {
                    page[pos..pos + 2].copy_from_slice(&(k.len() as u16).to_le_bytes());
                    page[pos + 2..pos + 4].copy_from_slice(&(v.len() as u16).to_le_bytes());
                    pos += 4;
                    page[pos..pos + k.len()].copy_from_slice(k);
                    pos += k.len();
                    page[pos..pos + v.len()].copy_from_slice(v);
                    pos += v.len();
                }
            }
            Node::Internal(keys, children) => {
                page[4] = KIND_INTERNAL;
                page[5..7].copy_from_slice(&(keys.len() as u16).to_le_bytes());
                page[pos..pos + 8].copy_from_slice(&children[0].to_le_bytes());
                pos += 8;
                for (i, k) in keys.iter().enumerate() {
                    page[pos..pos + 2].copy_from_slice(&(k.len() as u16).to_le_bytes());
                    pos += 2;
                    page[pos..pos + k.len()].copy_from_slice(k);
                    pos += k.len();
                    page[pos..pos + 8].copy_from_slice(&children[i + 1].to_le_bytes());
                    pos += 8;
                }
            }
        }
        let crc = crc32(&page[4..]);
        page[0..4].copy_from_slice(&crc.to_le_bytes());
        Ok(page)
    }

    fn decode(page: &[u8]) -> Result<Node> {
        let stored = u32::from_le_bytes(page[0..4].try_into().unwrap());
        if stored != crc32(&page[4..]) {
            return Err(Error::Corrupt("page checksum mismatch".into()));
        }
        let count = u16::from_le_bytes(page[5..7].try_into().unwrap()) as usize;
        let mut pos = HEADER;
        let take = |pos: &mut usize, n: usize| -> Result<std::ops::Range<usize>> {
            if *pos + n > PAGE_SIZE {
                return Err(Error::Corrupt("page entry out of bounds".into()));
            }
            let r = *pos..*pos + n;
            *pos += n;
            Ok(r)
        };
        match page[4] {
            KIND_LEAF => {
                let mut entries = Vec::with_capacity(count);
                for _ in 0..count {
                    let h = take(&mut pos, 4)?;
                    let kl = u16::from_le_bytes(page[h.start..h.start + 2].try_into().unwrap()) as usize;
                    let vl = u16::from_le_bytes(page[h.start + 2..h.end].try_into().unwrap()) as usize;
                    let k = take(&mut pos, kl)?;
                    let v = take(&mut pos, vl)?;
                    entries.push((page[k].to_vec(), page[v].to_vec()));
                }
                Ok(Node::Leaf(entries))
            }
            KIND_INTERNAL => {
                let mut keys = Vec::with_capacity(count);
                let mut children = Vec::with_capacity(count + 1);
                let c = take(&mut pos, 8)?;
                children.push(u64::from_le_bytes(page[c].try_into().unwrap()));
                for _ in 0..count {
                    let h = take(&mut pos, 2)?;
                    let kl = u16::from_le_bytes(page[h].try_into().unwrap()) as usize;
                    let k = take(&mut pos, kl)?;
                    keys.push(page[k].to_vec());
                    let c = take(&mut pos, 8)?;
                    children.push(u64::from_le_bytes(page[c].try_into().unwrap()));
                }
                Ok(Node::Internal(keys, children))
            }
            _ => Err(Error::Corrupt("unknown page kind".into())),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Meta {
    txid: u64,
    root: u64,
    page_count: u64,
    entries: u64,
}

impl Meta {
    fn encode(&self) -> Vec<u8> {
        let mut page = vec![0u8; PAGE_SIZE];
        page[4..12].copy_from_slice(META_MAGIC);
        page[12..20].copy_from_slice(&self.txid.to_le_bytes());
        page[20..28].copy_from_slice(&self.root.to_le_bytes());
        page[28..36].copy_from_slice(&self.page_count.to_le_bytes());
        page[36..44].copy_from_slice(&self.entries.to_le_bytes());
        let crc = crc32(&page[4..]);
        page[0..4].copy_from_slice(&crc.to_le_bytes());
        page
    }

    fn decode(page: &[u8]) -> Option<Meta> {
        let stored = u32::from_le_bytes(page[0..4].try_into().unwrap());
        if stored != crc32(&page[4..]) || &page[4..12] != META_MAGIC {
            return None;
        }
        let n = |a: usize| u64::from_le_bytes(page[a..a + 8].try_into().unwrap());
        Some(Meta { txid: n(12), root: n(20), page_count: n(28), entries: n(36) })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeReport {
    pub entries: u64,
    pub pages_reachable: u64,
    pub pages_in_file: u64,
    pub height: u32,
}

pub struct BTree {
    file: File,
    meta: Meta,
    root: u64,
    next_page: u64,
    entries: u64,
    dirty: HashMap<u64, Node>,
}

fn validate(key: &[u8], value: Option<&[u8]>) -> Result<()> {
    if key.is_empty() || key.len() > MAX_KEY_LEN {
        return Err(Error::InvalidInput(format!("key length must be 1..={MAX_KEY_LEN} bytes")));
    }
    if let Some(v) = value {
        if key.len() + v.len() > MAX_ENTRY_LEN {
            return Err(Error::InvalidInput(format!("key plus value must be at most {MAX_ENTRY_LEN} bytes")));
        }
    }
    Ok(())
}

fn split_point(sizes: &[usize]) -> usize {
    let total: usize = sizes.iter().sum();
    let mut acc = 0;
    for (i, s) in sizes.iter().enumerate() {
        acc += s;
        if acc * 2 >= total {
            return (i + 1).clamp(1, sizes.len() - 1);
        }
    }
    sizes.len() / 2
}

impl BTree {
    pub fn open(path: impl AsRef<Path>) -> Result<BTree> {
        let mut file = OpenOptions::new().read(true).write(true).create(true).open(path)?;
        let len = file.metadata()?.len();
        let mut metas = Vec::new();
        for slot in 0..2u64 {
            if len >= (slot + 1) * PAGE_SIZE as u64 {
                let mut page = vec![0u8; PAGE_SIZE];
                file.seek(SeekFrom::Start(slot * PAGE_SIZE as u64))?;
                file.read_exact(&mut page)?;
                if let Some(m) = Meta::decode(&page) {
                    metas.push(m);
                }
            }
        }
        let meta = match metas.into_iter().max_by_key(|m| m.txid) {
            Some(m) => m,
            None if len == 0 => {
                let m = Meta { txid: 0, root: 0, page_count: 2, entries: 0 };
                file.seek(SeekFrom::Start(0))?;
                file.write_all(&m.encode())?;
                file.write_all(&m.encode())?;
                file.sync_all()?;
                m
            }
            None => return Err(Error::Corrupt("no valid meta page".into())),
        };
        Ok(BTree { file, meta, root: meta.root, next_page: meta.page_count, entries: meta.entries, dirty: HashMap::new() })
    }

    pub fn len(&self) -> u64 {
        self.meta.entries
    }

    pub fn is_empty(&self) -> bool {
        self.meta.entries == 0
    }

    pub fn pages_in_file(&self) -> u64 {
        self.meta.page_count
    }

    pub fn txid(&self) -> u64 {
        self.meta.txid
    }

    fn read_node(&mut self, pid: u64) -> Result<Node> {
        if let Some(n) = self.dirty.get(&pid) {
            return Ok(n.clone());
        }
        if pid < 2 || pid >= self.meta.page_count {
            return Err(Error::Corrupt(format!("page {pid} out of range")));
        }
        let mut page = vec![0u8; PAGE_SIZE];
        self.file.seek(SeekFrom::Start(pid * PAGE_SIZE as u64))?;
        self.file.read_exact(&mut page)?;
        Node::decode(&page)
    }

    fn store_node(&mut self, pid: u64, node: Node) -> u64 {
        let id = if self.dirty.contains_key(&pid) { pid } else { self.alloc() };
        self.dirty.insert(id, node);
        id
    }

    fn alloc(&mut self) -> u64 {
        let id = self.next_page;
        self.next_page += 1;
        id
    }

    fn discard(&mut self, pid: u64) {
        self.dirty.remove(&pid);
    }

    pub fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut pid = self.root;
        if pid == 0 {
            return Ok(None);
        }
        loop {
            match self.read_node(pid)? {
                Node::Leaf(entries) => {
                    return Ok(entries
                        .binary_search_by(|(k, _)| k.as_slice().cmp(key))
                        .ok()
                        .map(|i| entries[i].1.clone()));
                }
                Node::Internal(keys, children) => {
                    pid = children[keys.partition_point(|k| k.as_slice() <= key)];
                }
            }
        }
    }

    pub fn range(&mut self, start: Option<&[u8]>, end: Option<&[u8]>, limit: usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut out = Vec::new();
        if self.root != 0 && limit > 0 {
            let root = self.root;
            self.collect(root, start, end, limit, &mut out)?;
        }
        Ok(out)
    }

    fn collect(&mut self, pid: u64, start: Option<&[u8]>, end: Option<&[u8]>, limit: usize, out: &mut Vec<(Vec<u8>, Vec<u8>)>) -> Result<bool> {
        match self.read_node(pid)? {
            Node::Leaf(entries) => {
                for (k, v) in entries {
                    if start.map_or(false, |s| k.as_slice() < s) {
                        continue;
                    }
                    if end.map_or(false, |e| k.as_slice() >= e) {
                        return Ok(true);
                    }
                    out.push((k, v));
                    if out.len() >= limit {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Node::Internal(keys, children) => {
                let first = start.map_or(0, |s| keys.partition_point(|k| k.as_slice() <= s));
                for i in first..children.len() {
                    if i > 0 && end.map_or(false, |e| keys[i - 1].as_slice() >= e) {
                        return Ok(true);
                    }
                    if self.collect(children[i], start, end, limit, out)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    pub fn apply(&mut self, ops: &[(Vec<u8>, Option<Vec<u8>>)]) -> Result<()> {
        for (k, v) in ops {
            validate(k, v.as_deref())?;
        }
        let result = self.apply_inner(ops);
        if result.is_err() {
            self.rollback();
        }
        result
    }

    fn rollback(&mut self) {
        self.dirty.clear();
        self.root = self.meta.root;
        self.next_page = self.meta.page_count;
        self.entries = self.meta.entries;
    }

    fn apply_inner(&mut self, ops: &[(Vec<u8>, Option<Vec<u8>>)]) -> Result<()> {
        for (k, v) in ops {
            match v {
                Some(value) => self.insert_one(k, value)?,
                None => self.remove_one(k)?,
            }
        }
        self.commit()
    }

    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        self.apply(&[(key.to_vec(), Some(value.to_vec()))])
    }

    pub fn remove(&mut self, key: &[u8]) -> Result<()> {
        self.apply(&[(key.to_vec(), None)])
    }

    fn insert_one(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        if self.root == 0 {
            let id = self.store_node(0, Node::Leaf(vec![(key.to_vec(), value.to_vec())]));
            self.root = id;
            self.entries += 1;
            return Ok(());
        }
        let root = self.root;
        let (id, split, added) = self.insert_rec(root, key, value)?;
        self.root = match split {
            None => id,
            Some((sep, right)) => self.store_node(0, Node::Internal(vec![sep], vec![id, right])),
        };
        if added {
            self.entries += 1;
        }
        Ok(())
    }

    fn insert_rec(&mut self, pid: u64, key: &[u8], value: &[u8]) -> Result<(u64, Option<(Vec<u8>, u64)>, bool)> {
        match self.read_node(pid)? {
            Node::Leaf(mut entries) => {
                let added = match entries.binary_search_by(|(k, _)| k.as_slice().cmp(key)) {
                    Ok(i) => {
                        entries[i].1 = value.to_vec();
                        false
                    }
                    Err(i) => {
                        entries.insert(i, (key.to_vec(), value.to_vec()));
                        true
                    }
                };
                let node = Node::Leaf(entries);
                if node.size() <= PAGE_SIZE {
                    return Ok((self.store_node(pid, node), None, added));
                }
                let Node::Leaf(mut left) = node else { unreachable!() };
                let sizes: Vec<usize> = left.iter().map(|(k, v)| 4 + k.len() + v.len()).collect();
                let at = split_point(&sizes);
                let right = left.split_off(at);
                let sep = right[0].0.clone();
                let lid = self.store_node(pid, Node::Leaf(left));
                let rid = self.alloc();
                self.dirty.insert(rid, Node::Leaf(right));
                Ok((lid, Some((sep, rid)), added))
            }
            Node::Internal(mut keys, mut children) => {
                let idx = keys.partition_point(|k| k.as_slice() <= key);
                let (cid, split, added) = self.insert_rec(children[idx], key, value)?;
                children[idx] = cid;
                if let Some((sep, rid)) = split {
                    keys.insert(idx, sep);
                    children.insert(idx + 1, rid);
                }
                let node = Node::Internal(keys, children);
                if node.size() <= PAGE_SIZE {
                    return Ok((self.store_node(pid, node), None, added));
                }
                let Node::Internal(mut lkeys, mut lchildren) = node else { unreachable!() };
                let sizes: Vec<usize> = lkeys.iter().map(|k| 10 + k.len()).collect();
                let mid = split_point(&sizes).min(lkeys.len() - 2);
                let mut rkeys = lkeys.split_off(mid);
                let sep = rkeys.remove(0);
                let rchildren = lchildren.split_off(mid + 1);
                let lid = self.store_node(pid, Node::Internal(lkeys, lchildren));
                let rid = self.alloc();
                self.dirty.insert(rid, Node::Internal(rkeys, rchildren));
                Ok((lid, Some((sep, rid)), added))
            }
        }
    }

    fn remove_one(&mut self, key: &[u8]) -> Result<()> {
        if self.root == 0 {
            return Ok(());
        }
        let root = self.root;
        let (id, removed) = self.remove_rec(root, key)?;
        if removed {
            self.root = id.unwrap_or(0);
            self.entries -= 1;
        }
        Ok(())
    }

    fn remove_rec(&mut self, pid: u64, key: &[u8]) -> Result<(Option<u64>, bool)> {
        match self.read_node(pid)? {
            Node::Leaf(mut entries) => match entries.binary_search_by(|(k, _)| k.as_slice().cmp(key)) {
                Err(_) => Ok((Some(pid), false)),
                Ok(i) => {
                    entries.remove(i);
                    if entries.is_empty() {
                        self.discard(pid);
                        return Ok((None, true));
                    }
                    Ok((Some(self.store_node(pid, Node::Leaf(entries))), true))
                }
            },
            Node::Internal(mut keys, mut children) => {
                let idx = keys.partition_point(|k| k.as_slice() <= key);
                let (child, removed) = self.remove_rec(children[idx], key)?;
                if !removed {
                    return Ok((Some(pid), false));
                }
                match child {
                    Some(c) => {
                        children[idx] = c;
                        self.try_merge(&mut keys, &mut children, idx)?;
                    }
                    None => {
                        children.remove(idx);
                        if idx < keys.len() {
                            keys.remove(idx);
                        } else {
                            keys.pop();
                        }
                    }
                }
                if children.is_empty() {
                    self.discard(pid);
                    return Ok((None, true));
                }
                if children.len() == 1 {
                    self.discard(pid);
                    return Ok((Some(children[0]), true));
                }
                Ok((Some(self.store_node(pid, Node::Internal(keys, children))), true))
            }
        }
    }

    fn try_merge(&mut self, keys: &mut Vec<Vec<u8>>, children: &mut Vec<u64>, idx: usize) -> Result<()> {
        if children.len() < 2 {
            return Ok(());
        }
        let Node::Leaf(child) = self.read_node(children[idx])? else { return Ok(()) };
        let child_size = Node::Leaf(child.clone()).size();
        if child_size > PAGE_SIZE / 4 {
            return Ok(());
        }
        let lo = if idx + 1 < children.len() { idx } else { idx - 1 };
        let Node::Leaf(a) = self.read_node(children[lo])? else { return Ok(()) };
        let Node::Leaf(b) = self.read_node(children[lo + 1])? else { return Ok(()) };
        let mut merged = a;
        merged.extend(b);
        let node = Node::Leaf(merged);
        if node.size() > PAGE_SIZE {
            return Ok(());
        }
        let (left_id, right_id) = (children[lo], children[lo + 1]);
        self.discard(left_id);
        self.discard(right_id);
        let id = self.alloc();
        self.dirty.insert(id, node);
        children[lo] = id;
        children.remove(lo + 1);
        keys.remove(lo);
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if self.dirty.is_empty() && self.root == self.meta.root && self.entries == self.meta.entries {
            return Ok(());
        }
        let mut ids: Vec<u64> = self.dirty.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let page = self.dirty[&id].encode()?;
            self.file.seek(SeekFrom::Start(id * PAGE_SIZE as u64))?;
            self.file.write_all(&page)?;
        }
        self.file.sync_data()?;
        let next = Meta { txid: self.meta.txid + 1, root: self.root, page_count: self.next_page, entries: self.entries };
        self.file.seek(SeekFrom::Start((next.txid % 2) * PAGE_SIZE as u64))?;
        self.file.write_all(&next.encode())?;
        self.file.sync_data()?;
        self.meta = next;
        self.dirty.clear();
        Ok(())
    }

    pub fn check(&mut self) -> Result<TreeReport> {
        let mut report = TreeReport { entries: 0, pages_reachable: 0, pages_in_file: self.meta.page_count, height: 0 };
        if self.root != 0 {
            let root = self.root;
            report.height = self.check_node(root, None, None, &mut report)?;
        }
        if report.entries != self.meta.entries {
            return Err(Error::Corrupt("entry count does not match tree contents".into()));
        }
        Ok(report)
    }

    fn check_node(&mut self, pid: u64, lo: Option<&[u8]>, hi: Option<&[u8]>, report: &mut TreeReport) -> Result<u32> {
        report.pages_reachable += 1;
        match self.read_node(pid)? {
            Node::Leaf(entries) => {
                if entries.is_empty() {
                    return Err(Error::Corrupt("empty leaf".into()));
                }
                for w in entries.windows(2) {
                    if w[0].0 >= w[1].0 {
                        return Err(Error::Corrupt("leaf keys out of order".into()));
                    }
                }
                if lo.map_or(false, |l| entries[0].0.as_slice() < l) || hi.map_or(false, |h| entries[entries.len() - 1].0.as_slice() >= h) {
                    return Err(Error::Corrupt("leaf key outside parent bounds".into()));
                }
                report.entries += entries.len() as u64;
                Ok(1)
            }
            Node::Internal(keys, children) => {
                if children.len() != keys.len() + 1 || keys.is_empty() {
                    return Err(Error::Corrupt("malformed internal node".into()));
                }
                let mut depth = None;
                for (i, c) in children.iter().enumerate() {
                    let clo = if i == 0 { lo } else { Some(keys[i - 1].as_slice()) };
                    let chi = if i == keys.len() { hi } else { Some(keys[i].as_slice()) };
                    let d = self.check_node(*c, clo, chi, report)?;
                    if depth.map_or(false, |x| x != d) {
                        return Err(Error::Corrupt("leaves at different depths".into()));
                    }
                    depth = Some(d);
                }
                Ok(depth.unwrap() + 1)
            }
        }
    }
}