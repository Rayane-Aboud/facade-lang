# FacadeLang error codes

Every diagnostic has a stable code. Tests match on the code, not on the
message text, so messages can improve without breaking tests.

| Range | Pass | Milestone |
|-------|------|-----------|
| E00xx | syntax (lexer, parser) | M1 |
| E01xx | Feature 1: dimensions, units, kinds | M2 |
| E02xx | Feature 2: tolerances, refinements | M3 |
| E03xx | Feature 3: construction plans | M4 |

## Syntax errors

| Code | Meaning | Example | Fix |
|------|---------|---------|-----|
| E0001 | unexpected character | `let a = 1 @` | remove it; names are ASCII only (write `um`, not `µm`) |
| E0002 | expected *X*, found *Y* | `param offset = 0.0 m` | add what is expected: here the type, `param offset : Length = 0.0 m` |
| E0003 | a power must be a whole number | `s^2.5` | dimensions need integer exponents |
| E0004 | number too large | `1e999 m` | use a smaller number or a larger unit |
| E0005 | only one tolerance per expression | `x +/- 1 mm +/- 2 mm` | combine in brackets: `(x +/- 1 mm) + (y +/- 2 mm)` |

One mistake can cause a second, follow-up error (a *cascading error*): in
`let a = 1 @ 2`, the lexer drops `@` (E0001), and the parser then finds a
lone `2` (E0002). Fix the first error first.
