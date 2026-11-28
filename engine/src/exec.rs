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
    pub truncated: bool,
}

impl QueryResult {
    fn affected(count: usize) -> QueryResult {
        QueryResult { columns: Vec::new(), rows: Vec::new(), affected: count, truncated: false }
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
        Statement::Select { table, projection, filter, group_by, having, order_by, limit, offset } => {
            select(db, &table, projection, filter, group_by, having, order_by, limit, offset, (MAX_MATERIALIZED_ROWS, MAX_MATERIALIZED_BYTES))
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

pub const MAX_MATERIALIZED_ROWS: usize = 1_000_000;

pub const MAX_MATERIALIZED_BYTES: usize = 128 * 1024 * 1024;

pub fn bounded_rows(db: &Database, table: &str, max_rows: usize, max_bytes: usize) -> Result<Vec<Vec<Value>>> {
let schema = db.schema(table)?;
matching_rows_limited(db, &schema, &None, Some((max_rows, max_bytes)))
}
pub fn matching_rows_limited(db: &Database, schema: &Schema, filter: &Option<Expr>, limit: Option<(usize, usize)>) -> Result<Vec<Vec<Value>>> {
if let Some(expr) = filter {
        check_columns(expr, schema)?;
    }
let mut matched = Vec::new();
let mut bytes = 0usize;
db.scan_each(&schema.name, 64, |row, size| {
let keep = match filter {
Some(expr) => passes(expr, schema, &row)?,
None => true,
};
if keep {
bytes += size;
matched.push(row);
if let Some((max_rows, max_bytes)) = limit {
if matched.len() > max_rows || bytes > max_bytes { return Err(crate::error::Error::InvalidInput(format!("query matches more than {} rows or {} bytes, add a WHERE clause to narrow it", max_rows, max_bytes))); }
}
}
Ok(true)
})?;
Ok(matched)
}

pub fn select(
    db: &Database,
    table: &str,
    projection: Projection,
    filter: Option<Expr>,
    group_by: Vec<String>,
    having: Option<Vec<(String, BinOp, Value)>>,
    order_by: Option<(String, bool)>,
    limit: Option<usize>,
    offset: usize,
    materialize: (usize, usize),
) -> Result<QueryResult> {
    let schema = db.schema(table)?;
if let Projection::Distinct(names) = &projection { if !group_by.is_empty() { return fail("SELECT DISTINCT is not supported with GROUP BY"); } return distinct_select(db, &schema, names, &filter, &order_by, limit, offset, (materialize.0, materialize.1 / (names.len() + 1))); }
    if !group_by.is_empty() || matches!(projection, Projection::Items(_)) {
            return grouped_select(db, &schema, &projection, filter, &group_by, having, order_by, limit, offset, materialize);    }
    if let Projection::Aggregates(items) = &projection {
        if order_by.is_some() {
            return fail("ORDER BY is not supported with aggregates");
        }
let rows = matching_rows_limited(db, &schema, &filter, Some(materialize))?;
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
        let rows = if limit == Some(0) || offset > 0 { Vec::new() } else { vec![values] };
        return Ok(QueryResult { columns, rows, affected: 0, truncated: false });
    }
    let names: Vec<String> = match projection {
        Projection::All => schema.columns.iter().map(|c| c.name.clone()).collect(),
        Projection::Columns(names) => names,
        Projection::Aggregates(_) => Vec::new(),
        Projection::Items(_) => Vec::new(),
Projection::Distinct(_) => Vec::new(),
    };
    let mut indexes = Vec::new();
    for name in &names {
        indexes.push(column_position(&schema, name)?);
    }
    let order = match &order_by {
        Some((name, descending)) => Some((column_position(&schema, name)?, *descending)),
        None => None,
    };
let top_keep = limit.or(Some(MAX_RESULT_ROWS)).map(|l| offset.saturating_add(l.min(MAX_RESULT_ROWS)).saturating_add(1)).filter(|k| *k <= 2 * MAX_RESULT_ROWS + 1);
let mut rows = match (order, top_keep) {
(None, _) => matching_rows_window(db, &schema, &filter, offset, limit)?,
(Some((index, descending)), Some(keep)) => matching_rows_top(db, &schema, &filter, index, descending, keep)?,
_ => matching_rows_limited(db, &schema, &filter, Some(materialize))?,
};
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
    if order.is_some() { rows.drain(..offset.min(rows.len())); }
    let truncated = cap_rows(&mut rows, limit);
    let rows = rows
        .into_iter()
        .map(|row| indexes.iter().map(|i| row[*i].clone()).collect())
        .collect();
    Ok(QueryResult { columns: names, rows, affected: 0, truncated })
}

fn compare_keys(keys: &[usize], a: &[crate::value::Value], b: &[crate::value::Value]) -> std::cmp::Ordering {
    for &key in keys {
        let ordering = a[key].compare(&b[key]);
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    std::cmp::Ordering::Equal
}

fn grouped_select(
    db: &Database,
    schema: &Schema,
    projection: &Projection,
    filter: Option<Expr>,
    group_by: &[String],
    having: Option<Vec<(String, BinOp, Value)>>,
    order_by: Option<(String, bool)>,
    limit: Option<usize>,
    offset: usize,
    materialize: (usize, usize),
) -> Result<QueryResult> {
    let items: Vec<crate::aggregate::SelectItem> = match projection {
        Projection::All => return fail("SELECT * is not supported with GROUP BY"),
Projection::Distinct(_) => return fail("SELECT DISTINCT is not supported with GROUP BY"),
        Projection::Columns(names) => names
            .iter()
            .cloned()
            .map(crate::aggregate::SelectItem::Column)
            .collect(),
        Projection::Aggregates(list) => list
            .iter()
            .cloned()
            .map(crate::aggregate::SelectItem::Agg)
            .collect(),
        Projection::Items(items) => items.clone(),
    };
    if group_by.is_empty() {
        return fail("columns can only be selected next to aggregates when GROUP BY names them");
    }
    let mut keys = Vec::new();
    for name in group_by {
        let position = column_position(schema, name)?;
        if !keys.contains(&position) {
            keys.push(position);
        }
    }
    let mut columns = Vec::new();
    let mut plan = Vec::new();
    for item in &items {
        match item {
            crate::aggregate::SelectItem::Column(name) => {
                let position = column_position(schema, name)?;
                if !keys.contains(&position) {
                    return fail(format!("column {name} must appear in GROUP BY"));
                }
                columns.push(name.clone());
                plan.push((None, Some(position)));
            }
            crate::aggregate::SelectItem::Agg(aggregate) => {
                let index = match &aggregate.column {
                    Some(name) => Some(column_position(schema, name)?),
                    None => None,
                };
                columns.push(aggregate.label());
                plan.push((Some(aggregate.func), index));
            }
        }
    }
let aggregate_order = |name| columns.iter().zip(plan.iter()).position(|(label, (func, _))| func.is_some() && label == name);
    let order = match &order_by {
Some((name, descending)) if aggregate_order(name).is_some() => aggregate_order(name).map(|output| (output, *descending)),
        Some((name, descending)) => {
            let position = column_position(schema, name)?;
            if !keys.contains(&position) {
                return fail("ORDER BY with GROUP BY must name a grouping column");
            }
            match plan
                .iter()
                .position(|(func, index)| func.is_none() && *index == Some(position))
            {
                Some(output) => Some((output, *descending)),
                None => return fail("ORDER BY column must be in the select list"),
            }
        }
        None => None,
    };
    let mut rows = matching_rows_limited(db, schema, &filter, Some(materialize))?;
    rows.sort_by(|a, b| compare_keys(&keys, a, b));
    let mut output: Vec<Vec<crate::value::Value>> = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        let mut end = start + 1;
        while end < rows.len()
            && compare_keys(&keys, &rows[start], &rows[end]) == std::cmp::Ordering::Equal
        {
            end += 1;
        }
        let group = &rows[start..end];
        let mut line = Vec::new();
        for (func, index) in &plan {
            match func {
                Some(func) => match aggregate::evaluate(*func, *index, group) {
                    Ok(value) => line.push(value),
                    Err(message) => return fail(message),
                },
                None => line.push(
                    index
                        .map(|i| group[0][i].clone())
                        .unwrap_or(crate::value::Value::Null),
                ),
            }
        }
        output.push(line);
        start = end;
    }
    for (label, op, value) in having.iter().flatten() {
        let index = match aggregate_order(label) {
            Some(index) => index,
            None => return fail("HAVING aggregate must be in the select list"),
        };
        let mut kept = Vec::new();
        for line in output {
            if binary(op.clone(), line[index].clone(), value.clone())? == Value::Bool(true) {
                kept.push(line);
            }
        }
        output = kept;
    }
    if let Some((position, descending)) = order {
        output.sort_by(|a, b| {
            let ordering = a[position].compare(&b[position]);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
    output.drain(..offset.min(output.len()));
    let truncated = cap_rows(&mut output, limit);
    Ok(QueryResult { columns, rows: output, affected: 0, truncated })
}

fn update(db: &Database, table: &str, sets: Vec<(String, Expr)>, filter: Option<Expr>) -> Result<QueryResult> { update_limited(db, table, sets, filter, (MAX_MATERIALIZED_ROWS, MAX_MATERIALIZED_BYTES)) }
pub fn update_limited(db: &Database, table: &str, sets: Vec<(String, Expr)>, filter: Option<Expr>, limit: (usize, usize)) -> Result<QueryResult> {
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
    for row in matching_rows_limited(db, &schema, &filter, Some(limit))? {
        let mut new_row = row.clone();
        for (index, expr) in &assignments {
            new_row[*index] = eval(expr, Some((&schema, &row)))?;
        }
        changes.push((row[schema.primary_key].clone(), new_row));
    }
    db.update_many(table, &changes)?;
    Ok(QueryResult::affected(changes.len()))
}

fn delete(db: &Database, table: &str, filter: Option<Expr>) -> Result<QueryResult> { delete_limited(db, table, filter, (MAX_MATERIALIZED_ROWS, MAX_MATERIALIZED_BYTES)) }
pub fn delete_limited(db: &Database, table: &str, filter: Option<Expr>, limit: (usize, usize)) -> Result<QueryResult> {
    let schema = db.schema(table)?;
    let keys: Vec<Value> = matching_rows_limited(db, &schema, &filter, Some(limit))?
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

fn like_match(text: &[char], pattern: &[char]) -> bool {
    let mut t = 0;
    let mut p = 0;
    let mut star: Option<usize> = None;
    let mut mark = 0;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == '%' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if p < pattern.len() && (pattern[p] == '_' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '%' {
        p += 1;
    }
    p == pattern.len()
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
        BinOp::Like => match (&l, &r) {
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
            (Value::Text(text), Value::Text(pattern)) => {
                let text: Vec<char> = text.chars().collect();
                let pattern: Vec<char> = pattern.chars().collect();
                Ok(Value::Bool(like_match(&text, &pattern)))
            }
            _ => fail("LIKE needs text on both sides"),
        },
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
use crate::parser::{parse_script, ScriptItem};

pub enum Outcome {
    Single(QueryResult),
    Transaction { committed: bool, results: Vec<QueryResult> },
}

pub fn execute_script(db: &Database, sql: &str) -> Result<Outcome> {
    let mut items = parse_script(sql)?;
    if items.is_empty() {
        return fail("empty statement");
    }
    if !matches!(items[0], ScriptItem::Begin) {
        if items.len() > 1 {
            return fail("multiple statements require BEGIN and COMMIT");
        }
        return match items.remove(0) {
            ScriptItem::Statement(statement) => run(db, statement).map(Outcome::Single),
            _ => fail("COMMIT and ROLLBACK need BEGIN first"),
        };
    }
    let committed = match items.pop() {
        Some(ScriptItem::Commit) => true,
        Some(ScriptItem::Rollback) => false,
        _ => return fail("transaction must end with COMMIT or ROLLBACK"),
    };
    items.remove(0);
    let mut statements = Vec::new();
    for item in items {
        match item {
            ScriptItem::Statement(statement) => statements.push(statement),
            _ => return fail("nested or misplaced transaction control is not supported"),
        }
    }
    let results = db.transaction(committed, |view| {
        statements.into_iter().map(|s| run(view, s)).collect::<Result<Vec<_>>>()
    })?;
    Ok(Outcome::Transaction { committed, results })
}

pub const MAX_RESULT_ROWS: usize = 10000;

fn cap_rows<T>(rows: &mut Vec<T>, limit: Option<usize>) -> bool {
    let wanted = limit.unwrap_or(usize::MAX);
    let truncated = rows.len() > MAX_RESULT_ROWS && wanted > MAX_RESULT_ROWS;
    rows.truncate(wanted.min(MAX_RESULT_ROWS));
    truncated
}

fn matching_rows_window(db: &Database, schema: &Schema, filter: &Option<Expr>, offset: usize, limit: Option<usize>) -> Result<Vec<Vec<Value>>> {
if let Some(expr) = filter {
check_columns(expr, schema)?;
}
let keep = limit.unwrap_or(usize::MAX).min(MAX_RESULT_ROWS).saturating_add(1);
let mut skipped = 0usize;
let mut matched = Vec::new();
db.scan_each(&schema.name, 64, |row, _| {
let hit = match filter {
Some(expr) => passes(expr, schema, &row)?,
None => true,
};
if hit {
if skipped < offset {
skipped += 1;
} else {
matched.push(row);
}
}
Ok(matched.len() < keep)
})?;
Ok(matched)
}

fn matching_rows_top(db: &Database, schema: &Schema, filter: &Option<Expr>, index: usize, descending: bool, keep: usize) -> Result<Vec<Vec<Value>>> {
if let Some(expr) = filter {
check_columns(expr, schema)?;
}
let trim_at = keep.saturating_mul(2).max(64);
let mut seq = 0usize;
let mut buffer: Vec<(usize, Vec<Value>)> = Vec::new();
db.scan_each(&schema.name, 64, |row, _| {
let hit = match filter {
Some(expr) => passes(expr, schema, &row)?,
None => true,
};
if hit {
buffer.push((seq, row));
seq += 1;
if buffer.len() >= trim_at {
trim_top(&mut buffer, index, descending, keep);
}
}
Ok(true)
})?;
trim_top(&mut buffer, index, descending, keep);
Ok(buffer.into_iter().map(|(_, row)| row).collect())
}

fn trim_top(buffer: &mut Vec<(usize, Vec<Value>)>, index: usize, descending: bool, keep: usize) {
buffer.sort_by(|a, b| {
let ordering = a.1[index].compare(&b.1[index]);
let ordering = if descending { ordering.reverse() } else { ordering };
ordering.then(a.0.cmp(&b.0))
});
buffer.truncate(keep);
}

fn distinct_select(db: &Database, schema: &Schema, names: &[String], filter: &Option<Expr>, order_by: &Option<(String, bool)>, limit: Option<usize>, offset: usize, materialize: (usize, usize)) -> Result<QueryResult> {
let mut indexes = Vec::new();
for name in names { indexes.push(column_position(schema, name)?); }
let order = match order_by { Some((name, descending)) => { let target = column_position(schema, name)?; match indexes.iter().position(|&i| i == target) { Some(p) => Some((p, *descending)), None => return fail("ORDER BY column must appear in the SELECT DISTINCT list") } } None => None };
let found = matching_rows_limited(db, schema, filter, Some(materialize))?;
let mut rows: Vec<Vec<crate::value::Value>> = found.into_iter().map(|row| indexes.iter().map(|&i| row[i].clone()).collect()).collect();
let keys: Vec<usize> = (0..indexes.len()).collect();
rows.sort_by(|a, b| compare_keys(&keys, a, b));
rows.dedup_by(|a, b| compare_keys(&keys, a, b) == std::cmp::Ordering::Equal);
if let Some((p, descending)) = order { rows.sort_by(|a, b| { let ordering = a[p].compare(&b[p]); if descending { ordering.reverse() } else { ordering } }); }
rows.drain(..offset.min(rows.len()));
let truncated = cap_rows(&mut rows, limit);
Ok(QueryResult { columns: names.to_vec(), rows, affected: 0, truncated })
}