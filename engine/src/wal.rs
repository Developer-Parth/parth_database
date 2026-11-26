
use crate::crc::crc32;
use crate::error::Result;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const HEADER: usize = 8;
pub const MAX_RECORD: usize = 256 * 1024 * 1024;

pub struct Wal {
    file: File,
    len: u64,
}

pub struct Recovered {
    pub records: Vec<Vec<u8>>,
    pub discarded_bytes: u64,
}

impl Wal {
    pub fn open(path: &Path) -> Result<(Wal, Recovered)> {
        let mut file = OpenOptions::new().read(true).write(true).create(true).open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut records = Vec::new();
        let mut pos = 0usize;
        while buf.len() - pos >= HEADER {
            let len = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
            let crc = u32::from_le_bytes(buf[pos + 4..pos + 8].try_into().unwrap());
            if len > MAX_RECORD || buf.len() - pos - HEADER < len {
                break;
            }
            let payload = &buf[pos + HEADER..pos + HEADER + len];
            if crc32(payload) != crc {
                break;
            }
            records.push(payload.to_vec());
            pos += HEADER + len;
        }

        let discarded = (buf.len() - pos) as u64;
        if discarded > 0 {
            file.set_len(pos as u64)?;
            file.sync_all()?;
        }
        file.seek(SeekFrom::Start(pos as u64))?;
        Ok((Wal { file, len: pos as u64 }, Recovered { records, discarded_bytes: discarded }))
    }

    pub fn append(&mut self, payload: &[u8], sync: bool) -> Result<()> {
        let mut frame = Vec::with_capacity(HEADER + payload.len());
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(&crc32(payload).to_le_bytes());
        frame.extend_from_slice(payload);
        if let Err(e) = self.file.write_all(&frame) {
            let _ = self.file.set_len(self.len);
            let _ = self.file.seek(SeekFrom::Start(self.len));
            return Err(e.into());
        }
        if sync {
            self.file.sync_data()?;
        }
        self.len += frame.len() as u64;
        Ok(())
    }

    pub fn sync(&mut self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn reset(&mut self) -> Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.sync_all()?;
        self.len = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("parth_db_wal_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn roundtrip() {
        let p = tmp("roundtrip");
        {
            let (mut w, r) = Wal::open(&p).unwrap();
            assert!(r.records.is_empty());
            w.append(b"one", true).unwrap();
            w.append(b"two", true).unwrap();
        }
        let (_, r) = Wal::open(&p).unwrap();
        assert_eq!(r.records, vec![b"one".to_vec(), b"two".to_vec()]);
        assert_eq!(r.discarded_bytes, 0);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn torn_tail_is_truncated_at_every_cut_point() {
        let p = tmp("torn");
        {
            let (mut w, _) = Wal::open(&p).unwrap();
            w.append(b"first-record", true).unwrap();
            w.append(b"second-record", true).unwrap();
        }
        let full = std::fs::read(&p).unwrap();
        let first_end = HEADER + b"first-record".len();
        for cut in 0..full.len() {
            std::fs::write(&p, &full[..cut]).unwrap();
            let (mut w, r) = Wal::open(&p).unwrap();
            let expected = if cut >= full.len() { 2 } else if cut >= first_end { 1 } else { 0 };
            assert_eq!(r.records.len(), expected, "cut at {cut}");
            w.append(b"after", true).unwrap();
            drop(w);
            let (_, r2) = Wal::open(&p).unwrap();
            assert_eq!(r2.records.len(), expected + 1);
            assert_eq!(r2.discarded_bytes, 0);
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn corrupt_record_stops_replay() {
        let p = tmp("corrupt");
        {
            let (mut w, _) = Wal::open(&p).unwrap();
            w.append(b"aaaa", true).unwrap();
            w.append(b"bbbb", true).unwrap();
        }
        let mut data = std::fs::read(&p).unwrap();
        let n = data.len();
        data[n - 1] ^= 0xFF;
        std::fs::write(&p, &data).unwrap();
        let (_, r) = Wal::open(&p).unwrap();
        assert_eq!(r.records, vec![b"aaaa".to_vec()]);
        assert!(r.discarded_bytes > 0);
        let _ = std::fs::remove_file(&p);
    }
}