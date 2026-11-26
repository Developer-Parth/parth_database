use crate::value::Value;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AggFunc {
    Count,
    Sum,
    Min,
    Max,
    Avg,
}

impl AggFunc {
    pub fn parse(word: &str) -> Option<AggFunc> {
        match word.to_ascii_uppercase().as_str() {
            "COUNT" => Some(AggFunc::Count),
            "SUM" => Some(AggFunc::Sum),
            "MIN" => Some(AggFunc::Min),
            "MAX" => Some(AggFunc::Max),
            "AVG" => Some(AggFunc::Avg),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AggFunc::Count => "count",
            AggFunc::Sum => "sum",
            AggFunc::Min => "min",
            AggFunc::Max => "max",
            AggFunc::Avg => "avg",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Aggregate {
    pub func: AggFunc,
    pub column: Option<String>,
}

impl Aggregate {
    pub fn label(&self) -> String {
        let target = self.column.as_deref().unwrap_or("*");
        format!("{}({})", self.func.name(), target)
    }
}

pub fn evaluate(func: AggFunc, index: Option<usize>, rows: &[Vec<Value>]) -> Result<Value, String> {
    let values: Vec<&Value> = match index {
        Some(i) => rows
            .iter()
            .map(|row| &row[i])
            .filter(|v| !matches!(v, Value::Null))
            .collect(),
        None => Vec::new(),
    };
    match func {
        AggFunc::Count => {
            let count = if index.is_some() { values.len() } else { rows.len() };
            Ok(Value::Int(count as i64))
        }
        AggFunc::Min => Ok(extreme(&values, Ordering::Less)),
        AggFunc::Max => Ok(extreme(&values, Ordering::Greater)),
        AggFunc::Sum | AggFunc::Avg => total(func, &values),
    }
}

fn extreme(values: &[&Value], wanted: Ordering) -> Value {
    let mut best: Option<&Value> = None;
    for value in values {
        best = match best {
            Some(current) if value.compare(current) != wanted => Some(current),
            _ => Some(*value),
        };
    }
    best.cloned().unwrap_or(Value::Null)
}

fn total(func: AggFunc, values: &[&Value]) -> Result<Value, String> {
    if values.is_empty() {
        return Ok(Value::Null);
    }
    let mut int_sum: Option<i64> = Some(0);
    let mut real_sum = 0.0f64;
    for value in values {
        match value {
            Value::Int(n) => {
                int_sum = int_sum.and_then(|sum| sum.checked_add(*n));
                real_sum += *n as f64;
            }
            Value::Real(r) => {
                int_sum = None;
                real_sum += *r;
            }
            _ => return Err(format!("{} needs a numeric column", func.name())),
        }
    }
    if func == AggFunc::Avg {
        return Ok(Value::Real(real_sum / values.len() as f64));
    }
    match int_sum {
        Some(sum) => Ok(Value::Int(sum)),
        None => Ok(Value::Real(real_sum)),
    }
}