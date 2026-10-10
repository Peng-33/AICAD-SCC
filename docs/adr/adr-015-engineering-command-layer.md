# ADR-015: Engineering command layer and the `Engineering` error stage

- Status: Accepted
- Date: 2026-08-10

## Background

AICAD Phase 1 introduces `crates/scc-engineering`: the unified
`EngineeringCommand` operation protocol that every upper-layer entry point
(DSL, tool calling, natural language, UI, mouse editing) must eventually lower
into. The layer validates each command in isolation and reports failures
through the existing ADR-002 structured error contract.

## Decision

- Add an additive `ErrorStage::Engineering` variant (serde string
  `"engineering"`, code prefix `ENG`) to `scc_core::error::ErrorStage`.
- `crates/scc-engineering` owns an `EngineeringError` enum that converts to
  `scc_core::StructuredError` with `SCC-ENG-{NNN}` codes registered in
  `docs/error-codes.md`.
- `scc-engineering` depends only on `scc-core` and deliberately does **not**
  reuse `scc_geometry::Point2`, `scc_core::runtime::ParameterValue`, or
  `scc_core::constraint::ConstraintKind`: engineering IDs target features, not
  2-D entities, and the layer must stay upstream of the geometry solver.

## Rejected alternatives

- **Reuse `EntityId` / `ConstraintKind` directly.** The engineering layer
  references features (which lower to *many* entities) and expresses higher-level
  intents (e.g. concentricity) with no direct solver equivalent. Binding the
  command protocol to the solver vocabulary would couple the two layers
  (violating the EngineeringCommand ≠ SCC Mutation separation).
- **Keep engineering errors outside the structured contract.** A standalone
  `EngineeringError` without `StructuredError` integration would break the
  machine-readable `SCC-{STAGE}-{NNN}` CLI protocol that all other stages follow.
- **Reuse `scc_geometry::Point2`.** This would make `scc-engineering` depend on
  the geometry crate, reversing the intended upstream→downstream dependency.

## Impact

- `ErrorStage` gains one variant. All existing usages are non-exhaustive
  matches or equality comparisons; no exhaustive match requires updating.
- Existing documents serialize with their existing stage strings; `"engineering"`
  is only emitted by new code, so no golden fixture changes.
- The engineering crate is not wired into the CLI or the proposal→geometry
  pipeline in Phase 1, so the existing 2-D pipeline is unaffected.

## Compatibility

Additive and backward compatible. `SCC-ENG-001`..`SCC-ENG-008` are new codes in
the `engineering` stage; legacy documents and CLI JSON output formats are
unchanged.

## Migration plan

None required. Phase 2 (`EngineeringModel`) builds the semantic model on top of
the command protocol and will consume `EngineeringError` unchanged.

## Phase 4 amendment: parameter declaration and binding are commands

The Phase 4 Protocol Completeness Gate proved that the original nine commands
could **not** build a parameterized model from an empty state through
`CommandExecutor` alone: parameters were only creatable via the direct model
API (`EngineeringModel::create_parameter`) and bindings via
`bind_feature_parameter`. The DSL must only ever emit `EngineeringCommand`, so
the protocol was completed with two **backward-compatible** variants:

- `EngineeringCommand::DeclareParameter { id, value, origin, authority }` —
  routes through the model's existing atomic `create_parameter` validation.
- `EngineeringCommand::BindFeatureParameter { feature, slot, parameter }` —
  routes through the Phase 3 strong-typed `FeatureParameterSlot` validation.

Existing command variants and their behavior are unchanged; serde output for
existing documents is unchanged (`op`-tagged, new variants use
`declare_parameter` / `bind_feature_parameter`).
