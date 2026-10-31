//! The parser: tokens to AST, by recursive descent. One method per rule of
//! `docs/grammar.md`, section 3.
//!
//! Each rule returns `Option<T>`: `None` means an error was reported in
//! `diagnostics`. The caller usually passes it on with `?`.

// The rules are written bottom-up: `parse` (step 5d) will use all of them.
// Until then they are only called from tests.
#![cfg_attr(not(test), allow(dead_code))]

use diagnostics::{Diagnostic, Span};

use crate::ast::{BinOp, Expr, ExprKind, Name, Product, ProductKind, Quantity, Type, TypeKind};
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

    // Rules: expressions, from the loosest operator to the tightest.

    /// `expr = additive , [ PLUSMINUS , additive ] ;`
    ///
    /// At most one tolerance: `x +/- 1 mm +/- 2 mm` is E0005.
    fn expr(&mut self) -> Option<Expr> {
        let start = self.start();
        let value = self.additive()?;
        if !self.eat(TokenKind::PlusMinus) {
            return Some(value);
        }
        let tolerance = self.additive()?;
        if self.peek() == TokenKind::PlusMinus {
            self.diagnostics.push(Diagnostic {
                code: "E0005",
                message: "a value can have only one tolerance; use brackets to combine \
                          toleranced values"
                    .to_string(),
                span: self.tokens[self.pos].span.clone(),
            });
            return None;
        }
        Some(Expr {
            kind: ExprKind::Tolerance {
                value: Box::new(value),
                tolerance: Box::new(tolerance),
            },
            span: self.span_from(start),
        })
    }

    /// `additive = term , { ( "+" | "-" ) , term } ;` (left-associative)
    fn additive(&mut self) -> Option<Expr> {
        let start = self.start();
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.term()?;
            left = self.binary(op, left, right, start);
        }
        Some(left)
    }

    /// `term = unary , { ( "*" | "/" ) , unary } ;` (left-associative)
    fn term(&mut self) -> Option<Expr> {
        let start = self.start();
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.unary()?;
            left = self.binary(op, left, right, start);
        }
        Some(left)
    }

    /// `unary = "-" , unary | power ;`
    fn unary(&mut self) -> Option<Expr> {
        let start = self.start();
        if !self.eat(TokenKind::Minus) {
            return self.power();
        }
        let operand = self.unary()?;
        Some(Expr {
            kind: ExprKind::Neg(Box::new(operand)),
            span: self.span_from(start),
        })
    }

    /// `power = primary , [ "^" , int ] ;`
    fn power(&mut self) -> Option<Expr> {
        let start = self.start();
        let base = self.primary()?;
        if !self.eat(TokenKind::Caret) {
            return Some(base);
        }
        let power = self.int()?;
        Some(Expr {
            kind: ExprKind::Power {
                base: Box::new(base),
                power,
            },
            span: self.span_from(start),
        })
    }

    /// `primary = quantity | IDENT , [ "(" , args ] | "(" , expr , ")" ;`
    fn primary(&mut self) -> Option<Expr> {
        let start = self.start();
        let kind = match self.peek() {
            TokenKind::Number => ExprKind::Quantity(self.quantity()?),
            TokenKind::Ident => {
                let name = self.name()?;
                if self.eat(TokenKind::LParen) {
                    let args = self.args()?;
                    ExprKind::Call { func: name, args }
                } else {
                    ExprKind::Name(name)
                }
            }
            TokenKind::LParen => {
                self.advance();
                let mut inner = self.expr()?;
                self.expect(TokenKind::RParen)?;
                inner.span = self.span_from(start); // the span covers the brackets
                return Some(inner);
            }
            _ => {
                self.error_expected("an expression");
                return None;
            }
        };
        Some(Expr {
            kind,
            span: self.span_from(start),
        })
    }

    /// The arguments of a call, after its `(`: `[ expr , { "," , expr } ] , ")"`.
    fn args(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        if self.eat(TokenKind::RParen) {
            return Some(args);
        }
        loop {
            args.push(self.expr()?);
            if self.eat(TokenKind::RParen) {
                return Some(args);
            }
            if !self.eat(TokenKind::Comma) {
                self.error_expected("`,` or `)`");
                return None;
            }
        }
    }

    // Rules: types and quantities.

    /// `type = "Tol" , "<" , type , "," , quantity , ">" | dim_type ;`
    fn ty(&mut self) -> Option<Type> {
        let start = self.start();
        let kind = match self.peek() {
            TokenKind::Tol => {
                self.advance();
                self.expect(TokenKind::Lt)?;
                let inner = self.ty()?;
                self.expect(TokenKind::Comma)?;
                let tolerance = self.quantity()?;
                self.expect(TokenKind::Gt)?;
                TypeKind::Tol {
                    inner: Box::new(inner),
                    tolerance,
                }
            }
            TokenKind::Ident | TokenKind::LParen => TypeKind::Product(self.product()?),
            _ => {
                self.error_expected("a type");
                return None;
            }
        };
        Some(Type {
            kind,
            span: self.span_from(start),
        })
    }

    /// `quantity = NUMBER , [ unit ] ;`
    /// `unit = IDENT , [ "^" , int ] | "[" , unit_expr , "]" ;`
    ///
    /// A simple unit is one factor only (decision D1), so in `2.5 m * t` the
    /// quantity stops after `m`. Only a bracketed unit may contain `*` and `/`.
    fn quantity(&mut self) -> Option<Quantity> {
        let start = self.start();
        let number = self.expect(TokenKind::Number)?;
        let value = self.number_value(&number)?;
        let unit = match self.peek() {
            TokenKind::Ident => Some(self.product_factor()?),
            TokenKind::LBracket => {
                let bracket_start = self.start();
                self.advance();
                let mut unit = self.product()?;
                self.expect(TokenKind::RBracket)?;
                unit.span = self.span_from(bracket_start); // the span covers the brackets
                Some(unit)
            }
            _ => None,
        };
        Some(Quantity {
            value,
            unit,
            span: self.span_from(start),
        })
    }

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

    /// The value of a `Number` token. The lexer only produces valid number
    /// text, so parsing cannot fail; but the value may not fit in an `f64`.
    fn number_value(&mut self, token: &Token) -> Option<f64> {
        let text = self.text(&token.span);
        let value: f64 = text.parse().expect("the lexer only produces valid numbers");
        if value.is_finite() {
            Some(value)
        } else {
            self.diagnostics.push(Diagnostic {
                code: "E0004",
                message: format!("number `{text}` is too large"),
                span: token.span.clone(),
            });
            None
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

    /// Build `lhs op rhs`, spanning from `start` to the last consumed token.
    fn binary(&self, op: BinOp, lhs: Expr, rhs: Expr, start: usize) -> Expr {
        Expr {
            kind: ExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span: self.span_from(start),
        }
    }

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

    /// `0.9 m` is `(0.9 m)`; a number without a unit is just the number.
    fn show_quantity(q: &Quantity) -> String {
        match &q.unit {
            Some(unit) => format!("({} {})", q.value, show(unit)),
            None => format!("{}", q.value),
        }
    }

    /// `Tol<Length, 2.5 mm>` is `(Tol Length (2.5 mm))`.
    fn show_type(t: &Type) -> String {
        match &t.kind {
            TypeKind::Product(p) => show(p),
            TypeKind::Tol { inner, tolerance } => {
                format!("(Tol {} {})", show_type(inner), show_quantity(tolerance))
            }
        }
    }

    /// `a + b * c` is `(+ a (* b c))`; `f(x, y)` is `(call f x y)`.
    fn show_expr(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Quantity(q) => show_quantity(q),
            ExprKind::Name(name) => name.text.clone(),
            ExprKind::Call { func, args } => {
                let mut parts = vec![func.text.clone()];
                parts.extend(args.iter().map(show_expr));
                format!("(call {})", parts.join(" "))
            }
            ExprKind::Neg(x) => format!("(neg {})", show_expr(x)),
            ExprKind::Binary { op, lhs, rhs } => {
                let op = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                };
                format!("({op} {} {})", show_expr(lhs), show_expr(rhs))
            }
            ExprKind::Power { base, power } => format!("(^ {} {power})", show_expr(base)),
            ExprKind::Tolerance { value, tolerance } => {
                format!("(± {} {})", show_expr(value), show_expr(tolerance))
            }
        }
    }

    fn product_ok(source: &str) -> String {
        show(&parse_ok(source, |p| p.product()))
    }

    fn quantity_ok(source: &str) -> String {
        show_quantity(&parse_ok(source, |p| p.quantity()))
    }

    fn type_ok(source: &str) -> String {
        show_type(&parse_ok(source, |p| p.ty()))
    }

    fn expr_ok(source: &str) -> String {
        show_expr(&parse_ok(source, |p| p.expr()))
    }

    // Products

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

    // Quantities

    #[test]
    fn quantity_without_unit() {
        assert_eq!(quantity_ok("42"), "42");
    }

    #[test]
    fn quantity_with_simple_unit() {
        assert_eq!(quantity_ok("0.9 m"), "(0.9 m)");
        assert_eq!(quantity_ok("3 m^2"), "(3 m^2)");
        assert_eq!(quantity_ok("2.5mm"), "(2.5 mm)");
    }

    #[test]
    fn quantity_with_compound_unit_in_brackets() {
        assert_eq!(quantity_ok("9.81 [m/s^2]"), "(9.81 (/ m s^2))");
        assert_eq!(quantity_ok("0.3 [W/(m^2*K)]"), "(0.3 (/ W (* m^2 K)))");
    }

    #[test]
    fn quantity_simple_unit_stops_before_operator() {
        // Decision D1: `2.5 m * t` is `(2.5 m) * t`, not the unit `m*t`.
        let source = "2.5 m * t";
        let (tokens, _) = lex(source);
        let mut parser = Parser::new(source, tokens);
        let quantity = parser.quantity().expect("quantity parses");
        assert_eq!(show_quantity(&quantity), "(2.5 m)");
        assert_eq!(parser.peek(), TokenKind::Star);
    }

    #[test]
    fn quantity_values_with_exponents() {
        assert_eq!(parse_ok("1e-3 m", |p| p.quantity()).value, 0.001);
        assert_eq!(parse_ok("1e6 Pa", |p| p.quantity()).value, 1_000_000.0);
    }

    #[test]
    fn quantity_span_covers_number_and_unit() {
        let quantity = parse_ok("9.81 [m/s^2]", |p| p.quantity());
        assert_eq!(quantity.span, 0..12);
        assert_eq!(quantity.unit.expect("has a unit").span, 5..12);
    }

    #[test]
    fn quantity_unclosed_unit_bracket_is_e0002() {
        let errors = parse_errors("9.81 [m/s^2", |p| p.quantity());
        assert_eq!(errors[0].code, "E0002");
        assert_eq!(errors[0].message, "expected `]`, found end of file");
    }

    #[test]
    fn quantity_too_large_is_e0004() {
        let errors = parse_errors("1e999 m", |p| p.quantity());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "E0004");
        assert_eq!(errors[0].message, "number `1e999` is too large");
        assert_eq!(errors[0].span, 0..5);
    }

    // Types

    #[test]
    fn type_dimension_or_kind() {
        assert_eq!(type_ok("Thickness"), "Thickness");
        assert_eq!(type_ok("Length^3"), "Length^3");
    }

    #[test]
    fn type_with_tolerance() {
        assert_eq!(type_ok("Tol<Length, 2.5 mm>"), "(Tol Length (2.5 mm))");
        assert_eq!(parse_ok("Tol<Length, 2.5 mm>", |p| p.ty()).span, 0..19);
    }

    #[test]
    fn type_tol_without_tolerance_is_e0002() {
        let errors = parse_errors("Tol<Length>", |p| p.ty());
        assert_eq!(errors[0].code, "E0002");
        assert_eq!(errors[0].message, "expected `,`, found `>`");
    }

    #[test]
    fn type_starting_with_number_is_e0002() {
        let errors = parse_errors("3 m", |p| p.ty());
        assert_eq!(errors[0].message, "expected a type, found a number");
        assert_eq!(errors[0].span, 0..1);
    }

    // Expressions

    #[test]
    fn expr_names_and_quantities() {
        assert_eq!(expr_ok("thickness"), "thickness");
        assert_eq!(expr_ok("0.9 m"), "(0.9 m)");
    }

    #[test]
    fn expr_mul_binds_tighter_than_add() {
        assert_eq!(expr_ok("a + b * c"), "(+ a (* b c))");
        assert_eq!(expr_ok("a * b + c"), "(+ (* a b) c)");
    }

    #[test]
    fn expr_add_and_sub_are_left_associative() {
        assert_eq!(expr_ok("a - b - c"), "(- (- a b) c)");
        assert_eq!(expr_ok("a / b * c"), "(* (/ a b) c)");
    }

    #[test]
    fn expr_brackets_override_precedence() {
        assert_eq!(expr_ok("(a + b) * c"), "(* (+ a b) c)");
    }

    #[test]
    fn expr_power_binds_tighter_than_minus() {
        assert_eq!(expr_ok("-x^2"), "(neg (^ x 2))");
        assert_eq!(expr_ok("- -x"), "(neg (neg x))");
    }

    #[test]
    fn expr_unit_binds_tighter_than_power() {
        // `2 m^2` is two square metres; `(2 m)^2` squares the quantity.
        assert_eq!(expr_ok("2 m^2"), "(2 m^2)");
        assert_eq!(expr_ok("(2 m)^2"), "(^ (2 m) 2)");
    }

    #[test]
    fn expr_unit_then_multiplication() {
        // Decision D1, now inside a full expression.
        assert_eq!(expr_ok("2.5 m * t"), "(* (2.5 m) t)");
    }

    #[test]
    fn expr_calls() {
        assert_eq!(expr_ok("f()"), "(call f)");
        assert_eq!(expr_ok("patch(curve)"), "(call patch curve)");
        assert_eq!(
            expr_ok("thicken(mid, thickness)"),
            "(call thicken mid thickness)"
        );
        assert_eq!(
            expr_ok("area(mid) * thickness"),
            "(* (call area mid) thickness)"
        );
    }

    #[test]
    fn expr_tolerance_binds_loosest() {
        assert_eq!(expr_ok("10 mm +/- 2.5 mm"), "(± (10 mm) (2.5 mm))");
        assert_eq!(expr_ok("a + b ± 1 mm"), "(± (+ a b) (1 mm))");
    }

    #[test]
    fn expr_toleranced_values_combine_in_brackets() {
        assert_eq!(
            expr_ok("(x +/- 1 mm) + (y +/- 2 mm)"),
            "(+ (± x (1 mm)) (± y (2 mm)))"
        );
    }

    #[test]
    fn expr_spans() {
        let e = parse_ok("(a) * b", |p| p.expr());
        assert_eq!(e.span, 0..7);
        let ExprKind::Binary { lhs, .. } = &e.kind else {
            panic!("expected a binary expression, got {e:?}");
        };
        assert_eq!(lhs.span, 0..3); // the span of `a` covers its brackets
    }

    #[test]
    fn expr_second_tolerance_is_e0005() {
        let errors = parse_errors("x +/- 1 mm +/- 2 mm", |p| p.expr());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "E0005");
        assert_eq!(errors[0].span, 11..14);
    }

    #[test]
    fn expr_missing_operand_is_e0002() {
        let errors = parse_errors("a +", |p| p.expr());
        assert_eq!(errors[0].code, "E0002");
        assert_eq!(errors[0].message, "expected an expression, found end of file");
    }

    #[test]
    fn expr_call_arguments_need_commas() {
        let errors = parse_errors("f(a b)", |p| p.expr());
        assert_eq!(errors[0].message, "expected `,` or `)`, found a name");
        assert_eq!(errors[0].span, 4..5);
    }

    #[test]
    fn expr_power_is_not_chained() {
        // `^` has no associativity: `x^2^3` stops after `x^2`.
        let source = "x^2^3";
        let (tokens, _) = lex(source);
        let mut parser = Parser::new(source, tokens);
        let e = parser.expr().expect("x^2 parses");
        assert_eq!(show_expr(&e), "(^ x 2)");
        assert_eq!(parser.peek(), TokenKind::Caret);
    }
}
