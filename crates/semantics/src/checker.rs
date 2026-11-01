//! The type checker (`docs/feature-1-dimensions.md`, section 3). One method
//! per typing rule; the rule names are in the comments.
//!
//! Besides its type, each expression also gets its SI value when it is known
//! without running geometry (constant folding). The playground uses these
//! values to draw the panel.

use diagnostics::{Diagnostic, Span};
use syntax::ast::{BinOp, Decl, DeclKind, Expr, ExprKind, Name, Predicate, Program};

use crate::dimension::Dimension;
use crate::signature::{Signature, error};
use crate::types::Ty;

/// One entry of Γ: a declared value, its type, and its SI value if known.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub name: String,
    pub ty: Ty,
    pub value: Option<f64>,
}

/// The result of checking a program.
#[derive(Debug)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    /// The final environment Γ, in declaration order.
    pub bindings: Vec<Binding>,
}

/// The type of an expression, and its SI value if known.
type Typed = (Ty, Option<f64>);

/// Check a parsed program, starting from the signature `sig` (the prelude).
pub fn check_program(sig: Signature, program: &Program) -> Analysis {
    let mut checker = Checker {
        sig,
        env: Vec::new(),
        diags: Vec::new(),
    };
    for decl in &program.decls {
        checker.declaration(decl);
    }
    Analysis {
        diagnostics: checker.diags,
        bindings: checker.env,
    }
}

struct Checker {
    /// Σ: grows when the program declares its own kinds, units or externs.
    sig: Signature,
    /// Γ: the values declared so far (section 3.1).
    env: Vec<Binding>,
    diags: Vec<Diagnostic>,
}

impl Checker {
    // Declarations (section 3.5)

    fn declaration(&mut self, decl: &Decl) {
        match &decl.kind {
            // D-Param
            DeclKind::Param {
                name,
                ty,
                default,
                refinement,
            } => {
                let Some(sigma) = self.sig.resolve_type(ty, &mut self.diags) else {
                    return;
                };
                let v = match default {
                    Some(e) => self.annotated(e, &sigma),
                    None => None,
                };
                self.bind(name, sigma, v);
                // Checked with x in scope: `where thickness > 0 m`.
                if let Some(p) = refinement {
                    self.predicate(p);
                }
            }
            // D-Let
            DeclKind::Let {
                name,
                ty: Some(ty),
                value,
            } => {
                let Some(sigma) = self.sig.resolve_type(ty, &mut self.diags) else {
                    return;
                };
                let v = self.annotated(value, &sigma);
                self.bind(name, sigma, v);
            }
            // D-Infer: x gets the type of its value.
            DeclKind::Let {
                name,
                ty: None,
                value,
            } => {
                if let Some((tau, v)) = self.expr(value) {
                    self.bind(name, tau, v);
                }
            }
            // Feature 3, not checked yet.
            DeclKind::Component { .. } | DeclKind::Plan { .. } => {}
            // Declarations of concepts go to Σ.
            DeclKind::Dimension { .. }
            | DeclKind::Unit { .. }
            | DeclKind::Kind { .. }
            | DeclKind::Extern { .. } => self.sig.declare(decl, &mut self.diags),
        }
    }

    /// `Γ ⊢ e : τ` and `τ ◁ σ`, for a declaration with a type annotation.
    fn annotated(&mut self, e: &Expr, sigma: &Ty) -> Option<f64> {
        let (tau, v) = self.expr(e)?;
        if !self.is_assignable(&tau, sigma) {
            self.mismatch(&tau, sigma, e.span.clone());
        }
        v
    }

    /// Assignability `τ ◁ σ` (section 2.7).
    fn is_assignable(&self, tau: &Ty, sigma: &Ty) -> bool {
        match tau {
            // A-Kind: a plain quantity of the right dimension receives the kind.
            Ty::Quantity(d) if self.sig.dim(sigma).as_ref() == Some(d) => true,
            // A-Sub
            _ => self.sig.is_subtype(tau, sigma),
        }
    }

