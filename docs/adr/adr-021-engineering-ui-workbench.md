# ADR-021: Engineering UI Workbench

- Status: Accepted
- Date: 2026-08-11
- Related: ADR-016 (EngineeringModel), ADR-017 (lowering + SourceMap), ADR-019
  (tools), ADR-020 (intent), `apps/scc-ui/public/js/engineering-workbench.js`

## Background

Phases 1–6 made the engineering pipeline deterministic and LLM-safe. Phase 7
adds the human-operable workbench: a Feature Tree, Properties / Relations /
Constraints / Review panels, parameter editing, UNRESOLVED resolution, natural
language edits, undo/redo, and a 2D preview — all driven by the real
EngineeringModel.

## Decisions

### Decision 1 — EngineeringModel stays the only engineering state

The UI keeps a serialized model snapshot plus UI-only state (selection,
panels, zoom). No `UiParameterModel` / `FrontendFeatureState` duplicates exist.

### Decision 2 — every engineering change goes through Tool/Command

Parameter edits, UNRESOLVED resolution, and NL edits all become tool calls
applied by the backend (`/api/engineering/tool`, `/api/engineering/intent`).
The UI never writes model JSON or mutates engineering fields.

### Decision 3 — the front end never computes derived values

Derived values and dependency lists come from the backend
`EngineeringWorkbenchView` / evaluator. The UI renders, it does not evaluate.

### Decision 4 — derived parameters are read-only by default

`WorkbenchParameter.editable` is derived in Rust: `derived` and
`ai_assumption` origins are not editable. There is no "type 15 and silently
clear the expression" path.

### Decision 5 — UNRESOLVED is explicit, never shown as 0

The view carries `UNRESOLVED` and the Review panel surfaces it; resolution is a
`set_parameter` tool call that re-solves.

### Decision 6 — WorkbenchView is a derived, read-only view

`EngineeringModel::workbench_view()` produces it; there is no write path back
into the model from the view.

### Decision 7 — geometry selection uses the real SourceMap inverse

`EngineeringSourceMap::entity_to_feature()` is derived in Rust; the UI maps an
SCC entity to an engineering feature by map lookup, never by parsing entity-id
strings.

### Decision 8 — pattern-derived entities select the pattern feature

Array-derived circles resolve to the single `HolePattern` feature, which stays
one feature in the model.

### Decision 9 — a failed mutation leaves model and preview unchanged

The backend applies tool calls atomically (`apply_all`); on failure the client
keeps its last snapshot and preview.

### Decision 10 — raw Engineering JSON is not the normal-user editing surface

The workbench shows engineering concepts. Raw model JSON stays in the
developer/debug mode of the existing UI.

### Decision 11 — Phase 7 reuses the existing UI stack

No new front-end framework, no theme rewrite, no overall layout overhaul; the
workbench is an additive module and page.

## Consequences

- The mounting-plate fixture (with `hole_diameter := plate_height / 8` and
  UNRESOLVED offsets) is visible in the workbench; editing `plate_width`
  re-lowers and re-solves, and derived values update without front-end math.
- Undo/redo restores model snapshots and re-derives everything.
- Intent edits reuse Phase 6 `runIntent`; there is exactly one intent service.

## Technical debt

Carried forward: Constraint SourceMap counter prediction, ConstructionCircle
export semantics, complex-part two-pass planner. New deferred items:
Derived→direct explicit conversion, GUI feature creation, direct canvas drag,
advanced constraint authoring.
