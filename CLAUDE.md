# FacadeLang — Project Brief for Claude

## 1. What this project is

FacadeLang is a small, typed domain-specific language for **Design Computation** (architecture and building engineering), written in **Rust**.

It is a research prototype supporting a PhD application (PL4DC studentship, University of Kent, supervisor Dr Dominic Orchard). It builds on the paper:

> Aish, Fisher, Orchard, Torry. *Programming Languages for the Future of Design Computation.* Onward! '24. https://doi.org/10.1145/3689492.3689812

**Thesis line.** Types as an encoding of the *feasible and constructible* design space:

1. **What may be built**: units, kinds of quantity, tolerances and refinements make type checking a *static feasibility check* on designs, before simulation or construction.
2. **How it may be built**: linear / typestate checking rejects invalid construction sequences.

**Language strategy.**
- **Rust is the main implementation.** All milestones M0–M5 are in Rust. It is the engineered tool: fast, runs in the browser via WASM, and can be called from C#/.NET through the C interface (the industry tools — BHoM, Dynamo, Grasshopper, Revit — are C#).
- **Haskell is an optional reference specification** (Milestone 6). A short, readable checker for Feature 1 that mirrors the formal typing rules, run against the Rust checker in differential tests. Haskell is the supervisor's main research language (with Granule), and is excellent for expressing the theory.
- The two languages communicate **through a JSON export of the AST** (a process boundary), never through FFI.
- If time is short, skip Milestone 6: the project is complete without it.

**Motivating bug (paper Fig. 5).** In Dynamo, the façade panel program takes three unnamed number sliders (inset = -1, offset = 0, thickness = 0.9). Swapping inset and thickness runs silently and builds a wrong panel. In FacadeLang, the same swap must be a compile-time error.

## 2. How to work with me (read this first)

I am learning the theory while building. Follow these rules in every session:

1. **One milestone at a time** (see section 6). Never start the next milestone until I confirm the current one is done.
2. **Math before code.** For each feature, first explain the formal model in plain language, then give the typing rules, then a small worked example, and only then write code. Wait for me to say "ok" before coding.
3. **Small steps.** Each step should be small enough to review: one module or one rule at a time. Show me the plan for the step before writing it.
4. **Tests with every step.** No rule without a test. Prefer one passing and one failing example per typing rule.
5. **Explain your choices.** When you pick a crate, a pattern or a data structure, say why in two or three sentences, and name the alternative.
6. **Keep the docs in sync.** Each feature has a file in `docs/` with its formal rules and a proof sketch (section 5). Update it in the same step as the code.
7. **Don't over-engineer.** Use the architecture in section 4, but no abstraction without a current use.
8. **English level:** explain clearly and simply (B2 English). Define every technical term the first time you use it.

## 3. The language (target syntax)

This is a sketch; refine it with me in Milestone 1.

```
-- quantity kinds: Thickness is a Length with a specific meaning
kind Thickness : Length
kind Inset     : Length

-- inputs (the sliders of paper Fig. 5), now named and typed
param inset     : Inset     = -1.0 m
param offset    : Length    =  0.0 m
param thickness : Thickness =  0.9 m  where thickness > 0 m
param frame     : Tol<Length, 2.5 mm> = 10.0 mm

-- the geometric process (pure functions, dataflow)
let curve = offset_curve(base, inset)
let surf  = patch(curve)
let mid   = offset_surface(surf, offset)
let panel = thicken(mid, thickness)        -- thicken : (Surface, Thickness) -> Solid
let vol   = area(mid) * thickness          -- inferred: Length^3

-- construction plan (Feature 3)
component f1 : Frame
component g1 : Glazing
plan assembly {
  install f1;
  install g1 on f1;
}
```

Expected error for the swap `thicken(mid, inset)`:

```
error[E0102]: kind mismatch
  expected `Thickness`, found `Inset`
  both are `Length`, but they mean different things
```

Geometry functions (`offset_curve`, `patch`, ...) are **opaque built-ins** with declared types. We do not implement real geometry: the project is about the type system.

## 4. Architecture

A Rust workspace organised as a **compiler pipeline** with **ports and adapters**.

```
facadelang/
  Cargo.toml                 (workspace)
  crates/
    syntax/       lexer, parser, AST, source spans
    diagnostics/  Diagnostic type, error codes, rendering (ariadne)
    semantics/    name resolution, type checker, the three features
    cli/          `facadec check file.fcd`, `facadec ast --json file.fcd` (M6)
    wasm/         (Milestone 5) browser build of the checker
  haskell-ref/    (Milestone 6) Haskell reference checker for Feature 1 (Cabal project)
  tests/diff/     (Milestone 6) differential tests: Rust vs Haskell results
  docs/
    feature-1-dimensions.md
    feature-2-tolerances.md
    feature-3-construction.md
  examples/
    ok/           programs that must type-check
    err/          programs that must fail, one bug each
```

