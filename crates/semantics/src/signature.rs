//! The signature Σ (`docs/feature-1-dimensions.md`, sections 2 and 3.1): the
//! global declarations `dimension`, `unit`, `kind` and `extern`, and the
//! relations on types that use them: `dim`, subtyping `<:` and join `⊔`.

use std::collections::{HashMap, HashSet};

use diagnostics::{Diagnostic, Span};
use syntax::ast::{Decl, DeclKind, Name, Product, ProductKind, Quantity, Type, TypeKind, UnitDef};

use crate::dimension::Dimension;
use crate::types::{GEOMETRY, Ty};

/// A unit `(d, s)`: a dimension and its scale to SI (section 2.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub dimension: Dimension,
    pub scale: f64,
}

/// The type of an `extern` function: `(σ₁, …, σₙ) → σ`.
#[derive(Debug, Clone, PartialEq)]
pub struct Extern {
    pub params: Vec<Ty>,
    pub result: Ty,
}

#[derive(Debug, Default)]
pub struct Signature {
    dimensions: HashMap<String, Dimension>,
    units: HashMap<String, Unit>,
    /// Base dimensions that already have their base unit (at most one each).
    has_base_unit: HashSet<String>,
    /// Each kind and its parent: another kind, or a plain `Q(d)`.
    kinds: HashMap<String, Ty>,
    externs: HashMap<String, Extern>,
}

impl Signature {
    /// Add one declaration to Σ. Value declarations (`param`, `let`) and
    /// Feature 3 (`component`, `plan`) are not part of Σ and are ignored.
    pub fn declare(&mut self, decl: &Decl, diags: &mut Vec<Diagnostic>) {
        match &decl.kind {
            DeclKind::Dimension { name, definition } => {
                if !self.is_new_type_name(name, diags) {
                    return;
                }
                let dimension = match definition {
                    None => Some(Dimension::base(&name.text)),
                    Some(p) => self.product_dimension(p, diags),
                };
                if let Some(d) = dimension {
                    self.dimensions.insert(name.text.clone(), d);
                }
            }
            DeclKind::Unit { name, definition } => self.declare_unit(name, definition, diags),
            DeclKind::Kind { name, parent } => {
                if !self.is_new_type_name(name, diags) {
                    return;
                }
                if let Some(parent) = self.kind_parent(parent, diags) {
                    self.kinds.insert(name.text.clone(), parent);
                }
            }
            DeclKind::Extern {
                name,
                params,
                result,
            } => {
                if self.externs.contains_key(&name.text) {
                    duplicate(name, diags);
                    return;
                }
                let params: Vec<Option<Ty>> =
                    params.iter().map(|t| self.resolve_type(t, diags)).collect();
                let result = self.resolve_type(result, diags);
                // Declared only if every type is known (errors already reported).
                if let (Some(params), Some(result)) = (params.into_iter().collect(), result) {
                    self.externs
                        .insert(name.text.clone(), Extern { params, result });
                }
            }
            DeclKind::Param { .. }
            | DeclKind::Let { .. }
            | DeclKind::Component { .. }
            | DeclKind::Plan { .. } => {}
        }
    }

    fn declare_unit(&mut self, name: &Name, definition: &UnitDef, diags: &mut Vec<Diagnostic>) {
        if self.units.contains_key(&name.text) {
            duplicate(name, diags);
            return;
        }
        let unit = match definition {
            // `unit m : Length`: the base unit of a base dimension, scale 1.
            UnitDef::Base(dim) => {
                let Some(d) = self.dimensions.get(&dim.text).cloned() else {
                    let message = format!("unknown dimension `{}`", dim.text);
                    error(diags, "E0103", message, dim.span.clone());
                    return;
                };
                if d != Dimension::base(&dim.text) {
                    let message = format!(
                        "`{}` is a derived dimension; a base unit needs a base dimension",
                        dim.text
                    );
                    error(diags, "E0109", message, dim.span.clone());
                    return;
                }
                if !self.has_base_unit.insert(dim.text.clone()) {
                    let message = format!("`{}` already has a base unit", dim.text);
                    error(diags, "E0109", message, name.span.clone());
                    return;
                }
                Unit {
                    dimension: d,
                    scale: 1.0,
                }
            }
            // `unit mm = 0.001 m`: the dimension and SI value of the quantity.
            UnitDef::Scaled(q) => {
                let Some((dimension, scale)) = self.quantity(q, diags) else {
                    return;
                };
                if scale <= 0.0 {
                    let message = "a unit must have a positive scale".to_string();
                    error(diags, "E0109", message, q.span.clone());
                    return;
                }
                Unit { dimension, scale }
            }
        };
        self.units.insert(name.text.clone(), unit);
    }

