use crate::ast::{AstNode, Op};
use crate::lexer::{Lexer, LexerError, Token};
use std::fmt;

#[derive(Debug)]
pub struct ParserError(pub String);

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<LexerError> for ParserError {
    fn from(LexerError(msg): LexerError) -> Self {
        ParserError(msg)
    }
}

pub struct Parser {
    lexer: Lexer,
}

impl Parser {
    pub fn new(lexer: Lexer) -> Self {
        Parser { lexer }
    }

    fn expect(&mut self, token: Token) -> Result<(), ParserError> {
        if token == *self.lexer.peek() {
            self.lexer.next()?;
            Ok(())
        } else {
            let peek = self.lexer.peek();
            Err(ParserError(format!("Expected {token} but got {peek}")))
        }
    }

    // fn expect_int(&mut self) -> Result<i32, ParserError> {
    //     if let Token::Int(n) = *self.lexer.peek() {
    //         self.lexer.next()?;
    //         Ok(n)
    //     } else {
    //         let peek = self.lexer.peek();
    //         Err(ParserError(format!("Expected <int> but got {peek}")))
    //     }
    // }

    fn parse_atom(&mut self) -> Result<AstNode, ParserError> {
        match *self.lexer.peek() {
            Token::Int(n) => {
                self.lexer.next()?;
                Ok(AstNode::Int(n))
            }
            Token::LParen => {
                self.lexer.next()?;
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            _ => Err(ParserError(String::from("Atom expected"))),
        }
    }

    fn parse_binary(&mut self, min: u32) -> Result<AstNode, ParserError> {
        let mut lhs = self.parse_atom()?;
        while let Some(op) = Op::from_token(self.lexer.peek())
            && op.precedence() >= min
        {
            self.lexer.next()?;
            let rhs = self.parse_binary(op.precedence() + 1)?;

            lhs = AstNode::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_expr(&mut self) -> Result<AstNode, ParserError> {
        self.parse_binary(0)
    }

    pub fn parse(&mut self) -> Result<AstNode, ParserError> {
        let expr = self.parse_expr()?;
        self.expect(Token::Eof)?;
        Ok(expr)
    }
}
