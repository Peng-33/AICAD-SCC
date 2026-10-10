# ADR-005: Anchor Roles and Geometry Coordinate Semantics

- Status: Accepted for v0.3.0
- Date: 2026-07-28
- Owners: SCC architecture

## Context

Proposal anchors retained distinct intent in Semantic IR, but lowering collapsed origin_point and fixed_center into generic Fixed constraints and collapsed reference_edge and datum_line into generic Datum constraints. Once execution produced GraphState, M3 could no longer distinguish those meanings.

This loss became blocking when implementing CenterOf: a rectangle fixed by its center must produce different coordinates from a legacy rectangle whose primitive origin is fixed. Solving without preserving the distinction would produce plausible but incorrect geometry.

## Decision

### 1. Preserve anchor roles additively

Constraint and RuntimeConstraint carry an optional anchor_role:

- origin_point
- fixed_center
- reference_edge
- datum_line

Lowering assigns the role, and RuntimeCommand::AddConstraint transports it unchanged into GraphState. The field uses serde(default) and is omitted when absent, so legacy ConstraintIR and GraphState documents remain readable and retain their previous serialized shape.

ConstraintKind remains the behavioral category (Fixed or Datum). The role records proposal intent; it does not create new authority levels or constraint kinds.

### 2. Coordinate conventions

GeometryModel uses these world-coordinate conventions:

- Point: position.
- Circle and Arc: geometric center.
- Rectangle: lower-left origin, plus positive width and height.
- Line: explicit start and end points.

fixed_center places the entity's geometric center at world (0, 0). For a 100 by 60 rectangle, the serialized lower-left origin is therefore (-50, -30).

origin_point places the generated Point at world (0, 0).

For compatibility, a legacy Fixed constraint with no anchor role keeps the foundation behavior: it fixes the primitive coordinate anchor at (0, 0). For Rectangle, that coordinate anchor is the lower-left origin.

Datum roles do not create coordinates. They remain accepted read-only references for later relation solvers.

### 3. CenterOf behavior

An active two-axis CenterOf constraint makes the source geometric center equal the target geometric center. The current source support matrix is Point, Circle, Arc, and Rectangle. A Line may serve as a target when its explicit endpoints define a midpoint.

Lowering preserves centered-axis selection in numeric constraint parameters:

- axis_x = 1
- axis_y = 1

Missing axis and axis = "both" produce both markers. Legacy CenterOf constraints with no axis markers are interpreted as two-axis constraints. Single-axis and unknown-axis CenterOf constraints are rejected with SCC-GEOM-005 until partial-axis degrees-of-freedom solving is implemented; they are never silently treated as two-axis constraints.

### 4. Candidate consistency

Explicit coordinates, Fixed anchors, and CenterOf relations are independent position candidates. If multiple candidates exist, every candidate must resolve to the same point within 1e-9.

- Missing position input returns SCC-GEOM-002.
- Circular CenterOf dependencies return SCC-GEOM-001.
- Contradictory candidates return SCC-GEOM-006.

### 5. Determinism and boundary

Position resolution uses deterministic GraphState ordering, memoizes resolved centers, and consumes &GraphState read-only. M3 does not write solved coordinates back into M2.

## Compatibility

- Existing JSON without anchor_role still deserializes.
- Existing empty-parameter CenterOf JSON remains a supported two-axis legacy form.
- New v0.3 ConstraintIR and GraphState output include anchor_role only for lowered anchors.
- New centered relations include axis marker parameters, so affected canonical Golden files intentionally change.
- Rust callers constructing Constraint or RuntimeConstraint literals must initialize the new optional field.

## Rejected alternatives

- New ConstraintKind variants for every anchor: expands solver dispatch and breaks exhaustive matches without improving behavioral grouping.
- Encoding anchor roles as numeric flags: loses type safety and makes GraphState harder to inspect.
- Reusing ConstraintContext.version_hint: mixes schema metadata with semantic intent.
- Treating every Fixed entity as center-fixed: changes legacy Rectangle coordinate behavior.
- Ignoring single-axis selection: can emit geometry that violates user intent.

## Follow-up

- Implement partial-axis CenterOf together with degrees-of-freedom reporting.
- Add anchored two-point Distance.
- Extend coordinate semantics for translated/rotated Line and Polygon primitives before enabling their CenterOf source behavior.
