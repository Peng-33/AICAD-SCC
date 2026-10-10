# ADR-009: Explicit Line Parallelism Semantics

- Status: Accepted for v0.3.0
- Date: 2026-07-29
- Owners: SCC architecture

## Context

The Proposal language, Semantic IR, analysis, and lowering layers already preserve parallel_to as ConstraintKind::Parallel and restrict it to Line entities. The M3 geometry solver still rejected every active Parallel constraint.

Parallel also needs to remain distinct from Align. A parallel relation declares equal direction axes but does not require both lines to occupy the same supporting line, overlap, or share an endpoint. M3 needs a deterministic validation subset that honors this distinction without inventing a rotation or translation.

## Decision

### 1. Parallel means equal unoriented Line direction

The v0.3.0 Parallel subset accepts only two Line entities and an empty constraint parameter map. Their direction vectors must be parallel or anti-parallel. Collinear, offset, overlapping, disjoint, and oppositely ordered line segments are all valid when their directions satisfy the relation.

The semantic analyzer continues to reject parallel_to when either endpoint is not a Line. Runtime states that bypass analysis receive SCC-GEOM-005 for unsupported entity kinds or unknown parameters.

### 2. Parallel validates explicit geometry

Each line must already provide x1, y1, x2, and y2 through GraphState. Parallel does not rotate, translate, resize, or otherwise construct either line. It does not constrain the perpendicular offset between their supporting lines.

Missing endpoint parameters retain SCC-GEOM-002. A zero-length line has no direction and returns SCC-GEOM-003. A supported pair whose directions are not parallel returns SCC-GEOM-006 on the source entity. Failed solving writes no GeometryModel.

### 3. Validation is normalized and deterministic

The solver normalizes both line direction vectors and compares the absolute two-dimensional cross product against 1e-9. This makes endpoint order irrelevant while treating the relation as exact within the documented tolerance.

Validation iterates the deterministic constraint map after primitive construction. A successful Parallel contributes to solve_report.evaluated_constraints. Because this subset requires explicit endpoints and performs no construction, successful models retain exact deterministic convergence and zero remaining degrees of freedom.

## Compatibility

- No Proposal, ConstraintIR, GraphState, GeometryModel, or structured-error field is added.
- Valid Line-to-Line parallel_to input changes from SCC-GEOM-005 to successful validation.
- Nonparallel explicit Line pairs now return SCC-GEOM-006 instead of the generic unsupported-constraint error.
- Existing analysis and lowering contracts are unchanged.
- GraphState remains read-only and serialization ordering is unchanged.

## Rejected alternatives

- Reuse Align collinearity: incorrectly rejects valid offset parallel lines.
- Require equal vector orientation: makes endpoint ordering semantically significant.
- Require equal segment lengths: adds a dimension not declared by parallel_to.
- Rotate the subject automatically: invents an authority choice and leaves translation and endpoint placement underconstrained.
- Report unresolved rotational degrees of freedom while still emitting explicit geometry: contradicts the current GeometryModel success contract.

## Follow-up

- Add construction-capable directional relations only after the Proposal language declares which entity may rotate and how its remaining coordinates are constrained.
- Consolidate scale-aware directional tolerances if later geometry ranges make the fixed 1e-9 threshold insufficient.
