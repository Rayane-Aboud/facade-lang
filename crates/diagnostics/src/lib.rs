use std::fmt;
use std::ops::Range;

pub type Span = Range<usize>;

#[derive(Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error[{}]: {} (at bytes {}..{})",
            self.code, self.message, self.span.start, self.span.end
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_shows_code_message_and_span() {
        let d = Diagnostic {
            code: "E0102",
            message: "kind mismatch".to_string(),
            span: 10..15,
        };
        assert_eq!(
            d.to_string(),
            "error[E0102]: kind mismatch (at bytes 10..15)"
        );
    }
}
