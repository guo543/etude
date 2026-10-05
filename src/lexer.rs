use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Int(i32),
    Add,
    Sub,
    Mul,
    Div,
    Eof,
    LParen,
    RParen,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Int(n) => write!(f, "<{n}>"),
            Token::Add => write!(f, "<+>"),
            Token::Sub => write!(f, "<->"),
            Token::Mul => write!(f, "<*>"),
            Token::Div => write!(f, "</>"),
            Token::Eof => write!(f, "<eof>"),
            Token::LParen => write!(f, "<(>"),
            Token::RParen => write!(f, "<)>"),
        }
    }
}

#[derive(Debug)]
pub struct LexerError(pub String);

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for LexerError {}

#[derive(Debug)]
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    peek: Token,
}

impl Lexer {
    pub fn new(input: &str) -> Result<Self, LexerError> {
        let mut lexer = Lexer {
            input: input.chars().collect(),
            pos: 0,
            peek: Token::Eof,
        };
        lexer.next()?;
        Ok(lexer)
    }

    pub fn peek(&self) -> &Token {
        &self.peek
    }

    fn whitespace(&mut self) {
        while let Some(c) = self.input.get(self.pos)
            && (*c == ' ' || *c == '\n' || *c == '\r' || *c == '\t')
        {
            self.pos += 1;
        }
    }

    fn number(&mut self) -> Result<(), LexerError> {
        let mut num_str = String::new();
        while let Some(c) = self.input.get(self.pos)
            && c.is_ascii_digit()
        {
            num_str.push(*c);
            self.pos += 1;
        }
        match num_str.parse::<i32>() {
            Ok(n) => {
                self.peek = Token::Int(n);
                Ok(())
            }
            Err(_) => Err(LexerError(format!("Unable to parse to integer: {num_str}"))),
        }
    }

    pub fn next(&mut self) -> Result<(), LexerError> {
        self.whitespace();

        let Some(c) = self.input.get(self.pos).copied() else {
            self.peek = Token::Eof;
            return Ok(());
        };

        let token = match c {
            '+' => Token::Add,
            '-' => Token::Sub,
            '*' => Token::Mul,
            '/' => Token::Div,
            '(' => Token::LParen,
            ')' => Token::RParen,
            c if c.is_ascii_digit() => return self.number(),
            c => {
                return Err(LexerError(format!(
                    "Unknown character at {}: \"{c}\"",
                    self.pos
                )));
            }
        };
        self.pos += 1;
        self.peek = token;
        Ok(())
    }
}
