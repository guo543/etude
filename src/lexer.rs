use std::fmt;
use std::str::CharIndices;

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenKind {
    Int,
    Ident,
    Let,
    Def,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    Comma,
    SemiColon,
    LParen,
    RParen,
    LBrace,
    RBrace,
    /// A character that cannot start any token. The lexer never fails;
    /// the parser reports these when it reaches them.
    Unknown,
    /// End of input. `Lexer::next_token` keeps returning it once the input
    /// runs out.
    Eof,
}

/// How the token is named in error messages, e.g. "expected `;`, found
/// end of input". Use `{:?}` for the variant name when debugging.
impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let text = match self {
            TokenKind::Int => "an integer",
            TokenKind::Ident => "an identifier",
            TokenKind::Let => "`let`",
            TokenKind::Def => "`def`",
            TokenKind::Eq => "`=`",
            TokenKind::Plus => "`+`",
            TokenKind::Minus => "`-`",
            TokenKind::Star => "`*`",
            TokenKind::Slash => "`/`",
            TokenKind::Comma => "`,`",
            TokenKind::SemiColon => "`;`",
            TokenKind::LParen => "`(`",
            TokenKind::RParen => "`)`",
            TokenKind::LBrace => "`{`",
            TokenKind::RBrace => "`}`",
            TokenKind::Unknown => "an unknown character",
            TokenKind::Eof => "end of input",
        };
        f.write_str(text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, start: usize, end: usize) -> Self {
        Token {
            kind,
            span: Span { start, end },
        }
    }
}