    /// Dimensions, kinds and geometry types share one namespace.
    fn is_new_type_name(&self, name: &Name, diags: &mut Vec<Diagnostic>) -> bool {
        let taken = self.dimensions.contains_key(&name.text)
            || self.kinds.contains_key(&name.text)
            || GEOMETRY.contains(&name.text.as_str());
        if taken {
            duplicate(name, diags);
        }
        !taken
    }

    /// The parent of a kind (section 2.4): another kind, or the plain
    /// quantity of a dimension.
    fn kind_parent(&self, parent: &Product, diags: &mut Vec<Diagnostic>) -> Option<Ty> {
        match &parent.kind {
            ProductKind::Atom { name, power: 1 } if self.kinds.contains_key(&name.text) => {
                Some(Ty::Kind(name.text.clone()))
            }
            _ => self.product_dimension(parent, diags).map(Ty::Quantity),
        }
    }

    /// The meaning `⟦T⟧` of a type written in the source (section 3.2).
    pub fn resolve_type(&self, ty: &Type, diags: &mut Vec<Diagnostic>) -> Option<Ty> {
        match &ty.kind {
            TypeKind::Product(p) => match &p.kind {
                ProductKind::Atom { name, power: 1 } if GEOMETRY.contains(&name.text.as_str()) => {
                    Some(Ty::Geometry(name.text.clone()))
                }
                ProductKind::Atom { name, power: 1 } if self.kinds.contains_key(&name.text) => {
                    Some(Ty::Kind(name.text.clone()))
                }
                _ => self.product_dimension(p, diags).map(Ty::Quantity),
            },
            // `Tol<T, q>` means `⟦T⟧`; Feature 1 only checks the dimension of q.
            TypeKind::Tol { inner, tolerance } => {
                let inner_ty = self.resolve_type(inner, diags)?;
                let (tol_dim, _) = self.quantity(tolerance, diags)?;
                match self.dim(&inner_ty) {
                    Some(d) if d == tol_dim => {}
                    Some(d) => {
                        let message = format!(
                            "dimension mismatch: the tolerance is `{tol_dim}`, but the value is `{d}`"
                        );
                        error(diags, "E0101", message, tolerance.span.clone());
                    }
                    None => {
                        let message = format!(
                            "`{inner_ty}` is not a quantity, so it cannot have a tolerance"
                        );
                        error(diags, "E0105", message, inner.span.clone());
                    }
                }
                Some(inner_ty)
            }
        }
    }

    /// The meaning `⟦p⟧` of a dimension expression (section 2.2). A kind
    /// inside a product contributes only its dimension (section 3.2).
    pub fn product_dimension(&self, p: &Product, diags: &mut Vec<Diagnostic>) -> Option<Dimension> {
        match &p.kind {
            ProductKind::Atom { name, power } => {
                let d = match self.dimensions.get(&name.text) {
                    Some(d) => Some(d.clone()),
                    None => self.dim(&Ty::Kind(name.text.clone())),
                };
                match d {
                    Some(d) => Some(d.pow(*power)),
                    None => {
                        let message = format!("unknown dimension `{}`", name.text);
                        error(diags, "E0103", message, name.span.clone());
                        None
                    }
                }
            }
            ProductKind::Mul(a, b) => {
                // Both sides first, so errors in both are reported.
                let a = self.product_dimension(a, diags);
                let b = self.product_dimension(b, diags);
                Some(&a? * &b?)
            }
            ProductKind::Div(a, b) => {
                let a = self.product_dimension(a, diags);
                let b = self.product_dimension(b, diags);
                Some(&a? / &b?)
            }
        }
    }

    /// The meaning of a unit expression (section 2.3): dimensions combine as
    /// in `⟦p⟧`, and scales multiply as numbers.
    pub fn unit(&self, p: &Product, diags: &mut Vec<Diagnostic>) -> Option<Unit> {
        match &p.kind {
            ProductKind::Atom { name, power } => match self.units.get(&name.text) {
                Some(u) => Some(Unit {
                    dimension: u.dimension.pow(*power),
                    scale: u.scale.powi(*power),
                }),
                None => {
                    let message = format!("unknown unit `{}`", name.text);
                    error(diags, "E0104", message, name.span.clone());
                    None
                }
            },
            ProductKind::Mul(a, b) => {
                let a = self.unit(a, diags);
                let b = self.unit(b, diags);
                let (a, b) = (a?, b?);
                Some(Unit {
                    dimension: &a.dimension * &b.dimension,
                    scale: a.scale * b.scale,
                })
            }
            ProductKind::Div(a, b) => {
                let a = self.unit(a, diags);
                let b = self.unit(b, diags);
                let (a, b) = (a?, b?);
                Some(Unit {
                    dimension: &a.dimension / &b.dimension,
                    scale: a.scale / b.scale,
                })
            }
        }
    }

