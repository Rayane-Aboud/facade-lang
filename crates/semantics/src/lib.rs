use diagnostics::Diagnostic;

pub fn check(_source: &str) -> Vec<Diagnostic> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_program_is_accepted() {
        assert!(check("").is_empty());
    }

    #[test]
    fn fig5_panel_example_is_accepted() {
        let source = include_str!("../../../examples/ok/fig5_panel.fcd");
        assert_eq!(check(source), vec![]);
    }
}
