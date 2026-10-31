# FacadeLang architecture

This page explains how the code is organised and why. It is updated whenever
the structure changes.

## 1. The pipeline

FacadeLang is checked by a pipeline of passes. Each pass is a function with a
clear input and output type, so passes can be tested on their own.

```
source text ──► tokens ──► AST ──► resolved AST ──► typed result
             lexer     parser   name resolution   type checker
             └──── syntax ────┘ └────────── semantics ──────────┘
```

| Pass            | Crate       | Status              |
|-----------------|-------------|---------------------|
| lexer, parser   | `syntax`    | M1                  |
| name resolution | `semantics` | M2                  |
| type checker    | `semantics` | M2 (F1), M3 (F2), M4 (F3) |

Today `semantics::check` is the single entry point and accepts every program.

Every pass reports problems as `Diagnostic` values (code, message, span) and
keeps going, so one run reports all errors (*error accumulation*).

## 2. Crates and dependencies

```
cli ──► semantics ──► syntax ──► diagnostics
  └─────────┴──────────┴──────────►┘
```

Arrows mean "depends on". They only point down: a lower crate can never call
a higher one. Cargo rejects dependency cycles, so this rule is enforced by
the build, not just by convention.

- **`diagnostics`**: `Span` (byte offsets `start..end`) and `Diagnostic`.
  It is at the bottom so the parser and the checker can both report errors
  without depending on each other.
- **`syntax`**: lexer, parser, AST. No I/O: takes `&str`, returns data.
- **`semantics`**: name resolution and the three features. No I/O.
- **`cli`**: the `facadec` binary. Reads files, calls `semantics::check`,
  prints results.

## 3. Ports and adapters

The core (`syntax`, `semantics`, `diagnostics`) does no I/O. Everything that
touches the outside world is a thin *adapter* around the same core:

- `cli`: files and the terminal (now);
- `editors/vscode`: highlighting and unit completion, reading the `unit`
  declarations in `prelude/std.fcd` (now);
- `wasm`: the browser playground (M5);
- the JSON AST export: a process boundary for the Haskell reference
  checker (M6) and other tools, with no FFI.

A new front end (an LSP server, a Dynamo importer) is a new adapter; the
checker does not change.

### `facadec` exit codes

| Code | Meaning                         |
|------|---------------------------------|
| 0    | program accepted                |
| 1    | program rejected (diagnostics)  |
| 2    | usage error or file not readable |

Scripts (including the M6 differential tests) can tell "the design is wrong"
apart from "the tool was called wrongly".

## 4. Tests

| Kind              | Where                        | What it checks                       |
|-------------------|------------------------------|--------------------------------------|
| unit tests        | `mod tests` in `src/*.rs`    | one module from the inside           |
| integration tests | `crates/cli/tests/cli.rs`    | the real `facadec` binary, end to end |
| example corpus    | `examples/ok`, `examples/err`| planned in M1, see below             |

The CLI tests only check the adapter's own job (arguments, file reading,
exit codes, output). All checking logic is tested in the library crates.

**Planned in M1:** one data-driven test in `semantics` runs `check` on every
file in `examples/`. Files in `ok/` must give no diagnostics; each file in
`err/` has exactly one bug and states its expected error code in a comment
on its first line, e.g. `-- expect: E0102`. Adding a test case is then adding
a file.

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy` with
warnings as errors, and `cargo test` on every push and pull request.

## 5. Decisions

| Decision | Why | Alternative not taken |
|----------|-----|-----------------------|
| Cargo workspace with several crates | Crate boundaries make the layering real: the compiler enforces it | One crate with modules: simpler, but nothing stops one layer reaching into another |
| `Span` lives in `diagnostics` | Both parser and checker need it to report errors | `Span` in `syntax`: `semantics` would depend on the parser just to report errors |
| Toolchain pinned in `rust-toolchain.toml` | Same compiler locally and in CI | Unpinned: "works on my machine" problems |
| No external crates in M0 | Nothing needs them yet | `ariadne`, `chumsky`, `insta` arrive in M1–M2, when they have a use |
| CLI tests use `std::process::Command` | Tests the real binary; no extra dependency | `assert_cmd` or `trycmd`: nicer, but not needed for a handful of tests |
| File extension `.fcd` | Short, no clash we know of | `.fl` (used by FLTK's FLUID), `.facade` (long) |
| Binary named `facadec` | Compiler naming convention (`rustc`, `ghc`, `javac`) | `facade`, `fl` |
| CI without a build cache | No external dependencies yet, builds take seconds | `Swatinem/rust-cache`: add when builds get slow |
