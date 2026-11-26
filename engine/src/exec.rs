use crate::error::{Error, Result};
use crate::parser::{parse, BinOp, Expr, Projection, Statement};
use crate::aggregate;
use crate::table::{Column, Database, Schema};
use crate::value::Value;

#[derive(Debug, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub affected: usize,
}

impl QueryResult {
    fn affected(count: usize) -> QueryResult {
        QueryResult { columns: Vec::new(), rows: Vec::new(), affected: count }
    }
}

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::InvalidInput(message.into()))
}

pub fn execute(db: &Database, sql: &str) -> Result<QueryResult> {
    run(db, parse(sql)?)
}

pub fn run(db: &Database, statement: Statement) -> Result<QueryResult> {
    match statement {
        Statement::CreateTable { name, columns } => {
            let primary_key = columns.iter().position(|c| c.primary_key).unwrap_or(0);
            let columns = columns.into_iter().map(|c| Column { name: c.name, ty: c.ty }).collect();
            db.create_table(Schema { name, columns, primary_key })?;
            Ok(QueryResult::affected(0))
        }
        Statement::DropTable { name } => {
            db.drop_table(&name)?;
            Ok(QueryResult::affected(0))
        }
        Statement::Insert { table, columns, rows } => insert(db, &table, columns, rows),
        Statement::Select { table, projection, filter, order_by, limit } => {
            select(db, &table, projection, filter, order_by, limit)
        }
        Statement::Update { table, sets, filter } => update(db, &table, sets, filter),
        Statement::Delete { table, filter } => delete(db, &table, filter),
    }
}

fn column_position(schema: &Schema, name: &str) -> Result<usize> {
    match schema.column_index(name) {
        Some(i) => Ok(i),
        None => fail(format!("no such column: {name}")),
    }
}

fn insert(db: &Database, table: &str, columns: Option<Vec<String>>, rows: Vec<Vec<Expr>>) -> Result<QueryResult> {
    let schema = db.schema(table)?;
    let targets: Vec<usize> = match columns {
        None => (0..schema.columns.len()).collect(),
        Some(names) => {
            let mut targets = Vec::new();
            for name in &names {
                let index = column_position(&schema, name)?;
                if targets.contains(&index) {
                    return fail(format!("column listed twice: {name}"));
                }
                targets.push(index);
            }
            targets
        }
    };
    let mut full_rows = Vec::new();
    for exprs in &rows {
        if exprs.len() != targets.len() {
            return fail("wrong number of values");
        }
        let mut row = vec![Value::Null; schema.columns.len()];
        for (expr, index) in exprs.iter().zip(&targets) {
            row[*index] = eval(expr, None)?;
        }
        full_rows.push(row);
    }
    db.insert_many(table, &full_rows)?;
    Ok(QueryResult::affected(full_rows.len()))
}

fn matching_rows(db: &Database, schema: &Schema, filter: &Option<Expr>) -> Result<Vec<Vec<Value>>> {
    if let Some(expr) = filter {
        check_columns(expr, schema)?;
    }
    let mut matched = Vec::new();
    for row in db.scan(&schema.name)? {
        let keep = match filter {
            Some(expr) => passes(expr, schema, &row)?,
            None => true,
        };
        if keep {
            matched.push(row);
        }
    }
    Ok(matched)
}

