use crate::error::{Error, Result};
use crate::disk::DiskStore;
use crate::store::Batch;
use crate::txn::Kv;
use crate::value::{decode_row, encode_key, encode_row, DecodeError, Value};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnType {
    Int,
    Real,
    Text,
    Bool,
}

impl ColumnType {
    fn code(self) -> i64 {
        match self {
            ColumnType::Int => 0,
            ColumnType::Real => 1,
            ColumnType::Text => 2,
            ColumnType::Bool => 3,
        }
    }

    fn from_code(code: i64) -> Option<ColumnType> {
        match code {
            0 => Some(ColumnType::Int),
            1 => Some(ColumnType::Real),
            2 => Some(ColumnType::Text),
            3 => Some(ColumnType::Bool),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub name: String,
    pub ty: ColumnType,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    pub name: String,
    pub columns: Vec<Column>,
    pub primary_key: usize,
}

impl Schema {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }

    fn encode(&self) -> Vec<u8> {
        let mut values = vec![
            Value::Text(self.name.clone()),
            Value::Int(self.primary_key as i64),
        ];
        for c in &self.columns {
            values.push(Value::Text(c.name.clone()));
            values.push(Value::Int(c.ty.code()));
        }
        encode_row(&values)
    }

    fn decode(bytes: &[u8]) -> Result<Schema> {
        let values = decode_row(bytes).map_err(corrupt)?;
        if values.len() < 4 || values.len() % 2 != 0 {
            return Err(Error::Corrupt("bad catalog entry".into()));
        }
        let name = match &values[0] {
            Value::Text(s) => s.clone(),
            _ => return Err(Error::Corrupt("bad table name".into())),
        };
        let primary_key = match &values[1] {
            Value::Int(i) if *i >= 0 => *i as usize,
            _ => return Err(Error::Corrupt("bad primary key".into())),
        };
        let mut columns = Vec::new();
        for pair in values[2..].chunks(2) {
            let col_name = match &pair[0] {
                Value::Text(s) => s.clone(),
                _ => return Err(Error::Corrupt("bad column name".into())),
            };
            let ty = match &pair[1] {
                Value::Int(i) => ColumnType::from_code(*i),
                _ => None,
            };
            let ty = ty.ok_or_else(|| Error::Corrupt("bad column type".into()))?;
            columns.push(Column { name: col_name, ty });
        }
        if primary_key >= columns.len() {
            return Err(Error::Corrupt("primary key out of range".into()));
        }
        Ok(Schema { name, columns, primary_key })
    }
}

fn corrupt(e: DecodeError) -> Error {
    Error::Corrupt(e.0.to_string())
}

fn invalid(message: &str) -> Error {
    Error::InvalidInput(message.to_string())
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    name.len() <= 64 && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn catalog_key(table: &str) -> Vec<u8> {
    let mut key = b"c\0".to_vec();
    key.extend_from_slice(table.as_bytes());
    key
}

fn table_prefix(table: &str) -> Vec<u8> {
    let mut key = b"t\0".to_vec();
    key.extend_from_slice(table.as_bytes());
    key.push(0);
    key
}

pub(crate) fn row_key(table: &str, pk: &Value) -> Result<Vec<u8>> {
    let mut key = table_prefix(table);
    key.extend(encode_key(pk).map_err(corrupt)?);
    Ok(key)
}

pub struct Database {
    pub(crate) store: Kv,
}

impl Database {
    pub fn open(dir: impl AsRef<Path>) -> Result<Database> {
        Ok(Database { store: Kv::new(DiskStore::open(dir)?) })
    }

    pub fn store(&self) -> &DiskStore {
        self.store.disk()
    }

    pub fn tables(&self) -> Result<Vec<Schema>> {
        self.store
            .scan_prefix(b"c\0", usize::MAX)?
            .into_iter()
            .map(|(_, v)| Schema::decode(&v))
            .collect()
    }

    pub fn schema(&self, table: &str) -> Result<Schema> {
        match self.store.get(&catalog_key(table))? {
            Some(bytes) => Schema::decode(&bytes),
            None => Err(Error::InvalidInput(format!("no such table: {table}"))),
        }
    }

    pub fn create_table(&self, schema: Schema) -> Result<()> {
        if !valid_name(&schema.name) {
            return Err(invalid("invalid table name"));
        }
        if schema.columns.is_empty() || schema.primary_key >= schema.columns.len() {
            return Err(invalid("table needs columns and a valid primary key"));
        }
        for (i, c) in schema.columns.iter().enumerate() {
            if !valid_name(&c.name) {
                return Err(invalid("invalid column name"));
            }
            if schema.columns[..i].iter().any(|o| o.name == c.name) {
                return Err(invalid("duplicate column name"));
            }
        }
        match schema.columns[schema.primary_key].ty {
            ColumnType::Int | ColumnType::Text => {}
            _ => return Err(invalid("primary key must be integer or text")),
        }
        if self.store.contains(&catalog_key(&schema.name))? {
            return Err(Error::InvalidInput(format!("table exists: {}", schema.name)));
        }
        self.store.put(catalog_key(&schema.name), schema.encode())
    }

    pub fn drop_table(&self, table: &str) -> Result<()> {
        self.schema(table)?;
        let mut batch = Batch::new();
        for (key, _) in self.store.scan_prefix(&table_prefix(table), usize::MAX)? {
            batch.delete(key);
        }
        batch.delete(catalog_key(table));
        self.store.write(batch)
    }

    pub(crate) fn check_row(schema: &Schema, row: &[Value]) -> Result<Vec<Value>> {
        if row.len() != schema.columns.len() {
            return Err(invalid("wrong number of values"));
        }
        let mut out = Vec::with_capacity(row.len());
        for (i, (value, column)) in row.iter().zip(&schema.columns).enumerate() {
            let fixed = match (value, column.ty) {
                (Value::Null, _) if i != schema.primary_key => Value::Null,
                (Value::Int(n), ColumnType::Int) => Value::Int(*n),
                (Value::Int(n), ColumnType::Real) => Value::Real(*n as f64),
                (Value::Real(r), ColumnType::Real) => Value::Real(*r),
                (Value::Text(s), ColumnType::Text) => Value::Text(s.clone()),
                (Value::Bool(b), ColumnType::Bool) => Value::Bool(*b),
                _ => return Err(Error::InvalidInput(format!("bad value for column {}", column.name))),
            };
            out.push(fixed);
        }
        Ok(out)
    }

    pub fn insert(&self, table: &str, row: &[Value]) -> Result<()> {
        let schema = self.schema(table)?;
        let row = Self::check_row(&schema, row)?;
        let key = row_key(table, &row[schema.primary_key])?;
        if self.store.contains(&key)? {
            return Err(invalid("duplicate primary key"));
        }
        self.store.put(key, encode_row(&row))
    }

    pub fn get(&self, table: &str, pk: &Value) -> Result<Option<Vec<Value>>> {
        self.schema(table)?;
        match self.store.get(&row_key(table, pk)?)? {
            Some(bytes) => Ok(Some(decode_row(&bytes).map_err(corrupt)?)),
            None => Ok(None),
        }
    }

    pub fn update(&self, table: &str, pk: &Value, row: &[Value]) -> Result<bool> {
        let schema = self.schema(table)?;
        let row = Self::check_row(&schema, row)?;
        let old_key = row_key(table, pk)?;
        if !self.store.contains(&old_key)? {
            return Ok(false);
        }
        let new_key = row_key(table, &row[schema.primary_key])?;
        if new_key != old_key && self.store.contains(&new_key)? {
            return Err(invalid("duplicate primary key"));
        }
        let mut batch = Batch::new();
        if new_key != old_key {
            batch.delete(old_key);
        }
        batch.put(new_key, encode_row(&row));
        self.store.write(batch)?;
        Ok(true)
    }

    pub fn delete(&self, table: &str, pk: &Value) -> Result<bool> {
        self.schema(table)?;
        let key = row_key(table, pk)?;
        if !self.store.contains(&key)? {
            return Ok(false);
        }
        self.store.delete(key)?;
        Ok(true)
    }

    pub fn scan(&self, table: &str) -> Result<Vec<Vec<Value>>> {
        self.schema(table)?;
        self.store
            .scan_prefix(&table_prefix(table), usize::MAX)?
            .into_iter()
            .map(|(_, v)| decode_row(&v).map_err(corrupt))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pd_table_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn people() -> Schema {
        Schema {
            name: "people".into(),
            columns: vec![
                Column { name: "id".into(), ty: ColumnType::Int },
                Column { name: "name".into(), ty: ColumnType::Text },
                Column { name: "score".into(), ty: ColumnType::Real },
            ],
            primary_key: 0,
        }
    }

    fn person(id: i64, name: &str) -> Vec<Value> {
        vec![Value::Int(id), Value::Text(name.into()), Value::Null]
    }

    #[test]
    fn insert_get_scan_in_key_order() {
        let dir = temp_dir("basic");
        let db = Database::open(&dir).unwrap();
        db.create_table(people()).unwrap();
        for id in [30, -4, 7] {
            db.insert("people", &person(id, "x")).unwrap();
        }
        let ids: Vec<Value> = db.scan("people").unwrap().into_iter().map(|r| r[0].clone()).collect();
        assert_eq!(ids, vec![Value::Int(-4), Value::Int(7), Value::Int(30)]);
        assert_eq!(db.get("people", &Value::Int(7)).unwrap(), Some(person(7, "x")));
        assert_eq!(db.get("people", &Value::Int(8)).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_bad_rows_and_duplicates() {
        let dir = temp_dir("bad");
        let db = Database::open(&dir).unwrap();
        db.create_table(people()).unwrap();
        db.insert("people", &person(1, "a")).unwrap();
        assert!(db.insert("people", &person(1, "b")).is_err());
        assert!(db.insert("people", &[Value::Int(2)]).is_err());
        assert!(db.insert("people", &[Value::Null, Value::Null, Value::Null]).is_err());
        assert!(db.insert("people", &[Value::Int(2), Value::Int(5), Value::Null]).is_err());
        assert!(db.insert("nothing", &person(1, "a")).is_err());
        assert_eq!(db.scan("people").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_and_delete() {
        let dir = temp_dir("upd");
        let db = Database::open(&dir).unwrap();
        db.create_table(people()).unwrap();
        db.insert("people", &person(1, "a")).unwrap();
        db.insert("people", &person(2, "b")).unwrap();
        assert!(db.update("people", &Value::Int(1), &person(1, "z")).unwrap());
        assert!(db.update("people", &Value::Int(1), &person(2, "z")).is_err());
        assert!(db.update("people", &Value::Int(1), &person(9, "moved")).unwrap());
        assert_eq!(db.get("people", &Value::Int(1)).unwrap(), None);
        assert!(!db.update("people", &Value::Int(77), &person(78, "n")).unwrap());
        assert!(db.delete("people", &Value::Int(2)).unwrap());
        assert!(!db.delete("people", &Value::Int(2)).unwrap());
        assert_eq!(db.scan("people").unwrap(), vec![person(9, "moved")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schema_validation() {
        let dir = temp_dir("schema");
        let db = Database::open(&dir).unwrap();
        let mut bad = people();
        bad.name = "1bad".into();
        assert!(db.create_table(bad).is_err());
        let mut dup = people();
        dup.columns[1].name = "id".into();
        assert!(db.create_table(dup).is_err());
        let mut real_key = people();
        real_key.primary_key = 2;
        assert!(db.create_table(real_key).is_err());
        db.create_table(people()).unwrap();
        assert!(db.create_table(people()).is_err());
        assert_eq!(db.tables().unwrap(), vec![people()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_table_removes_rows_and_catalog() {
        let dir = temp_dir("drop");
        let db = Database::open(&dir).unwrap();
        db.create_table(people()).unwrap();
        db.insert("people", &person(1, "a")).unwrap();
        db.drop_table("people").unwrap();
        assert!(db.schema("people").is_err());
        assert!(db.drop_table("people").is_err());
        db.create_table(people()).unwrap();
        assert!(db.scan("people").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn data_and_catalog_survive_reopen() {
        let dir = temp_dir("reopen");
        {
            let db = Database::open(&dir).unwrap();
            db.create_table(people()).unwrap();
            db.insert("people", &person(5, "kept")).unwrap();
        }
        let db = Database::open(&dir).unwrap();
        assert_eq!(db.schema("people").unwrap(), people());
        assert_eq!(db.get("people", &Value::Int(5)).unwrap(), Some(person(5, "kept")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}