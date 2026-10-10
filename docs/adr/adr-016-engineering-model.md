# ADR-016: EngineeringModel — the authoritative design state layer

- Status: Accepted
- Date: 2026-08-11

## Background

Phase 1 (`scc-engineering`) established `EngineeringCommand`, a stable operation
protocol. Phase 2 adds the state layer that those commands act on: an
`EngineeringModel` that deterministically remembers one part, its features,
parameters, constraints, feature→parameter bindings, and drawing properties, and
resolves parameter expressions. The goal is a reliable way to "remember a
mechanical part" without any LLM participating in state maintenance or
arithmetic.

## Decisions

### Decision 1 — EngineeringModel is the single authoritative engineering state

`crates/scc-engineering/src/model.rs::EngineeringModel` is the only engineering
state in this layer. Derived structures — the parameter dependency graph,
resolved values, validation reports — are computed on demand and are never a
second persistent truth. No `FeatureGraphState` / `EngineeringRuntimeState` /
`EngineeringDocumentState` duplicates exist.

### Decision 2 — all state changes arrive through EngineeringCommand

`CommandExecutor` (`src/executor.rs`) is the only path that mutates the model.
It applies each command atomically (validate-before-write for single commands;
working-copy-plus-commit for `apply_all` batches). Parameters were initially
created via `EngineeringModel::create_parameter` as a setup API; the Phase 4
Protocol Completeness Gate added `DeclareParameter` and `BindFeatureParameter`
commands so that a parameterized model can be built from an empty state purely
through `CommandExecutor` (the direct model methods remain as the executor's
delegation targets and keep their validation).

### Decision 3 — expressions use the structured AST, never dynamic evaluation

Phase 1's `Expression` AST is evaluated by `src/evaluator.rs` with no `eval()`,
shell, or dynamic code. References resolve through the parameter dependency
graph; `Unresolved` propagates instead of being guessed; division by zero,
overflow to non-finite, and missing references produce structured errors, never
panics.

### Decision 4 — the parameter dependency graph must be acyclic

`ParameterDependencyGraph` (`src/dependency.rs`) is derived from model
parameters. `SetExpression` checks the *would-be* graph for a cycle before
writing; `validate_model` and evaluation reject any cycle. Cycle detection uses
an iterative white/gray/black DFS, so no recursion stack overflow is possible.

### Decision 5 — HolePattern stays a pattern in EngineeringModel

Circular / Grid / Corner hole patterns remain parametric pattern features in the
model. They are never expanded into `Hole01..HoleNN`; expansion is a lowering
(Phase 3) concern.

### Decision 6 — EngineeringModel does not depend on SemanticIR / ConstraintIR / GeometryModel

The model stores engineering intent only. No `EngineeringModel → SemanticIR` /
`ConstraintIR` / `GeometryModel` lowering, no SVG/DXF/PDF, and no
CadQuery/OCCT/3D in this phase.

### Decision 7 — removing a referenced feature is rejected, not cascaded

`RemoveFeature` refuses to delete a feature referenced by a constraint,
reporting the target feature and the reference location (`SCC-ENG-013`). Feature
→ feature references are carried by `EngineeringConstraint`; feature → parameter
references are carried by the explicit, verifiable `feature_parameters` binding.
Cascade deletion is deferred.

## Consequences

- Determinism: all persisted state uses `BTreeMap`/`BTreeSet`; no `HashMap`,
  no random UUIDs, no timestamps. Identical command sequences produce identical
  models and identical serialization.
- `SetParameter` overwrites an expression with a direct value, sets origin to
  `User`, and clears the expression relation (`SCC-ENG` semantics documented in
  `src/executor.rs`).
- `Unresolved` is preserved through model storage, validation, and evaluation.
- New error codes `SCC-ENG-009`..`SCC-ENG-024` are registered in
  `docs/error-codes.md`.

## Compatibility

Additive. Phase 1 types (`EngineeringCommand`, `EngineeringFeature`,
`ParameterValue`, `Expression`, `EngineeringConstraint`, `DrawingProperty`,
IDs, `Point2D`) are unchanged in shape; `Expression` gained
`referenced_parameters()`, `EngineeringConstraint` gained
`referenced_features()`/`references_feature()`, and `DrawingProperty` gained
`kind()` plus `DrawingPropertyKind` — all additive. Existing command serde
output is unchanged.

## Phase 3 amendment: feature→slot→parameter binding

Phase 3 (ADR-017 Decision 4) discovered that `feature_parameters:
BTreeMap<FeatureId, BTreeSet<ParameterId>>` could not express which field of a
feature a parameter drives (`plate_width` → `BaseProfile.width` vs `height`).
The field was upgraded to `BTreeMap<FeatureId, BTreeMap<FeatureParameterSlot,
ParameterId>>` with a strongly-typed `FeatureParameterSlot`. This is a deliberate
serde-shape correction required by lowering; all Phase 2 tests were updated to
the slot form.

## Migration plan

None required. Phase 3 (`EngineeringLowering`) consumes the model read-only and
lowers engineering features/constraints through the formal SCC pipeline (see
ADR-017).
