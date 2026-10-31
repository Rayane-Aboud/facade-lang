//! The parser: tokens to AST, by recursive descent. One method per rule of
//! `docs/grammar.md`, section 3.
//!
//! Each rule returns `Option<T>`: `None` means an error was reported in
//! `diagnostics`. The caller usually passes it on with `?`.

use diagnostics::{Diagnostic, Span};

use crate::ast::{
    BinOp, CmpOp, Comparison, Decl, DeclKind, Expr, ExprKind, Name, Predicate, Product,
    ProductKind, Program, Quantity, Step, Type, TypeKind, UnitDef,
};
use crate::lexer::lex;
use crate::token::{Token, TokenKind};

/// Parse a whole program. Lexer and parser diagnostics are returned together.
///
/// The program contains every declaration that parsed without errors; a
/// broken declaration is reported and left out.
pub fn parse(source: &str) -> (Program, Vec<Diagnostic>) {
    let (tokens, mut diagnostics) = lex(source);
    let mut parser = Parser::new(source, tokens);
    let program = parser.program();
    diagnostics.append(&mut parser.diagnostics);
    (program, diagnostics)
}

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

    // Rules: programs and declarations.

    /// `program = { decl } ;`
    ///
    /// After a broken declaration, skip to the next one and continue, so one
    /// run reports every broken declaration.
    fn program(&mut self) -> Program {
        let mut decls = Vec::new();
        while self.peek() != TokenKind::Eof {
            match self.decl() {
                Some(decl) => decls.push(decl),
                None => self.recover(),
            }
        }
        Program { decls }
    }

    /// Skip tokens until a declaration keyword or the end of the input.
    ///
    /// It does not skip a keyword it is already on: if the error was found at
    /// the start of the next declaration, that declaration is still parsed.
    /// The loop in `program` always makes progress: either `decl` consumes a
    /// keyword, or this method consumes at least one other token.
    fn recover(&mut self) {
        while !self.at_decl_start() {
            self.advance();
        }
    }

    fn at_decl_start(&self) -> bool {
        use TokenKind::*;
        matches!(
            self.peek(),
            Dimension | Unit | Kind | Extern | Param | Let | Component | Plan | Eof
        )
    }

    /// `decl = dimension_decl | unit_decl | kind_decl | extern_decl
    ///        | param_decl | let_decl | component_decl | plan_decl ;`
    ///
    /// The first keyword decides which one.
    fn decl(&mut self) -> Option<Decl> {
        let start = self.start();
        let kind = match self.peek() {
            TokenKind::Dimension => self.dimension_decl()?,
            TokenKind::Unit => self.unit_decl()?,
            TokenKind::Kind => self.kind_decl()?,
            TokenKind::Extern => self.extern_decl()?,
            TokenKind::Param => self.param_decl()?,
            TokenKind::Let => self.let_decl()?,
            TokenKind::Component => self.component_decl()?,
            TokenKind::Plan => self.plan_decl()?,
            _ => {
                self.error_expected("a declaration");
                return None;
            }
        };
        Some(Decl {
            kind,
            span: self.span_from(start),
        })
    }

    /// `dimension_decl = "dimension" , IDENT , [ "=" , dim_type ] ;`
    fn dimension_decl(&mut self) -> Option<DeclKind> {
        self.advance();
        let name = self.name()?;
        let definition = if self.eat(TokenKind::Eq) {
            Some(self.product()?)
        } else {
            None
        };
        Some(DeclKind::Dimension { name, definition })
    }

    /// `unit_decl = "unit" , IDENT , ( ":" , IDENT | "=" , quantity ) ;`
    fn unit_decl(&mut self) -> Option<DeclKind> {
        self.advance();
        let name = self.name()?;
        let definition = match self.peek() {
            TokenKind::Colon => {
                self.advance();
                UnitDef::Base(self.name()?)
            }
            TokenKind::Eq => {
                self.advance();
                UnitDef::Scaled(self.quantity()?)
            }
            _ => {
                self.error_expected("`:` or `=`");
                return None;
            }
        };
        Some(DeclKind::Unit { name, definition })
    }

    /// `kind_decl = "kind" , IDENT , ":" , dim_type ;`
    fn kind_decl(&mut self) -> Option<DeclKind> {
        self.advance();
        let name = self.name()?;
        self.expect(TokenKind::Colon)?;
        let parent = self.product()?;
        Some(DeclKind::Kind { name, parent })
    }

    /// `extern_decl = "extern" , IDENT , "(" , [ type , { "," , type } ] , ")" ,
    ///                "->" , type ;`
    fn extern_decl(&mut self) -> Option<DeclKind> {
        self.advance();
        let name = self.name()?;
        self.expect(TokenKind::LParen)?;
        let params = self.list(Self::ty)?;
        self.expect(TokenKind::Arrow)?;
        let result = self.ty()?;
        Some(DeclKind::Extern {
            name,
            params,
            result,
        })
    }

    /// `param_decl = "param" , IDENT , ":" , type , [ "=" , expr ] ,
    ///               [ "where" , predicate ] ;`
    fn param_decl(&mut self) -> Option<DeclKind> {
        self.advance();
        let name = self.name()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.ty()?;
        let default = if self.eat(TokenKind::Eq) {
            Some(self.expr()?)
        } else {
            None
        };
        let refinement = if self.eat(TokenKind::Where) {
            Some(self.predicate()?)
        } else {
            None
        };
        Some(DeclKind::Param {
            name,
            ty,
            default,
            refinement,
        })
    }

    /// `let_decl = "let" , IDENT , [ ":" , type ] , "=" , expr ;`
    fn let_decl(&mut self) -> Option<DeclKind> {
        self.advance();
        let name = self.name()?;
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq)?;
        let value = self.expr()?;
        Some(DeclKind::Let { name, ty, value })
    }

    /// `component_decl = "component" , IDENT , ":" , IDENT ;`
    fn component_decl(&mut self) -> Option<DeclKind> {
        self.advance();
        let name = self.name()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.name()?;
        Some(DeclKind::Component { name, ty })
    }

    /// `plan_decl = "plan" , IDENT , "{" , { step } , "}" ;`
    fn plan_decl(&mut self) -> Option<DeclKind> {
        self.advance();
        let name = self.name()?;
        self.expect(TokenKind::LBrace)?;
        let mut steps = Vec::new();
        while !self.eat(TokenKind::RBrace) {
            steps.push(self.step()?);
        }
        Some(DeclKind::Plan { name, steps })
    }

    /// `step = "install" , IDENT , [ "on" , IDENT ] , ";" ;`
    fn step(&mut self) -> Option<Step> {
        let start = self.start();
        if !self.eat(TokenKind::Install) {
            self.error_expected("`install` or `}`");
            return None;
        }
        let component = self.name()?;
        let on = if self.eat(TokenKind::On) {
            Some(self.name()?)
        } else {
            None
        };
        self.expect(TokenKind::Semi)?;
        Some(Step {
            component,
            on,
            span: self.span_from(start),
        })
    }

    // Rules: predicates.

    /// `predicate = comparison , { "and" , comparison } ;`
    fn predicate(&mut self) -> Option<Predicate> {
        let start = self.start();
        let mut comparisons = vec![self.comparison()?];
        while self.eat(TokenKind::And) {
            comparisons.push(self.comparison()?);
        }
        Some(Predicate {
            comparisons,
            span: self.span_from(start),
        })
    }

    /// `comparison = expr , ( "<" | "<=" | ">" | ">=" | "==" ) , expr ;`
    fn comparison(&mut self) -> Option<Comparison> {
        let start = self.start();
        let lhs = self.expr()?;
        let op = match self.peek() {
            TokenKind::Lt => CmpOp::Lt,
            TokenKind::Le => CmpOp::Le,
            TokenKind::Gt => CmpOp::Gt,
            TokenKind::Ge => CmpOp::Ge,
            TokenKind::EqEq => CmpOp::Eq,
            _ => {
                self.error_expected("a comparison (`<`, `<=`, `>`, `>=` or `==`)");
                return None;
            }
        };
        self.advance();
        let rhs = self.expr()?;
        Some(Comparison {
            lhs,
            op,
            rhs,
            span: self.span_from(start),
        })
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
                    let args = self.list(Self::expr)?;
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

    /// A comma-separated list after its opening `(`, up to and including the
    /// `)`: `[ item , { "," , item } ] , ")"`. Used for call arguments and for
    /// the parameter types of an `extern`.
    fn list<T>(&mut self, mut item: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let mut items = Vec::new();
        if self.eat(TokenKind::RParen) {
            return Some(items);
        }
        loop {
            items.push(item(self)?);
            if self.eat(TokenKind::RParen) {
                return Some(items);
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

    /// `where t > 0 m and t < 2 m` is `(where (> t (0 m)) (< t (2 m)))`.
    fn show_predicate(p: &Predicate) -> String {
        let mut parts = vec!["where".to_string()];
        parts.extend(p.comparisons.iter().map(|c| {
            let op = match c.op {
                CmpOp::Lt => "<",
                CmpOp::Le => "<=",
                CmpOp::Gt => ">",
                CmpOp::Ge => ">=",
                CmpOp::Eq => "==",
            };
            format!("({op} {} {})", show_expr(&c.lhs), show_expr(&c.rhs))
        }));
        format!("({})", parts.join(" "))
    }

    /// An optional part: shown with `show`, or `_` when it is missing.
    fn show_opt<T>(x: &Option<T>, show: impl Fn(&T) -> String) -> String {
        x.as_ref().map_or("_".to_string(), show)
    }

    /// `let vol = area(mid) * t` is `(let vol _ (* (call area mid) t))`.
    fn show_decl(d: &Decl) -> String {
        match &d.kind {
            DeclKind::Dimension { name, definition } => match definition {
                Some(p) => format!("(dimension {} {})", name.text, show(p)),
                None => format!("(dimension {})", name.text),
            },
            DeclKind::Unit { name, definition } => match definition {
                UnitDef::Base(dim) => format!("(unit {} : {})", name.text, dim.text),
                UnitDef::Scaled(q) => format!("(unit {} = {})", name.text, show_quantity(q)),
            },
            DeclKind::Kind { name, parent } => format!("(kind {} {})", name.text, show(parent)),
            DeclKind::Extern {
                name,
                params,
                result,
            } => {
                let params: Vec<String> = params.iter().map(show_type).collect();
                let result = show_type(result);
                format!("(extern {} ({}) {result})", name.text, params.join(" "))
            }
            DeclKind::Param {
                name,
                ty,
                default,
                refinement,
            } => format!(
                "(param {} {} {} {})",
                name.text,
                show_type(ty),
                show_opt(default, show_expr),
                show_opt(refinement, show_predicate)
            ),
            DeclKind::Let { name, ty, value } => format!(
                "(let {} {} {})",
                name.text,
                show_opt(ty, show_type),
                show_expr(value)
            ),
            DeclKind::Component { name, ty } => format!("(component {} {})", name.text, ty.text),
            DeclKind::Plan { name, steps } => {
                let mut parts = vec![format!("plan {}", name.text)];
                parts.extend(steps.iter().map(|s| match &s.on {
                    Some(on) => format!("(install {} on {})", s.component.text, on.text),
                    None => format!("(install {})", s.component.text),
                }));
                format!("({})", parts.join(" "))
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

    fn decl_ok(source: &str) -> String {
        show_decl(&parse_ok(source, |p| p.decl()))
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
        assert_eq!(
            errors[0].message,
            "expected an expression, found end of file"
        );
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

    // Declarations

    #[test]
    fn decl_dimension() {
        assert_eq!(decl_ok("dimension Length"), "(dimension Length)");
        assert_eq!(
            decl_ok("dimension Area = Length^2"),
            "(dimension Area Length^2)"
        );
    }

    #[test]
    fn decl_unit_base_and_scaled() {
        assert_eq!(decl_ok("unit m : Length"), "(unit m : Length)");
        assert_eq!(decl_ok("unit mm = 0.001 m"), "(unit mm = (0.001 m))");
        assert_eq!(
            decl_ok("unit N = 1 [kg*m/s^2]"),
            "(unit N = (1 (/ (* kg m) s^2)))"
        );
    }

    #[test]
    fn decl_unit_needs_colon_or_equals() {
        let errors = parse_errors("unit mm 0.001 m", |p| p.decl());
        assert_eq!(errors[0].message, "expected `:` or `=`, found a number");
    }

    #[test]
    fn decl_kind() {
        assert_eq!(
            decl_ok("kind Thickness : Length"),
            "(kind Thickness Length)"
        );
    }

    #[test]
    fn decl_extern() {
        assert_eq!(
            decl_ok("extern thicken(Surface, Thickness) -> Solid"),
            "(extern thicken (Surface Thickness) Solid)"
        );
        assert_eq!(decl_ok("extern now() -> Time"), "(extern now () Time)");
    }

    #[test]
    fn decl_param() {
        assert_eq!(decl_ok("param base : Curve"), "(param base Curve _ _)");
        assert_eq!(
            decl_ok("param thickness : Thickness = 0.9 m where thickness > 0 m"),
            "(param thickness Thickness (0.9 m) (where (> thickness (0 m))))"
        );
        assert_eq!(
            decl_ok("param frame : Tol<Length, 2.5 mm> = 10 mm"),
            "(param frame (Tol Length (2.5 mm)) (10 mm) _)"
        );
    }

    #[test]
    fn decl_param_negative_default_is_a_negation() {
        // `-1.0 m` is `-(1.0 m)`: there are no negative literals in the grammar.
        assert_eq!(
            decl_ok("param inset : Inset = -1.0 m"),
            "(param inset Inset (neg (1 m)) _)"
        );
    }

    #[test]
    fn decl_predicate_with_and() {
        assert_eq!(
            decl_ok("param t : Thickness where t > 0 m and t <= 2 m"),
            "(param t Thickness _ (where (> t (0 m)) (<= t (2 m))))"
        );
    }

    #[test]
    fn decl_let() {
        assert_eq!(
            decl_ok("let vol = area(mid) * thickness"),
            "(let vol _ (* (call area mid) thickness))"
        );
        assert_eq!(
            decl_ok("let vol : Volume = area(mid) * thickness"),
            "(let vol Volume (* (call area mid) thickness))"
        );
    }

    #[test]
    fn decl_component_and_plan() {
        assert_eq!(decl_ok("component f1 : Frame"), "(component f1 Frame)");
        assert_eq!(
            decl_ok("plan assembly { install f1; install g1 on f1; }"),
            "(plan assembly (install f1) (install g1 on f1))"
        );
    }

    #[test]
    fn decl_unclosed_plan_is_e0002() {
        let errors = parse_errors("plan p { install a;", |p| p.decl());
        assert_eq!(
            errors[0].message,
            "expected `install` or `}`, found end of file"
        );
    }

    #[test]
    fn decl_span_covers_the_whole_declaration() {
        assert_eq!(parse_ok("let a = 1 m", |p| p.decl()).span, 0..11);
    }

    // Programs and error recovery

    #[test]
    fn program_empty() {
        let (program, diagnostics) = parse("-- only a comment\n");
        assert_eq!(program.decls, vec![]);
        assert_eq!(diagnostics, vec![]);
    }

    #[test]
    fn program_fig5_example_parses() {
        let (program, diagnostics) = parse(include_str!("../../../examples/ok/fig5_panel.fcd"));
        assert_eq!(diagnostics, vec![]);
        assert_eq!(program.decls.len(), 9);
        assert_eq!(
            show_decl(&program.decls[3]),
            "(param thickness Thickness (0.9 m) (where (> thickness (0 m))))"
        );
        assert_eq!(
            show_decl(&program.decls[7]),
            "(let panel _ (call thicken mid thickness))"
        );
    }

    #[test]
    fn program_prelude_parses() {
        let (program, diagnostics) = parse(include_str!("../../../prelude/std.fcd"));
        assert_eq!(diagnostics, vec![]);
        assert!(!program.decls.is_empty());
    }

    #[test]
    fn program_reports_every_broken_declaration() {
        let source = "param a = 1\nlet b = 2\nlet c =\nparam d : Length";
        let (program, diagnostics) = parse(source);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            vec![
                "expected `:`, found `=`",
                "expected an expression, found `param`",
            ]
        );
        // `b` and `d` survive; `d` comes right after the error in `c`.
        let names: Vec<String> = program.decls.iter().map(show_decl).collect();
        assert_eq!(
            names,
            vec!["(let b _ 2)", "(param d Length _ _)"]
        );
    }

    #[test]
    fn program_skips_junk_between_declarations() {
        let (program, diagnostics) = parse("3 + 4\nlet a = 1");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "expected a declaration, found a number"
        );
        assert_eq!(program.decls.len(), 1);
    }

    #[test]
    fn program_returns_lexer_and_parser_errors_together() {
        let (_, diagnostics) = parse("let a = 1 @\nlet b =");
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, vec!["E0001", "E0002"]);
    }
}
