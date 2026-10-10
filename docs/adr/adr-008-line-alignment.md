# ADR-008: Explicit Line Alignment Semantics

- Status: Accepted for v0.3.0
- Date: 2026-07-29
- Owners: SCC architecture

## Context

The Proposal language already preserved aligned_with through Semantic IR and lowering as ConstraintKind::Align, but the M3 solver rejected every active Align constraint. The relation also lacked a precise geometric contract: alignment could mean equal direction, collinearity, endpoint coincidence, or an instruction to move one entity.

M3 needs a deterministic subset that can be validated from GraphState without inventing coordinates or mutating source geometry.

## Decision

### 1. Align means Line-to-Line collinearity

The v0.3.0 Align subset accepts only two Line entities and an empty constraint parameter map. Both lines must have parallel or anti-parallel direction vectors and lie on the same infinite supporting line. Endpoint order and segment overlap do not affect alignment.

The semantic analyzer rejects aligned_with when either endpoint is not a Line. Runtime states that bypass analysis receive SCC-GEOM-005 for unsupported entity kinds or unknown parameters.

### 2. Align validates explicit geometry

Each line must already provide x1, y1, x2, and y2 through GraphState. Align does not translate, rotate, resize, or otherwise construct either line, and it never invents an endpoint or direction.

Missing endpoint parameters retain SCC-GEOM-002. A zero-length aligned line has no direction and returns SCC-GEOM-003. A supported pair that is not collinear returns SCC-GEOM-006 on the source entity. Failed solving writes no GeometryModel.

### 3. Validation is normalized and deterministic

The solver normalizes both direction vectors. It compares the absolute two-dimensional cross product of the normalized directions and the perpendicular offset from the target supporting line against 1e-9. Anti-parallel directions therefore pass while parallel offset lines fail.

Validation iterates the existing deterministic constraint map after primitive construction. A successful Align contributes to solve_report.evaluated_constraints. Because the accepted subset requires explicit endpoints and performs no construction, successful models retain zero remaining degrees of freedom and exact deterministic convergence.

## Compatibility

- No Proposal, ConstraintIR, GraphState, GeometryModel, or structured-error field is added.
- Valid Line-to-Line aligned_with input changes from SCC-GEOM-005 to successful validation.
- Non-collinear Line pairs now produce SCC-GEOM-006 instead of the generic unsupported-constraint error.
- Analysis is intentionally tightened so Circle, Arc, Rectangle, Polygon, and Point alignment is rejected before lowering.
- GraphState remains read-only and serialization ordering is unchanged.

## Rejected alternatives

- Treat Align as Parallel: accepts offset lines and loses the stronger alignment meaning.
- Require equal direction instead of parallel or anti-parallel direction: makes endpoint ordering semantically significant.
- Require overlapping segments or coincident endpoints: adds constraints not declared by aligned_with.
- Move or rotate the subject automatically: invents construction choices and makes results order-dependent.
- Apply Align to any entity with a conceptual axis: the current primitive contract does not expose a single unambiguous axis for those types.

## Follow-up

- Consider construction-capable alignment only after the Proposal language can declare which entity may move and which coordinates remain authoritative.
- Add scale-aware tolerance policy if M3 later supports coordinates whose magnitude makes the fixed 1e-9 threshold insufficient.
