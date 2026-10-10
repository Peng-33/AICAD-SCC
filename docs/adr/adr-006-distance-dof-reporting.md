# ADR-006: Point Distance and Degrees-of-Freedom Reporting

- Status: Accepted for v0.3.0
- Date: 2026-07-29
- Owners: SCC architecture

## Context

A two-dimensional Distance constraint fixes the separation between two points but does not, by itself, fix direction. The first implementation placed an unpositioned point on the positive X axis relative to an anchored point. That output was deterministic, but it silently added a horizontal-alignment constraint that the GraphState did not contain and therefore reported an underconstrained model as solved.

The v0.3.0 acceptance contract requires unsupported or underconstrained input to produce stable SCC-GEOM diagnostics rather than plausible false success. GeometryModel also needs a machine-readable account of convergence and remaining degrees of freedom.

## Decision

### 1. Distance does not imply direction

An active non-self Distance between Point entities is an unsigned Euclidean constraint. It never implies horizontal alignment, vertical alignment, or a positive-axis direction.

When both point positions are independently resolved by explicit coordinates, Fixed, or supported CenterOf constraints, the solver validates their Euclidean separation within 1e-9. A mismatch returns SCC-GEOM-006.

A zero-valued point distance may propagate an already resolved point position because coincidence has no directional ambiguity. Nonzero Distance never creates a position candidate.

### 2. Underconstrained distance components

Before constructing primitives, M3 analyzes each active Point-to-Point Distance connected component. Each point without an independent position starts with two translational degrees of freedom. A nonzero distance equation removes one generic degree of freedom; a zero distance removes two. Components with no independently positioned point retain the rigid-motion floor: three degrees for a component containing a nonzero distance and two for an all-zero coincident component.

If remaining degrees of freedom are greater than zero, solving stops with SCC-GEOM-007 Underconstrained. The diagnostic carries the deterministically ordered unresolved entity IDs and states the remaining count. No GeometryModel file is written by the CLI.

A distance system whose generic continuous count reaches zero but still requires an unimplemented circle-intersection or discrete-solution selection remains SCC-GEOM-005. It is not assigned an arbitrary solution.

### 3. Successful solve report

Every successful GeometryModel includes solve_report with:

- status = fully_constrained
- method = deterministic_direct
- convergence = exact
- evaluated_constraints = the number of active constraints
- degrees_of_freedom.remaining = 0

The current direct solver is non-iterative. Exact means every emitted primitive was constructed from supported direct relations and every supported postcondition was validated; it does not claim support for deferred constraint kinds.

### 4. Diagnostic boundary

SCC-GEOM-007 is recoverable by changing the input. Its structured form includes entity_ids, details.remaining_degrees_of_freedom, and a suggestion to add independent positioning constraints or explicit coordinates. Geometry-stage failures continue to use CLI exit code 4.

Missing entity parameters outside an analyzed point-distance component continue to use SCC-GEOM-002. Unsupported relation kinds and unsupported discrete construction continue to use SCC-GEOM-005.

## Compatibility

- StructuredError gains an optional deterministic details map; errors without details retain their previous serialized shape.
- GeometryModel gains the additive solve_report field in the unreleased v0.3.0 schema.
- The Rust GeometryModel constructor accepts evaluated_constraints and the report types are public exports of scc-geometry.
- SCC-GEOM-001 through SCC-GEOM-006 retain their meanings; SCC-GEOM-007 is additive.
- The anchored-points example is retained as a negative acceptance fixture and now demonstrates the correct underconstrained diagnostic.
- The source GraphState remains read-only.

## Rejected alternatives

- Canonical positive-X placement: deterministic but changes user intent by inventing alignment.
- Returning a successful model with remaining degrees of freedom: violates the no-false-success acceptance rule.
- Encoding the count only in human-readable text: prevents stable machine consumption.
- Selecting one of two circle-intersection solutions by map order: deterministic but geometrically unjustified without an orientation constraint.

## Follow-up

- Add direction-bearing Align constraints before constructing nonzero distances from a single anchored point.
- Extend the analyzer to partial-axis CenterOf and later rotational degrees of freedom for Line, Arc, Polygon, Parallel, and Perpendicular.
- Define deterministic discrete-solution selection only when Proposal and GraphState carry sufficient orientation intent.
