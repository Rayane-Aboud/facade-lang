use diagnostics::{Diagnostic, Span};

use crate::token::{Token, TokenKind, keyword};

pub fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut lexer = Lexer {
        source,
        pos: 0,
        tokens: Vec::new(),
        diagnostics: Vec::new(),
    };

    lexer.run();
    (lexer.tokens, lexer.diagnostics)
}

struct Lexer<'a> {
    source: &'a str,

    /// Byte offset of the next character to read
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer<'_> {
    fn run(&mut self) {
        while let Some(c) = self.peek_nth(0) {
            let start = self.pos;
            self.bump();
            match c {
                _ if c.is_whitespace() => {}
                '-' if self.eat('-') => self.eat_while(|c| c != '\n'),
                'a'..='z' | 'A'..='Z' | '_' => self.ident_or_keyword(start),
                '0'..='9' => self.number(start),
                _ => match self.symbol(c) {
                    Some(kind) => self.push(kind, start..self.pos),
                    None => self.unexpected(start, c),
                },
            }
        }
        self.push(TokenKind::Eof, self.pos..self.pos);
    }

    fn ident_or_keyword(&mut self, start: usize) {
        self.eat_while(|c| c.is_ascii_alphanumeric() || c == '_');
        let kind = keyword(&self.source[start..self.pos]).unwrap_or(TokenKind::Ident);
        self.push(kind, start..self.pos);
    }

    fn number(&mut self, start: usize) {
        self.eat_while(|c| c.is_ascii_digit());

        // Fraction, only if a digit follows the dot: `1.` is not a number.
        if self.peek_nth(0) == Some('.') && self.peek_nth(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            self.eat_while(|c| c.is_ascii_digit());
        }

        // Exponent, only if a digit follows: `1e` is the number 1, then the unit `e`.
        if matches!(self.peek_nth(0), Some('e' | 'E')) {
            let digit_at = if matches!(self.peek_nth(1), Some('+' | '-')) {
                2
            } else {
                1
            };
            if self.peek_nth(digit_at).is_some_and(|c| c.is_ascii_digit()) {
                for _ in 0..digit_at {
                    self.bump();
                }
                self.eat_while(|c| c.is_ascii_digit());
            }
        }

        self.push(TokenKind::Number, start..self.pos);
    }

    fn symbol(&mut self, c: char) -> Option<TokenKind> {
        use TokenKind::*;
        let kind = match c {
            '(' => LParen,
            ')' => RParen,
            '[' => LBracket,
            ']' => RBracket,
            '{' => LBrace,
            '}' => RBrace,
            ',' => Comma,
            ':' => Colon,
            ';' => Semi,
            '*' => Star,
            '/' => Slash,
            '^' => Caret,
            '±' => PlusMinus,
            '<' if self.eat('=') => Le,
            '<' => Lt,
            '>' if self.eat('=') => Ge,
            '>' => Gt,
            '=' if self.eat('=') => EqEq,
            '=' => Eq,
            '-' if self.eat('>') => Arrow,
            '-' => Minus,
            '+' if self.peek_nth(0) == Some('/') && self.peek_nth(1) == Some('-') => {
                self.bump();
                self.bump();
                PlusMinus
            }
            '+' => Plus,
            _ => return None,
        };
        Some(kind)
    }

    fn unexpected(&mut self, start: usize, c: char) {
        self.diagnostics.push(Diagnostic {
            code: "E0001",
            message: format!("unexpected character `{c}`"),
            span: start..self.pos,
        });
    }

    //helpers
    fn peek_nth(&self, n: usize) -> Option<char> {
        self.source[self.pos..].chars().nth(n)
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek_nth(0) {
            self.pos += c.len_utf8();
        }
    }

    fn eat(&mut self, expected: char) -> bool {
        let found = self.peek_nth(0) == Some(expected);
        if found {
            self.bump();
        }
        found
    }

    fn eat_while(&mut self, pred: impl Fn(char) -> bool) {
        while self.peek_nth(0).is_some_and(&pred) {
            self.bump();
        }
    }

    fn push(&mut self, kind: TokenKind, span: Span) {
        self.tokens.push(Token { kind, span });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind as K;

    fn lex_ok(source: &str) -> Vec<(TokenKind, &str)> {
        let (tokens, diagnostics) = lex(source);
        assert_eq!(diagnostics, vec![], "source: {source:?}");
        tokens
            .into_iter()
            .map(|t| (t.kind, &source[t.span]))
            .collect()
    }

    fn kinds_ok(source: &str) -> Vec<TokenKind> {
        lex_ok(source).into_iter().map(|(kind, _)| kind).collect()
    }

    #[test]
    fn empty_source_is_just_eof() {
        assert_eq!(lex_ok(""), vec![(K::Eof, "")]);
    }

    #[test]
    fn whitespace_and_comments_produce_no_tokens() {
        assert_eq!(
            lex_ok("  \t\n-- a comment\n   -- another"),
            vec![(K::Eof, "")]
        );
    }

    #[test]
    fn comment_runs_to_end_of_line_only() {
        assert_eq!(
            lex_ok("a -- b\nc"),
            vec![(K::Ident, "a"), (K::Ident, "c"), (K::Eof, "")]
        );
    }

    #[test]
    fn keywords_and_names_are_whole_words() {
        assert_eq!(
            lex_ok("param thickness kinds Tol"),
            vec![
                (K::Param, "param"),
                (K::Ident, "thickness"),
                (K::Ident, "kinds"),
                (K::Tol, "Tol"),
                (K::Eof, ""),
            ]
        );
    }

    #[test]
    fn spans_are_byte_ranges() {
        let (tokens, _) = lex("let vol");
        let spans: Vec<Span> = tokens.into_iter().map(|t| t.span).collect();
        assert_eq!(spans, vec![0..3, 4..7, 7..7]);
    }

    #[test]
    fn numbers_with_fraction_and_exponent() {
        assert_eq!(
            lex_ok("42 0.9 1e-3 1E6 7.25e+2"),
            vec![
                (K::Number, "42"),
                (K::Number, "0.9"),
                (K::Number, "1e-3"),
                (K::Number, "1E6"),
                (K::Number, "7.25e+2"),
                (K::Eof, ""),
            ]
        );
    }

    #[test]
    fn unit_can_follow_number_without_space() {
        assert_eq!(
            lex_ok("2.5mm"),
            vec![(K::Number, "2.5"), (K::Ident, "mm"), (K::Eof, "")]
        );
    }

    #[test]
    fn exponent_without_digits_is_a_unit() {
        assert_eq!(
            lex_ok("1e 1em"),
            vec![
                (K::Number, "1"),
                (K::Ident, "e"),
                (K::Number, "1"),
                (K::Ident, "em"),
                (K::Eof, ""),
            ]
        );
    }

    #[test]
    fn every_single_character_symbol() {
        assert_eq!(
            kinds_ok("( ) [ ] { } < > , : ; = + - * / ^ ±"),
            vec![
                K::LParen,
                K::RParen,
                K::LBracket,
                K::RBracket,
                K::LBrace,
                K::RBrace,
                K::Lt,
                K::Gt,
                K::Comma,
                K::Colon,
                K::Semi,
                K::Eq,
                K::Plus,
                K::Minus,
                K::Star,
                K::Slash,
                K::Caret,
                K::PlusMinus,
                K::Eof,
            ]
        );
    }

    #[test]
    fn multi_character_symbols_use_longest_match() {
        assert_eq!(
            kinds_ok("<= >= == -> +/-"),
            vec![K::Le, K::Ge, K::EqEq, K::Arrow, K::PlusMinus, K::Eof]
        );
    }

    #[test]
    fn longest_match_works_without_spaces() {
        assert_eq!(
            kinds_ok("a<=b 1+/-2 ==="),
            vec![
                K::Ident,
                K::Le,
                K::Ident,
                K::Number,
                K::PlusMinus,
                K::Number,
                K::EqEq,
                K::Eq,
                K::Eof,
            ]
        );
    }
    #[test]
    fn plus_slash_without_minus_is_two_tokens() {
        assert_eq!(kinds_ok("+/ x"), vec![K::Plus, K::Slash, K::Ident, K::Eof]);
    }

    #[test]
    fn both_tolerance_spellings_are_one_token() {
        assert_eq!(lex_ok("±"), vec![(K::PlusMinus, "±"), (K::Eof, "")]);
        assert_eq!(lex_ok("+/-"), vec![(K::PlusMinus, "+/-"), (K::Eof, "")]);
        // `±` is 2 bytes in UTF-8.
        let (tokens, _) = lex("±");
        assert_eq!(tokens[0].span, 0..2);
    }

    #[test]
    fn double_dash_is_a_comment_even_before_a_number() {
        // Documented in grammar.md (D7): `x --1` is `x` and a comment.
        assert_eq!(kinds_ok("x --1"), vec![K::Ident, K::Eof]);
        assert_eq!(
            kinds_ok("x - -1"),
            vec![K::Ident, K::Minus, K::Minus, K::Number, K::Eof]
        );
    }

    #[test]
    fn example_files_lex_without_errors() {
        for source in [
            include_str!("../../../examples/ok/fig5_panel.fcd"),
            include_str!("../../../prelude/std.fcd"),
        ] {
            let tokens = kinds_ok(source);
            assert_eq!(tokens.last(), Some(&K::Eof));
        }
    }

    #[test]
    fn dot_without_digit_is_not_part_of_number() {
        let (tokens, diagnostics) = lex("1. x");
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec![K::Number, K::Ident, K::Eof]);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "E0001");
        assert_eq!(diagnostics[0].span, 1..2);
    }

    #[test]
    fn unexpected_character_is_reported_and_lexing_continues() {
        let (tokens, diagnostics) = lex("a @ b");
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec![K::Ident, K::Ident, K::Eof]);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "E0001");
        assert_eq!(diagnostics[0].message, "unexpected character `@`");
        assert_eq!(diagnostics[0].span, 2..3);
    }

    #[test]
    fn unexpected_multibyte_character_has_full_span() {
        // `µ` is 2 bytes in UTF-8, so its span is 2 bytes long.
        let (_, diagnostics) = lex("a µ b");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span, 2..4);
    }
}
