use serde::Serialize;
use wasm_bindgen::prelude::*;

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

#[derive(Serialize)]
struct JsBinding {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    /// The SI value, or `null` when it is not known (geometry, calls).
    value: Option<f64>,
}

#[derive(Serialize)]
struct JsAnalysis {
    diagnostics: Vec<JsDiagnostic>,
    bindings: Vec<JsBinding>,
}

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
    };
    serde_json::to_string(&result).expect("plain data always converts to JSON")
}

#[wasm_bindgen]
pub fn prelude() -> String {
    semantics::PRELUDE.to_string()
}

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
    use serde_json::Value;

    const FIG5: &str = include_str!("../../../examples/ok/fig5_panel.fcd");
    const SWAP: &str = include_str!("../../../examples/err/fig5_swap.fcd");

    fn json(source: &str) -> Value {
        serde_json::from_str(&check(source)).expect("check returns valid JSON")
    }

    fn binding<'a>(result: &'a Value, name: &str) -> &'a Value {
        let bindings = result["bindings"].as_array().expect("bindings is a list");
        bindings
            .iter()
            .find(|b| b["name"] == name)
            .expect("declared")
    }

    #[test]
    fn fig5_has_no_errors_and_inferred_types() {
        let result = json(FIG5);
        assert_eq!(result["diagnostics"], serde_json::json!([]));
        assert_eq!(binding(&result, "vol")["type"], "Length^3");
        assert_eq!(binding(&result, "vol")["value"], Value::Null);
        assert_eq!(binding(&result, "thickness")["value"], 0.9);
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
}