/// Hands out tokens one at a time. Like rustc's lexer cursor, it holds a plain
/// `CharIndices` and peeks by cloning it, which is cheap (two pointers) and
/// keeps `as_str()` and `offset()` available.
pub struct Lexer<'src> {
    src: &'src str,
    input: CharIndices<'src>,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Lexer {
            src,
            input: src.char_indices(),
        }
    }

    pub fn src(&self) -> &'src str {
        self.src
    }

    /// Byte offset of the next character.
    fn pos(&self) -> usize {
        self.input.offset()
    }

    /// The next character, without consuming it.
    fn peek(&self) -> Option<char> {
        self.input.clone().next().map(|(_, c)| c)
    }

    fn bump(&mut self) -> Option<char> {
        self.input.next().map(|(_, c)| c)
    }

    fn eat_while(&mut self, pred: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&pred) {
            self.bump();
        }
    }

    /// The next token, or `Eof` (with an empty span at the end of the
    /// input) once there are none left. Keeps returning `Eof` after that.
    /// Never fails: a character that cannot start a token comes back as
    /// `Unknown`. Integers are not range-checked here; the parser does that.
    pub fn next_token(&mut self) -> Token {
        self.eat_while(|c| c.is_ascii_whitespace());

        let start = self.pos();
        let Some(c) = self.bump() else {
            return Token::new(TokenKind::Eof, start, start);
        };

        let kind = match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '=' => TokenKind::Eq,
            ',' => TokenKind::Comma,
            ';' => TokenKind::SemiColon,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            c if c.is_ascii_alphabetic() => {
                self.eat_while(|c| c.is_ascii_alphanumeric() || c == '_');
                match &self.src[start..self.pos()] {
                    "let" => TokenKind::Let,
                    "def" => TokenKind::Def,
                    _ => TokenKind::Ident,
                }
            }
            c if c.is_ascii_digit() => {
                self.eat_while(|c| c.is_ascii_alphanumeric());
                TokenKind::Int
            }
            _ => TokenKind::Unknown,
        };

        Token::new(kind, start, self.pos())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use TokenKind::*;

    /// Every token up to, but not including, `Eof`.
    fn lex(src: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(src);
        let mut tokens = Vec::new();
        loop {
            match lexer.next_token() {
                Token { kind: Eof, .. } => return tokens,
                tok => tokens.push(tok),
            }
        }
    }

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src).into_iter().map(|t| t.kind).collect()
    }

    /// Each token's kind together with the source text its span covers.
    fn tokens(src: &str) -> Vec<(TokenKind, &str)> {
        lex(src)
            .into_iter()
            .map(|t| (t.kind, &src[t.span.start..t.span.end]))
            .collect()
    }

    fn spans(src: &str) -> Vec<(usize, usize)> {
        lex(src)
            .into_iter()
            .map(|t| (t.span.start, t.span.end))
            .collect()
    }

    #[test]
    fn single_tokens() {
        let cases = [
            ("+", Plus),
            ("-", Minus),
            ("*", Star),
            ("/", Slash),
            ("=", Eq),
            (",", Comma),
            (";", SemiColon),
            ("(", LParen),
            (")", RParen),
            ("{", LBrace),
            ("}", RBrace),
            ("let", Let),
            ("def", Def),
            ("42", Int),
            ("x", Ident),
        ];
        for (src, expected) in cases {
            assert_eq!(tokens(src), vec![(expected, src)], "input: {src:?}");
        }
    }

    #[test]
    fn empty_and_whitespace_only() {
        assert_eq!(kinds(""), vec![]);
        assert_eq!(kinds(" "), vec![]);
        assert_eq!(kinds(" \t\r\n  \n"), vec![]);
    }

    #[test]
    fn whitespace_between_tokens() {
        assert_eq!(
            tokens("1\t+\n2\r\n"),
            vec![(Int, "1"), (Plus, "+"), (Int, "2")]
        );
        assert_eq!(tokens("  x  "), vec![(Ident, "x")]);
    }

    #[test]
    fn adjacent_tokens() {
        assert_eq!(tokens("1+2"), vec![(Int, "1"), (Plus, "+"), (Int, "2")]);
        assert_eq!(
            tokens("x=(y);"),
            vec![
                (Ident, "x"),
                (Eq, "="),
                (LParen, "("),
                (Ident, "y"),
                (RParen, ")"),
                (SemiColon, ";")
            ]
        );
        assert_eq!(kinds("((1))"), vec![LParen, LParen, Int, RParen, RParen]);
        assert_eq!(tokens("-5"), vec![(Minus, "-"), (Int, "5")]);
        assert_eq!(kinds("2*-3/x"), vec![Int, Star, Minus, Int, Slash, Ident]);
    }

    #[test]
    fn let_statement() {
        assert_eq!(
            tokens("let x = 1 + 2;"),
            vec![
                (Let, "let"),
                (Ident, "x"),
                (Eq, "="),
                (Int, "1"),
                (Plus, "+"),
                (Int, "2"),
                (SemiColon, ";")
            ]
        );
    }

    #[test]
    fn keyword_only_matches_whole_word() {
        assert_eq!(tokens("letter"), vec![(Ident, "letter")]);
        assert_eq!(tokens("let1"), vec![(Ident, "let1")]);
        assert_eq!(tokens("le"), vec![(Ident, "le")]);
        assert_eq!(tokens("Let"), vec![(Ident, "Let")]);
        assert_eq!(kinds("let let"), vec![Let, Let]);
        assert_eq!(kinds("let(x)"), vec![Let, LParen, Ident, RParen]);
        assert_eq!(tokens("define"), vec![(Ident, "define")]);
        assert_eq!(tokens("def_"), vec![(Ident, "def_")]);
        assert_eq!(kinds("def f()"), vec![Def, Ident, LParen, RParen]);
    }

    #[test]
    fn identifiers() {
        assert_eq!(tokens("x1"), vec![(Ident, "x1")]);
        assert_eq!(tokens("abc123def"), vec![(Ident, "abc123def")]);
        assert_eq!(tokens("foo bar"), vec![(Ident, "foo"), (Ident, "bar")]);
    }

    #[test]
    fn identifiers_are_ascii_only() {
        assert_eq!(tokens("é"), vec![(Unknown, "é")]);
        assert_eq!(tokens("日本"), vec![(Unknown, "日"), (Unknown, "本")]);
        assert_eq!(
            tokens("aéb"),
            vec![(Ident, "a"), (Unknown, "é"), (Ident, "b")]
        );
    }

    #[test]
    fn underscores_in_identifiers() {
        assert_eq!(tokens("a_b"), vec![(Ident, "a_b")]);
        assert_eq!(tokens("x_"), vec![(Ident, "x_")]);
        assert_eq!(tokens("a__1"), vec![(Ident, "a__1")]);
        // An identifier cannot start with `_`.
        assert_eq!(tokens("_a"), vec![(Unknown, "_"), (Ident, "a")]);
    }

    #[test]
    fn number_followed_by_letters_is_one_token() {
        // The parser rejects these as invalid integer literals.
        assert_eq!(tokens("123abc"), vec![(Int, "123abc")]);
        assert_eq!(tokens("2let"), vec![(Int, "2let")]);
        assert_eq!(
            tokens("1x2 + 3"),
            vec![(Int, "1x2"), (Plus, "+"), (Int, "3")]
        );
        // `_` is not part of a number.
        assert_eq!(tokens("1_0"), vec![(Int, "1"), (Unknown, "_"), (Int, "0")]);
    }

    #[test]
    fn integers() {
        assert_eq!(tokens("0"), vec![(Int, "0")]);
        assert_eq!(tokens("007"), vec![(Int, "007")]);
        assert_eq!(kinds("1 2 3"), vec![Int, Int, Int]);
    }

    #[test]
    fn integers_are_not_range_checked() {
        // Overflow is the parser's job; the lexer only finds the digits.
        let src = "1 + 99999999999999999999";
        assert_eq!(lex(src)[2], Token::new(Int, 4, 24));
    }

    #[test]
    fn unknown_char() {
        assert_eq!(lex("1 $ 2")[1], Token::new(Unknown, 2, 3));
    }

    #[test]
    fn lexing_continues_after_unknown_char() {
        assert_eq!(kinds("1 $ 2"), vec![Int, Unknown, Int]);
        assert_eq!(kinds("$$"), vec![Unknown, Unknown]);
    }

    #[test]
    fn unknown_chars() {
        for c in ['$', '_', '!', '.', '٣', '€', 'é'] {
            let src = c.to_string();
            assert_eq!(tokens(&src), vec![(Unknown, src.as_str())], "input: {c:?}");
        }
    }

    #[test]
    fn unknown_multibyte_char_span() {
        assert_eq!(lex("1€")[1], Token::new(Unknown, 1, 4));
    }

    #[test]
    fn token_spans() {
        assert_eq!(spans("let x = 42"), vec![(0, 3), (4, 5), (6, 7), (8, 10)]);
        assert_eq!(spans("(1)"), vec![(0, 1), (1, 2), (2, 3)]);
        assert_eq!(spans("  foo  "), vec![(2, 5)]);
    }

    #[test]
    fn spans_are_byte_offsets() {
        // 'é' is two bytes in UTF-8, each CJK character three.
        assert_eq!(spans("é + 1"), vec![(0, 2), (3, 4), (5, 6)]);
        assert_eq!(spans("日本 = 1"), vec![(0, 3), (3, 6), (7, 8), (9, 10)]);
    }

    #[test]
    fn non_ascii_whitespace_is_not_skipped() {
        // U+00A0 NO-BREAK SPACE is two bytes.
        assert_eq!(
            tokens("1\u{a0}2"),
            vec![(Int, "1"), (Unknown, "\u{a0}"), (Int, "2")]
        );
    }

    #[test]
    fn next_token_returns_eof_at_end() {
        let mut lexer = Lexer::new("1  ");
        assert_eq!(lexer.next_token().kind, Int);
        for _ in 0..2 {
            assert_eq!(lexer.next_token(), Token::new(Eof, 3, 3));
        }
        assert_eq!(Lexer::new("").next_token(), Token::new(Eof, 0, 0));
    }

    #[test]
    fn token_kind_display() {
        let cases = [
            (Int, "an integer"),
            (Ident, "an identifier"),
            (Let, "`let`"),
            (Def, "`def`"),
            (Eq, "`=`"),
            (Plus, "`+`"),
            (Minus, "`-`"),
            (Star, "`*`"),
            (Slash, "`/`"),
            (Comma, "`,`"),
            (SemiColon, "`;`"),
            (LParen, "`(`"),
            (RParen, "`)`"),
            (LBrace, "`{`"),
            (RBrace, "`}`"),
            (Unknown, "an unknown character"),
            (Eof, "end of input"),
        ];
        for (kind, expected) in cases {
            assert_eq!(kind.to_string(), expected);
        }
    }

    /// Exhaustively lex every string up to length 4 over a small alphabet and
    /// check invariants that must hold for any input.
    #[test]
    fn invariants_on_all_short_inputs() {
        const ALPHABET: [char; 11] = ['a', 'l', 'e', 't', '_', '1', '+', ';', ' ', 'é', '$'];

        let mut inputs = vec![String::new()];
        let mut frontier = vec![String::new()];
        for _ in 0..4 {
            frontier = frontier
                .iter()
                .flat_map(|s| ALPHABET.iter().map(move |c| format!("{s}{c}")))
                .collect();
            inputs.extend(frontier.iter().cloned());
        }

        for src in &inputs {
            let mut prev_end = 0;
            for tok in lex(src) {
                let span = tok.span;
                assert!(span.start < span.end, "empty span in {src:?}");
                assert!(span.start >= prev_end, "overlapping spans in {src:?}");
                assert!(span.end <= src.len(), "span out of bounds in {src:?}");
                prev_end = span.end;

                // Panics if the span is not on char boundaries.
                let text = &src[span.start..span.end];
                assert!(!text.trim().is_empty(), "whitespace-only token in {src:?}");

                match tok.kind {
                    Int => {
                        assert!(text.starts_with(|c: char| c.is_ascii_digit()));
                        assert!(text.bytes().all(|b| b.is_ascii_alphanumeric()), "{text:?}");
                    }
                    Ident => {
                        assert!(text.starts_with(|c: char| c.is_ascii_alphabetic()));
                        assert!(text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
                        assert!(text != "let" && text != "def", "{text:?}");
                    }
                    Let => assert_eq!(text, "let"),
                    Def => assert_eq!(text, "def"),
                    _ => assert_eq!(text.chars().count(), 1, "{text:?}"),
                }
            }
        }
    }
}
