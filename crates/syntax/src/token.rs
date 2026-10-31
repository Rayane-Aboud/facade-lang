use diagnostics::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords: quantities (Feature 1)
    /// What sort of physical thing a value is: `dimension Area = Length^2`.
    Dimension,
    /// How a dimension is measured: `unit mm = 0.001 m`.
    Unit,
    /// What a value means, as a refinement of a dimension: `kind Thickness : Length`.
    Kind,

    // Keywords: declarations
    /// A built-in function, declared by its type only:
    /// `extern thicken(Surface, Thickness) -> Solid`.
    Extern,
    /// A design input, like a slider: `param thickness : Thickness = 0.9 m`.
    Param,
    /// A computed value: `let vol = area(mid) * thickness`.
    Let,

    // Keywords: refinements and tolerances (Feature 2)
    /// Starts a condition on a value: `where thickness > 0 m`.
    Where,
    /// Joins conditions: `where t > 0 m and t < 2 m`.
    And,
    /// A value with a tolerance (an interval): `Tol<Length, 2.5 mm>`.
    Tol,

    // Keywords: construction (Feature 3)
    /// A physical part, installed exactly once: `component f1 : Frame`.
    Component,
    /// An ordered construction sequence: `plan assembly { … }`.
    Plan,
    /// One construction step: `install f1;`.
    Install,
    /// What a component is attached to: `install g1 on f1;`.
    On,

    // Brackets: ( ) [ ] { } < >
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Lt,
    Gt,

    // Punctuation: , : ;
    Comma,
    Colon,
    Semi,

    // Operators: = + - * / ^ -> <= >= == and ± (also written +/-)
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Arrow,
    Le,
    Ge,
    EqEq,
    PlusMinus,

    // Everything else
    Ident,
    Number,

    /// End of input. The lexer always emits exactly one, as the last token.
    Eof,

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}


pub fn keyword(word: &str) -> Option<TokenKind> {
    use TokenKind::*;
    match word {
        "dimension" => Some(Dimension),
        "unit" => Some(Unit),
        "kind" => Some(Kind),
        "extern" => Some(Extern),
        "param" => Some(Param),
        "let" => Some(Let),
        "where" => Some(Where),
        "and" => Some(And),
        "component" => Some(Component),
        "plan" => Some(Plan),
        "install" => Some(Install),
        "on" => Some(On),
        "Tol" => Some(Tol),
        _ => None,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_recognises_every_keyword() {
        let cases = [
            ("dimension", TokenKind::Dimension),
            ("unit", TokenKind::Unit),
            ("kind", TokenKind::Kind),
            ("extern", TokenKind::Extern),
            ("param", TokenKind::Param),
            ("let", TokenKind::Let),
            ("where", TokenKind::Where),
            ("and", TokenKind::And),
            ("component", TokenKind::Component),
            ("plan", TokenKind::Plan),
            ("install", TokenKind::Install),
            ("on", TokenKind::On),
            ("Tol", TokenKind::Tol),
        ];
        for (word, kind) in cases {
            assert_eq!(keyword(word), Some(kind), "word: {word}");
        }
    }

    #[test]
    fn keyword_rejects_names_and_wrong_case() {
        for word in ["kinds", "Kind", "units", "tol", "thickness", ""] {
            assert_eq!(keyword(word), None, "word: {word}");
        }
    }
}
