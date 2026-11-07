# FacadeLang

**A typed language for building design: swapping two numbers in a façade program becomes a compile-time error, not a wrong building.**

[**Try it in the browser →**](https://rayaneaboud.dev/facade-lang/) (the checker runs as WebAssembly, with a live 3D preview)

FacadeLang is a research prototype in Rust. It explores one idea from *Programming Languages for the Future of
Design Computation* (Aish, Fisher, Orchard, Torry, Onward! '24): **types can encode what may be built**. Units,
kinds of quantity and tolerances turn type checking into a static feasibility check on a design, before any
simulation or construction.

## The bug it catches

In visual design tools like Dynamo, a façade panel takes three unnamed number sliders: inset, offset, thickness.
Pass the inset where the thickness should go and the program runs silently and builds the wrong panel (paper, Fig. 5).

```fcd
-- expect: E0102
param inset     : Inset     = -1.0 m
param thickness : Thickness =  0.9 m  where thickness > 0 m

let panel = thicken(mid, inset)   -- error E0102: expected Thickness, found Inset
```

Both values are lengths, so a unit checker alone would accept this. FacadeLang also checks the **kind** of quantity
(`Inset`, `Thickness`, `BayWidth`...), which programs can declare for their own domain.

## What a program looks like

```fcd
kind BayWidth    : Length
kind FloorHeight : Length

param bays   : Count       = 8
param bay    : BayWidth    = 1.5 m
param storey : FloorHeight = 3.6 m
param pane   : PaneThickness = 24 mm   where pane > 6 mm and pane < 60 mm

let facade     = grid(cell, bays, floors, bay, storey)
let glass_area = area(vision) * bays * floors        -- inferred: Length^2
let glass_mass = glass_area * pane * 2500 [kg/m^3]   -- inferred: Mass
```

Full examples: [`examples/ok`](examples/ok) (a curtain wall, an office building, a twisting tower) and
[`examples/err`](examples/err) (one bug per file, with its expected error code).

## What is built

| Part | What it does |
|---|---|
| Lexer and parser | Hand-written, with error recovery so one run reports every error; readable messages with [ariadne](https://github.com/zesterer/ariadne) |
| Type checker | Dimensions as an abelian group, units, user-defined kinds with subtyping, inferred types for every `let`. Tolerances and refinements (`where pane > 6 mm`) are parsed; checking them is the next milestone |
| `facadec` CLI | `facadec check file.fcd`, with exit codes scripts can rely on (0 accepted, 1 rejected, 2 usage error) |
| WASM playground | Editor with live diagnostics, inferred types and a 3D preview of the design, deployed by CI to GitHub Pages |
| VS Code extension | Syntax highlighting and unit completion for `.fcd` files |
| Docs | [Grammar (EBNF)](docs/grammar.md), [typing rules for dimensions](docs/feature-1-dimensions.md), [error codes](docs/errors.md), [architecture](docs/architecture.md) |

## Architecture

```
source ──► tokens ──► AST ──► typed result
        lexer     parser    type checker
        └─ syntax ──┘      └─ semantics ─┘
```

A Cargo workspace whose crate boundaries enforce the layering: `diagnostics` ← `syntax` ← `semantics` ← `cli` / `wasm`.
The core does no I/O; the CLI, the browser playground and the editor extension are thin adapters around it.
Design decisions and the alternatives not taken are written down in [docs/architecture.md](docs/architecture.md).

## Run it

```bash
cargo run --bin facadec -- check examples/ok/facade.fcd     # accepted
cargo run --bin facadec -- check examples/err/fig5_swap.fcd # rejected with E0102
cargo test --workspace
```

The toolchain is pinned in `rust-toolchain.toml`. CI runs `cargo fmt --check`, `cargo clippy -D warnings` and the
tests on every push, including a corpus test over every file in `examples/`.

## Status

| Milestone | Status |
|---|---|
| M0 workspace, CLI, CI | done |
| M1 lexer, parser, error messages, editor support | done |
| M2 dimensions, units and kinds (Feature 1) | done |
| M5 WASM playground with 3D preview | done |
| M3 tolerances and refinements, M4 construction plans (typestate) | next |

## License

MIT or Apache-2.0.