    /// `Γ, x : τ`, after checking `x ∉ Γ` (E0107).
    fn bind(&mut self, name: &Name, ty: Ty, value: Option<f64>) {
        if self.lookup(&name.text).is_some() {
            let message = format!("`{}` is already declared", name.text);
            self.report("E0107", message, name.span.clone());
            return;
        }
        self.env.push(Binding {
            name: name.text.clone(),
            ty,
            value,
        });
    }

    /// `x : τ ∈ Γ`
    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.env.iter().find(|b| b.name == name)
    }

    /// T-Cmp, for every comparison of a `where` clause (section 3.4).
    fn predicate(&mut self, p: &Predicate) {
        for c in &p.comparisons {
            let lhs = self.quantity_expr(&c.lhs);
            let rhs = self.quantity_expr(&c.rhs);
            if let (Some((l, _)), Some((r, _))) = (lhs, rhs) {
                self.same_dimension(&l, &r, c.span.clone());
            }
        }
    }

    // Expressions (section 3.3)

    /// `Γ ⊢ e : τ`. `None` means an error was reported.
    fn expr(&mut self, e: &Expr) -> Option<Typed> {
        match &e.kind {
            // T-Lit and T-Num: the value is normalised to SI.
            ExprKind::Quantity(q) => {
                let (d, v) = self.sig.quantity(q, &mut self.diags)?;
                Some((Ty::Quantity(d), Some(v)))
            }
            // T-Var
            ExprKind::Name(name) => {
                let found = self.lookup(&name.text).map(|b| (b.ty.clone(), b.value));
                if found.is_none() {
                    let message = format!("unknown name `{}`", name.text);
                    self.report("E0103", message, name.span.clone());
                }
                found
            }
            // T-Neg
            ExprKind::Neg(inner) => {
                let (ty, v) = self.quantity_expr(inner)?;
                Some((ty, v.map(|v| -v)))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                // Both sides first, so errors in both are reported.
                let l = self.quantity_expr(lhs);
                let r = self.quantity_expr(rhs);
                let ((lt, lv), (rt, rv)) = (l?, r?);
                match op {
                    BinOp::Add => Some((
                        self.add(&lt, &rt, e.span.clone())?,
                        both(lv, rv, |a, b| a + b),
                    )),
                    BinOp::Sub => Some((
                        self.add(&lt, &rt, e.span.clone())?,
                        both(lv, rv, |a, b| a - b),
                    )),
                    BinOp::Mul => Some((self.mul(&lt, &rt), both(lv, rv, |a, b| a * b))),
                    BinOp::Div => Some((self.div(&lt, &rt), both(lv, rv, |a, b| a / b))),
                }
            }
            // T-Pow: multiply the dimension, forget the kind.
            ExprKind::Power { base, power } => {
                let (ty, v) = self.quantity_expr(base)?;
                let d = self.dim(&ty).pow(*power);
                Some((Ty::Quantity(d), v.map(|v| v.powi(*power))))
            }
            // T-Tol: the tolerance has the dimension of the value.
            ExprKind::Tolerance { value, tolerance } => {
                let v = self.quantity_expr(value);
                let t = self.quantity_expr(tolerance);
                let ((vt, vv), (tt, _)) = (v?, t?);
                self.same_dimension(&vt, &tt, tolerance.span.clone())?;
                Some((vt, vv))
            }
            ExprKind::Call { func, args } => self.call(func, args, e.span.clone()),
        }
    }

    /// `Γ ⊢ e : τ` where τ must be a quantity type (E0105 otherwise).
    fn quantity_expr(&mut self, e: &Expr) -> Option<Typed> {
        let (ty, v) = self.expr(e)?;
        if !ty.is_quantity() {
            let message = format!("`{ty}` is not a quantity, so it cannot be used in arithmetic");
            self.report("E0105", message, e.span.clone());
            return None;
        }
        Some((ty, v))
    }

    /// T-Add (also for `-`): equal dimensions, result `τ₁ ⊕ τ₂` (section 2.8).
    fn add(&mut self, a: &Ty, b: &Ty, span: Span) -> Option<Ty> {
        self.same_dimension(a, b, span)?;
        Some(match (a, b) {
            // A plain side takes the other side's kind.
            (Ty::Quantity(_), _) => b.clone(),
            (_, Ty::Quantity(_)) => a.clone(),
            // Two kinds: their join. (Warning W0101 is future work.)
            _ => self.sig.join(a, b),
        })
    }

    /// T-Scale, T-Scale' and T-Mul (decisions K2 and K3).
    fn mul(&self, a: &Ty, b: &Ty) -> Ty {
        if is_number(b) {
            a.clone()
        } else if is_number(a) {
            b.clone()
        } else {
            Ty::Quantity(&self.dim(a) * &self.dim(b))
        }
    }

    /// T-Scale and T-Div. Dividing *by* a number keeps the kind; `1 / x` does not.
    fn div(&self, a: &Ty, b: &Ty) -> Ty {
        if is_number(b) {
            a.clone()
        } else {
            Ty::Quantity(&self.dim(a) / &self.dim(b))
        }
    }

    /// T-Call: every argument must be a subtype of its parameter.
    fn call(&mut self, func: &Name, args: &[Expr], span: Span) -> Option<Typed> {
        let typed: Vec<Option<Typed>> = args.iter().map(|a| self.expr(a)).collect();
        let Some(f) = self.sig.extern_fn(&func.text).cloned() else {
            let message = format!("unknown function `{}`", func.text);
            self.report("E0103", message, func.span.clone());
            return None;
        };
        if f.params.len() != args.len() {
            let message = format!(
                "`{}` takes {} argument(s), but {} were given",
                func.text,
                f.params.len(),
                args.len()
            );
            self.report("E0106", message, span);
        } else {
            for ((arg, typed), param) in args.iter().zip(typed).zip(&f.params) {
                if let Some((tau, _)) = typed
                    && !self.sig.is_subtype(&tau, param)
                {
                    self.mismatch(&tau, param, arg.span.clone());
                }

            }
        }
        // The result type is known even if an argument is wrong, so one
        // mistake does not cause more errors in later lines.
        Some((f.result, None))
    }

    // Errors

    /// The premise `dim(τ₁) = dim(τ₂)` of T-Add, T-Tol and T-Cmp (E0101).
    fn same_dimension(&mut self, a: &Ty, b: &Ty, span: Span) -> Option<()> {
        if self.dim(a) == self.dim(b) {
            return Some(());
        }
        let message = format!(
            "dimension mismatch: {} and {} have different dimensions",
            self.describe(a),
            self.describe(b)
        );
        self.report("E0101", message, span);
        None
    }

    /// Report `τ` found where `σ` was expected (sections 3.3 and 3.5).
    fn mismatch(&mut self, found: &Ty, expected: &Ty, span: Span) {
        let (code, message) = match (self.sig.dim(found), self.sig.dim(expected)) {
            (Some(df), Some(de)) if df == de => {
                let detail = match found {
                    Ty::Quantity(_) => format!(
                        "a plain `{df}` has no kind; declare it first, e.g. `param x : {expected} = ...`"
                    ),
                    _ => format!("both are `{de}`, but they mean different things"),
                };
                let message =
                    format!("kind mismatch: expected `{expected}`, found `{found}` ({detail})");
                ("E0102", message)
            }
            (Some(_), Some(_)) => {
                let message = format!(
                    "dimension mismatch: expected {}, found {}",
                    self.describe(expected),
                    self.describe(found)
                );
                ("E0101", message)
            }
            _ => {
                let message = format!("type mismatch: expected `{expected}`, found `{found}`");
                ("E0108", message)
            }
        };
        self.report(code, message, span);
    }

    /// `` `Thickness` (Length) `` for a kind, so messages also show the dimension.
    fn describe(&self, ty: &Ty) -> String {
        match ty {
            Ty::Kind(_) => format!("`{ty}` ({})", self.dim(ty)),
            _ => format!("`{ty}`"),
        }
    }

    /// `dim(τ)` for a quantity type.
    fn dim(&self, ty: &Ty) -> Dimension {
        self.sig
            .dim(ty)
            .expect("quantity types always have a dimension")
    }

    fn report(&mut self, code: &'static str, message: String, span: Span) {
        error(&mut self.diags, code, message, span);
    }
}

