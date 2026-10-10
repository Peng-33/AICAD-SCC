# ADR-017: EngineeringLowering — the bridge into the existing SCC pipeline

- Status: Accepted
- Date: 2026-08-11

## Background

Phases 1–2 established `EngineeringCommand` and `EngineeringModel` (the
authoritative design state). Phase 3 must move an engineering model
deterministically into the existing SCC 2-D pipeline so that a solved
`GeometryModel` (plate outline, holes) can be produced — without a new drawing
engine and without bypassing the existing semantic analysis.

## Decisions

### Decision 1 — EngineeringModel lowers to the Proposal vocabulary, not fabricated ConstraintIR

`crates/scc-engineering/src/lowering.rs` produces
[`scc_proposal::schema::Proposal`] directly. The existing
`analyze_detailed → try_lower → execute → solve` pipeline then runs unchanged:
`EngineeringModel → Proposal → scc-analysis → SemanticIr → scc-lowering →
ConstraintIr → scc-execution → GraphState → scc-geometry → GeometryModel`.
`scc-analysis` still builds the declaration graph, checks references, types,
cycles, and conflicts. The engineering layer never writes `ConstraintIr` or
`GraphState` by hand.

To position entities at explicit coordinates, the Proposal vocabulary gained
two additive dimension kinds, `x` and `y` (`SemanticDimension::X/Y`,
`PARAM_X/PARAM_Y`), which the standard `lower_dimension` turns into
`Distance(source == target)` parameters that the execution layer writes onto the
entity (`x`, `y`). The geometry solver already resolves those parameters.

### Decision 2 — EngineeringLowering is pure and deterministic

`lower_engineering_model` is a pure function of its input: no model mutation,
no network, no time, no randomness, no global state. Identical models produce
identical Proposals, source maps, and geometry (100-run determinism test).

### Decision 3 — engineering parameters are resolved before lowering

Lowering builds a short-lived [`ResolvedEngineeringView`] (`validate_model` →
evaluate all parameters in dependency order). The model keeps its source-of-truth
expressions; lowering reads resolved values only.

### Decision 4 — parameter slot binding is strongly typed, never string-inferred

The Phase 2 `FeatureId → Set<ParameterId>` relation could not say which field a
parameter drives, so it was upgraded to
`FeatureId → (FeatureParameterSlot → ParameterId)`
(`crates/scc-engineering/src/binding.rs`). `FeatureParameterSlot` is a typed
enum (width, height, center_x/y, diameter, radius, length, count, pcd,
start_angle, rows, columns, spacing_x/y, offset_x/y). Slots are validated
against the feature type at bind time (`SCC-ENG-028`). Lowering resolves a slot
as: bound parameter (resolved) → literal → origin default for position slots.
A bound-but-`Unresolved` parameter is a hard error (`SCC-ENG-027`), never a
fallback to the literal.

### Decision 5 — HolePattern stays a pattern in EngineeringModel

The model keeps one pattern feature (circular/grid/corner). The SCC pipeline
expands patterns through its existing `ArrayRelation` mechanism. Only circular
patterns with a non-zero start angle — which the SCC circular array cannot
express — are expanded deterministically in pure Rust inside lowering
(degrees → radians formula, stable IDs).

### Decision 6 — unsupported features/constraints fail closed

`Slot`, `Fillet` (`SCC-ENG-025`) and `Concentric` (`SCC-ENG-026`) have no
correct SCC mapping and are rejected with structured errors rather than silently
dropped or approximated. `ConstructionCircle` lowers to a circle entity as
reference geometry (its construction semantics stay in the model + source map).

### Decision 7 — lowering maintains a strong-typed source map

[`EngineeringSourceMap`] maps `FeatureId → BTreeSet<EntityId>` and
`EngineeringConstraintId → BTreeSet<ConstraintId>` (deterministically predicted
from the SCC lowering counter). Provenance is never recovered by parsing entity
id strings.

### Decision 8 — DrawingState does not participate in geometry lowering

Drawing properties are intentionally ignored by the Phase 3 geometry lowering;
no drawing engine is created.

## Consequences

- Constraints map to SCC relations/anchors where the vocabulary exists:
  `Symmetric → symmetric_about`, `Aligned → aligned_with`,
  `Distance → distance` (resolved value), `Tangent → tangent_to`,
  `Fixed → fixed_center` anchor. The geometry solver's own constraint coverage
  is a separate SCC limitation that surfaces as `SCC-GEOM-*` at solve time.
- New error codes `SCC-ENG-025`..`SCC-ENG-030` are registered in
  `docs/error-codes.md`.
- The `x`/`y` dimension kinds are additive: existing proposals and golden
  fixtures are unaffected; new unit tests cover their lowering.

## Compatibility

- `SemanticDimension::X/Y` and `PARAM_X/PARAM_Y` are additive variants/constants.
- `EngineeringModel.feature_parameters` changed type from
  `FeatureId → Set<ParameterId>` to `FeatureId → Slot → ParameterId`. This is a
  deliberate Phase-3-required correction (the set form could not express field
  binding); ADR-016 is annotated accordingly.

## Migration plan

None required. The engineering layer is not yet wired into the CLI or UI.
`lower_engineering_model` / `lower_and_solve` are library APIs consumed by the
Phase 3 acceptance tests; DSL (Phase 4) will build on them.

## Phase 4 note — technical debt

Two items are recorded here so they are not mistaken for complete:

- **Constraint SourceMap counter prediction.** The engineering constraint
  source map currently *predicts* `scc-lowering`'s generated constraint ids
  (`c_{N}`) from emission order. Long-term, the source map must consume
  mappings returned by the actual lowerer rather than predicting generated ids.
- **ConstructionCircle export semantics.** `EngineeringModel` correctly
  preserves construction-circle semantics, but the existing SVG/DXF export
  receives it as ordinary circle geometry. True construction-line styling /
  filtering belongs to the Drawing Engine phase.
