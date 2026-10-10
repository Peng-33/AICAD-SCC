# ADR-010: Explicit Line Perpendicularity Semantics

- Status: Accepted for v0.3.0
- Date: 2026-07-29
- Owners: SCC architecture

## Context

The Proposal language, Semantic IR, analysis, and lowering layers already preserve perpendicular_to as ConstraintKind::Perpendicular and restrict it to Line entities. After Align and Parallel validation were implemented, Perpendicular remained the final unsupported basic Line direction relation in the planned deterministic sequence.

The relation needs a precise boundary between validating an explicitly declared right angle and constructing one by rotating or moving geometry. M3 can safely validate the former from GraphState but lacks authority and remaining-coordinate semantics for the latter.

## Decision

### 1. Perpendicular means orthogonal unoriented Line directions

The v0.3.0 Perpendicular subset accepts only two Line entities and an empty constraint parameter map. Their normalized direction vectors must have a dot product of zero within tolerance. Reversing either line endpoint order does not change the result.

The line segments may be offset and need not intersect within their finite extents. The relation constrains direction only; it does not add endpoint coincidence, intersection placement, or length equality.

The semantic analyzer continues to reject perpendicular_to when either endpoint is not a Line. Runtime states that bypass analysis receive SCC-GEOM-005 for unsupported entity kinds or unknown parameters.

### 2. Perpendicular validates explicit geometry

Each line must already provide x1, y1, x2, and y2 through GraphState. Perpendicular does not rotate, translate, resize, or otherwise construct either line.

Missing endpoint parameters retain SCC-GEOM-002. A zero-length line has no direction and returns SCC-GEOM-003. A supported pair whose directions are not orthogonal returns SCC-GEOM-006 on the source entity. Failed solving writes no GeometryModel.

### 3. Validation is normalized and deterministic

The solver normalizes both line direction vectors and compares the absolute two-dimensional dot product against 1e-9. This is independent of line length, position, and endpoint ordering.

Validation iterates the deterministic constraint map after primitive construction. A successful Perpendicular contributes to solve_report.evaluated_constraints. Because this subset requires explicit endpoints and performs no construction, successful models retain exact deterministic convergence and zero remaining degrees of freedom.

## Compatibility

- No Proposal, ConstraintIR, GraphState, GeometryModel, or structured-error field is added.
- Valid Line-to-Line perpendicular_to input changes from SCC-GEOM-005 to successful validation.
- Nonperpendicular explicit Line pairs now return SCC-GEOM-006 instead of the generic unsupported-constraint error.
- Existing analysis, lowering, and Parallel-versus-Perpendicular MCS conflict detection remain unchanged.
- GraphState remains read-only and serialization ordering is unchanged.

## Rejected alternatives

- Require finite segment intersection: adds positional constraints that perpendicular_to does not declare.
- Require a specific clockwise or counterclockwise orientation: makes endpoint order semantically significant.
- Infer Perpendicular from a fixed 90-degree endpoint transformation: invents which endpoint and entity may move.
- Rotate the subject automatically: leaves translation and endpoint placement underconstrained and introduces an undeclared authority choice.
- Treat near-right-angle explicit geometry as an underconstrained success: emits geometry that violates the declared relation.

## Follow-up

- Add construction-capable directional relations only after the Proposal language declares which entity may rotate and how its remaining coordinates are constrained.
- Consolidate scale-aware angular tolerance policy if later solver modes require a threshold other than the fixed 1e-9 normalized dot-product tolerance.
- Expand relation property tests when randomized geometry generation is introduced.
