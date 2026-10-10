# AICAD Engineering DSL v1 — Grammar Specification

- Status: Accepted
- Date: 2026-08-11
- Related: ADR-018, `crates/scc-dsl`

This document is the formal grammar for the AICAD Engineering DSL v1. It is
the basis for future Grammar-Constrained Decoding and tooling. The DSL is a
declarative engineering description language: it lowers **only** to
`EngineeringCommand` values (never to a model, IR, or geometry directly).

## Lexical conventions

| Token | Rule |
|---|---|
| `identifier` | `[a-z][a-z0-9_-]{0,63}` (shared engineering ID rule; no Unicode identifiers) |
| `number` | `[0-9]+ ("." [0-9]+)?` — a leading `-` is lexed as `Minus` and combined by the parser |
| `string` | `"` … `"` with `\"` and `\\` escapes; unterminated strings are an error |
| `comment` | `#` to end of line (ignored by the lexer) |
| keywords | **uppercase** only (canonical uppercase); an unknown uppercase word is an error |
| case policy | keywords must be uppercase; identifiers must be lowercase; mixed case is rejected |

Whitespace (spaces, tabs) and empty lines are ignored; CRLF works. A hyphen
immediately inside a word is part of the identifier (per the shared ID rule);
binary subtraction requires whitespace around `-`.

## Document structure

```text
document   = header config* { statement } EOF ;
header     = ( "AICAD" | "VERSION" ) number NL ;   // version must be 1
config     = units | origin ;
units      = "UNITS" "mm" NL ;                      // only mm in v1
origin     = "ORIGIN" "CENTER" NL ;                 // only CENTER in v1
```

Blocks are keyword-delimited: the next top-level keyword ends the current
block. There is no `{`, `}` or `END`.

## Statements

```text
statement = part | param | base_rect | hole | hole_pattern
          | construction_circle | binding | constraint | drawing ;
```

### PART

```text
part = "PART" identifier [ string ] NL ;
```

### PARAM

```text
param       = "PARAM" identifier ("=" value | ":=" expression) NL ;
value       = "UNRESOLVED" | number ;
expression  = additive ;                 // see Expression below
```

`=` is a direct value (origin `User`); `:=` is a derived expression (origin
`Derived`). Expressions are single-line.

### BASE_RECT

```text
base_rect = "BASE_RECT" identifier NL field* ;
  field = "WIDTH" number NL          (required)
        | "HEIGHT" number NL         (required) ;
```

### HOLE

```text
hole = "HOLE" identifier NL field* ;
  field = "CENTER" number number NL  (optional; desugars to implicit
                                       `{id}_center_x`/`{id}_center_y`
                                       parameters bound to CenterX/CenterY)
        | "DIA" number NL            (required) ;
```

Phase 1 `Hole` has no native position field; `CENTER` is desugared through the
command protocol (see ADR-018).

### HOLE_PATTERN

```text
hole_pattern = "HOLE_PATTERN" identifier subtype NL field* ;
subtype = "CIRCULAR" | "GRID" | "CORNER" ;

CIRCULAR fields:
  "CENTER" number number    (optional, default 0 0; the feature's native center)
  "PCD"    number           (required)
  "COUNT"  number           (required; positive integer)
  "DIA"    number           (required)
  "START_ANGLE" number      (optional, default 0; degrees)

GRID fields:
  "CENTER" number number    (optional; desugars to implicit center bindings)
  "ROWS" number             (required; positive integer)
  "COLUMNS" number          (required; positive integer)
  "SPACING_X" number        (required)
  "SPACING_Y" number        (required)
  "DIA" number              (required)

CORNER fields:
  "BASE" identifier         (optional; must reference a declared BASE_RECT)
  "OFFSET_X" number         (required)
  "OFFSET_Y" number         (required)
  "DIA" number              (required)
```

The pattern is always one `HolePattern` feature; the DSL never expands it.

### CONSTRUCTION_CIRCLE

```text
construction_circle = "CONSTRUCTION_CIRCLE" identifier NL field* ;
  field = "CENTER" number number NL  (optional; desugars like HOLE)
        | "DIA" number NL            (required) ;
```

### BIND

```text
binding = "BIND" identifier slot "<-" identifier NL ;
slot    = "WIDTH" | "HEIGHT" | "CENTER_X" | "CENTER_Y" | "DIAMETER"
        | "RADIUS" | "LENGTH" | "COUNT" | "PCD" | "START_ANGLE"
        | "ROWS" | "COLUMNS" | "SPACING_X" | "SPACING_Y"
        | "OFFSET_X" | "OFFSET_Y" ;
```

`slot` maps one-to-one to the strong-typed `FeatureParameterSlot`; slot/feature
compatibility is enforced by the engineering layer at apply time.

### CONSTRAINT

```text
constraint = "CONSTRAINT" identifier kind args NL ;
kind = "FIXED" feature
     | "SYMMETRIC" feature feature
     | "ALIGNED" feature feature
     | "TANGENT" feature feature
     | "DISTANCE" feature feature number ;
```

`CONCENTRIC` is recognized but disabled in v1 (unsupported, never silently
ignored).

### DRAWING

```text
drawing = "DRAWING" property NL ;
property = "SCALE" number
         | "SHOW_CENTERLINES" boolean
         | "SHOW_CONSTRUCTION_GEOMETRY" boolean
         | "PROJECTION_METHOD" ("FIRST_ANGLE" | "THIRD_ANGLE") ;
boolean = "TRUE" | "FALSE" | "true" | "false" ;
```

## Expression grammar

```text
expression    = additive ;
additive      = multiplicative { ("+" | "-") multiplicative } ;
multiplicative= primary { ("*" | "/") primary } ;
primary       = number | "-" number | identifier | "(" expression ")" ;
```

Precedence: `*`/`/` bind tighter than `+`/`-`; all operators are
left-associative. Parentheses override precedence. Expressions are converted
to the structured `Expression` AST — never eval'd from a string.

## Known-but-disabled v1 keywords

`FILLET`, `SLOT`, `CONCENTRIC` are recognized keywords that fail with
`SCC-DSL-009` (`UnsupportedDslFeature`); they are never silently ignored,
because the underlying Phase 3 lowering cannot lower them correctly.

## Reference rules

- A parameter, feature, or base profile must be **declared before use** in the
  document (matches how `CommandExecutor::apply_all` executes the commands).
- `PARAM x := <expr>` validates every referenced parameter is already declared.
- `BIND` validates the feature and parameter are already declared.
- Slot/feature validity is checked by the engineering layer (`SCC-ENG-028`),
  never duplicated in the DSL.
- The whole document is applied atomically (working-copy + commit).

## Security boundary

The DSL can never execute code, access files, the network, environment
variables, or include other files. There is no `INCLUDE`/`IMPORT`/`EXEC`/
`SCRIPT`, and no JSON/DSL hybrid.