    /// A literal `v u` normalised to SI (section 2.3): its dimension and its
    /// SI value `v · s`. A literal without a unit is dimensionless.
    pub fn quantity(&self, q: &Quantity, diags: &mut Vec<Diagnostic>) -> Option<(Dimension, f64)> {
        match &q.unit {
            None => Some((Dimension::one(), q.value)),
            Some(p) => self
                .unit(p, diags)
                .map(|u| (u.dimension, q.value * u.scale)),
        }
    }

    /// `dim(τ)` (section 2.5). `None` for geometry.
    pub fn dim(&self, ty: &Ty) -> Option<Dimension> {
        match ty {
            Ty::Quantity(d) => Some(d.clone()),
            Ty::Kind(_) => self.dim(&self.parent(ty)?),
            Ty::Geometry(_) => None,
        }
    }

    /// `parent(k)` (section 2.6). Only kinds have a parent.
    pub fn parent(&self, ty: &Ty) -> Option<Ty> {
        match ty {
            Ty::Kind(k) => self.kinds.get(k).cloned(),
            _ => None,
        }
    }

    /// Subtyping `τ₁ <: τ₂` (section 2.6): go up from τ₁ by S-Kind, one parent
    /// at a time, until τ₂ is found (S-Refl, S-Trans) or there is no parent.
    pub fn is_subtype(&self, sub: &Ty, sup: &Ty) -> bool {
        let mut current = Some(sub.clone());
        while let Some(ty) = current {
            if ty == *sup {
                return true;
            }
            current = self.parent(&ty);
        }
        false
    }

    /// The join `τ₁ ⊔ τ₂` (section 2.6): the nearest ancestor of τ₁ that is
    /// also above τ₂. Both must have the same dimension; then the root `Q(d)`
    /// is always a common ancestor.
    pub fn join(&self, a: &Ty, b: &Ty) -> Ty {
        let mut current = a.clone();
        loop {
            if self.is_subtype(b, &current) {
                return current;
            }
            match self.parent(&current) {
                Some(parent) => current = parent,
                None => return current,
            }
        }
    }

    /// The type of an `extern` function, if it is declared.
    pub fn extern_fn(&self, name: &str) -> Option<&Extern> {
        self.externs.get(name)
    }
}

pub(crate) fn error(diags: &mut Vec<Diagnostic>, code: &'static str, message: String, span: Span) {
    diags.push(Diagnostic {
        code,
        message,
        span,
    });
}

