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
