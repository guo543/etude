use std::fmt;
use std::num::IntErrorKind;
use std::str::CharIndices;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenKind<'a> {
    Int(i64),
    Ident(&'a str),
    Let,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    /// A character that cannot start any token. The lexer never fails;
    /// the parser reports these when it reaches them.
    Unknown(char),
    /// Digits that do not fit in an `i64`, with the reason from `str::parse`.
    InvalidInt(IntErrorKind),
    /// End of input. `Lexer::next_token` keeps returning it once the input
    /// runs out.
    Eof,
}

impl fmt::Display for TokenKind<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenKind::Int(n) => write!(f, "<int: {n}>"),
            TokenKind::Ident(id) => write!(f, "<id: {id}>"),
            TokenKind::Let => write!(f, "<let>"),
            TokenKind::Eq => write!(f, "<=>"),
            TokenKind::Plus => write!(f, "<+>"),
            TokenKind::Minus => write!(f, "<->"),
            TokenKind::Star => write!(f, "<*>"),
            TokenKind::Slash => write!(f, "</>"),
            TokenKind::LParen => write!(f, "<(>"),
            TokenKind::RParen => write!(f, "<)>"),
            TokenKind::Unknown(c) => write!(f, "<unknown: {c}>"),
            TokenKind::InvalidInt(_) => write!(f, "<invalid int>"),
            TokenKind::Eof => write!(f, "<eof>"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Token<'a> {
    pub kind: TokenKind<'a>,
    pub span: Span,
}

impl<'a> Token<'a> {
    pub fn new(kind: TokenKind<'a>, start: usize, end: usize) -> Self {
        Token {
            kind,
            span: Span { start, end },
        }
    }
}

