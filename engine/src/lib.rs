//! parth_db: a small crash-safe embedded database.
//!
//! Layers:
//! - `crc`: CRC32 checksums used to detect torn or corrupted records.
//! - `wal`: append-only write-ahead log with framed, checksummed batches.
//! - `store`: ordered key-value store with atomic batches, snapshots and recovery.

pub mod crc;
pub mod error;
pub mod store;
pub mod wal;

pub use error::{Error, Result};
pub use store::{Batch, Op, Options, Store, SyncMode};