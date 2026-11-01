use diagnostics::Diagnostic;

pub mod checker;
pub mod dimension;
pub mod signature;
pub mod types;

use checker::Analysis;
use signature::Signature;

/// The standard prelude (`prelude/std.fcd`), built into the program so it
/// also works where there is no file system (the browser, M5).
pub const PRELUDE: &str = include_str!("../../../prelude/std.fcd");

/// Σ built from the prelude. The prelude has no errors (tested in `signature`).
pub fn prelude_signature() -> Signature {
    let (program, _) = syntax::parser::parse(PRELUDE);
    let mut sig = Signature::default();
    let mut diags = Vec::new();
    for decl in &program.decls {
        sig.declare(decl, &mut diags);
    }
    sig
}

/// Parse and type-check a program: its errors, and the types and values of
/// its names. With syntax errors, type checking is skipped: a half-parsed
/// program would only produce confusing follow-up errors.
pub fn analyse(source: &str) -> Analysis {
    let (program, syntax_errors) = syntax::parser::parse(source);
    if !syntax_errors.is_empty() {
        return Analysis {
            diagnostics: syntax_errors,
            bindings: Vec::new(),
        };
    }
    checker::check_program(prelude_signature(), &program)
}

/// Check a whole FacadeLang program. An empty result means it is accepted.
pub fn check(source: &str) -> Vec<Diagnostic> {
    analyse(source).diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_program_is_accepted() {
        assert!(check("").is_empty());
    }

    #[test]
    fn syntax_errors_are_reported() {
        let codes: Vec<&str> = check("param x = 3").iter().map(|d| d.code).collect();
        assert_eq!(codes, vec!["E0002"]);
    }
}
