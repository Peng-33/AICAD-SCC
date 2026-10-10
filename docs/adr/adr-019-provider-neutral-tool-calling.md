# ADR-019: Provider-neutral tool calling layer

- Status: Accepted
- Date: 2026-08-11
- Related: ADR-015, ADR-016, ADR-017, ADR-018, `crates/scc-tools`

## Background

Phases 1–4 established `EngineeringCommand` → `EngineeringModel` →
`EngineeringLowering` → SCC, and the DSL as the first text front end. Phase 5
adds the second controlled front end: a provider-neutral tool calling layer
that a future LLM/agent (Phase 6) can invoke to make bounded engineering
operations. It is built and tested entirely without an LLM.

## Decisions

### Decision 1 — Tool calling is a second controlled front end for EngineeringCommand

`ToolCall → validation → ToolAdapter → EngineeringCommand[]`. The tool layer
has no state and no CAD business logic of its own; both front ends (DSL and
tools) meet at `EngineeringCommand`.

### Decision 2 — the tool adapter never modifies EngineeringModel directly

`adapt_tool_call` returns commands; only `CommandExecutor::apply_all` mutates
the model.

### Decision 3 — one tool call may map to many commands, applied atomically

A composite tool (e.g. `add_hole`) emits a command sequence; `apply_tool_call`
and `apply_tool_calls_atomically` both run working-copy + commit, so any
failure leaves the model completely unchanged.

### Decision 4 — ToolDefinition is provider-neutral

Internal schemas carry no vendor fields. A Phase 6 provider adapter maps them
to OpenAI/local/other schemas; the tool layer itself depends on no LLM SDK.

### Decision 5 — the tool surface exposes feature-level operations, never SCC internals

Tools speak Part/Feature/Parameter/Constraint/Drawing. Proposal, SemanticIR,
ConstraintIR, GraphState, EntityId, MCS, Overlay, and JSON persistence are not
exposed.

### Decision 6 — patterns stay high-level; no arrays

Tool arguments support no arrays in v1. A circular pattern is
`pcd`/`count`/`start_angle`, never a `points` array; the adapter never expands
patterns into hole entities.

### Decision 7 — tool arguments never carry a full EngineeringModel

There is no `replace_model`/`features:[...]`/`constraints:[...]` tool. Each
call expresses one bounded operation.

### Decision 8 — unsupported engine capabilities are not exposed as tools

No `add_slot`, `add_fillet`, or concentric tool, because Phase 3 lowering
cannot lower them (`Available Tools == Executable Capabilities`).

### Decision 9 — the tool registry is deterministic

`BTreeMap`-backed, duplicate registration rejected, `list()` in stable sorted
order — important for reproducible Phase 6 provider schema input.

### Decision 10 — DSL and tools produce equivalent models, not two state worlds

A shared fixture proves `DSL → Model A` and `Tools → Model B` yield `A == B`
and identical geometry. Where a feature has no native position (Hole, Grid,
ConstructionCircle), both front ends use the same center desugaring
(`{id}_center_x`/`{id}_center_y` parameters bound to `CenterX`/`CenterY`).

## Expression reuse (Phase 4)

`set_expression` accepts a DSL expression string (`"plate_width / 8"`) and
parses it through `scc-dsl`'s expression parser — never `eval()`. This
introduces the dependency `scc-tools → scc-dsl` (option A from the Phase 5
brief); the reverse dependency is forbidden. No shared-parser refactor was
undertaken because the DSL parser is tightly coupled to its lexer/parser.

## Authority / origin policy

Tools do not let a caller choose `AuthorityLevel` or arbitrary
`ParameterOrigin`. `declare_parameter` fixes `origin = User`, `authority =
L0`; `set_expression` results in `origin = Derived` via the existing
`CommandExecutor` rule. No `AiAssumption` tool exists in this phase.

## Consequences

- The full chain works with no LLM: Tool → commands → model → lowering → SCC →
  GeometryModel.
- Malformed calls are rejected before any command is built; engineering and
  SCC errors are preserved (`CommandApplicationFailed` keeps its source as the
  structured error's `source` chain).

## Technical debt

Unchanged from earlier phases: the constraint `SourceMap` still predicts
`scc-lowering`'s generated ids, and `ConstructionCircle` export still treats it
as an ordinary circle (construction-line styling belongs to the Drawing Engine
phase).
