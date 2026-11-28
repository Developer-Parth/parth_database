use crate::error::{Error, Result};
use crate::lexer::{tokenize, Token};
use crate::table::ColumnType;
use crate::aggregate::{AggFunc, Aggregate, SelectItem};
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Literal(Value),
    Column(String),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    IsNull(Box<Expr>, bool),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    pub ty: ColumnType,
    pub primary_key: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Projection {
    Aggregates(Vec<Aggregate>),
    Items(Vec<SelectItem>),
    All,
    Columns(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    CreateTable { name: String, columns: Vec<ColumnDef> },
    DropTable { name: String },
    Insert { table: String, columns: Option<Vec<String>>, rows: Vec<Vec<Expr>> },
    Select {
        table: String,
        projection: Projection,
        filter: Option<Expr>,
        group_by: Vec<String>,
        order_by: Option<(String, bool)>,
        limit: Option<usize>, offset: usize,
    },
    Update { table: String, sets: Vec<(String, Expr)>, filter: Option<Expr> },
    Delete { table: String, filter: Option<Expr> },
}

const RESERVED: &[&str] = &[
    "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "DROP",
    "TABLE", "PRIMARY", "KEY", "ORDER", "BY", "ASC", "DESC", "LIMIT", "AND", "OR", "NOT", "IS", "NULL",
    "TRUE", "FALSE",
];

const MAX_DEPTH: usize = 64;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
}

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::InvalidInput(message.into()))
}

