# FacadeLang grammar

This document fixes the syntax of FacadeLang (Milestone 1). The parser in
`crates/syntax` follows these rules. The meaning of programs (types, units,
tolerances, plans) is defined in the `feature-*.md` files, not here.

## 1. Notation (EBNF)

| Symbol      | Meaning                     |
|-------------|-----------------------------|
| `a , b`     | `a` then `b`                |
| `a \| b`    | `a` or `b`                  |
| `[ a ]`     | optional `a`                |
| `{ a }`     | zero or more `a`            |
| `( a )`     | grouping                    |
| `"text"`    | the literal text            |
| `UPPER`     | a token (section 2)         |

## 2. Lexical rules (tokens)

```ebnf
IDENT   = letter , { letter | digit | "_" } ;      (* not a keyword *)
NUMBER  = digit , { digit } , [ "." , digit , { digit } ] ,
          [ ( "e" | "E" ) , [ "+" | "-" ] , digit , { digit } ] ;
letter  = "a" … "z" | "A" … "Z" | "_" ;
digit   = "0" … "9" ;
```

**Keywords** (reserved, cannot be names):

```
dimension  unit  kind  extern  param  let  where  and
component  plan  install  on  Tol
```

**Symbols:**

```
(  )  [  ]  {  }  <  >  ,  :  ;  =  +  -  *  /  ^
->  <=  >=  ==  ±  +/-
```

**Whitespace and comments.** Spaces, tabs and newlines only separate tokens;
they have no other meaning. A comment starts with `--` and runs to the end of
the line. The lexer always takes the longest match, so `<=` is one token, not
`<` then `=`.

## 3. Grammar

### Programs and declarations

```ebnf
program        = { decl } ;

decl           = dimension_decl | unit_decl | kind_decl | extern_decl
               | param_decl | let_decl | component_decl | plan_decl ;

dimension_decl = "dimension" , IDENT , [ "=" , dim_type ] ;
unit_decl      = "unit" , IDENT , ( ":" , IDENT | "=" , quantity ) ;
kind_decl      = "kind" , IDENT , ":" , dim_type ;
extern_decl    = "extern" , IDENT , "(" , [ type , { "," , type } ] , ")" ,
                 "->" , type ;
param_decl     = "param" , IDENT , ":" , type , [ "=" , expr ] ,
                 [ "where" , predicate ] ;
let_decl       = "let" , IDENT , [ ":" , type ] , "=" , expr ;
component_decl = "component" , IDENT , ":" , IDENT ;
plan_decl      = "plan" , IDENT , "{" , { step } , "}" ;
step           = "install" , IDENT , [ "on" , IDENT ] , ";" ;
```

### Types

```ebnf
type       = "Tol" , "<" , type , "," , quantity , ">"
           | dim_type ;
dim_type   = dim_factor , { ( "*" | "/" ) , dim_factor } ;
dim_factor = IDENT , [ "^" , int ]
           | "(" , dim_type , ")" ;
int        = [ "-" ] , NUMBER ;          (* NUMBER must be a whole number *)
```

An `IDENT` in a type can name a dimension (`Length`), a kind (`Thickness`) or
a geometry type (`Curve`, `Surface`, `Solid`). Which one it is gets decided
by name resolution, not by the grammar.

### Quantities and units

```ebnf
quantity    = NUMBER , [ unit ] ;
unit        = IDENT , [ "^" , int ]              (* simple:   2.5 mm, 3 m^2 *)
            | "[" , unit_expr , "]" ;            (* compound: 9.81 [m/s^2]  *)
unit_expr   = unit_factor , { ( "*" | "/" ) , unit_factor } ;
unit_factor = IDENT , [ "^" , int ]
            | "(" , unit_expr , ")" ;
```

### Expressions

```ebnf
expr     = additive , [ ( "±" | "+/-" ) , additive ] ;
additive = term , { ( "+" | "-" ) , term } ;
term     = unary , { ( "*" | "/" ) , unary } ;
unary    = "-" , unary
         | power ;
power    = primary , [ "^" , int ] ;
primary  = quantity
         | IDENT , [ "(" , [ expr , { "," , expr } ] , ")" ]   (* name or call *)
         | "(" , expr , ")" ;
```

### Predicates (`where` clauses)

