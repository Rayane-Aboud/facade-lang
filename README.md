# FacadeLang

FacadeLang is a small typed language for building design, written in Rust. The idea is simple: if you
pass a thickness where an inset was expected, you should get an error before anything is built, not a
wrong building at the end.

You can try it in the browser here: [rayaneaboud.dev/facade-lang](https://rayaneaboud.dev/facade-lang/).
The checker runs as WebAssembly and shows a 3D preview of the design while you type.

I built it as a research prototype. It follows an idea from the paper *Programming Languages for the
Future of Design Computation* (Aish, Fisher, Orchard, Torry, Onward! '24): types can describe what is
allowed to be built. With units, kinds of quantity and tolerances, the type checker can reject a design
that makes no sense, before any simulation or construction.

## The bug it catches

In visual tools like Dynamo, a façade panel often takes three number sliders: inset, offset and
thickness. If you plug the inset where the thickness should go, the program still runs and builds the
wrong panel (Fig. 5 in the paper). In FacadeLang the same mistake is a compile-time error:

```fcd
-- expect: E0102
param inset     : Inset     = -1.0 m
param thickness : Thickness =  0.9 m  where thickness > 0 m

let panel = thicken(mid, inset)   -- error E0102: expected Thickness, found Inset
```

Both values are lengths, so checking units alone would not be enough. FacadeLang also checks the kind of
quantity (`Inset`, `Thickness`, `BayWidth`...), and a program can declare its own kinds.

## What a program looks like

```fcd
kind BayWidth    : Length
kind FloorHeight : Length

param bays   : Count         = 8
param bay    : BayWidth      = 1.5 m
param storey : FloorHeight   = 3.6 m
param pane   : PaneThickness = 24 mm   where pane > 6 mm and pane < 60 mm

let facade     = grid(cell, bays, floors, bay, storey)
let glass_area = area(vision) * bays * floors        -- inferred: Length^2
let glass_mass = glass_area * pane * 2500 [kg/m^3]   -- inferred: Mass
```

There are more examples in [`examples/ok`](examples/ok) (a curtain wall, an office building, a twisting
tower) and in [`examples/err`](examples/err), where each file contains one bug and the error it should give.

## What I built

- A hand-written lexer and parser with error recovery, so one run reports every error. The messages are
  printed with [ariadne](https://github.com/zesterer/ariadne) to make them easy to read.
- A type checker for dimensions, units and user-defined kinds with subtyping. It also infers the type of
  every `let`. Tolerances and refinements (like `where pane > 6 mm`) are parsed, and checking them is the
  next milestone.
- A command-line tool, `facadec check file.fcd`, with exit codes that scripts can rely on (0 accepted,
  1 rejected, 2 usage error).
- The browser playground, built from the same checker compiled to WebAssembly and deployed by CI.
- A VS Code extension with syntax highlighting and unit completion.
- Documentation: the [grammar](docs/grammar.md), the [typing rules for dimensions](docs/feature-1-dimensions.md),
  the [error codes](docs/errors.md) and the [architecture](docs/architecture.md).

## How the code is organised

```
source ──► tokens ──► AST ──► typed result
        lexer     parser    type checker
        └─ syntax ──┘      └─ semantics ─┘
```

The project is a Cargo workspace. The crates only depend downwards (`diagnostics` ← `syntax` ←
`semantics` ← `cli` / `wasm`), so the layering is enforced by the compiler. The core does no I/O, and the
command line, the playground and the editor extension are thin layers around it. I wrote down each design
decision, and the alternative I didn't take, in [docs/architecture.md](docs/architecture.md).

## Running it

```bash
cargo run --bin facadec -- check examples/ok/facade.fcd     # accepted
cargo run --bin facadec -- check examples/err/fig5_swap.fcd # rejected with E0102
cargo test --workspace
```

The Rust version is pinned in `rust-toolchain.toml`. On every push, CI runs `cargo fmt --check`,
`cargo clippy` with warnings as errors, and all the tests, including one that checks every file in
`examples/`.

## Status

- Done: workspace, CLI and CI; lexer, parser, error messages and editor support; dimensions, units and
  kinds; the WebAssembly playground with 3D preview.
- Next: tolerances and refinements, then construction plans (checking that building steps happen in a
  valid order).

## License

MIT or Apache-2.0.
