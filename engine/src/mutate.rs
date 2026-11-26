use crate::error::{Error, Result};
use crate::store::Batch;
use crate::table::{row_key, Database};
use crate::value::{encode_row, Value};
use std::collections::HashSet;

fn duplicate() -> Error {
    Error::InvalidInput("duplicate primary key".to_string())
}

impl Database {
    pub fn insert_many(&self, table: &str, rows: &[Vec<Value>]) -> Result<()> {
        let schema = self.schema(table)?;
        let mut batch = Batch::new();
        let mut seen = HashSet::new();
        for row in rows {
            let row = Self::check_row(&schema, row)?;
            let key = row_key(table, &row[schema.primary_key])?;
            if !seen.insert(key.clone()) || self.store().contains(&key)? {
                return Err(duplicate());
            }
            batch.put(key, encode_row(&row));
        }
        if batch.is_empty() {
            return Ok(());
        }
        self.store().write(batch)
    }

    pub fn update_many(&self, table: &str, changes: &[(Value, Vec<Value>)]) -> Result<()> {
        let schema = self.schema(table)?;
        let mut old_keys = HashSet::new();
        let mut new_rows = Vec::new();
        for (old_pk, row) in changes {
            old_keys.insert(row_key(table, old_pk)?);
            let row = Self::check_row(&schema, row)?;
            new_rows.push((row_key(table, &row[schema.primary_key])?, row));
        }
        let mut new_keys = HashSet::new();
        for (key, _) in &new_rows {
            let taken = self.store().contains(key)? && !old_keys.contains(key);
            if !new_keys.insert(key.clone()) || taken {
                return Err(duplicate());
            }
        }
        let mut batch = Batch::new();
        for key in old_keys.difference(&new_keys) {
            batch.delete(key.clone());
        }
        for (key, row) in new_rows {
            batch.put(key, encode_row(&row));
        }
        if batch.is_empty() {
            return Ok(());
        }
        self.store().write(batch)
    }

    pub fn delete_many(&self, table: &str, pks: &[Value]) -> Result<()> {
        self.schema(table)?;
        let mut batch = Batch::new();
        for pk in pks {
            batch.delete(row_key(table, pk)?);
        }
        if batch.is_empty() {
            return Ok(());
        }
        self.store().write(batch)
    }
}