use crate::error::{Error, Result};
use crate::lexer::{tokenize, Token};
use crate::table::ColumnType;
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
        order_by: Option<(String, bool)>,
        limit: Option<usize>,
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
            return fail("exactly one PRIMARY KEY column is required");
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
        let projection = if self.eat_symbol("*") {
            Projection::All
        } else {
            Projection::Columns(self.name_list()?)
        };
        self.expect_keyword("FROM")?;
        let table = self.identifier()?;
        let filter = self.where_clause()?;
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
        Ok(Statement::Select { table, projection, filter, order_by, limit })
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
                order_by: Some(("name".into(), true)),
                limit: Some(5),
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