//! The browser adapter (M5): the checker compiled to WebAssembly.
//!
//! JavaScript calls `check(source)` and gets one JSON string back: the
//! errors, the types of the names, and the dataflow steps the page uses to
//! render the geometry. Like the CLI, this crate only translates; all
//! checking is done by `semantics`.

use semantics::signature::Signature;
use serde::Serialize;
use syntax::ast::{DeclKind, Expr, ExprKind};
use wasm_bindgen::prelude::*;

/// One error, in the form the web page needs.
#[derive(Serialize)]
struct JsDiagnostic {
    code: &'static str,
    message: String,
    /// Where to underline in the editor. Lines and columns count from 1;
    /// columns count UTF-16 units, as JavaScript strings do.
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    /// The same text as the CLI prints, without colours.
    rendered: String,
}

/// One entry of the final environment Γ.
#[derive(Serialize)]
struct JsBinding {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    /// The SI value, or `null` when it is not known (geometry, calls).
    value: Option<f64>,
}

/// One step of the dataflow: `let name = call(args)`.
#[derive(Serialize)]
struct JsStep {
    name: String,
    call: String,
    args: Vec<JsArg>,
}

/// An argument: the name of an earlier value, or a constant in SI.
/// In JSON: `{"name": "mid"}` or `{"value": 0.03}`.
#[derive(Serialize)]
#[serde(untagged)]
enum JsArg {
    Name { name: String },
    Value { value: Option<f64> },
}

#[derive(Serialize)]
struct JsAnalysis {
    diagnostics: Vec<JsDiagnostic>,
    bindings: Vec<JsBinding>,
    steps: Vec<JsStep>,
}

/// Check a program. Returns its errors, the types of its names and its
/// dataflow steps, as JSON.
#[wasm_bindgen]
pub fn check(source: &str) -> String {
    let analysis = semantics::analyse(source);
    let diagnostics = analysis
        .diagnostics
        .iter()
        .map(|d| {
            let (line, column) = position(source, d.span.start);
            let (end_line, end_column) = position(source, d.span.end);
            JsDiagnostic {
                code: d.code,
                message: d.message.clone(),
                line,
                column,
                end_line,
                end_column,
                rendered: diagnostics::render(d, "main.fcd", source, false),
            }
        })
        .collect();
    let bindings = analysis
        .bindings
        .iter()
        .map(|b| JsBinding {
            name: b.name.clone(),
            ty: b.ty.to_string(),
            value: b.value,
        })
        .collect();
    let result = JsAnalysis {
        diagnostics,
        bindings,
        steps: steps(source),
    };
    serde_json::to_string(&result).expect("plain data always converts to JSON")
}

/// The text of the standard prelude, for the docs section of the page.
#[wasm_bindgen]
pub fn prelude() -> String {
    semantics::PRELUDE.to_string()
}

/// The `let x = f(...)` steps of the program, read from the syntax tree, for
/// the page's geometry preview. Type errors do not stop them: the preview
/// shows what an untyped tool such as Dynamo would build.
fn steps(source: &str) -> Vec<JsStep> {
    let (program, syntax_errors) = syntax::parser::parse(source);
    if !syntax_errors.is_empty() {
        return Vec::new();
    }
    // Σ with the program's own units too, to read literals such as `1 inch`.
    let mut sig = semantics::prelude_signature();
    let mut ignored = Vec::new();
    for decl in &program.decls {
        sig.declare(decl, &mut ignored);
    }
    program
        .decls
        .iter()
        .filter_map(|decl| match &decl.kind {
            DeclKind::Let {
                name,
                value:
                    Expr {
                        kind: ExprKind::Call { func, args },
                        ..
                    },
                ..
            } => Some(JsStep {
                name: name.text.clone(),
                call: func.text.clone(),
                args: args.iter().map(|a| argument(a, &sig)).collect(),
            }),
            _ => None,
        })
        .collect()
}

fn argument(e: &Expr, sig: &Signature) -> JsArg {
    match &e.kind {
        ExprKind::Name(n) => JsArg::Name {
            name: n.text.clone(),
        },
        _ => JsArg::Value {
            value: constant(e, sig),
        },
    }
}

