//! The parser: tokens to AST, by recursive descent. One method per rule of
//! `docs/grammar.md`, section 3.
//!
//! Each rule returns `Option<T>`: `None` means an error was reported in
//! `diagnostics`. The caller usually passes it on with `?`.

// The rules are written bottom-up: `parse` (step 5d) will use all of them.
// Until then they are only called from tests.
#![cfg_attr(not(test), allow(dead_code))]

use diagnostics::{Diagnostic, Span};

use crate::ast::{Name, Product, ProductKind};
use crate::token::{Token, TokenKind};

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    /// Index of the current token. Never moves past the final `Eof`.
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Parser {
            source,
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    // Rules

    /// `product = factor , { ( "*" | "/" ) , factor } ;`
    ///
    /// Used for both `dim_type` and `unit_expr`, which have the same shape.
    /// The loop makes `*` and `/` left-associative: `a / b / c` is `(a / b) / c`.
    fn product(&mut self) -> Option<Product> {
        let start = self.start();
        let mut left = self.product_factor()?;
        while matches!(self.peek(), TokenKind::Star | TokenKind::Slash) {
            let op = self.advance().kind;
            let right = self.product_factor()?;
            let (lhs, rhs) = (Box::new(left), Box::new(right));
            let kind = match op {
                TokenKind::Star => ProductKind::Mul(lhs, rhs),
                _ => ProductKind::Div(lhs, rhs),
            };
            left = Product {
                kind,
                span: self.span_from(start),
            };
        }
        Some(left)
    }

    /// `factor = IDENT , [ "^" , int ] | "(" , product , ")" ;`
    fn product_factor(&mut self) -> Option<Product> {
        let start = self.start();
        match self.peek() {
            TokenKind::LParen => {
                self.advance();
                let mut inner = self.product()?;
                self.expect(TokenKind::RParen)?;
                inner.span = self.span_from(start); // the span covers the brackets
                Some(inner)
            }
            TokenKind::Ident => {
                let name = self.name()?;
                let power = if self.eat(TokenKind::Caret) {
                    self.int()?
                } else {
                    1
                };
                Some(Product {
                    kind: ProductKind::Atom { name, power },
                    span: self.span_from(start),
                })
            }
            _ => {
                self.error_expected("a name or `(`");
                None
            }
        }
    }

    /// `int = [ "-" ] , NUMBER ;` where the number must be a whole number.
    fn int(&mut self) -> Option<i32> {
        let negative = self.eat(TokenKind::Minus);
        let token = self.expect(TokenKind::Number)?;
        let text = self.text(&token.span);
        match text.parse::<i32>() {
            Ok(n) => Some(if negative { -n } else { n }),
            Err(_) => {
                self.diagnostics.push(Diagnostic {
                    code: "E0003",
                    message: format!("a power must be a whole number, found `{text}`"),
                    span: token.span,
                });
                None
            }
        }
    }

    /// A name: one `IDENT` token, with its text copied out of the source.
    fn name(&mut self) -> Option<Name> {
        let token = self.expect(TokenKind::Ident)?;
        Some(Name {
            text: self.text(&token.span).to_string(),
            span: token.span,
        })
    }

    // Helpers

    /// The kind of the current token, without consuming it.
    fn peek(&self) -> TokenKind {
        self.tokens[self.pos].kind
    }

    /// Where the current token starts: the start of the node about to be parsed.
    fn start(&self) -> usize {
        self.tokens[self.pos].span.start
    }

    /// From `start` to the end of the last consumed token.
    fn span_from(&self, start: usize) -> Span {
        let end = if self.pos == 0 {
            start
        } else {
            self.tokens[self.pos - 1].span.end
        };
        start..end
    }

    /// Consume the current token and return it. Stays on `Eof` at the end.
    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if token.kind != TokenKind::Eof {
            self.pos += 1;
        }
        token
    }

    /// If the current token is `kind`, consume it and return true.
    fn eat(&mut self, kind: TokenKind) -> bool {
        let found = self.peek() == kind;
        if found {
            self.advance();
        }
        found
    }

    /// Consume a token of `kind`, or report E0002 and return `None`.
    fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if self.peek() == kind {
            Some(self.advance())
        } else {
            self.error_expected(kind.describe());
            None
        }
    }

    /// Report E0002: "expected <what>, found <current token>".
    fn error_expected(&mut self, expected: &str) {
        let found = &self.tokens[self.pos];
        self.diagnostics.push(Diagnostic {
            code: "E0002",
            message: format!("expected {expected}, found {}", found.kind.describe()),
            span: found.span.clone(),
        });
    }

    /// The source text of `span`. Borrowed from the source, not from the parser.
    fn text(&self, span: &Span) -> &'a str {
        &self.source[span.clone()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    /// Parse `source` with `rule`. Check that there are no diagnostics and that
    /// the whole input was used, then return the result.
    fn parse_ok<'s, T>(source: &'s str, rule: impl FnOnce(&mut Parser<'s>) -> Option<T>) -> T {
        let (tokens, lex_errors) = lex(source);
        assert_eq!(lex_errors, vec![], "source: {source:?}");
        let mut parser = Parser::new(source, tokens);
        let result = rule(&mut parser);
        assert_eq!(parser.diagnostics, vec![], "source: {source:?}");
        assert_eq!(
            parser.peek(),
            TokenKind::Eof,
            "input not fully used: {source:?}"
        );
        result.expect("rule returned None without a diagnostic")
    }

    /// Parse `source` with `rule`, check that it fails, and return the diagnostics.
    fn parse_errors<'s, T>(
        source: &'s str,
        rule: impl FnOnce(&mut Parser<'s>) -> Option<T>,
    ) -> Vec<Diagnostic> {
        let (tokens, _) = lex(source);
        let mut parser = Parser::new(source, tokens);
        let result = rule(&mut parser);
        assert!(result.is_none(), "expected a failure: {source:?}");
        parser.diagnostics
    }

    /// A product as an S-expression: `kg*m/s^2` is `(/ (* kg m) s^2)`.
    fn show(p: &Product) -> String {
        match &p.kind {
            ProductKind::Atom { name, power: 1 } => name.text.clone(),
            ProductKind::Atom { name, power } => format!("{}^{power}", name.text),
            ProductKind::Mul(l, r) => format!("(* {} {})", show(l), show(r)),
            ProductKind::Div(l, r) => format!("(/ {} {})", show(l), show(r)),
        }
    }

    fn product_ok(source: &str) -> String {
        show(&parse_ok(source, |p| p.product()))
    }

    #[test]
    fn product_single_name() {
        assert_eq!(product_ok("Length"), "Length");
    }

    #[test]
    fn product_powers_can_be_negative() {
        assert_eq!(product_ok("Length^3"), "Length^3");
        assert_eq!(product_ok("s^-2"), "s^-2");
    }

    #[test]
    fn product_mul_and_div_are_left_associative() {
        assert_eq!(product_ok("a / b / c"), "(/ (/ a b) c)");
        assert_eq!(
            product_ok("Mass * Length / Time^2"),
            "(/ (* Mass Length) Time^2)"
        );
    }

    #[test]
    fn product_brackets_group() {
        assert_eq!(product_ok("W / (m^2 * K)"), "(/ W (* m^2 K))");
    }

    #[test]
    fn product_spans_cover_the_source_text() {
        assert_eq!(parse_ok("kg*m", |p| p.product()).span, 0..4);
        assert_eq!(parse_ok("s^-2", |p| p.product()).span, 0..4);
        assert_eq!(parse_ok("(a)", |p| p.product()).span, 0..3);
    }

    #[test]
    fn product_missing_operand_is_e0002() {
        let errors = parse_errors("Length *", |p| p.product());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "E0002");
        assert_eq!(
            errors[0].message,
            "expected a name or `(`, found end of file"
        );
        assert_eq!(errors[0].span, 8..8);
    }

    #[test]
    fn product_starting_with_operator_is_e0002() {
        let errors = parse_errors("* a", |p| p.product());
        assert_eq!(errors[0].message, "expected a name or `(`, found `*`");
        assert_eq!(errors[0].span, 0..1);
    }

    #[test]
    fn product_unclosed_bracket_is_e0002() {
        let errors = parse_errors("(a * b", |p| p.product());
        assert_eq!(errors[0].code, "E0002");
        assert_eq!(errors[0].message, "expected `)`, found end of file");
    }

    #[test]
    fn product_fractional_power_is_e0003() {
        let errors = parse_errors("s^2.5", |p| p.product());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "E0003");
        assert_eq!(
            errors[0].message,
            "a power must be a whole number, found `2.5`"
        );
        assert_eq!(errors[0].span, 2..5);
    }
}
