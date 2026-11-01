use diagnostics::Diagnostic;

pub mod dimension;

/// Check a whole FacadeLang program. An empty result means it is accepted.
///
/// For now this only parses (syntax errors); the type checker comes in M2.
pub fn check(source: &str) -> Vec<Diagnostic> {
    let (_program, diagnostics) = syntax::parser::parse(source);
    diagnostics
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
