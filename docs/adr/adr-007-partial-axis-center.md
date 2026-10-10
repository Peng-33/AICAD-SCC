# ADR-007: Partial-Axis CenterOf and Coordinate Resolution

- Status: Accepted for v0.3.0
- Date: 2026-07-29
- Owners: SCC architecture

## Context

ADR-005 defined two-axis CenterOf and deliberately rejected single-axis input until degrees-of-freedom reporting existed. ADR-006 then introduced SCC-GEOM-007 and successful solve reports. The remaining gap was that axis = x or axis = y survived Proposal through GraphState but M3 still returned SCC-GEOM-005 without evaluating the selected coordinate.

Treating a partial-axis relation as full centering would add an undeclared constraint. Emitting a primitive with an invented value for the other coordinate would create the same false-success problem corrected for Point Distance.

## Decision

### 1. CenterOf is evaluated per coordinate axis

CenterOf with axis_x = 1 constrains only the source geometric center X coordinate to equal the target geometric center X coordinate. axis_y = 1 has the corresponding Y behavior. Both markers, or the legacy empty parameter map, constrain both axes.

The source coordinate on an unselected axis must come from another supported source:

- an explicit x or y entity parameter
- a Fixed anchor
- another CenterOf constraint selecting that axis
- zero-distance coincidence for Point entities

Two independent partial CenterOf constraints may therefore combine into a fully positioned entity. A selected axis is checked independently for conflicting candidates within 1e-9; an unselected axis is not compared with the target.

### 2. Position resolution is axis-based

M3 memoizes and resolves coordinates by the key entity ID plus axis. Cycle detection is also axis-specific. Rectangle x and y parameters remain lower-left primitive coordinates and are converted independently to geometric-center coordinates using half the width or height.

CenterOf remains directional for construction: the source derives a selected coordinate from the target. M3 does not reverse the relation to position an unresolved target from its source.

### 3. Degrees-of-freedom behavior

Before primitive construction, every entity that is the source of an active partial CenterOf is checked on both axes. Each unresolved coordinate contributes one remaining degree of freedom.

If any coordinate remains unresolved, solving returns SCC-GEOM-007 with deterministic entity_ids and details.remaining_degrees_of_freedom. The CLI retains geometry exit code 4 and writes no GeometryModel.

If both axes are resolvable, normal primitive construction and conflict validation proceed. Successful GeometryModel output retains fully_constrained, deterministic_direct, exact, and remaining = 0 in solve_report.

### 4. Strict axis validation

A CenterOf axis map is supported only when it is empty for the legacy two-axis form or contains one or both known markers axis_x and axis_y with value 1. Unknown keys, empty explicit selections, other values, and extra parameters return SCC-GEOM-005.

## Compatibility

- No Proposal, ConstraintIR, GraphState, GeometryModel, or structured-error field is added.
- Existing two-axis and legacy empty-parameter CenterOf behavior is unchanged.
- Single-axis CenterOf changes from unconditional SCC-GEOM-005 to either successful solving, SCC-GEOM-006 for a selected-axis conflict, or SCC-GEOM-007 for unresolved coordinates.
- Unknown axis selections remain SCC-GEOM-005.
- The source GraphState remains read-only and output ordering remains deterministic.

## Rejected alternatives

- Treat a single selected axis as both axes: violates Proposal intent.
- Fill the unselected coordinate with zero: emits false success.
- Serialize partially positioned primitives: weakens the GeometryModel primitive contract.
- Reverse CenterOf automatically: changes the directional construction model and can make dependency resolution order-sensitive.

## Follow-up

- Extend degrees-of-freedom analysis to Line rotation for construction-capable Parallel and Perpendicular relations.
- Add property tests for multi-relation axis graphs and cycle combinations.