/// Hands out tokens one at a time. Like rustc's lexer cursor, it holds a plain
/// `CharIndices` and peeks by cloning it, which is cheap (two pointers) and
/// keeps `as_str()` and `offset()` available.
pub struct Lexer<'a> {
    chars: CharIndices<'a>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer {
            chars: src.char_indices(),
        }
    }

    /// Byte offset of the next character.
    fn pos(&self) -> usize {
        self.chars.offset()
    }

    /// The next character, without consuming it.
    fn peek(&self) -> Option<char> {
        self.chars.clone().next().map(|(_, c)| c)
    }

    fn bump(&mut self) -> Option<char> {
        self.chars.next().map(|(_, c)| c)
    }

    fn eat_while(&mut self, pred: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&pred) {
            self.bump();
        }
    }

    /// The next token, or `Eof` (with an empty span at the end of the
    /// input) once there are none left. Keeps returning `Eof` after that.
    /// Never fails: bad input comes back as `Unknown` or `InvalidInt`.
    pub fn next_token(&mut self) -> Token<'a> {
        self.eat_while(char::is_whitespace);

        // Everything from here on; the token's text is a prefix of it.
        let rest = self.chars.as_str();
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
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            c if c.is_alphabetic() => {
                self.eat_while(char::is_alphanumeric);
                match &rest[..self.pos() - start] {
                    "let" => TokenKind::Let,
                    ident => TokenKind::Ident(ident),
                }
            }
            c if c.is_ascii_digit() => {
                self.eat_while(|c| c.is_ascii_digit());
                match rest[..self.pos() - start].parse() {
                    Ok(n) => TokenKind::Int(n),
                    Err(e) => TokenKind::InvalidInt(*e.kind()),
                }
            }
            c => TokenKind::Unknown(c),
        };

        Token::new(kind, start, self.pos())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use TokenKind::*;

    /// Every token up to, but not including, `Eof`.
    fn lex(src: &str) -> Vec<Token<'_>> {
        let mut lexer = Lexer::new(src);
        let mut tokens = Vec::new();
        loop {
            match lexer.next_token() {
                Token { kind: Eof, .. } => return tokens,
                tok => tokens.push(tok),
            }
        }
    }

    fn kinds(src: &str) -> Vec<TokenKind<'_>> {
        lex(src).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn single_tokens() {
        let cases = [
            ("+", Plus),
            ("-", Minus),
            ("*", Star),
            ("/", Slash),
            ("=", Eq),
            ("(", LParen),
            (")", RParen),
            ("let", Let),
            ("42", Int(42)),
            ("x", Ident("x")),
        ];
        for (src, expected) in cases {
            assert_eq!(kinds(src), vec![expected], "input: {src:?}");
        }
    }

    fn spans(src: &str) -> Vec<(usize, usize)> {
        lex(src)
            .into_iter()
            .map(|t| (t.span.start, t.span.end))
            .collect()
    }

    #[test]
    fn empty_and_whitespace_only() {
        assert_eq!(kinds(""), vec![]);
        assert_eq!(kinds(" "), vec![]);
        assert_eq!(kinds(" \t\r\n  \n"), vec![]);
    }

    #[test]
    fn whitespace_between_tokens() {
        assert_eq!(kinds("1\t+\n2\r\n"), vec![Int(1), Plus, Int(2)]);
        assert_eq!(kinds("  x  "), vec![Ident("x")]);
    }

    #[test]
    fn adjacent_tokens() {
        assert_eq!(kinds("1+2"), vec![Int(1), Plus, Int(2)]);
        assert_eq!(
            kinds("x=(y)"),
            vec![Ident("x"), Eq, LParen, Ident("y"), RParen]
        );
        assert_eq!(kinds("((1))"), vec![LParen, LParen, Int(1), RParen, RParen]);
        assert_eq!(kinds("-5"), vec![Minus, Int(5)]);
        assert_eq!(
            kinds("2*-3/x"),
            vec![Int(2), Star, Minus, Int(3), Slash, Ident("x")]
        );
    }

    #[test]
    fn let_statement() {
        assert_eq!(
            kinds("let x = 1 + 2"),
            vec![Let, Ident("x"), Eq, Int(1), Plus, Int(2)]
        );
    }

    #[test]
    fn keyword_only_matches_whole_word() {
        assert_eq!(kinds("letter"), vec![Ident("letter")]);
        assert_eq!(kinds("let1"), vec![Ident("let1")]);
        assert_eq!(kinds("le"), vec![Ident("le")]);
        assert_eq!(kinds("Let"), vec![Ident("Let")]);
        assert_eq!(kinds("let let"), vec![Let, Let]);
        assert_eq!(kinds("let(x)"), vec![Let, LParen, Ident("x"), RParen]);
    }

    #[test]
    fn identifiers() {
        assert_eq!(kinds("x1"), vec![Ident("x1")]);
        assert_eq!(kinds("abc123def"), vec![Ident("abc123def")]);
        assert_eq!(kinds("foo bar"), vec![Ident("foo"), Ident("bar")]);
        assert_eq!(kinds("é"), vec![Ident("é")]);
        assert_eq!(kinds("日本"), vec![Ident("日本")]);
    }

    #[test]
    fn number_followed_by_letters_splits() {
        assert_eq!(kinds("123abc"), vec![Int(123), Ident("abc")]);
        assert_eq!(kinds("2let"), vec![Int(2), Let]);
        assert_eq!(kinds("1x2"), vec![Int(1), Ident("x2")]);
    }

    #[test]
    fn integers() {
        assert_eq!(kinds("0"), vec![Int(0)]);
        assert_eq!(kinds("007"), vec![Int(7)]);
        assert_eq!(kinds("1 2 3"), vec![Int(1), Int(2), Int(3)]);
        assert_eq!(kinds(&i64::MAX.to_string()), vec![Int(i64::MAX)]);
    }

    #[test]
    fn integer_overflow() {
        let src = "1 + 9223372036854775808"; // i64::MAX + 1
        assert_eq!(
            lex(src)[2],
            Token::new(InvalidInt(IntErrorKind::PosOverflow), 4, 23)
        );
    }

    #[test]
    fn unknown_char() {
        assert_eq!(lex("1 $ 2")[1], Token::new(Unknown('$'), 2, 3));
    }

    #[test]
    fn lexing_continues_after_bad_input() {
        assert_eq!(kinds("1 $ 2"), vec![Int(1), Unknown('$'), Int(2)]);
        assert_eq!(
            kinds("99999999999999999999 + x"),
            vec![InvalidInt(IntErrorKind::PosOverflow), Plus, Ident("x")]
        );
    }

    #[test]
    fn unknown_chars() {
        // `_` and non-ASCII digits are not part of the language (yet).
        for c in ['$', '_', '!', '.', ',', '٣', '€'] {
            assert_eq!(kinds(&c.to_string()), vec![Unknown(c)], "input: {c:?}");
        }
    }

    #[test]
    fn unknown_multibyte_char_span() {
        assert_eq!(lex("1€")[1], Token::new(Unknown('€'), 1, 4));
    }

    #[test]
    fn token_spans() {
        assert_eq!(spans("let x = 42"), vec![(0, 3), (4, 5), (6, 7), (8, 10)]);
        assert_eq!(spans("(1)"), vec![(0, 1), (1, 2), (2, 3)]);
        assert_eq!(spans("  foo  "), vec![(2, 5)]);
    }

    #[test]
    fn spans_are_byte_offsets() {
        // 'é' is two bytes in UTF-8.
        assert_eq!(spans("é + 1"), vec![(0, 2), (3, 4), (5, 6)]);
        assert_eq!(spans("日本 = 1"), vec![(0, 6), (7, 8), (9, 10)]);
    }

    #[test]
    fn next_token_returns_eof_at_end() {
        let mut lexer = Lexer::new("1  ");
        assert_eq!(lexer.next_token().kind, Int(1));
        for _ in 0..2 {
            assert_eq!(lexer.next_token(), Token::new(Eof, 3, 3));
        }
        assert_eq!(Lexer::new("").next_token(), Token::new(Eof, 0, 0));
    }

    #[test]
    fn token_kind_display() {
        let cases = [
            (Int(42), "<int: 42>"),
            (Ident("x"), "<id: x>"),
            (Let, "<let>"),
            (Eq, "<=>"),
            (Plus, "<+>"),
            (Minus, "<->"),
            (Star, "<*>"),
            (Slash, "</>"),
            (LParen, "<(>"),
            (RParen, "<)>"),
            (Unknown('$'), "<unknown: $>"),
            (InvalidInt(IntErrorKind::PosOverflow), "<invalid int>"),
            (Eof, "<eof>"),
        ];
        for (kind, expected) in cases {
            assert_eq!(kind.to_string(), expected);
        }
    }

    /// Exhaustively lex every string up to length 4 over a small alphabet and
    /// check invariants that must hold for any input.
    #[test]
    fn invariants_on_all_short_inputs() {
        const ALPHABET: [char; 9] = ['a', 'l', 'e', 't', '1', '+', ' ', 'é', '$'];

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
                    Int(n) => assert_eq!(text.parse::<i64>().unwrap(), n),
                    InvalidInt(_) => assert!(text.parse::<i64>().is_err()),
                    Ident(name) => assert_eq!(text, name),
                    Let => assert_eq!(text, "let"),
                    Unknown(c) => assert_eq!(text, c.to_string()),
                    _ => assert_eq!(text.chars().count(), 1),
                }
            }
        }
    }
}