fn select(
    db: &Database,
    table: &str,
    projection: Projection,
    filter: Option<Expr>,
    order_by: Option<(String, bool)>,
    limit: Option<usize>,
) -> Result<QueryResult> {
    let schema = db.schema(table)?;
    if let Projection::Aggregates(items) = &projection {
        let rows = matching_rows(db, &schema, &filter)?;
        let mut columns = Vec::new();
        let mut values = Vec::new();
        for item in items {
            let index = match &item.column {
                Some(name) => Some(column_position(&schema, name)?),
                None => None,
            };
            columns.push(item.label());
            match aggregate::evaluate(item.func, index, &rows) {
                Ok(value) => values.push(value),
                Err(message) => return fail(message),
            }
        }
        return Ok(QueryResult { columns, rows: vec![values], affected: 0 });
    }
    let names: Vec<String> = match projection {
        Projection::All => schema.columns.iter().map(|c| c.name.clone()).collect(),
        Projection::Columns(names) => names,
        Projection::Aggregates(_) => Vec::new(),
    };
    let mut indexes = Vec::new();
    for name in &names {
        indexes.push(column_position(&schema, name)?);
    }
    let order = match &order_by {
        Some((name, descending)) => Some((column_position(&schema, name)?, *descending)),
        None => None,
    };
    let mut rows = matching_rows(db, &schema, &filter)?;
    if let Some((index, descending)) = order {
        rows.sort_by(|a, b| {
            let ordering = a[index].compare(&b[index]);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
    if let Some(limit) = limit {
        rows.truncate(limit);
    }
    let rows = rows
        .into_iter()
        .map(|row| indexes.iter().map(|i| row[*i].clone()).collect())
        .collect();
    Ok(QueryResult { columns: names, rows, affected: 0 })
}

fn update(db: &Database, table: &str, sets: Vec<(String, Expr)>, filter: Option<Expr>) -> Result<QueryResult> {
    let schema = db.schema(table)?;
    let mut assignments = Vec::new();
    for (name, expr) in &sets {
        check_columns(expr, &schema)?;
        let index = column_position(&schema, name)?;
        if assignments.iter().any(|(i, _): &(usize, &Expr)| *i == index) {
            return fail(format!("column assigned twice: {name}"));
        }
        assignments.push((index, expr));
    }
    let mut changes = Vec::new();
    for row in matching_rows(db, &schema, &filter)? {
        let mut new_row = row.clone();
        for (index, expr) in &assignments {
            new_row[*index] = eval(expr, Some((&schema, &row)))?;
        }
        changes.push((row[schema.primary_key].clone(), new_row));
    }
    db.update_many(table, &changes)?;
    Ok(QueryResult::affected(changes.len()))
}

fn delete(db: &Database, table: &str, filter: Option<Expr>) -> Result<QueryResult> {
    let schema = db.schema(table)?;
    let keys: Vec<Value> = matching_rows(db, &schema, &filter)?
        .into_iter()
        .map(|row| row[schema.primary_key].clone())
        .collect();
    db.delete_many(table, &keys)?;
    Ok(QueryResult::affected(keys.len()))
}

fn check_columns(expr: &Expr, schema: &Schema) -> Result<()> {
    match expr {
        Expr::Literal(_) => Ok(()),
        Expr::Column(name) => column_position(schema, name).map(|_| ()),
        Expr::Not(inner) | Expr::Neg(inner) | Expr::IsNull(inner, _) => check_columns(inner, schema),
        Expr::Binary(_, left, right) => {
            check_columns(left, schema)?;
            check_columns(right, schema)
        }
    }
}

fn passes(expr: &Expr, schema: &Schema, row: &[Value]) -> Result<bool> {
    match eval(expr, Some((schema, row)))? {
        Value::Bool(b) => Ok(b),
        Value::Null => Ok(false),
        _ => fail("WHERE must be a boolean expression"),
    }
}

fn truth(value: Value) -> Result<Option<bool>> {
    match value {
        Value::Bool(b) => Ok(Some(b)),
        Value::Null => Ok(None),
        _ => fail("expected a boolean"),
    }
}

pub fn eval(expr: &Expr, context: Option<(&Schema, &[Value])>) -> Result<Value> {
    match expr {
        Expr::Literal(v) => Ok(v.clone()),
        Expr::Column(name) => match context {
            Some((schema, row)) => Ok(row[column_position(schema, name)?].clone()),
            None => fail(format!("column not allowed here: {name}")),
        },
        Expr::IsNull(inner, negated) => {
            let is_null = eval(inner, context)? == Value::Null;
            Ok(Value::Bool(is_null != *negated))
        }
        Expr::Not(inner) => match truth(eval(inner, context)?)? {
            Some(b) => Ok(Value::Bool(!b)),
            None => Ok(Value::Null),
        },
        Expr::Neg(inner) => match eval(inner, context)? {
            Value::Null => Ok(Value::Null),
            Value::Int(n) => match n.checked_neg() {
                Some(v) => Ok(Value::Int(v)),
                None => fail("integer overflow"),
            },
            Value::Real(r) => Ok(Value::Real(-r)),
            _ => fail("cannot negate this value"),
        },
        Expr::Binary(op, left, right) => {
            let l = eval(left, context)?;
            let r = eval(right, context)?;
            binary(*op, l, r)
        }
    }
}

fn binary(op: BinOp, l: Value, r: Value) -> Result<Value> {
    match op {
        BinOp::And => {
            let (a, b) = (truth(l)?, truth(r)?);
            Ok(match (a, b) {
                (Some(false), _) | (_, Some(false)) => Value::Bool(false),
                (Some(true), Some(true)) => Value::Bool(true),
                _ => Value::Null,
            })
        }
        BinOp::Or => {
            let (a, b) = (truth(l)?, truth(r)?);
            Ok(match (a, b) {
                (Some(true), _) | (_, Some(true)) => Value::Bool(true),
                (Some(false), Some(false)) => Value::Bool(false),
                _ => Value::Null,
            })
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            if l == Value::Null || r == Value::Null {
                return Ok(Value::Null);
            }
            let comparable = matches!(
                (&l, &r),
                (Value::Int(_) | Value::Real(_), Value::Int(_) | Value::Real(_))
                    | (Value::Text(_), Value::Text(_))
                    | (Value::Bool(_), Value::Bool(_))
            );
            if !comparable {
                return fail("cannot compare values of different types");
            }
            let ordering = l.compare(&r);
            Ok(Value::Bool(match op {
                BinOp::Eq => ordering.is_eq(),
                BinOp::Ne => ordering.is_ne(),
                BinOp::Lt => ordering.is_lt(),
                BinOp::Le => ordering.is_le(),
                BinOp::Gt => ordering.is_gt(),
                _ => ordering.is_ge(),
            }))
        }
        _ => arithmetic(op, l, r),
    }
}

fn arithmetic(op: BinOp, l: Value, r: Value) -> Result<Value> {
    match (l, r) {
        (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
        (Value::Int(a), Value::Int(b)) => {
            let result = match op {
                BinOp::Add => a.checked_add(b),
                BinOp::Sub => a.checked_sub(b),
                BinOp::Mul => a.checked_mul(b),
                _ => {
                    if b == 0 {
                        return fail("division by zero");
                    }
                    a.checked_div(b)
                }
            };
            match result {
                Some(v) => Ok(Value::Int(v)),
                None => fail("integer overflow"),
            }
        }
        (a @ (Value::Int(_) | Value::Real(_)), b @ (Value::Int(_) | Value::Real(_))) => {
            let (a, b) = (as_real(&a), as_real(&b));
            if op == BinOp::Div && b == 0.0 {
                return fail("division by zero");
            }
            let result = match op {
                BinOp::Add => a + b,
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                _ => a / b,
            };
            if result.is_finite() {
                Ok(Value::Real(result))
            } else {
                fail("numeric overflow")
            }
        }
        _ => fail("arithmetic needs numbers"),
    }
}

fn as_real(value: &Value) -> f64 {
    match value {
        Value::Int(n) => *n as f64,
        Value::Real(r) => *r,
        _ => 0.0,
    }
}