pub fn parse(sql: &str) -> Result<Statement> {
    let mut parser = Parser { tokens: tokenize(sql)?, pos: 0, depth: 0 };
    let statement = parser.statement()?;
    parser.eat_symbol(";");
    if parser.pos != parser.tokens.len() {
        return fail("unexpected input after statement");
    }
    Ok(statement)
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_keyword(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(w)) if w.eq_ignore_ascii_case(word))
    }

    fn eat_keyword(&mut self, word: &str) -> bool {
        let found = self.peek_keyword(word);
        if found {
            self.pos += 1;
        }
        found
    }

    fn expect_keyword(&mut self, word: &str) -> Result<()> {
        if self.eat_keyword(word) {
            Ok(())
        } else {
            fail(format!("expected {word}"))
        }
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        let found = matches!(self.peek(), Some(Token::Symbol(s)) if *s == symbol);
        if found {
            self.pos += 1;
        }
        found
    }

    fn expect_symbol(&mut self, symbol: &str) -> Result<()> {
        if self.eat_symbol(symbol) {
            Ok(())
        } else {
            fail(format!("expected {symbol}"))
        }
    }

    fn identifier(&mut self) -> Result<String> {
        match self.peek() {
            Some(Token::Word(w)) if !RESERVED.iter().any(|r| w.eq_ignore_ascii_case(r)) => {
                let name = w.clone();
                self.pos += 1;
                Ok(name)
            }
            _ => fail("expected a name"),
        }
    }

    fn statement(&mut self) -> Result<Statement> {
        if self.eat_keyword("CREATE") {
            self.create_table()
        } else if self.eat_keyword("DROP") {
            self.expect_keyword("TABLE")?;
            Ok(Statement::DropTable { name: self.identifier()? })
        } else if self.eat_keyword("INSERT") {
            self.insert()
        } else if self.eat_keyword("SELECT") {
            self.select()
        } else if self.eat_keyword("UPDATE") {
            self.update()
        } else if self.eat_keyword("DELETE") {
            self.expect_keyword("FROM")?;
            let table = self.identifier()?;
            Ok(Statement::Delete { table, filter: self.where_clause()? })
        } else {
            fail("expected a statement")
        }
    }

    fn create_table(&mut self) -> Result<Statement> {
        self.expect_keyword("TABLE")?;
        let name = self.identifier()?;
        self.expect_symbol("(")?;
        let mut columns = Vec::new();
        loop {
            let column = self.identifier()?;
            let ty = self.column_type()?;
            let primary_key = if self.eat_keyword("PRIMARY") {
                self.expect_keyword("KEY")?;
                true
            } else {
                false
            };
            columns.push(ColumnDef { name: column, ty, primary_key });
            if !self.eat_symbol(",") {
                break;
            }
        }
        self.expect_symbol(")")?;
        if columns.iter().filter(|c| c.primary_key).count() != 1 {
            return fail("exactly one PRIMARY KEY column is required, for example CREATE TABLE t (id INT PRIMARY KEY, name TEXT)");
        }
        Ok(Statement::CreateTable { name, columns })
    }

    fn column_type(&mut self) -> Result<ColumnType> {
        let word = match self.peek() {
            Some(Token::Word(w)) => w.to_ascii_uppercase(),
            _ => return fail("expected a column type"),
        };
        let ty = match word.as_str() {
            "INT" | "INTEGER" => ColumnType::Int,
            "REAL" | "FLOAT" => ColumnType::Real,
            "TEXT" => ColumnType::Text,
            "BOOL" | "BOOLEAN" => ColumnType::Bool,
            _ => return fail(format!("unknown column type: {word}")),
        };
        self.pos += 1;
        Ok(ty)
    }

    fn aggregate_ahead(&mut self) -> bool {
        let start = self.pos;
        let named = match self.peek() {
            Some(Token::Word(w)) => AggFunc::parse(w).is_some(),
            _ => false,
        };
        let mut found = false;
        if named {
            self.pos += 1;
            found = self.eat_symbol("(");
        }
        self.pos = start;
        found
    }

    fn select_items(&mut self) -> Result<Projection> {
        let mut items = Vec::new();
        loop {
            if self.aggregate_ahead() {
                items.push(SelectItem::Agg(self.aggregate()?));
            } else {
                items.push(SelectItem::Column(self.identifier()?));
            }
            if !self.eat_symbol(",") {
                break;
            }
        }
        let aggregates = items
            .iter()
            .filter(|item| matches!(item, SelectItem::Agg(_)))
            .count();
        if aggregates == items.len() {
            let list = items
                .into_iter()
                .filter_map(|item| match item {
                    SelectItem::Agg(aggregate) => Some(aggregate),
                    SelectItem::Column(_) => None,
                })
                .collect();
            Ok(Projection::Aggregates(list))
        } else if aggregates == 0 {
            let names = items
                .into_iter()
                .filter_map(|item| match item {
                    SelectItem::Column(name) => Some(name),
                    SelectItem::Agg(_) => None,
                })
                .collect();
            Ok(Projection::Columns(names))
        } else {
            Ok(Projection::Items(items))
        }
    }    fn aggregate(&mut self) -> Result<Aggregate> {
        let name = self.identifier()?;
        let func = match AggFunc::parse(&name) {
            Some(func) => func,
            None => return fail(format!("unknown function {name}")),
        };
        self.expect_symbol("(")?;
        let column = if self.eat_symbol("*") {
            if func != AggFunc::Count {
                return fail("only COUNT accepts *");
            }
            None
        } else {
            Some(self.identifier()?)
        };
        self.expect_symbol(")")?;
        Ok(Aggregate { func, column })
    }

    fn name_list(&mut self) -> Result<Vec<String>> {
        let mut names = vec![self.identifier()?];
        while self.eat_symbol(",") {
            names.push(self.identifier()?);
        }
        Ok(names)
    }

    fn insert(&mut self) -> Result<Statement> {
        self.expect_keyword("INTO")?;
        let table = self.identifier()?;
        let columns = if self.eat_symbol("(") {
            let names = self.name_list()?;
            self.expect_symbol(")")?;
            Some(names)
        } else {
            None
        };
        self.expect_keyword("VALUES")?;
        let mut rows = Vec::new();
        loop {
            self.expect_symbol("(")?;
            let mut row = vec![self.expr()?];
            while self.eat_symbol(",") {
                row.push(self.expr()?);
            }
            self.expect_symbol(")")?;
            rows.push(row);
            if !self.eat_symbol(",") {
                break;
            }
        }
        Ok(Statement::Insert { table, columns, rows })
    }

    fn select(&mut self) -> Result<Statement> {
if self.eat_keyword("DISTINCT") { return fail("SELECT DISTINCT is not supported"); }
        let projection = if self.eat_symbol("*") {
            Projection::All
        } else {
            self.select_items()?
        };        self.expect_keyword("FROM")?;
        let table = self.identifier()?;
        let filter = self.where_clause()?;
        let group_by = if self.eat_keyword("GROUP") {
            self.expect_keyword("BY")?;
            self.name_list()?
        } else {
            Vec::new()
        };
        if self.eat_keyword("HAVING") {
            return fail("HAVING is not supported");
        }
        let order_by = if self.eat_keyword("ORDER") {
            self.expect_keyword("BY")?;
            let column = self.identifier()?;
            let descending = if self.eat_keyword("DESC") {
                true
            } else {
                self.eat_keyword("ASC");
                false
            };
            Some((column, descending))
        } else {
            None
        };
        let limit = if self.eat_keyword("LIMIT") {
            match self.peek() {
                Some(Token::Int(n)) if *n >= 0 => {
                    let n = *n as usize;
                    self.pos += 1;
                    Some(n)
                }
                _ => return fail("LIMIT needs a non-negative integer"),
            }
        } else {
            None
        };
        let offset = if self.eat_keyword("OFFSET") {
            match self.peek() {
                Some(Token::Int(n)) if *n >= 0 => {
                    let n = *n as usize;
                    self.pos += 1;
                    n
                }
                _ => return fail("OFFSET needs a non-negative integer"),
            }
        } else {
            0
        };
        Ok(Statement::Select { table, projection, filter, group_by, order_by, limit, offset })
    }

    fn update(&mut self) -> Result<Statement> {
        let table = self.identifier()?;
        self.expect_keyword("SET")?;
        let mut sets = Vec::new();
        loop {
            let column = self.identifier()?;
            self.expect_symbol("=")?;
            sets.push((column, self.expr()?));
            if !self.eat_symbol(",") {
                break;
            }
        }
        Ok(Statement::Update { table, sets, filter: self.where_clause()? })
    }

    fn where_clause(&mut self) -> Result<Option<Expr>> {
        if self.eat_keyword("WHERE") {
            Ok(Some(self.expr()?))
        } else {
            Ok(None)
        }
    }

    fn expr(&mut self) -> Result<Expr> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return fail("expression is nested too deeply");
        }
        let result = self.or_expr();
        self.depth -= 1;
        result
    }

    fn or_expr(&mut self) -> Result<Expr> {
        let mut left = self.and_expr()?;
        while self.eat_keyword("OR") {
            left = Expr::Binary(BinOp::Or, Box::new(left), Box::new(self.and_expr()?));
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        let mut left = self.not_expr()?;
        while self.eat_keyword("AND") {
            left = Expr::Binary(BinOp::And, Box::new(left), Box::new(self.not_expr()?));
        }
        Ok(left)
    }

    fn not_expr(&mut self) -> Result<Expr> {
        if self.eat_keyword("NOT") {
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return fail("expression is nested too deeply");
            }
            let inner = self.not_expr();
            self.depth -= 1;
            return Ok(Expr::Not(Box::new(inner?)));
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Expr> {
        let left = self.additive()?;
        if self.eat_keyword("IS") {
            let negated = self.eat_keyword("NOT");
            self.expect_keyword("NULL")?;
            return Ok(Expr::IsNull(Box::new(left), negated));
        }
        let op = match self.peek() {
            Some(Token::Symbol("=")) => BinOp::Eq,
            Some(Token::Symbol("!=")) | Some(Token::Symbol("<>")) => BinOp::Ne,
            Some(Token::Symbol("<")) => BinOp::Lt,
            Some(Token::Symbol("<=")) => BinOp::Le,
            Some(Token::Symbol(">")) => BinOp::Gt,
            Some(Token::Symbol(">=")) => BinOp::Ge,
            _ => return Ok(left),
        };
        self.pos += 1;
        Ok(Expr::Binary(op, Box::new(left), Box::new(self.additive()?)))
    }

    fn additive(&mut self) -> Result<Expr> {
        let mut left = self.multiplicative()?;
        loop {
            let op = if self.eat_symbol("+") {
                BinOp::Add
            } else if self.eat_symbol("-") {
                BinOp::Sub
            } else {
                return Ok(left);
            };
            left = Expr::Binary(op, Box::new(left), Box::new(self.multiplicative()?));
        }
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = if self.eat_symbol("*") {
                BinOp::Mul
            } else if self.eat_symbol("/") {
                BinOp::Div
            } else {
                return Ok(left);
            };
            left = Expr::Binary(op, Box::new(left), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr> {
        if self.eat_symbol("-") {
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return fail("expression is nested too deeply");
            }
            let inner = self.unary();
            self.depth -= 1;
            return Ok(Expr::Neg(Box::new(inner?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr> {
        if self.eat_symbol("(") {
            let inner = self.expr()?;
            self.expect_symbol(")")?;
            return Ok(inner);
        }
        let literal = match self.peek() {
            Some(Token::Int(n)) => Some(Value::Int(*n)),
            Some(Token::Real(r)) => Some(Value::Real(*r)),
            Some(Token::Text(s)) => Some(Value::Text(s.clone())),
            Some(Token::Word(w)) if w.eq_ignore_ascii_case("NULL") => Some(Value::Null),
            Some(Token::Word(w)) if w.eq_ignore_ascii_case("TRUE") => Some(Value::Bool(true)),
            Some(Token::Word(w)) if w.eq_ignore_ascii_case("FALSE") => Some(Value::Bool(false)),
            _ => None,
        };
        if let Some(value) = literal {
            self.pos += 1;
            return Ok(Expr::Literal(value));
        }
        Ok(Expr::Column(self.identifier()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str) -> Box<Expr> {
        Box::new(Expr::Column(name.into()))
    }

    fn int(n: i64) -> Box<Expr> {
        Box::new(Expr::Literal(Value::Int(n)))
    }

    #[test]
    fn parses_create_table() {
        let statement = parse("create table t (id int primary key, name TEXT, ok bool);").unwrap();
        assert_eq!(
            statement,
            Statement::CreateTable {
                name: "t".into(),
                columns: vec![
                    ColumnDef { name: "id".into(), ty: ColumnType::Int, primary_key: true },
                    ColumnDef { name: "name".into(), ty: ColumnType::Text, primary_key: false },
                    ColumnDef { name: "ok".into(), ty: ColumnType::Bool, primary_key: false },
                ],
            }
        );
    }

    #[test]
    fn parses_insert_with_columns_and_many_rows() {
        let statement = parse("INSERT INTO t (id, name) VALUES (1, 'a'), (2, 'b')").unwrap();
        match statement {
            Statement::Insert { table, columns, rows } => {
                assert_eq!(table, "t");
                assert_eq!(columns, Some(vec!["id".to_string(), "name".to_string()]));
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[1][1], Expr::Literal(Value::Text("b".into())));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_select_with_everything() {
        let statement = parse("SELECT name FROM t WHERE id >= 2 ORDER BY name DESC LIMIT 5").unwrap();
        assert_eq!(
            statement,
            Statement::Select {
                table: "t".into(),
                projection: Projection::Columns(vec!["name".into()]),
                filter: Some(Expr::Binary(BinOp::Ge, col("id"), int(2))),
            group_by: Vec::new(),
            order_by: Some(("name".into(), true)),
                limit: Some(5), offset: 0,
            }
        );
    }

    #[test]
    fn operator_precedence() {
        let statement = parse("DELETE FROM t WHERE a = 1 OR b = 2 AND NOT c IS NULL").unwrap();
        let filter = match statement {
            Statement::Delete { filter: Some(f), .. } => f,
            other => panic!("unexpected {other:?}"),
        };
        match filter {
            Expr::Binary(BinOp::Or, _, right) => match *right {
                Expr::Binary(BinOp::And, _, not) => {
                    assert_eq!(*not, Expr::Not(Box::new(Expr::IsNull(col("c"), false))));
                }
                other => panic!("unexpected {other:?}"),
            },
            other => panic!("unexpected {other:?}"),
        }
        let arithmetic = parse("UPDATE t SET a = 1 + 2 * 3 WHERE b = -4").unwrap();
        match arithmetic {
            Statement::Update { sets, filter, .. } => {
                let two_times_three = Expr::Binary(BinOp::Mul, int(2), int(3));
                assert_eq!(sets[0].1, Expr::Binary(BinOp::Add, int(1), Box::new(two_times_three)));
                assert_eq!(filter, Some(Expr::Binary(BinOp::Eq, col("b"), Box::new(Expr::Neg(int(4))))));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn rejects_malformed_statements() {
        let bad = [
            "",
            "SELECT",
            "SELECT * t",
            "SELECT * FROM",
            "SELECT * FROM t WHERE",
            "SELECT * FROM t LIMIT -1",
            "SELECT * FROM t extra",
            "INSERT INTO t VALUES",
            "INSERT INTO t VALUES (1,)",
            "CREATE TABLE t (a INT)",
            "CREATE TABLE t (a INT PRIMARY KEY, b INT PRIMARY KEY)",
            "CREATE TABLE t (a BLOB PRIMARY KEY)",
            "CREATE TABLE t ()",
            "UPDATE t SET WHERE a = 1",
            "DELETE t",
            "DROP t",
            "SELECT * FROM select",
            "SELECT * FROM t WHERE (a = 1",
        ];
        for sql in bad {
            assert!(parse(sql).is_err(), "should fail: {sql}");
        }
    }

    #[test]
    fn deep_nesting_is_rejected_without_overflow() {
        let sql = format!("SELECT * FROM t WHERE {}1{}", "(".repeat(5000), ")".repeat(5000));
        assert!(parse(&sql).is_err());
        let nots = format!("SELECT * FROM t WHERE {}a", "NOT ".repeat(5000));
        assert!(parse(&nots).is_err());
        let negs = format!("SELECT * FROM t WHERE {}1", "-".repeat(5000));
        assert!(parse(&negs).is_err());
    }
}
pub enum ScriptItem {
    Begin,
    Commit,
    Rollback,
    Statement(Statement),
}

pub fn parse_script(sql: &str) -> Result<Vec<ScriptItem>> {
    let mut parser = Parser { tokens: tokenize(sql)?, pos: 0, depth: 0 };
    let mut items = Vec::new();
    loop {
        while parser.eat_symbol(";") {}
        if parser.peek().is_none() {
            break;
        }
        let item = if parser.eat_keyword("BEGIN") {
            parser.eat_keyword("TRANSACTION");
            ScriptItem::Begin
        } else if parser.eat_keyword("COMMIT") {
            ScriptItem::Commit
        } else if parser.eat_keyword("ROLLBACK") {
            ScriptItem::Rollback
        } else {
            ScriptItem::Statement(parser.statement()?)
        };
        items.push(item);
        if parser.peek().is_some() && !parser.eat_symbol(";") {
            return fail("unexpected input after statement");
        }
    }
    Ok(items)
}
