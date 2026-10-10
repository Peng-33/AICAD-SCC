# ADR-004: M3 Geometry Boundary and Model Contract

- Status: Accepted for v0.3.0 foundation
- Date: 2026-07-28
- Owners: SCC architecture

## Context

The frozen architecture requires M3 to consume GraphState read-only and to remain independent from the M2 state machine. The removed historical solver prototype consumed ConstraintIR directly and the removed CAD store returned mock bytes, so neither is a valid M3 foundation.

During v0.3.0 planning, the implementation exposed a contract gap: numeric constraint parameters survived lowering into ConstraintIR but were discarded when M2 built RuntimeConstraint. Dimension kinds were also collapsed into a self-Distance constraint with only a generic `value`, so M3 could not distinguish diameter, radius, width, height, length, or offset from GraphState.

## Decision

### 1. Independent crate

M3 is implemented in a new `scc-geometry` crate. It depends on `scc-core` only. `scc-execution` does not depend on any geometry solver or CAD library.

### 2. Read-only input boundary

The public solver boundary accepts `&GraphState` and never mutates it. JSON consumers accept legacy GraphState JSON and schema-1.x GraphState envelopes. The declared baseline is:

```rust
pub const SUPPORTED_GRAPH_STATE_SCHEMA: &str = "1.0";
```

### 3. Parameter preservation

`RuntimeConstraint` preserves the numeric parameter map from ConstraintIR. The field uses `serde(default)` and omits empty maps when serializing, so existing v0.2 GraphState JSON remains readable and unchanged for constraints without parameters.

Lowered dimensions retain both `value` and one semantic parameter key: `diameter`, `radius`, `width`, `height`, `length`, or `offset`. M2 emits deterministic SetParameter commands for those self-Distance dimension carriers, making the values available on RuntimeEntity without moving solver logic into M2.

### 4. GeometryModel output

`GeometryModel` records the source root and step and stores entities in a BTreeMap. Geometry primitives are Point, Line, Circle, Arc, Rectangle, and Polygon. The schema-envelope document type is `geometry_model`.

### 5. Strict support matrix

The foundation solver constructs explicitly parameterized Point, Line, Circle, Arc, and Rectangle geometry. Fixed entities with no `x`/`y` coordinates use the origin. Polygon construction and active relation solving are deferred.

Unsupported active constraints are rejected with a structured error. They are never silently ignored. The foundation accepts Fixed, Datum, and self-Distance dimension carriers only.

Anchor preservation, coordinate conventions, and the first CenterOf implementation extend this foundation through [ADR-005](adr-005-anchor-coordinate-semantics.md).

### 6. Error and CLI contract

Geometry failures use the `geometry` error stage and `SCC-GEOM-NNN` codes. `scc solve` returns exit code 4 for geometry-stage failures. Exit codes 0 through 3 retain their v0.2 meanings.

### 7. Determinism

All output maps use deterministic ordering. The same GraphState produces byte-identical GeometryModel JSON. Tests verify that solving leaves the source GraphState unchanged.

## Compatibility

- All workspace crates advance to version 0.3.0.
- v0.2 GraphState JSON deserializes because new runtime constraint parameters default to empty.
- v0.3 ConstraintIR includes semantic dimension keys, so canonical execution roots and Golden fixtures intentionally change.
- The new Geometry ErrorStage variant is a Rust source-compatibility change for exhaustive matches.

## Phased implementation

1. Foundation: parameter bridge, GeometryModel, strict constructor, `scc solve`, structured errors.
2. Constraint solving: CenterOf and two-point Distance, followed by Align, Parallel, and Perpendicular.
3. Primitive coverage: Polygon, arrays, tangent geometry, and arc-specific constraints.
4. Export: deterministic SVG first, then a documented DXF subset.
5. Production hardening: convergence policy, performance baselines, fuzzing, and large-model tests.

## Rejected alternatives

- Reading ConstraintIR directly in M3: violates the frozen M2/M3 boundary.
- Reintroducing solver code inside scc-execution: couples state transitions to geometry and breaks crate responsibilities.
- Emitting placeholder CAD bytes: creates false success and cannot be validated geometrically.
- Ignoring unsupported constraints: can emit geometry that contradicts the symbolic state.
