use std::fmt;
use std::str::CharIndices;

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenKind {
    // Literals and names
    Int,
    Ident,

    // Keywords
    Let,
    Def,
    Fun,
    If,
    Else,
    True,
    False,

    // Arithmetic operators
    Plus,
    Minus,
    Star,
    Slash,

    // Comparison operators
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical operators
    Bang,
    AndAnd,
    OrOr,

    // Punctuation
    Eq,
    FatArrow,
    Comma,
    Semicolon,

    // Brackets
    LParen,
    RParen,
    LBrace,
    RBrace,

    // Special
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            // Literals and names
            TokenKind::Int => "an integer",
            TokenKind::Ident => "an identifier",
            // Keywords
            TokenKind::Let => "`let`",
            TokenKind::Def => "`def`",
            TokenKind::Fun => "`fun`",
            TokenKind::If => "`if`",
            TokenKind::Else => "`else`",
            TokenKind::True => "`true`",
            TokenKind::False => "`false`",
            // Arithmetic operators
            TokenKind::Plus => "`+`",
            TokenKind::Minus => "`-`",
            TokenKind::Star => "`*`",
            TokenKind::Slash => "`/`",
            // Comparison operators
            TokenKind::EqEq => "`==`",
            TokenKind::Ne => "`!=`",
            TokenKind::Lt => "`<`",
            TokenKind::Le => "`<=`",
            TokenKind::Gt => "`>`",
            TokenKind::Ge => "`>=`",
            // Logical operators
            TokenKind::Bang => "`!`",
            TokenKind::AndAnd => "`&&`",
            TokenKind::OrOr => "`||`",
            // Punctuation
            TokenKind::Eq => "`=`",
            TokenKind::FatArrow => "`=>`",
            TokenKind::Comma => "`,`",
            TokenKind::Semicolon => "`;`",
            // Brackets
            TokenKind::LParen => "`(`",
            TokenKind::RParen => "`)`",
            TokenKind::LBrace => "`{`",
            TokenKind::RBrace => "`}`",
            // Special
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
            '=' if let Some('=') = self.peek() => {
                self.bump();
                TokenKind::EqEq
            }
            '=' if let Some('>') = self.peek() => {
                self.bump();
                TokenKind::FatArrow
            }
            '=' => TokenKind::Eq,
            '!' if let Some('=') = self.peek() => {
                self.bump();
                TokenKind::Ne
            }
            '!' => TokenKind::Bang,
            '<' if let Some('=') = self.peek() => {
                self.bump();
                TokenKind::Le
            }
            '<' => TokenKind::Lt,
            '>' if let Some('=') = self.peek() => {
                self.bump();
                TokenKind::Ge
            }
            '>' => TokenKind::Gt,
            '|' if let Some('|') = self.peek() => {
                self.bump();
                TokenKind::OrOr
            }
            '&' if let Some('&') = self.peek() => {
                self.bump();
                TokenKind::AndAnd
            }
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            c if c.is_ascii_alphabetic() => {
                self.eat_while(|c| c.is_ascii_alphanumeric() || c == '_');
                match &self.src[start..self.pos()] {
                    "let" => TokenKind::Let,
                    "def" => TokenKind::Def,
                    "true" => TokenKind::True,
                    "false" => TokenKind::False,
                    "fun" => TokenKind::Fun,
                    "if" => TokenKind::If,
                    "else" => TokenKind::Else,
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
            ("=>", FatArrow),
            ("==", EqEq),
            ("!=", Ne),
            ("<", Lt),
            ("<=", Le),
            (">", Gt),
            (">=", Ge),
            ("!", Bang),
            ("&&", AndAnd),
            ("||", OrOr),
            (",", Comma),
            (";", Semicolon),
            ("(", LParen),
            (")", RParen),
            ("{", LBrace),
            ("}", RBrace),
            ("let", Let),
            ("def", Def),
            ("fun", Fun),
            ("if", If),
            ("else", Else),
            ("true", True),
            ("false", False),
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
                (Semicolon, ";")
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
                (Semicolon, ";")
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
    fn new_keywords_only_match_whole_words() {
        for word in [
            "trueish", "true1", "True", "falsey", "false_", "funny", "fun_", "Fun",
        ] {
            assert_eq!(tokens(word), vec![(Ident, word)], "input: {word:?}");
        }
        assert_eq!(kinds("true false"), vec![True, False]);
        assert_eq!(kinds("fun(x)"), vec![Fun, LParen, Ident, RParen]);
    }

    #[test]
    fn arrow_is_one_token() {
        assert_eq!(tokens("=>"), vec![(FatArrow, "=>")]);
        assert_eq!(
            tokens("x => x"),
            vec![(Ident, "x"), (FatArrow, "=>"), (Ident, "x")]
        );
        // No spaces needed around it.
        assert_eq!(
            tokens("x=>x+1"),
            vec![
                (Ident, "x"),
                (FatArrow, "=>"),
                (Ident, "x"),
                (Plus, "+"),
                (Int, "1")
            ]
        );
    }

    #[test]
    fn equals_followed_by_other_characters() {
        // `=` at the very end: peeking finds nothing.
        assert_eq!(tokens("x ="), vec![(Ident, "x"), (Eq, "=")]);
        // A space splits two-character tokens.
        assert_eq!(tokens("= >"), vec![(Eq, "="), (Gt, ">")]);
        assert_eq!(tokens("= ="), vec![(Eq, "="), (Eq, "=")]);
        // Longest match from the left.
        assert_eq!(tokens("=>="), vec![(FatArrow, "=>"), (Eq, "=")]);
        assert_eq!(tokens("==>"), vec![(EqEq, "=="), (Gt, ">")]);
        assert_eq!(tokens("=>>"), vec![(FatArrow, "=>"), (Gt, ">")]);
        assert_eq!(tokens("==="), vec![(EqEq, "=="), (Eq, "=")]);
        assert_eq!(
            tokens("let f = x => x;"),
            vec![
                (Let, "let"),
                (Ident, "f"),
                (Eq, "="),
                (Ident, "x"),
                (FatArrow, "=>"),
                (Ident, "x"),
                (Semicolon, ";"),
            ]
        );
    }

    #[test]
    fn two_character_operators_take_both_characters() {
        for (src, kind) in [
            ("a==b", EqEq),
            ("a!=b", Ne),
            ("a<=b", Le),
            ("a>=b", Ge),
            ("a&&b", AndAnd),
            ("a||b", OrOr),
        ] {
            let op = &src[1..3];
            assert_eq!(
                tokens(src),
                vec![(Ident, "a"), (kind, op), (Ident, "b")],
                "input: {src:?}"
            );
        }
    }

    #[test]
    fn one_character_operators() {
        assert_eq!(tokens("a<b"), vec![(Ident, "a"), (Lt, "<"), (Ident, "b")]);
        assert_eq!(tokens("a>b"), vec![(Ident, "a"), (Gt, ">"), (Ident, "b")]);
        assert_eq!(tokens("!a"), vec![(Bang, "!"), (Ident, "a")]);
        assert_eq!(tokens("!!a"), vec![(Bang, "!"), (Bang, "!"), (Ident, "a")]);
        // `<` followed by something other than `=`.
        assert_eq!(tokens("<-1"), vec![(Lt, "<"), (Minus, "-"), (Int, "1")]);
    }

    #[test]
    fn comparison_operators_longest_match() {
        assert_eq!(tokens("<=="), vec![(Le, "<="), (Eq, "=")]);
        assert_eq!(tokens("!=="), vec![(Ne, "!="), (Eq, "=")]);
        assert_eq!(tokens("<=>"), vec![(Le, "<="), (Gt, ">")]);
        assert_eq!(tokens(">=="), vec![(Ge, ">="), (Eq, "=")]);
        assert_eq!(tokens("! ="), vec![(Bang, "!"), (Eq, "=")]);
        assert_eq!(tokens("< ="), vec![(Lt, "<"), (Eq, "=")]);
    }

    #[test]
    fn single_bar_and_ampersand_are_unknown() {
        assert_eq!(tokens("|"), vec![(Unknown, "|")]);
        assert_eq!(tokens("&"), vec![(Unknown, "&")]);
        assert_eq!(tokens("| |"), vec![(Unknown, "|"), (Unknown, "|")]);
        assert_eq!(tokens("|||"), vec![(OrOr, "||"), (Unknown, "|")]);
        assert_eq!(tokens("&&&"), vec![(AndAnd, "&&"), (Unknown, "&")]);
        assert_eq!(tokens("&|"), vec![(Unknown, "&"), (Unknown, "|")]);
    }

    #[test]
    fn if_and_else_are_keywords() {
        assert_eq!(
            kinds("if a { 1 } else { 2 }"),
            vec![If, Ident, LBrace, Int, RBrace, Else, LBrace, Int, RBrace]
        );
        assert_eq!(kinds("else if"), vec![Else, If]);
        for word in ["iff", "if_", "If", "elsewhere", "else1", "Else"] {
            assert_eq!(tokens(word), vec![(Ident, word)], "input: {word:?}");
        }
    }

    #[test]
    fn condition_shaped_input() {
        assert_eq!(
            tokens("!(a<=1)||b!=c&&d"),
            vec![
                (Bang, "!"),
                (LParen, "("),
                (Ident, "a"),
                (Le, "<="),
                (Int, "1"),
                (RParen, ")"),
                (OrOr, "||"),
                (Ident, "b"),
                (Ne, "!="),
                (Ident, "c"),
                (AndAnd, "&&"),
                (Ident, "d"),
            ]
        );
    }

    #[test]
    fn lambda_shaped_input() {
        assert_eq!(
            kinds("fun(a, b) => { a }"),
            vec![
                Fun, LParen, Ident, Comma, Ident, RParen, FatArrow, LBrace, Ident, RBrace
            ]
        );
        assert_eq!(
            spans("fun(a) => a"),
            vec![(0, 3), (3, 4), (4, 5), (5, 6), (7, 9), (10, 11)]
        );
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
        for c in ['$', '_', '.', '|', '&', '٣', '€', 'é'] {
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
            (Fun, "`fun`"),
            (True, "`true`"),
            (False, "`false`"),
            (Eq, "`=`"),
            (FatArrow, "`=>`"),
            (If, "`if`"),
            (Else, "`else`"),
            (Bang, "`!`"),
            (EqEq, "`==`"),
            (Ne, "`!=`"),
            (Lt, "`<`"),
            (Le, "`<=`"),
            (Gt, "`>`"),
            (Ge, "`>=`"),
            (OrOr, "`||`"),
            (AndAnd, "`&&`"),
            (Plus, "`+`"),
            (Minus, "`-`"),
            (Star, "`*`"),
            (Slash, "`/`"),
            (Comma, "`,`"),
            (Semicolon, "`;`"),
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
        const ALPHABET: [char; 17] = [
            'a', 'l', 'e', 't', '_', '1', '+', ';', '=', '>', '<', '!', '|', '&', ' ', 'é', '$',
        ];

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
                        let keywords = ["let", "def", "fun", "true", "false", "if", "else"];
                        assert!(!keywords.contains(&text), "{text:?}");
                    }
                    Let => assert_eq!(text, "let"),
                    Def => assert_eq!(text, "def"),
                    FatArrow => assert_eq!(text, "=>"),
                    EqEq => assert_eq!(text, "=="),
                    Ne => assert_eq!(text, "!="),
                    Le => assert_eq!(text, "<="),
                    Ge => assert_eq!(text, ">="),
                    OrOr => assert_eq!(text, "||"),
                    AndAnd => assert_eq!(text, "&&"),
                    _ => assert_eq!(text.chars().count(), 1, "{text:?}"),
                }
            }
        }
    }
}