```ebnf
predicate  = comparison , { "and" , comparison } ;
comparison = expr , ( "<" | "<=" | ">" | ">=" | "==" ) , expr ;
```

## 4. Operator precedence

From tightest to loosest. "Left" means `a - b - c` is `(a - b) - c`.

| Level | Operators                   | Associativity |
|-------|-----------------------------|---------------|
| 1     | unit after a number (`2 m`) | (none)        |
| 2     | `^` (integer exponent only) | none          |
| 3     | unary `-`                   | prefix        |
| 4     | `*`  `/`                    | left          |
| 5     | `+`  `-`                    | left          |
| 6     | `±`  `+/-`                  | none          |
| 7     | `<` `<=` `>` `>=` `==`      | none          |
| 8     | `and`                       | left          |

So `-x^2` is `-(x^2)`, `2 m * t` is `(2 m) * t`, and
`a + b +/- 1 mm` is `(a + b) ± 1 mm`.

## 5. Worked examples

**A literal with a unit.** `0.9 m` is `quantity = NUMBER(0.9) , unit(IDENT m)`.

**Units next to multiplication (decision D1).**

```
2.5 m * thickness      →  (2.5 m) * thickness      -- multiplication
9.81 [m/s^2] * t       →  (9.81 m/s²) * t          -- compound unit in brackets
```

**A declaration from `examples/ok/fig5_panel.fcd`.**

```
let vol = area(mid) * thickness
```

```
let_decl
├── "let"
├── IDENT  vol
├── "="
└── expr
    └── additive
        └── term
            ├── unary → power → primary  area( expr → … → IDENT mid )
            ├── "*"
            └── unary → power → primary  IDENT thickness
```

**A parameter with a refinement.**

```
param thickness : Thickness = 0.9 m where thickness > 0 m
```

is `param_decl` with type `dim_type(IDENT Thickness)`, value
`quantity(0.9, m)`, and predicate `comparison(thickness, ">", quantity(0, m))`.

**A prelude** (standard declarations, loaded before every program):

```
dimension Length
dimension Area = Length^2
unit m  : Length
unit mm = 0.001 m
unit N  = 1 [kg*m/s^2]
extern thicken(Surface, Thickness) -> Solid
```

## 6. Design decisions

| #  | Decision | Reason |
|----|----------|--------|
| D1 | A simple unit follows the number (`2.5 mm`); a compound unit goes in brackets (`9.81 [m/s^2]`) | Otherwise `2.5 m * t` is ambiguous (unit `m*t`, or multiplication?) |
| D2 | No top-level separators; newlines are whitespace; plan steps end with `;` | Every declaration starts with a keyword, so the parser always knows where the next one begins |
| D3 | Geometry types `Curve`, `Surface`, `Solid` are built in; geometry inputs are `param`s without a default (`param base : Curve`) | Geometry is opaque: the project is about the type system, not geometry |
| D4 | Built-in functions are declared with `extern` in a prelude | Domain concepts are data, not hard-coded (paper §6.4) |
| D5 | Tolerance in the type (`Tol<Length, 2.5 mm>`) and in values (`10 mm ± 2.5 mm`, ASCII `+/-`) | M3 needs both; `+-` was rejected because `a +-b` would mean `a ± b` |
| D6 | Predicates: comparisons joined with `and` only | `or` and `not` make interval checking (M3) harder; add them when needed |
| D7 | Comments: `--` to end of line | Matches the original sketch; note `x --1` is a comment |
| D8 | Dimensions and units are declared with `dimension` and `unit` | One list of units for the checker *and* for editor autocomplete; users can add units |

**One token of lookahead is enough.** Keywords are reserved and every
declaration starts with one, so after a `NUMBER` an `IDENT` is always a unit,
and after an `IDENT` a `(` always means a call. A grammar like this is called
*LL(1)*: the parser can always decide what to do by looking at the next token
only. This keeps the parser simple and its error messages precise.

## 7. Limitations

- Identifiers are ASCII only (write `um`, not `µm`).
- No offset units such as °C (only scale factors to SI).
- A number followed by a name is always read as a unit: `2 inset` is parsed
  as the unit `inset` and rejected later as an unknown unit.
- Component types (`Frame`, `Glazing`) are plain names for now; Milestone 4
  decides how they are declared.
- No `or` / `not` in predicates (D6).
