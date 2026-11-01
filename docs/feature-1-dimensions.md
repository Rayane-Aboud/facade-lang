# Feature 1: dimensions, units and kinds of quantity

This document defines the first part of FacadeLang's type system: what may
be combined with what, so that type checking is a *static feasibility check*
on designs. It follows paper §6.1 (Aish, Fisher, Orchard, Torry, Onward! '24)
and the dimension types of Kennedy (ESOP 1994; CEFP 2009).

Sections: 1 intuition · 2 formal definitions · 3 typing rules · 4 soundness
· 5 worked examples · 6 limitations.

## 1. Intuition

### The bug

In the Dynamo program of paper Fig. 5, a facade panel is built from three
number sliders: inset = −1, offset = 0, thickness = 0.9. To Dynamo they are
all just *numbers*. If you connect the inset slider where the thickness
should go, the program still runs, and it builds the wrong panel. Nothing
tells you.

FacadeLang gives every value a type that says **what it is**, in three
layers. Each layer catches a different kind of mistake.

### Three layers

| Layer | What it describes | Mistake it catches |
|-------|-------------------|--------------------|
| **Dimension** | the physical nature: length, time, mass… | `2 m + 3 s` (adding a length and a time) |
| **Unit** | the measuring scale: m, mm, kN… | `1000 mm + 1 m` must be 2 m, not 1001 of something |
| **Kind of quantity** | the *meaning* of a value of a given dimension | `thicken(mid, inset)`: an inset used as a thickness |

**Dimensions alone are not enough.** An inset and a thickness are both
lengths, so a checker with dimensions only accepts the swap. The same problem
is well known in physics: torque and energy both have the unit N·m (the
dimension M·L²·T⁻²), but adding a torque to an energy is meaningless.
Metrology calls the missing information the *kind of quantity* (ISO/IEC
Guide 99, "VIM": quantities of the same kind are mutually comparable).
FacadeLang makes kinds part of the type:

```
kind Thickness : Length      -- a Thickness is a Length with a specific meaning
kind Inset     : Length

extern thicken(Surface, Thickness) -> Solid

let panel = thicken(mid, inset)   -- error: expected Thickness, found Inset
```

**Units are only notation.** `2.5 mm` and `0.0025 m` are the same quantity.
The checker converts every literal to SI while checking, and then forgets
the unit: at run time a quantity is a plain `f64`. Units never appear in
types; dimensions and kinds do.

## 2. Formal definitions

### 2.1 Base dimensions

A **base dimension** is declared without a definition:

```
dimension Length
```

Let **B** be the set of declared base dimensions. The standard prelude
declares the seven SI base dimensions:

| Name | Symbol |
|------|--------|
| Length | L |
| Mass | M |
| Time | T |
| Current | I |
| Temperature | Θ |
| Amount | N |
| Luminosity | J |

A program may declare more (for example `dimension Currency`), so B is not
fixed to seven elements.

### 2.2 Dimensions

A **dimension** gives each base dimension an integer **exponent**. Formally,
a dimension is a function `d : B → ℤ` (ℤ = the integers), and we write it as
a product of powers, leaving out exponents equal to 0:

| Dimension | Written | Exponent vector (L, M, T) |
|-----------|---------|---------------------------|
| length | L | (1, 0, 0) |
| area | L² | (2, 0, 0) |
| velocity | L·T⁻¹ | (1, 0, −1) |
| force | M·L·T⁻² | (1, 1, −2) |
| dimensionless | 1 | (0, 0, 0) |

Let **D** be the set of all dimensions. Three operations are defined on D:

| Operation | Definition | Meaning |
|-----------|------------|---------|
| product `d₁ · d₂` | `(d₁ · d₂)(b) = d₁(b) + d₂(b)` | multiplying quantities **adds** exponents |
| inverse `d⁻¹` | `(d⁻¹)(b) = −d(b)` | dividing: `d₁ / d₂ = d₁ · d₂⁻¹` |
| power `dⁿ` | `(dⁿ)(b) = n · d(b)` for `n ∈ ℤ` | `x^n` multiplies exponents by n |

The **dimensionless** dimension `1` has every exponent 0.

**D is an abelian group.** A *group* is a set with an operation that is
associative, has an identity element, and gives every element an inverse.
*Abelian* means the operation is also commutative. For all
`d₁, d₂, d₃ ∈ D`:

| Law | Statement |
|-----|-----------|
| associativity | `(d₁ · d₂) · d₃ = d₁ · (d₂ · d₃)` |
| commutativity | `d₁ · d₂ = d₂ · d₁` |
| identity | `d · 1 = d` |
| inverse | `d · d⁻¹ = 1` |

All four hold because they hold for integer addition, exponent by exponent.
In fact D is the same group as ℤᴮ, vectors of integers added component by
component. This is the structure Kennedy uses for dimension types, and it is
what the implementation stores: a map from base dimension to its non-zero exponent. The laws
are tested with property-based tests (`proptest`).

**Derived dimension declarations** give a name to a dimension. They do not
create a new base:

```
dimension Area     = Length^2               -- Area     names L²
dimension Velocity = Length / Time          -- Velocity names L·T⁻¹
dimension Force    = Mass * Length / Time^2 -- Force    names M·L·T⁻²
```

The **meaning** `⟦p⟧ ∈ D` of a dimension expression `p` (the `Product` of
the AST) is defined by structural recursion:

```
⟦X⟧       = the dimension named X        (a base, or a derived name)
⟦p₁ * p₂⟧ = ⟦p₁⟧ · ⟦p₂⟧
⟦p₁ / p₂⟧ = ⟦p₁⟧ · ⟦p₂⟧⁻¹
⟦X^n⟧     = ⟦X⟧ⁿ
```

(The brackets `⟦ ⟧` are the usual notation for "the meaning of". *Structural
recursion* means one equation per kind of AST node, each using the meaning
of the node's children.)

### 2.3 Units

A **unit** is a pair `u = (d, s)`: a dimension `d ∈ D` and a **scale** `s`,
a positive real number that says how many SI base units one `u` is.

| Declaration | Meaning |
|-------------|---------|
| `unit m : Length` | m = (L, 1): the base unit of a base dimension |
| `unit mm = 0.001 m` | mm = (L, 0.001) |
| `unit N = 1 [kg*m/s^2]` | N = (M·L·T⁻², 1) |
| `unit kN = 1000 N` | kN = (M·L·T⁻², 1000) |

Rules for unit declarations:

- `unit u : X` requires X to be a **base** dimension, and each base dimension
  has at most one base unit. Its scale is 1.
- `unit u = v w` defines `u = (d, v · s)`, where `w = (d, s)` is the meaning
  of the unit expression `w`.

The meaning of a unit expression follows the same recursion as for
dimensions, with scales multiplied:

```
(d₁, s₁) · (d₂, s₂) = (d₁ · d₂,  s₁ · s₂)
(d, s)⁻¹            = (d⁻¹,      1 / s)
(d, s)ⁿ             = (dⁿ,       sⁿ)
```

So `[kg*m/s^2]` = (M, 1)·(L, 1)·(T, 1)⁻² = (M·L·T⁻², 1).

**Normalisation.** A literal `v u`, with `u = (d, s)`, denotes the SI value
`v · s` of dimension `d`. A literal without a unit is dimensionless.

| Literal | SI value | Dimension |
|---------|----------|-----------|
| `2.5 mm` | 0.0025 | L |
| `3 m^2` | 3 | L² |
| `9.81 [m/s^2]` | 9.81 | L·T⁻² |
| `1 kN` | 1000 | M·L·T⁻² |
| `42` | 42 | 1 |

After normalisation the unit is gone (*erasure*): the type keeps only the
dimension, and the value is a plain number in SI.

### 2.4 Kinds of quantity

A **kind** is a named refinement of a dimension. Each kind `k` has a
**parent**: a dimension expression, or another kind.

```
kind Thickness      : Length       -- parent: the dimension L
kind Inset          : Length
kind GlassThickness : Thickness    -- parent: another kind
kind UValue         : Power / (Area * Temperature)
```

The dimension of a kind is the dimension of its parent:
`dim(Thickness) = dim(GlassThickness) = L`.

Kinds form a **forest** (a set of trees). Each tree has a plain dimension at
its root, and every kind sits below its parent:

```
            Q(L)                      Q(M·T⁻³·Θ⁻¹)
          /      \                         |
   Thickness    Inset                   UValue
       |
 GlassThickness
```

### 2.5 Types

The types of Feature 1 are:

```
τ ::= Q(d)       a plain quantity of dimension d (d ∈ D), with no kind
    | k          a quantity of kind k
    | Curve | Surface | Solid      opaque geometry
```

Read `::=` as "is one of", and `|` as "or". This is the same notation as the
EBNF of `grammar.md`, but for types.

- A **dimension name** used as a type means its plain quantity type: the
  type `Length` is `Q(L)`, and `Area` is `Q(L²)`.
- `Q(d)` and kinds are **quantity types**. For them, `dim(τ)` is defined:
  `dim(Q(d)) = d`, and `dim(k)` is as in 2.4.
- Geometry types are **opaque**: the checker knows nothing about them
  except their name, and they have no dimension.

### 2.6 Subtyping

**Subtyping** `τ₁ <: τ₂` (read "τ₁ is a subtype of τ₂") means: a value of
type τ₁ can be used wherever a value of type τ₂ is expected. A Thickness can
be used where any Length is expected, but not the other way round.

`<:` is the smallest relation that contains these three rules:

```
──────── (S-Refl)        ─────────────────── (S-Kind)        τ₁ <: τ₂    τ₂ <: τ₃
 τ <: τ                   k <: parent(k)                     ───────────────────── (S-Trans)
                                                                    τ₁ <: τ₃
```

This is the **inference-rule** notation used in the rest of the document.
Above the line are the *premises* (what must already hold). Below the line
is the *conclusion* (what then holds). A rule with nothing above the line is
an *axiom*: it always holds. The name on the right labels the rule.

Here `parent(k)` is the parent kind, or `Q(d)` when the parent is a
dimension. So, in the forest of 2.4:

| Statement | Holds? | Why |
|-----------|--------|-----|
| `Thickness <: Q(L)` | yes | S-Kind |
| `GlassThickness <: Q(L)` | yes | S-Kind twice, then S-Trans |
| `Q(L) <: Thickness` | **no** | a plain length has no meaning attached |
| `Inset <: Thickness` | **no** | different branches: the Fig. 5 swap |
| `Q(L) <: Q(L²)` | no | different dimensions |
| `Curve <: Surface` | no | geometry types are only related to themselves |

Two facts follow directly from the definitions:

1. **Subtyping preserves dimension:** if `τ₁ <: τ₂` and both are quantity
   types, then `dim(τ₁) = dim(τ₂)`. (Each rule keeps the dimension.)
2. **Joins exist within a dimension:** any two quantity types τ₁, τ₂ with
   the same dimension have a *least common supertype* `τ₁ ⊔ τ₂` (read
   "join"): their nearest common ancestor in the tree. At worst it is the
   root `Q(d)`. For example `Thickness ⊔ Inset = Q(L)`,
   `GlassThickness ⊔ Thickness = Thickness`.

### 2.7 Giving a kind: assignability

Literals have no kind: `0.9 m` has type `Q(L)`. Kinds are given **where a
name is declared** with a type annotation (decision K1). So we need a second
relation, besides `<:`.

**Assignability** `τ ◁ σ` (read "τ can be given type σ"), for the value of a
declaration `param x : σ = e` or `let x : σ = e`, where e has type τ:

```
     τ <: σ                    dim(σ) = d
 ───────────── (A-Sub)       ─────────────── (A-Kind)
     τ ◁ σ                     Q(d) ◁ σ
```

- A-Sub: an already-kinded value must be a subtype, so you **cannot relabel**
  an Inset as a Thickness.
- A-Kind: a **plain** quantity of the right dimension may receive the kind.
  This is how `param thickness : Thickness = 0.9 m` is accepted.

Assignability is used **only** at declarations. Everywhere else (function
arguments), plain subtyping `<:` is required. That is where the Fig. 5 swap
is caught.

### 2.8 Combining kinds in addition

When two quantities of the same dimension are added, the result type is
`τ₁ ⊕ τ₂`:

```
Q(d) ⊕ τ    = τ              a plain quantity takes the other side's kind
τ    ⊕ Q(d) = τ
k₁   ⊕ k₂   = k₁ ⊔ k₂        two kinds: their join
```

So `thickness + 1 mm : Thickness`, `thickness + thickness : Thickness`, and
`thickness + inset : Q(L)`. When the two kinds are **incomparable** (neither
is a subtype of the other), the program is still accepted, but with warning
**W0101**: the result has lost its meaning. This is the first level of the
paper's "hierarchy of warnings".

## 3. Typing rules

### 3.1 Notation

A **typing judgement** `Γ ⊢ e : τ` reads: "in the environment Γ, the
expression e has type τ".

- **Γ** (Greek capital *Gamma*) is the **environment**: the list of value
  names declared so far, each with its type, written `x₁ : τ₁, …, xₙ : τₙ`.
  `Γ, x : τ` is Γ extended with one more name. `x : τ ∈ Γ` means "Γ gives x
  the type τ".
- **⊢** (the *turnstile*) separates what is assumed (left) from what is
  concluded (right).
- The **signature** Σ holds the global declarations: dimensions, units,
  kinds (section 2), and `extern` functions. It never changes while an
  expression is checked, so we leave it implicit and write `f : (σ₁, …, σₙ) → σ`
  for "Σ declares f with these parameter and result types".

A program is **accepted** if every rule needed for it has a derivation. A
*derivation* is a tree of rules, each premise proved by a rule above it,
down to axioms. If some premise cannot be derived, the checker reports the
error listed with the rule.

### 3.2 Types written in the source

A type annotation `T` in the source has a meaning `⟦T⟧`:

| Source | Meaning |
|--------|---------|
| a dimension name (`Length`) | `Q(d)` for the dimension it names |
| a kind name (`Thickness`) | that kind |
| `Curve`, `Surface`, `Solid` | the geometry type |
| any other product (`Length^3`, `Force / Area`) | `Q(⟦p⟧)`; a kind inside a product contributes only its dimension |
| `Tol<T, q>` | `⟦T⟧`, if `dim(q) = dim(⟦T⟧)` (E0101 otherwise); tolerances are Feature 2 |

An unknown name is E0103.

### 3.3 Expressions

**Literals and names.**

```
   u = (d, s)                                         x : τ ∈ Γ
 ─────────────── (T-Lit)     ──────────── (T-Num)    ─────────── (T-Var)
 Γ ⊢ v u : Q(d)              Γ ⊢ v : Q(1)            Γ ⊢ x : τ
```

E0104 if `u` is not a declared unit. E0103 if `x` is not in Γ.

**Negation** keeps the type:

```
 Γ ⊢ e : τ     τ is a quantity type
 ──────────────────────────────────── (T-Neg)
           Γ ⊢ −e : τ
```

**Addition and subtraction** need equal dimensions (this is the rule that
makes programs dimensionally homogeneous):

```
 Γ ⊢ e₁ : τ₁    Γ ⊢ e₂ : τ₂    dim(τ₁) = dim(τ₂)
 ──────────────────────────────────────────────── (T-Add)
          Γ ⊢ e₁ + e₂ : τ₁ ⊕ τ₂
```

The same rule, with the same premises, types `e₁ − e₂`. E0101 if the
dimensions differ. W0101 if τ₁ and τ₂ are incomparable kinds (2.8).

**Multiplication and division** add and subtract dimensions, and forget
kinds (decision K2), except when one side is a plain pure number (K3):

```
 Γ ⊢ e₁ : τ    Γ ⊢ e₂ : Q(1)                 Γ ⊢ e₁ : Q(1)    Γ ⊢ e₂ : τ
 ──────────────────────────── (T-Scale)       ──────────────────────────── (T-Scale')
 Γ ⊢ e₁ * e₂ : τ                               Γ ⊢ e₁ * e₂ : τ
 Γ ⊢ e₁ / e₂ : τ

 Γ ⊢ e₁ : τ₁    Γ ⊢ e₂ : τ₂                  Γ ⊢ e₁ : τ₁    Γ ⊢ e₂ : τ₂
 ──────────────────────────────── (T-Mul)     ───────────────────────────────── (T-Div)
 Γ ⊢ e₁ * e₂ : Q(dim τ₁ · dim τ₂)             Γ ⊢ e₁ / e₂ : Q(dim τ₁ · dim τ₂⁻¹)
```

T-Scale and T-Scale' apply when one side has exactly the type `Q(1)`;
otherwise T-Mul and T-Div apply. (If both sides are `Q(1)`, all rules give
`Q(1)`, so the choice does not matter.) `1 / thickness` is T-Div, not
scaling: its type is `Q(L⁻¹)`.

**Powers** multiply the dimension, and forget the kind:

```
 Γ ⊢ e : τ     τ is a quantity type     n ∈ ℤ
 ─────────────────────────────────────────── (T-Pow)
          Γ ⊢ e^n : Q((dim τ)ⁿ)
```

**Tolerances.** Feature 1 only checks that a tolerance has the dimension of
its value. Feature 2 gives `±` its full meaning.

```
 Γ ⊢ e₁ : τ₁    Γ ⊢ e₂ : τ₂    dim(τ₁) = dim(τ₂)
 ──────────────────────────────────────────────── (T-Tol)
              Γ ⊢ e₁ ± e₂ : τ₁
```

**Calls** require every argument to be a **subtype** of its parameter. This
is the rule that catches the Fig. 5 swap:

```
 f : (σ₁, …, σₙ) → σ     Γ ⊢ e₁ : τ₁  …  Γ ⊢ eₙ : τₙ     τ₁ <: σ₁  …  τₙ <: σₙ
 ──────────────────────────────────────────────────────────────────────────── (T-Call)
                            Γ ⊢ f(e₁, …, eₙ) : σ
```

Errors, for an argument `τᵢ` that is not a subtype of `σᵢ`:

| Situation | Code |
|-----------|------|
| both quantity types, different dimensions | E0101 dimension mismatch |
| both quantity types, same dimension | **E0102 kind mismatch** |
| any other pair (`Curve` for `Surface`, a quantity for a geometry) | E0108 type mismatch |
| wrong number of arguments | E0106 |
| `f` is not declared | E0103 |

**Geometry in arithmetic.** In T-Neg, T-Add, T-Mul, T-Div, T-Pow and T-Tol,
every operand must be a quantity type. A geometry operand is E0105.

**Brackets** leave no node in the AST (`grammar.md`), so they need no rule.

### 3.4 Predicates

A comparison is well-typed if both sides have the same dimension. Whether it
is *true* is Feature 2.

```
 Γ ⊢ e₁ : τ₁    Γ ⊢ e₂ : τ₂    dim(τ₁) = dim(τ₂)
 ──────────────────────────────────────────────── (T-Cmp)
              Γ ⊢ e₁ ⋈ e₂  ok
```

`⋈` is any of `<  <=  >  >=  ==`. A predicate `c₁ and … and cₙ` is ok if
every `cᵢ` is ok. E0101 if the dimensions differ; W0101 for incomparable
kinds, as for addition.

### 3.5 Declarations and programs

A declaration **extends** the environment. `Γ ⊢ decl ⇒ Γ'` reads: "in Γ,
the declaration is well-typed and produces Γ'".

```
 x ∉ Γ    σ = ⟦T⟧    Γ ⊢ e : τ    τ ◁ σ    Γ, x : σ ⊢ P ok
 ────────────────────────────────────────────────────────── (D-Param)
     Γ ⊢ param x : T = e where P  ⇒  Γ, x : σ

 x ∉ Γ    σ = ⟦T⟧    Γ ⊢ e : τ    τ ◁ σ
 ──────────────────────────────────────── (D-Let)
     Γ ⊢ let x : T = e  ⇒  Γ, x : σ

 x ∉ Γ    Γ ⊢ e : τ
 ─────────────────────────── (D-Infer)
 Γ ⊢ let x = e  ⇒  Γ, x : τ
```

- In D-Param, the default `= e` and the predicate `where P` are optional:
  leave out the matching premises. The predicate is checked with `x` already
  in scope, so `where thickness > 0 m` can mention `thickness`.
- D-Infer is **type inference**: without an annotation, `x` gets the type of
  its value.
- When `τ ◁ σ` fails: E0102 if both are quantity types of the same
  dimension (the relabelling case), E0101 if their dimensions differ, E0108
  otherwise.
- `x ∉ Γ` fails with E0107 (duplicate declaration).

A **program** `decl₁ … declₙ` is accepted if, starting from the empty
environment, each declaration is well-typed in the environment produced by
the ones before it (decision K5, declare before use):

```
∅ ⊢ decl₁ ⇒ Γ₁     Γ₁ ⊢ decl₂ ⇒ Γ₂     …     Γₙ₋₁ ⊢ declₙ ⇒ Γₙ
```

`dimension`, `unit`, `kind` and `extern` declarations build the signature Σ
(section 2), in the same declare-before-use order. Invalid unit declarations
(a base unit for a derived dimension, a second base unit, or a scale that is not positive) are E0109.
`component` and `plan` are Feature 3 and are ignored here.

### 3.6 Error codes

| Code | Meaning | Raised by |
|------|---------|-----------|
| E0101 | dimension mismatch | T-Add, T-Tol, T-Cmp, T-Call, D-Param, D-Let, `Tol<…>` |
| E0102 | kind mismatch | T-Call, D-Param, D-Let |
| E0103 | unknown name (value, type, kind or function) | T-Var, T-Call, ⟦T⟧ |
| E0104 | unknown unit | T-Lit |
| E0105 | not a quantity (geometry in arithmetic) | T-Neg, T-Add, T-Mul, T-Div, T-Pow, T-Tol, T-Cmp |
| E0106 | wrong number of arguments | T-Call |
| E0107 | duplicate declaration | all declarations |
| E0108 | type mismatch (other) | T-Call, D-Param, D-Let |
| E0109 | invalid unit declaration | `unit` |
| W0101 | incomparable kinds combined | T-Add, T-Cmp |

## 4. Soundness

### 4.1 What we prove

**Theorem 1 (dimensional homogeneity).** If a program is accepted, then in
every addition, subtraction, tolerance and comparison it contains, both
sides have the same dimension.

**Theorem 2 (kind safety).** If a program is accepted, then every argument
of every call has a type that is a subtype of the parameter type. In
particular, a value is never passed where an *incomparable* kind is
expected: an Inset never reaches a Thickness parameter.

**Theorem 3 (no relabelling).** If a program is accepted, every value
declared with a kind `k` is either a plain quantity of dimension `dim(k)`
or has a type that is a subtype of `k`.

Theorem 1 is the property of Kennedy's dimension types, the classical notion
of soundness for units of measure. Theorems 2 and 3 are the new part: they
say kinds are respected, which is what catches the Fig. 5 swap.

### 4.2 Proof sketch

The rules are **syntax-directed**: for each form of expression (literal,
name, `+`, `*`, call…) exactly one rule can conclude its type. (For `*` and
`/`, which of T-Scale and T-Mul applies is decided by the operand types.)
This gives an **inversion** property: if `Γ ⊢ e : τ` has a derivation, then
the derivation ends with the rule for the form of e, and its premises hold
for e's direct sub-expressions.

*Theorem 1*, by induction on the derivation. Take any sub-expression
`e₁ + e₂` of an accepted program. By inversion, its derivation ends with
T-Add, whose premises include `dim(τ₁) = dim(τ₂)`. The same holds for `−`
(T-Add), `±` (T-Tol) and comparisons (T-Cmp). Since every sub-expression of
an accepted program has a derivation (by inversion, going down the tree),
the property holds everywhere.

*Theorem 2*, the same way: by inversion, every call is concluded by T-Call,
whose premises include `τᵢ <: σᵢ` for each argument. If `τᵢ` and `σᵢ` were
incomparable kinds, `τᵢ <: σᵢ` would not hold, by definition.

*Theorem 3*: by inversion, every declaration with an annotation ends with
D-Param or D-Let, whose premise `τ ◁ σ` holds. Assignability has only two
rules: A-Kind (a plain quantity of the right dimension) and A-Sub (a
subtype).

### 4.3 Decidability

The checker visits each AST node once. Checking `τ₁ <: τ₂` walks up the
kind tree from τ₁, at most as many steps as the tree is deep; computing a
join walks up both. Dimension operations are integer additions, one per base
dimension. So checking always terminates, in time linear in the size of the
program (times the depth of the kind tree).

### 4.4 What is assumed

- **Extern signatures are trusted.** The checker cannot see inside
  `thicken`; if its declared type is wrong, the guarantee is about the
  declared type.
- **Homogeneity is a property of the program text**, not a proof that the
  design is physically correct. A homogeneous formula can still be the
  wrong formula.

## 5. Worked examples

All examples use the prelude (`prelude/std.fcd`) and the Fig. 5 program
(`examples/ok/fig5_panel.fcd`).

### 5.1 A literal receives a kind

```
param thickness : Thickness = 0.9 m
```

T-Lit gives `0.9 m : Q(L)`. The annotation means `⟦Thickness⟧ = Thickness`,
and `dim(Thickness) = L`, so A-Kind gives `Q(L) ◁ Thickness`. By D-Param,
`thickness : Thickness` is added to Γ.

### 5.2 Inference: the panel volume

```
let vol = area(mid) * thickness
```

Γ contains `mid : Surface` and `thickness : Thickness`, and Σ declares
`area : (Surface) → Q(L²)` (the prelude's `Area` means `Q(L²)`):

```
 area : (Surface) → Q(L²)    Γ ⊢ mid : Surface    Surface <: Surface
 ─────────────────────────────────────────────────────────────────── (T-Call)
              Γ ⊢ area(mid) : Q(L²)                                   Γ ⊢ thickness : Thickness
 ────────────────────────────────────────────────────────────────────────────────────────────── (T-Mul)
                         Γ ⊢ area(mid) * thickness : Q(L² · L) = Q(L³)
```

Neither side is `Q(1)`, so T-Mul applies, and the kind is forgotten. By
D-Infer, `vol : Q(L³)`, which is the prelude's `Volume`, with no annotation
written (paper Fig. 20, line 15).

### 5.3 The Fig. 5 swap

```
let panel = thicken(mid, inset)
```

Σ declares `thicken : (Surface, Thickness) → Solid`, and Γ contains
`inset : Inset`. T-Call needs the premise `Inset <: Thickness`. To derive
it, S-Kind could only take `Inset` up to its parent `Q(L)`, and `Q(L)` has
no parent, so no chain of S-Kind and S-Trans reaches `Thickness`. The
premise has no derivation. Both are quantity types with dimension L, so the
checker reports:

```
error[E0102]: kind mismatch
  expected `Thickness`, found `Inset`
  both are `Length`, but they mean different things
```

### 5.4 Relabelling is rejected

```
let t : Thickness = inset
```

D-Let needs `Inset ◁ Thickness`. A-Sub needs `Inset <: Thickness`, which
fails (5.3). A-Kind needs a *plain* `Q(L)`, but `Inset` is a kind. Neither
rule applies: E0102.

### 5.5 More small cases

| Expression | Type | Rule | Note |
|------------|------|------|------|
| `2 m + 3 s` | — | T-Add | E0101: L vs T |
| `1000 mm + 1 m` | `Q(L)` | T-Add | SI values: 1 + 1 = 2 |
| `thickness + 1 mm` | `Thickness` | T-Add, ⊕ | the plain side takes the kind |
| `thickness + inset` | `Q(L)` | T-Add, ⊕ | W0101: incomparable kinds |
| `2 * thickness` | `Thickness` | T-Scale' | scaling keeps the kind (K3) |
| `thickness * thickness` | `Q(L²)` | T-Mul | kinds forgotten (K2) |
| `1 / thickness` | `Q(L⁻¹)` | T-Div | not scaling |
| `thickness^2` | `Q(L²)` | T-Pow | |
| `mid + 1 m` | — | T-Add | E0105: `mid` is a Surface |
| `thicken(mid)` | — | T-Call | E0106: 1 argument, 2 expected |
| `thicken(mid, 0.9 m)` | — | T-Call | E0102: `Q(L)` is not a `Thickness` (see 6) |

### 5.6 The whole Fig. 5 program

| Name | Type | How |
|------|------|-----|
| `base` | `Curve` | D-Param, no default |
| `inset` | `Inset` | D-Param, A-Kind (`-1.0 m : Q(L)`, by T-Neg) |
| `offset` | `Q(L)` | D-Param, A-Sub (`Length` means `Q(L)`) |
| `thickness` | `Thickness` | D-Param, A-Kind; `where thickness > 0 m` ok by T-Cmp |
| `curve` | `Curve` | D-Infer, T-Call with `Inset <: Inset` |
| `surf` | `Surface` | D-Infer, T-Call |
| `mid` | `Surface` | D-Infer, T-Call with `Q(L) <: Q(L)` |
| `panel` | `Solid` | D-Infer, T-Call with `Thickness <: Thickness` |
| `vol` | `Q(L³)` | D-Infer, 5.2 |

The program is accepted. Swapping `inset` and `thickness` in either call is
E0102.

## 6. Limitations

- **Literals cannot be passed directly where a kind is expected.**
  `thicken(mid, 0.9 m)` is E0102, because a plain `Q(L)` is not a
  `Thickness`. This is deliberate (K1): it forces the meaning to be written
  once, at a declaration (`param thickness : Thickness = 0.9 m`), which is
  exactly what the unnamed sliders of Fig. 5 were missing. It may feel
  strict in quick experiments.
- **No dimension polymorphism.** There are no user-defined functions, so a
  function like `square(x) = x * x`, of type "for every d, Q(d) → Q(d²)",
  cannot be written. This is the natural next step (Kennedy 1994), noted as
  a possible extension after M2.
- **Kinds are forgotten by `*`, `/` and `^`**, except for scaling by a pure
  number. `thickness * thickness` is a plain area, never "Thickness²".
- **Only scale units.** Units with an offset (°C, °F) cannot be declared,
  and absolute temperatures are not distinguished from temperature
  differences.
- **No dimensionless kinds yet.** Angles (radians) and ratios are both
  dimensionless in SI and should be different kinds, but the grammar has no
  way to write the dimension `1` as a kind's parent.
- **Floating point.** Normalising to SI multiplies by scales such as 0.001,
  so values are subject to `f64` rounding. Exact comparisons are a Feature 2
  question.
- **One warning level.** W0101 is the only warning; the paper's full
  "hierarchy of warnings" is future work.