/// The value of `a ∘ b`, if both values are known and the result is finite.
fn both(a: Option<f64>, b: Option<f64>, op: impl Fn(f64, f64) -> f64) -> Option<f64> {
    let v = op(a?, b?);
    v.is_finite().then_some(v)
}

/// A pure number: exactly `Q(1)`.
fn is_number(ty: &Ty) -> bool {
    *ty == Ty::Quantity(Dimension::one())
}

#[cfg(test)]
mod tests {
    use crate::analyse;

    const FIG5: &str = include_str!("../../../examples/ok/fig5_panel.fcd");

    /// Params with kinds and a surface, used by many tests below.
    const KINDED: &str = "param t : Thickness = 0.9 m\nparam i : Inset = 1 m\nparam s : Surface\n";

    fn with(extra: &str) -> String {
        format!("{}{}", KINDED, extra)
    }

    fn codes(source: &str) -> Vec<&'static str> {
        analyse(source).diagnostics.iter().map(|d| d.code).collect()
    }

    /// The type of `name` in an accepted program, as text.
    fn type_of(source: &str, name: &str) -> String {
        let analysis = analyse(source);
        assert_eq!(analysis.diagnostics, vec![], "should be accepted");
        let b = analysis
            .bindings
            .iter()
            .find(|b| b.name == name)
            .expect("declared");
        b.ty.to_string()
    }

    fn value_of(source: &str, name: &str) -> f64 {
        let analysis = analyse(source);
        let b = analysis
            .bindings
            .iter()
            .find(|b| b.name == name)
            .expect("declared");
        b.value.expect("value is known")
    }

    // The Fig. 5 program (section 5.6)

    #[test]
    fn fig5_is_accepted_with_inferred_types() {
        assert_eq!(type_of(FIG5, "inset"), "Inset");
        assert_eq!(type_of(FIG5, "offset"), "Length");
        assert_eq!(type_of(FIG5, "panel"), "Solid");
        assert_eq!(type_of(FIG5, "vol"), "Length^3"); // inferred (section 5.2)
        assert_eq!(value_of(FIG5, "thickness"), 0.9);
    }

    #[test]
    fn fig5_swap_is_e0102() {
        let swapped = FIG5.replace("thicken(mid, thickness)", "thicken(mid, inset)");
        assert_eq!(codes(&swapped), vec!["E0102"]);
        assert_eq!(
            analyse(&swapped).diagnostics[0].message,
            "kind mismatch: expected `Thickness`, found `Inset` \
             (both are `Length`, but they mean different things)"
        );
    }

    // Literals and names: T-Lit, T-Num, T-Var

    #[test]
    fn literal_is_normalised_to_si() {
        assert_eq!(type_of("let x = 2.5 mm", "x"), "Length");
        assert!((value_of("let x = 2.5 mm", "x") - 0.0025).abs() < 1e-12);
        assert_eq!(type_of("let n = 42", "n"), "1");
    }

    #[test]
    fn unknown_unit_is_e0104() {
        assert_eq!(codes("let x = 2 parsec"), vec!["E0104"]);
    }

    #[test]
    fn unknown_name_is_e0103() {
        assert_eq!(codes("let y = z"), vec!["E0103"]);
        assert_eq!(codes("let y = x\nlet x = 1"), vec!["E0103"]); // declare before use
    }

    #[test]
    fn duplicate_is_e0107() {
        assert_eq!(codes("let x = 1\nlet x = 2"), vec!["E0107"]);
    }

    // Arithmetic: T-Neg, T-Add, T-Scale, T-Mul, T-Div, T-Pow, T-Tol

    #[test]
    fn add_needs_equal_dimensions() {
        assert!((value_of("let x = 1000 mm + 1 m", "x") - 2.0).abs() < 1e-12);
        assert_eq!(codes("let x = 2 m + 3 s"), vec!["E0101"]);
    }

    #[test]
    fn add_combines_kinds() {
        assert_eq!(type_of(&with("let a = t + 1 mm"), "a"), "Thickness");
        assert_eq!(type_of(&with("let b = t + i"), "b"), "Length");
    }

    #[test]
    fn scaling_keeps_the_kind() {
        assert_eq!(type_of(&with("let a = 2 * t"), "a"), "Thickness");
        assert_eq!(type_of(&with("let b = t / 2"), "b"), "Thickness");
        assert_eq!(value_of("let x = -1.0 m", "x"), -1.0);
    }

    #[test]
    fn mul_div_pow_forget_the_kind() {
        assert_eq!(type_of(&with("let a = t * t"), "a"), "Length^2");
        assert_eq!(type_of(&with("let b = 1 / t"), "b"), "Length^-1");
        assert_eq!(type_of(&with("let c = t^2"), "c"), "Length^2");
    }

    #[test]
    fn geometry_in_arithmetic_is_e0105() {
        assert_eq!(codes(&with("let x = s + 1 m")), vec!["E0105"]);
        assert_eq!(codes(&with("let y = -s")), vec!["E0105"]);
    }

    #[test]
    fn tolerance_needs_the_value_dimension() {
        assert_eq!(type_of("let x = 10 mm +/- 2.5 mm", "x"), "Length");
        assert_eq!(codes("let x = 10 mm +/- 2 s"), vec!["E0101"]);
    }

    // Calls: T-Call

    #[test]
    fn call_with_wrong_dimension_is_e0101() {
        assert_eq!(codes(&with("let p = thicken(s, 2 s)")), vec!["E0101"]);
    }

    #[test]
    fn call_with_plain_literal_for_a_kind_is_e0102() {
        assert_eq!(codes(&with("let p = thicken(s, 0.9 m)")), vec!["E0102"]);
    }

    #[test]
    fn call_with_wrong_geometry_is_e0108() {
        assert_eq!(
            codes(&with("param c : Curve\nlet p = thicken(c, t)")),
            vec!["E0108"]
        );
    }

    #[test]
    fn call_with_wrong_arity_is_e0106() {
        assert_eq!(codes(&with("let a = area(s, s)")), vec!["E0106"]);
    }

    #[test]
    fn unknown_function_is_e0103() {
        assert_eq!(codes("let x = foo(1)"), vec!["E0103"]);
    }

    #[test]
    fn user_subkind_is_accepted() {
        // GlassThickness <: Thickness by S-Kind: user-defined domain concepts.
        let source = "kind GlassThickness : Thickness\n\
                      param g : GlassThickness = 6 mm\n\
                      param s : Surface\n\
                      let p = thicken(s, g)";
        assert_eq!(codes(source), Vec::<&str>::new());
    }

    // Declarations and predicates: D-Param, D-Let, T-Cmp

    #[test]
    fn relabelling_is_e0102() {
        assert_eq!(codes(&with("let x : Thickness = i")), vec!["E0102"]);
    }

    #[test]
    fn param_with_wrong_dimension_is_e0101() {
        assert_eq!(codes("param t : Thickness = 2 s"), vec!["E0101"]);
    }

    #[test]
    fn refinement_needs_equal_dimensions() {
        assert_eq!(
            codes("param t : Thickness = 0.9 m where t > 0 m"),
            Vec::<&str>::new()
        );
        assert_eq!(
            codes("param t : Thickness = 0.9 m where t > 0 s"),
            vec!["E0101"]
        );
    }
}
