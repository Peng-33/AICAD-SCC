# ADR-018: AICAD Engineering DSL v1

- Status: Accepted
- Date: 2026-08-11
- Related: ADR-015, ADR-016, ADR-017, `docs/specs/aicad-engineering-dsl-v1.md`

## Background

Phases 1–3 established `EngineeringCommand` → `EngineeringModel` →
`EngineeringLowering` → SCC → `GeometryModel`. Phase 4 adds the first controlled
text front end: a declarative engineering DSL (`crates/scc-dsl`) whose only
output is `EngineeringCommand`. It proves that AICAD can describe an
engineering design without JSON arrays, LLM-generated structured data, or
arbitrary code.

## Decisions

### Decision 1 — the DSL is a text front end for EngineeringCommand, not a new state layer

`parse → DslDocument AST → EngineeringCommand[]`. The DSL owns no state
(no `DslRuntime`/`DslModel`/`DslGraph`); the engineering truth stays in
`EngineeringModel`.

### Decision 2 — the parser never modifies EngineeringModel

The lexer/parser produce a pure AST. Only `CommandExecutor` (via
`apply_all`) mutates the model.

### Decision 3 — DSL v1 is declarative, not a programming language

No `if`/`loop`/`function`/`macro`/user script / arbitrary expression eval.
Expressions are limited to `+ - * /` and parentheses.

### Decision 4 — DSL v1 exposes only reliably-lowerable features

`BASE_RECT`, `HOLE`, `HOLE_PATTERN` (CIRCULAR/GRID/CORNER),
`CONSTRUCTION_CIRCLE` are supported. `SLOT`, `FILLET`, `CONCENTRIC` are
recognized but disabled and fail closed (`SCC-DSL-009`).

### Decision 5 — expressions parse to the structured Expression AST

`DslExpression` is converted by the command builder to
`scc_engineering::expression::Expression`; no `eval()` or string storage.

### Decision 6 — feature↔parameter relations use the strong-typed FeatureParameterSlot

`BIND` maps DSL slot keywords to `FeatureParameterSlot` one-to-one; slot/feature
compatibility is enforced by the engineering layer (`SCC-ENG-028`), never
duplicated in the DSL, and never inferred from parameter id strings.

### Decision 7 — the whole document applies atomically

`apply_document` compiles and runs `CommandExecutor::apply_all`
(working-copy + commit): any failing command leaves the original model
completely unchanged.

### Decision 8 — DSL v1 has a stable version header

`AICAD 1` (or `VERSION 1`) is required as the first statement; other versions
are rejected (`SCC-DSL-010`).

### Decision 9 — unsupported keywords/features fail closed

Unknown keywords (`SCC-DSL-003`), disabled features (`SCC-DSL-009`), and
unparseable documents are rejected with positions, never skipped.

## Protocol amendment (Phase 4)

The Protocol Completeness Gate proved the Phase 1–3 command set could not build
a parameterized model from an empty state purely through commands. Two
**backward-compatible** command variants were added (see ADR-015/016
amendments):

- `EngineeringCommand::DeclareParameter { id, value, origin, authority }`
- `EngineeringCommand::BindFeatureParameter { feature, slot, parameter }`

Both delegate to the model's existing atomic validation; the DSL emits them for
`PARAM` and `BIND`.

## Consequences

- A full design can be described as plain text and rebuilt deterministically:
  DSL → commands → model → lowering → Proposal → SCC → GeometryModel.
- The DSL surface intentionally equals the executable capability: no "parse ✓,
  lower ✗" fake support.
- `HOLE`/`CONSTRUCTION_CIRCLE`/`GRID` accept `CENTER` by desugaring it into
  implicit `{id}_center_x`/`{id}_center_y` parameters bound to the
  `CenterX`/`CenterY` slots — position flows through the formal protocol.
  `HOLE_PATTERN CIRCULAR` stores `CENTER` in its native center field.

## Compatibility

The new commands are additive; existing command serde output is unchanged. The
DSL crate depends only on `scc-engineering` (and `scc-core` for authority),
never the reverse.

## Migration plan

None. `canonical formatting` (`format_document`) is explicitly deferred: it is
valuable for LLM round-tripping but out of scope for v1 (recorded here so it is
not forgotten).

## Technical debt

- Constraint `SourceMap` still predicts `scc-lowering`'s generated constraint
  ids instead of consuming mappings returned by the actual lowerer. Long-term,
  the source map must use the lowerer's own mappings (see ADR-017).
- `ConstructionCircle` semantics are preserved in `EngineeringModel`, but the
  existing SVG/DXF export receives it as an ordinary circle; true
  construction-line styling/filtering belongs to the Drawing Engine phase.