**Patterns, and why:**

- **Pipeline of passes.** `source -> tokens -> AST -> resolved AST -> typed result`. Each pass is a function with clear input and output types, so passes can be tested, replaced and added (like stages in rustc).
- **Ports and adapters (hexagonal).** `syntax` and `semantics` do no I/O. The CLI, the WASM build and any future LSP or Dynamo importer are thin adapters around the same core.
- **Spans everywhere.** Every AST node carries its source position, so every error can point at the exact code.
- **Error accumulation (diagnostic sink).** Passes collect all errors instead of stopping at the first one, as real compilers do.
- **Separate syntax tree and typed result.** The parser's AST is not mutated by the checker; the checker produces its own typed output. This keeps passes independent.
- **Stable AST interchange format.** The AST can be exported as versioned JSON (`serde`). This is the port that lets other languages (Haskell in M6, later Python or C# tools) consume FacadeLang programs without FFI.
- **Open for extension.** Quantity kinds and built-in functions are declared in data (a registry), not hard-coded in `match` arms, so users can add domain concepts (paper §6.4: "enable user-defined extension of domain concepts").

**Crates to consider** (discuss with me before adding): `chumsky` or `lalrpop` (parsing), `ariadne` (error display), `insta` (snapshot tests of error messages), `proptest` (property-based tests), `wasm-bindgen` (Milestone 5). The `uom` crate is related work for units in Rust; cite it, don't depend on it.

## 5. The three features: formal model

Each feature must be documented in its `docs/` file with: (a) intuition, (b) formal definitions, (c) typing rules in inference-rule notation, (d) a soundness statement with a proof sketch, (e) examples, (f) limitations.

### Feature 1: Dimensions, units and kinds of quantity (paper §6.1)

**Dimensions.** A dimension is a vector of integer exponents over the 7 SI base dimensions (length L, mass M, time T, current I, temperature Θ, amount N, luminous intensity J):

- dim(Length) = L¹, dim(Area) = L², dim(U-value W/(m²·K)) = M¹ T⁻³ Θ⁻¹.
- Dimensions form an **abelian group** under multiplication: multiplying quantities **adds** exponent vectors, dividing **subtracts** them, the dimensionless quantity is the zero vector.

**Typing rules (sketch):**

```
 Γ ⊢ e1 : Q(d)    Γ ⊢ e2 : Q(d)            Γ ⊢ e1 : Q(d1)   Γ ⊢ e2 : Q(d2)
 ───────────────────────────────── (Add)   ────────────────────────────────── (Mul)
       Γ ⊢ e1 + e2 : Q(d)                      Γ ⊢ e1 * e2 : Q(d1 + d2)
```

Adding quantities with different dimensions is a type error (E01xx).

**Units.** A unit is a dimension plus a scale factor to SI (mm = 10⁻³ m). Literals like `2.5 mm` are normalised to SI at check time. Units are **erased** after checking: at run time a quantity is a plain `f64`.

**Kinds of quantity.** A kind is a named refinement of a dimension with meaning (`kind Thickness : Length`). Kinds form a **subtyping hierarchy** (Thickness <: Length). Rules:
- Passing a `Length` where `Thickness` is expected: error.
- Passing an `Inset` where `Thickness` is expected: error (the Fig. 5 swap).
- Adding two different kinds of the same dimension: allowed, result is their least common supertype, with a **warning** (paper's "hierarchy of warnings").

**Inference.** `area(mid) * thickness` is inferred as `Q(L³)` without annotation (paper Fig. 20, line 15).

**Soundness statement.** Well-typed programs are *dimensionally homogeneous*: every addition, subtraction and comparison is between quantities of the same dimension (following Kennedy, *Types for Units-of-Measure*, 2010).

### Feature 2: Tolerances and refinements (paper §6.2)

**Tolerance as interval.** A toleranced value `x ± ε` denotes the interval `[x − ε, x + ε]`. Type: `Tol<Q(d), ε>`.

**Interval arithmetic (Moore, 1979):**
- `[a,b] + [c,d] = [a+c, b+d]`
- `[a,b] − [c,d] = [a−d, b−c]`
- `[a,b] × [c,d] = [min(ac,ad,bc,bd), max(ac,ad,bc,bd)]`

**Fundamental property (inclusion).** If x ∈ X and y ∈ Y, then x ∘ y ∈ X ∘ Y. So the computed interval always contains every value the real, manufactured part can take. This is the soundness property to test with `proptest`.

**Tolerance stack-up modes** (an open question in the paper: Fig. 20 line 16 gives no combination rule):
- **Worst case:** ε of a sum = sum of the ε's. Sound, can be pessimistic.
- **Statistical (RSS):** ε = √(Σ εᵢ²). Assumes independent errors; not sound in the worst case. Must be an explicit, documented choice, never the default.

**Refinements.** A `where` clause states a predicate, e.g. `thickness > 0 m`. The checker evaluates it over the value's interval:
- whole interval satisfies it → **proved**;
- no point satisfies it → **error**;
- only part satisfies it → **warning: cannot prove**.

This is *sound but incomplete*: it never accepts a false claim, but may fail to prove a true one. (This is how a decidable checker lives with Rice's theorem.)

**Compatibility check.** A compensator (seal, slot) has an *allowance* A; the parts it joins accumulate a tolerance T. The design is feasible only if T ⊆ A. Report an error otherwise.

### Feature 3: Construction order (beyond the paper)

**Model.** Each physical component is a **linear resource**: it exists once and is installed exactly once. Each component has a **state**: `Fabricated → Installed`.

**Operations as typed transitions:**
- `install x` requires `x : Fabricated`, produces `x : Installed`.
- `install x on y` additionally requires `y : Installed`.

**Checking.** Walk the plan step by step, tracking the state of every component (an abstract interpretation over a finite state map). Errors:
- install a component twice (linearity violated),
- install on a support not yet installed (order violated),
- unknown component.
Optional completeness check: at the end, every declared component is `Installed`.

**Soundness statement.** If a plan is accepted, then no component is used before its support is installed, and no component is installed twice.

**Links to theory.** This is the *typestate* idea; linearity relates to *linear types* (Orchard's Granule language). Rust's own ownership (move semantics) is an affine version of the same idea; Milestone 4 should include a short demo of the same check done with Rust's type system (typestate pattern), for comparison.

## 6. Milestones

Each milestone ends with: all tests passing, the `docs/` file updated, and a short summary to me of what was done and what I should understand.

- **M0. Setup.** Workspace, crates, CI-ready `cargo test`, one example file. Explain the architecture choices.
- **M1. Syntax.** Grammar (write it in EBNF in `docs/grammar.md` first), lexer, parser, AST with spans, parse errors with ariadne. Tests: every example in `examples/` parses.
- **M2. Feature 1.** Dimensions as exponent vectors, unit normalisation, kinds and subtyping, inference for `*` and `/`, the Fig. 5 swap error. Snapshot tests of error messages.
- **M3. Feature 2.** Interval type, arithmetic, worst-case and RSS modes, refinement checking (proved / error / cannot prove), compatibility check. Property-based test of the inclusion property.
- **M4. Feature 3.** Components, plans, state tracking, linearity and order errors. Plus the Rust typestate comparison demo.
- **M5. Playground (optional).** Compile the checker to WASM; minimal web page with an editor and live errors.
- **M6. Haskell reference + differential testing (optional).**
  1. Add `facadec ast --json` to export the parsed AST as versioned JSON (document the schema in `docs/ast-schema.md`).
  2. In `haskell-ref/`, write a Haskell checker for **Feature 1 only** (dimensions, units, kinds), reading that JSON (`aeson`). Keep it short and close to the typing rules in `docs/feature-1-dimensions.md`: one Haskell function per rule where possible.
  3. In `tests/diff/`, a script runs both checkers on every file in `examples/` and fails if they disagree (accepted vs rejected, and error code).
  4. Explain to me, step by step, how each Haskell function corresponds to a typing rule. Assume I am new to Haskell: introduce syntax as it appears.

## 7. Testing strategy

- **Unit tests** per typing rule: one accepted and one rejected example.
- **Example programs:** each file in `examples/err/` contains exactly one bug and an expected error code in a comment.
- **Snapshot tests** (`insta`) for the full text of error messages.
- **Property-based tests** (`proptest`) for mathematical properties: interval inclusion, dimension group laws (associativity, inverse).
- **Differential tests** (M6): the Rust checker and the Haskell reference checker must give the same verdict on every example.

## 8. References

- Aish, Fisher, Orchard, Torry. *Programming Languages for the Future of Design Computation.* Onward! 2024.
- Kennedy. *Dimension Types.* ESOP 1994; *Types for Units-of-Measure: Theory and Practice.* CEFP 2009/2010.
- Moore. *Methods and Applications of Interval Analysis.* SIAM, 1979.
- Vazou, Rondon, Jhala. *Abstract Refinement Types.* ESOP 2013.
- Orchard, Liepelt, Eades. *Quantitative Program Reasoning with Graded Modal Types.* ICFP 2019 (Granule).
- Strom, Yemini. *Typestate: A Programming Language Concept for Enhancing Software Reliability.* IEEE TSE 1986.
- Pierce. *Types and Programming Languages.* MIT Press, 2002.
