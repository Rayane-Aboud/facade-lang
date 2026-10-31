//! The abstract syntax tree (AST): the structure of a program, built by the
//! parser. Roughly one type per rule of `docs/grammar.md`, section 3.
//!
//! Every node carries the span of the source text it came from. Brackets
//! leave no node: the shape of the tree already shows the grouping.

use diagnostics::Span;

/// A whole `.fcd` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub decls: Vec<Decl>,
}

/// A name as written in the source: `thickness`, `Length`, `mm`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    pub text: String,
    pub span: Span,
}

// Declarations

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub kind: DeclKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeclKind {
    /// `dimension Length` or `dimension Area = Length^2`.
    Dimension {
        name: Name,
        definition: Option<Product>,
    },
    /// `unit m : Length` or `unit mm = 0.001 m`.
    Unit { name: Name, definition: UnitDef },
    /// `kind Thickness : Length`.
    Kind { name: Name, parent: Product },
    /// `extern thicken(Surface, Thickness) -> Solid`.
    Extern {
        name: Name,
        params: Vec<Type>,
        result: Type,
    },
    /// `param thickness : Thickness = 0.9 m where thickness > 0 m`.
    Param {
        name: Name,
        ty: Type,
        default: Option<Expr>,
        refinement: Option<Predicate>,
    },
    /// `let vol = area(mid) * thickness`, with an optional type annotation.
    Let {
        name: Name,
        ty: Option<Type>,
        value: Expr,
    },
    /// `component f1 : Frame`.
    Component { name: Name, ty: Name },
    /// `plan assembly { install f1; install g1 on f1; }`.
    Plan { name: Name, steps: Vec<Step> },
}

/// The right-hand side of a `unit` declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum UnitDef {
    /// `unit m : Length`: the base unit of a dimension.
    Base(Name),
    /// `unit mm = 0.001 m`: a multiple of another unit.
    Scaled(Quantity),
}

/// One step of a plan: `install g1 on f1;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub component: Name,
    pub on: Option<Name>,
    pub span: Span,
}

// Types

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    /// `Tol<Length, 2.5 mm>`: a value with a tolerance.
    Tol {
        inner: Box<Type>,
        tolerance: Quantity,
    },
    /// A dimension, kind or geometry type: `Length`, `Length^3`, `Thickness`, `Curve`.
    /// Which one a name means is decided by name resolution, not by the parser.
    Product(Product),
}

/// Names with integer powers, joined by `*` and `/`. Used for dimension
/// types (`Mass * Length / Time^2`) and for units (`kg*m/s^2`), which have
/// the same shape in the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub kind: ProductKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductKind {
    /// `Length` or `s^-2`. Without `^`, the power is 1.
    Atom {
        name: Name,
        power: i32,
    },
    Mul(Box<Product>, Box<Product>),
    Div(Box<Product>, Box<Product>),
}

// Expressions

/// A number with an optional unit: `0.9 m`, `9.81 [m/s^2]`, `42`.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub unit: Option<Product>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// `0.9 m`
    Quantity(Quantity),
    /// `thickness`
    Name(Name),
    /// `thicken(mid, thickness)`
    Call { func: Name, args: Vec<Expr> },
    /// `-x`
    Neg(Box<Expr>),
    /// `a + b`, `a - b`, `a * b`, `a / b`
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `x^2`. The power is a whole number, so the dimension is known statically.
    Power { base: Box<Expr>, power: i32 },
    /// `10 mm +/- 2.5 mm`
    Tolerance {
        value: Box<Expr>,
        tolerance: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

// Predicates

/// A `where` clause: one or more comparisons joined by `and`.
#[derive(Debug, Clone, PartialEq)]
pub struct Predicate {
    pub comparisons: Vec<Comparison>,
    pub span: Span,
}

/// `thickness > 0 m`
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub lhs: Expr,
    pub op: CmpOp,
    pub rhs: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}
