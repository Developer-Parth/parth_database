use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Text(String),
}

#[derive(Debug, PartialEq)]
pub struct DecodeError(pub &'static str);

impl Value {
    pub fn compare(&self, other: &Value) -> Ordering {
        match (self, other) {
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Null, _) => Ordering::Less,
            (_, Value::Null) => Ordering::Greater,
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Real(a), Value::Real(b)) => a.total_cmp(b),
            (Value::Int(a), Value::Real(b)) => (*a as f64).total_cmp(b),
            (Value::Real(a), Value::Int(b)) => a.total_cmp(&(*b as f64)),
            (Value::Text(a), Value::Text(b)) => a.cmp(b),
            (a, b) => a.rank().cmp(&b.rank()),
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Int(_) | Value::Real(_) => 2,
            Value::Text(_) => 3,
        }
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Value::Null => out.push(0),
            Value::Bool(b) => {
                out.push(1);
                out.push(*b as u8);
            }
            Value::Int(i) => {
                out.push(2);
                out.extend_from_slice(&i.to_le_bytes());
            }
            Value::Real(r) => {
                out.push(3);
                out.extend_from_slice(&r.to_le_bytes());
            }
            Value::Text(s) => {
                out.push(4);
                out.extend_from_slice(&(s.len() as u32).to_le_bytes());
                out.extend_from_slice(s.as_bytes());
            }
        }
    }

    pub fn decode(input: &[u8], pos: &mut usize) -> Result<Value, DecodeError> {
        let tag = *input.get(*pos).ok_or(DecodeError("truncated value"))?;
        *pos += 1;
        match tag {
            0 => Ok(Value::Null),
            1 => {
                let b = *input.get(*pos).ok_or(DecodeError("truncated bool"))?;
                *pos += 1;
                match b {
                    0 => Ok(Value::Bool(false)),
                    1 => Ok(Value::Bool(true)),
                    _ => Err(DecodeError("invalid bool")),
                }
            }
            2 => Ok(Value::Int(i64::from_le_bytes(take8(input, pos)?))),
            3 => Ok(Value::Real(f64::from_le_bytes(take8(input, pos)?))),
            4 => {
                let end = pos.checked_add(4).filter(|e| *e <= input.len());
                let end = end.ok_or(DecodeError("truncated length"))?;
                let len = u32::from_le_bytes(input[*pos..end].try_into().unwrap()) as usize;
                *pos = end;
                let stop = pos.checked_add(len).filter(|e| *e <= input.len());
                let stop = stop.ok_or(DecodeError("truncated text"))?;
                let text = std::str::from_utf8(&input[*pos..stop]).map_err(|_| DecodeError("invalid utf-8"))?;
                *pos = stop;
                Ok(Value::Text(text.to_string()))
            }
            _ => Err(DecodeError("unknown value tag")),
        }
    }
}

fn take8(input: &[u8], pos: &mut usize) -> Result<[u8; 8], DecodeError> {
    let end = pos.checked_add(8).filter(|e| *e <= input.len());
    let end = end.ok_or(DecodeError("truncated number"))?;
    let bytes: [u8; 8] = input[*pos..end].try_into().unwrap();
    *pos = end;
    Ok(bytes)
}

pub fn encode_row(values: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    for v in values {
        v.encode(&mut out);
    }
    out
}

pub fn decode_row(input: &[u8]) -> Result<Vec<Value>, DecodeError> {
    if input.len() < 4 {
        return Err(DecodeError("truncated row"));
    }
    let count = u32::from_le_bytes(input[..4].try_into().unwrap()) as usize;
    let mut pos = 4;
    let mut values = Vec::new();
    for _ in 0..count {
        values.push(Value::decode(input, &mut pos)?);
    }
    if pos != input.len() {
        return Err(DecodeError("trailing bytes in row"));
    }
    Ok(values)
}

pub fn encode_key(value: &Value) -> Result<Vec<u8>, DecodeError> {
    match value {
        Value::Int(i) => {
            let mut out = vec![2];
            out.extend_from_slice(&((*i as u64) ^ (1 << 63)).to_be_bytes());
            Ok(out)
        }
        Value::Text(s) => {
            let mut out = vec![4];
            out.extend_from_slice(s.as_bytes());
            Ok(out)
        }
        _ => Err(DecodeError("key must be integer or text")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_round_trip() {
        let row = vec![
            Value::Int(-5),
            Value::Text("h\u{e9}llo".into()),
            Value::Null,
            Value::Bool(true),
            Value::Real(2.5),
        ];
        assert_eq!(decode_row(&encode_row(&row)).unwrap(), row);
    }

    #[test]
    fn truncated_rows_are_rejected_at_every_length() {
        let row = vec![Value::Int(1), Value::Text("abc".into()), Value::Real(1.0)];
        let bytes = encode_row(&row);
        for cut in 0..bytes.len() {
            assert!(decode_row(&bytes[..cut]).is_err(), "cut {}", cut);
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_row(&[1, 0, 0, 0, 9]).is_err());
        assert!(decode_row(&[1, 0, 0, 0, 1, 7]).is_err());
        assert!(decode_row(&[1, 0, 0, 0, 4, 2, 0, 0, 0, 0xff, 0xfe]).is_err());
        let mut extra = encode_row(&[Value::Null]);
        extra.push(0);
        assert!(decode_row(&extra).is_err());
        assert!(decode_row(&[255, 255, 255, 255]).is_err());
    }

    #[test]
    fn integer_keys_sort_like_integers() {
        let nums = [i64::MIN, -100, -1, 0, 1, 7, 1000, i64::MAX];
        let keys: Vec<Vec<u8>> = nums.iter().map(|n| encode_key(&Value::Int(*n)).unwrap()).collect();
        for pair in keys.windows(2) {
            assert!(pair[0] < pair[1]);
        }
    }

    #[test]
    fn compare_orders_mixed_values() {
        assert_eq!(Value::Null.compare(&Value::Int(0)), Ordering::Less);
        assert_eq!(Value::Int(2).compare(&Value::Real(2.5)), Ordering::Less);
        assert_eq!(Value::Text("b".into()).compare(&Value::Text("a".into())), Ordering::Greater);
        assert_eq!(Value::Int(9).compare(&Value::Text("a".into())), Ordering::Less);
    }
}