fn duplicate(name: &Name, diags: &mut Vec<Diagnostic>) {
    let message = format!("`{}` is already declared", name.text);
    error(diags, "E0107", message, name.span.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Declare everything in `source`; return Σ and the error codes.
    fn declare_all(source: &str) -> (Signature, Vec<&'static str>) {
        let (program, parse_errors) = syntax::parser::parse(source);
        assert_eq!(parse_errors, vec![], "test source must parse");
        let mut sig = Signature::default();
        let mut diags = Vec::new();
        for decl in &program.decls {
            sig.declare(decl, &mut diags);
        }
        (sig, diags.iter().map(|d| d.code).collect())
    }

    fn length() -> Ty {
        Ty::Quantity(Dimension::base("Length"))
    }

    fn kind(name: &str) -> Ty {
        Ty::Kind(name.to_string())
    }

    /// The kind forest of section 2.4.
    const KINDS: &str = "
        dimension Length
        kind Thickness      : Length
        kind Inset          : Length
        kind GlassThickness : Thickness
    ";

    // Dimensions (section 2.2)

    #[test]
    fn prelude_has_no_errors() {
        let (_, codes) = declare_all(crate::PRELUDE);
        assert_eq!(codes, Vec::<&str>::new());
    }

    #[test]
    fn derived_dimensions_are_computed() {
        let (sig, _) = declare_all(crate::PRELUDE);
        assert_eq!(
            sig.dimensions["Force"].to_string(),
            "Length * Mass * Time^-2"
        );
        assert_eq!(
            sig.dimensions["Pressure"].to_string(),
            "Length^-1 * Mass * Time^-2"
        );
    }

    #[test]
    fn unknown_dimension_is_e0103() {
        let (_, codes) = declare_all("dimension Area = Length^2");
        assert_eq!(codes, vec!["E0103"]);
    }

    #[test]
    fn duplicate_type_name_is_e0107() {
        let (_, codes) = declare_all("dimension Length\ndimension Length\nkind Length : Length");
        assert_eq!(codes, vec!["E0107", "E0107"]);
    }

    // Units (section 2.3)

    #[test]
    fn units_are_normalised_to_si() {
        let (sig, _) = declare_all(crate::PRELUDE);
        assert_eq!(sig.units["mm"].scale, 0.001);
        assert_eq!(sig.units["kN"].scale, 1000.0);
        assert_eq!(sig.units["MPa"].scale, 1e6);
        assert_eq!(sig.units["N"].dimension, sig.dimensions["Force"]);
    }

    #[test]
    fn unknown_unit_is_e0104() {
        let (_, codes) = declare_all("dimension Length\nunit mm = 0.001 m");
        assert_eq!(codes, vec!["E0104"]);
    }

    #[test]
    fn base_unit_of_derived_dimension_is_e0109() {
        let (_, codes) =
            declare_all("dimension Length\ndimension Area = Length^2\nunit sqm : Area");
        assert_eq!(codes, vec!["E0109"]);
    }

    #[test]
    fn second_base_unit_is_e0109() {
        let (_, codes) = declare_all("dimension Length\nunit m : Length\nunit metre : Length");
        assert_eq!(codes, vec!["E0109"]);
    }

    // Kinds and subtyping (sections 2.4 and 2.6)

    #[test]
    fn kind_has_dimension_of_its_parent() {
        let (sig, codes) = declare_all(KINDS);
        assert_eq!(codes, Vec::<&str>::new());
        assert_eq!(sig.dim(&kind("GlassThickness")), sig.dim(&length()));
    }

    #[test]
    fn subtyping_table_of_section_2_6() {
        let (sig, _) = declare_all(KINDS);
        let area = Ty::Quantity(Dimension::base("Length").pow(2));
        let curve = Ty::Geometry("Curve".to_string());
        let surface = Ty::Geometry("Surface".to_string());

        assert!(sig.is_subtype(&kind("Thickness"), &length()));
        assert!(sig.is_subtype(&kind("GlassThickness"), &length()));
        assert!(sig.is_subtype(&kind("GlassThickness"), &kind("Thickness")));
        assert!(!sig.is_subtype(&length(), &kind("Thickness")));
        assert!(!sig.is_subtype(&kind("Inset"), &kind("Thickness"))); // the Fig. 5 swap
        assert!(!sig.is_subtype(&length(), &area));
        assert!(!sig.is_subtype(&curve, &surface));
    }

    #[test]
    fn join_is_nearest_common_ancestor() {
        let (sig, _) = declare_all(KINDS);
        assert_eq!(sig.join(&kind("Thickness"), &kind("Inset")), length());
        assert_eq!(
            sig.join(&kind("GlassThickness"), &kind("Thickness")),
            kind("Thickness")
        );
        assert_eq!(
            sig.join(&kind("Thickness"), &kind("GlassThickness")),
            kind("Thickness")
        );
    }

    // Types written in the source (section 3.2)

    #[test]
    fn extern_types_are_resolved() {
        let (sig, _) = declare_all(crate::PRELUDE);
        let thicken = Extern {
            params: vec![Ty::Geometry("Surface".to_string()), kind("Thickness")],
            result: Ty::Geometry("Solid".to_string()),
        };
        assert_eq!(sig.externs["thicken"], thicken);
        assert_eq!(
            sig.externs["area"].result,
            Ty::Quantity(Dimension::base("Length").pow(2))
        );
    }

    #[test]
    fn tolerance_must_have_the_value_dimension() {
        let source = "
            dimension Length
            dimension Time
            unit m : Length
            unit s : Time
            unit mm = 0.001 m
            extern f(Tol<Length, 2.5 mm>) -> Length
            extern g(Tol<Length, 2 s>) -> Length
        ";
        let (sig, codes) = declare_all(source);
        assert_eq!(codes, vec!["E0101"]);
        assert_eq!(sig.externs["f"].params, vec![length()]);
    }
}