/// The SI value of a literal such as `30 mm` or `-1.0 m`; `None` otherwise.
fn constant(e: &Expr, sig: &Signature) -> Option<f64> {
    match &e.kind {
        ExprKind::Quantity(q) => sig.quantity(q, &mut Vec::new()).map(|(_, v)| v),
        ExprKind::Neg(inner) => constant(inner, sig).map(|v| -v),
        _ => None,
    }
}

/// Line and column (both from 1) of a byte offset. Spans count bytes, but
/// JavaScript counts UTF-16 units, so a `±` before an error (2 bytes, 1 unit)
/// must not move the underline.
fn position(source: &str, byte: usize) -> (usize, usize) {
    let mut byte = byte.min(source.len());
    while !source.is_char_boundary(byte) {
        byte -= 1;
    }
    let before = &source[..byte];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.matches('\n').count() + 1;
    let column = before[line_start..].encode_utf16().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const FIG5: &str = include_str!("../../../examples/ok/fig5_panel.fcd");
    const SWAP: &str = include_str!("../../../examples/err/fig5_swap.fcd");

    fn json(source: &str) -> Value {
        serde_json::from_str(&check(source)).expect("check returns valid JSON")
    }

    /// The entry called `name` in the list `list` ("bindings" or "steps").
    fn find<'a>(result: &'a Value, list: &str, name: &str) -> &'a Value {
        let entries = result[list].as_array().expect("a list");
        entries.iter().find(|e| e["name"] == name).expect("present")
    }

    #[test]
    fn fig5_has_no_errors_and_inferred_types() {
        let result = json(FIG5);
        assert_eq!(result["diagnostics"], json!([]));
        assert_eq!(find(&result, "bindings", "vol")["type"], "Length^3");
        assert_eq!(find(&result, "bindings", "vol")["value"], Value::Null);
        assert_eq!(find(&result, "bindings", "thickness")["value"], 0.9);
    }

    #[test]
    fn swap_error_points_at_inset() {
        let result = json(SWAP);
        let d = &result["diagnostics"][0];
        assert_eq!(d["code"], "E0102");
        assert_eq!(d["line"], 13);
        assert_eq!(d["column"], 26);
        assert_eq!(d["end_column"], 31);
        assert!(
            d["rendered"]
                .as_str()
                .unwrap()
                .contains("thicken(mid, inset)")
        );
    }

    #[test]
    fn column_counts_characters_not_bytes() {
        // `±` is 2 bytes in UTF-8 but 1 unit in JavaScript.
        let result = json("let a = 1 m ± 1 mm + zz");
        let d = &result["diagnostics"][0];
        assert_eq!(d["code"], "E0103");
        assert_eq!(d["column"], 22);
    }

    #[test]
    fn syntax_errors_have_positions_too() {
        let result = json("param x = 3");
        assert_eq!(result["diagnostics"][0]["code"], "E0002");
        assert_eq!(result["diagnostics"][0]["line"], 1);
    }

    #[test]
    fn prelude_is_exposed() {
        assert!(prelude().contains("kind Thickness"));
    }

    // Dataflow steps

    #[test]
    fn steps_are_the_dataflow_of_fig5() {
        let result = json(FIG5);
        let names: Vec<&str> = result["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        // `vol = area(mid) * thickness` is arithmetic, not a call: not a step.
        assert_eq!(names, ["curve", "surf", "mid", "panel"]);
        let panel = find(&result, "steps", "panel");
        assert_eq!(panel["call"], "thicken");
        assert_eq!(
            panel["args"],
            json!([{"name": "mid"}, {"name": "thickness"}])
        );
    }

    #[test]
    fn steps_are_kept_when_types_are_wrong() {
        // The preview shows what an untyped tool would build.
        let result = json(SWAP);
        assert_eq!(find(&result, "steps", "panel")["args"][1]["name"], "inset");
    }

    #[test]
    fn literal_arguments_are_in_si() {
        let source = "unit inch = 25.4 mm\nparam s : Surface\nlet p = thicken(s, 1 inch)";
        let value = json(source)["steps"][0]["args"][1]["value"]
            .as_f64()
            .unwrap();
        assert!((value - 0.0254).abs() < 1e-12);
    }

    #[test]
    fn no_steps_after_a_syntax_error() {
        assert_eq!(json("let p = thicken(")["steps"], json!([]));
    }
}
