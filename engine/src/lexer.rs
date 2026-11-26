use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Word(String),
    Int(i64),
    Real(f64),
    Text(String),
    Symbol(&'static str),
}

fn syntax(message: String) -> Error {
    Error::InvalidInput(message)
}

pub fn tokenize(input: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token::Word(chars[start..i].iter().collect()));
        } else if c.is_ascii_digit() {
            let start = i;
            let mut real = false;
            while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && !real)) {
                real |= chars[i] == '.';
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            if real {
                let n = text.parse::<f64>().map_err(|_| syntax(format!("bad number: {text}")))?;
                tokens.push(Token::Real(n));
            } else {
                let n = text.parse::<i64>().map_err(|_| syntax(format!("number out of range: {text}")))?;
                tokens.push(Token::Int(n));
            }
        } else if c == '\'' {
            i += 1;
            let mut text = String::new();
            loop {
                match chars.get(i) {
                    None => return Err(syntax("unterminated string".into())),
                    Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                        text.push('\'');
                        i += 2;
                    }
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(ch) => {
                        text.push(*ch);
                        i += 1;
                    }
                }
            }
            tokens.push(Token::Text(text));
        } else {
            let next = chars.get(i + 1).copied();
            let (symbol, width) = match (c, next) {
                ('<', Some('=')) => ("<=", 2),
                ('>', Some('=')) => (">=", 2),
                ('<', Some('>')) => ("<>", 2),
                ('!', Some('=')) => ("!=", 2),
                ('(', _) => ("(", 1),
                (')', _) => (")", 1),
                (',', _) => (",", 1),
                (';', _) => (";", 1),
                ('*', _) => ("*", 1),
                ('+', _) => ("+", 1),
                ('-', _) => ("-", 1),
                ('/', _) => ("/", 1),
                ('=', _) => ("=", 1),
                ('<', _) => ("<", 1),
                ('>', _) => (">", 1),
                _ => return Err(syntax(format!("unexpected character: {c}"))),
            };
            tokens.push(Token::Symbol(symbol));
            i += width;
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_numbers_and_symbols() {
        let tokens = tokenize("SELECT a, 12, 3.5 FROM t WHERE a<=2;").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Word("SELECT".into()),
                Token::Word("a".into()),
                Token::Symbol(","),
                Token::Int(12),
                Token::Symbol(","),
                Token::Real(3.5),
                Token::Word("FROM".into()),
                Token::Word("t".into()),
                Token::Word("WHERE".into()),
                Token::Word("a".into()),
                Token::Symbol("<="),
                Token::Int(2),
                Token::Symbol(";"),
            ]
        );
    }

    #[test]
    fn strings_support_escaped_quotes() {
        assert_eq!(tokenize("'it''s'").unwrap(), vec![Token::Text("it's".into())]);
        assert_eq!(tokenize("''").unwrap(), vec![Token::Text(String::new())]);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(tokenize("'open").is_err());
        assert!(tokenize("a @ b").is_err());
        assert!(tokenize("99999999999999999999").is_err());
        assert!(tokenize("1.2.3").is_err());
    }
}