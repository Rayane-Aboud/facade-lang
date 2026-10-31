use ariadne::{Config, IndexType, Label, Report, ReportKind, Source};
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

pub fn render(diagnostic: &Diagnostic, file_name: &str, source: &str, color: bool) -> String {
    let span = (file_name, diagnostic.span.clone());
    // Our spans count bytes; ariadne counts characters unless told otherwise.
    let config = Config::default()
        .with_color(color)
        .with_index_type(IndexType::Byte);
    let mut out = Vec::new();
    Report::build(ReportKind::Error, span.clone())
        .with_config(config)
        .with_code(diagnostic.code)
        .with_message(&diagnostic.message)
        .with_label(Label::new(span).with_message(&diagnostic.message))
        .finish()
        .write((file_name, Source::from(source)), &mut out)
        .expect("writing to a Vec cannot fail");
    String::from_utf8(out).expect("ariadne writes UTF-8")
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
