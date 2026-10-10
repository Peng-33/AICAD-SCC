# ADR-035: Raw principal-view projection lowering

- Status: Accepted
- Date: 2026-09-03
- Related: ADR-032, ADR-033, ADR-034,
  crates/scc-projection/src/principal.rs

## Background

ADR-034 froze the orthographic frames, primitive schema, depth evidence,
provenance map, deterministic allocator, and validation gates, but
intentionally left population of the model to Phase 9.4. ADR-033 provides a
resolved `SpatialModel` containing one rectangular body and zero or more
resolved +Z cylindrical cuts. The missing boundary is therefore a pure
spatial-to-view lowering step: it must preserve every raw geometric candidate
that a later hidden-line classifier may need, without making a display or
drawing decision early.

The input is also serde-deserializable through public fields. Projection must
therefore fail closed on malformed derived data and arithmetic overflow rather
than relying on constructors that deserialization bypasses.

## Decision

Phase 9.4 adds the public entry point
project_principal_views(&SpatialModel) -> Result<ProjectionModel,
ProjectionError>. It accepts only the resolved SpatialModel. It does not
read EngineeringModel, GeometryModel, pattern parameters, projection method,
or drawing state; it does not re-solve or re-lower upstream data.

The function installs and fills exactly three views in strict generation order:
Front, Top, Left. The existing ADR-034 frame contract is reused unchanged:

| View | u | v | depth |
| --- | --- | --- | --- |
| Front | x | y | z |
| Top | x | -z | y |
| Left | z | y | -x |

Coordinates are millimetres in 1:1 view space. There is no Sheet,
DrawingScale, First-angle, or Third-angle behavior in this phase.

## Raw candidate rules

The body is read from SpatialBody.footprint and SpatialBody.z; the stored
SpatialModel.bounds is an integrity witness, not the projection geometry
source. A rectangular body produces a near face loop followed by a far face
loop in every view. Each loop has four edges in corner order
0 -> 1 -> 2 -> 3 -> 0. Front uses z1 then z0, Top uses y1 then y0, and
Left uses x0 then x1 for depth -x0 then -x1. A projected edge whose endpoints
collapse is omitted.

For every cylindrical cut, only center, radius, and resolved cut.z =
[za, zb] are consumed. SpatialCutKind is not used to recompute an interval.
The raw candidates are:

- Front: two axial rim circles at depths za and zb.
- Top: z=za and z=zb rim segments from (cx-r,-z) to (cx+r,-z),
  each with depth [cy-r, cy+r], followed by left and right tangent
  segments with depth [cy, cy].
- Left: z=za and z=zb vertical rim segments from (z,cy-r) to
  (z,cy+r), each with depth [-(cx+r), -(cx-r)], followed by lower and
  upper tangent segments with depth [-cx, -cx].

Thus each cut contributes 2 / 4 / 4 candidates to Front / Top / Left.
Candidates are generated for cuts in SpatialModel.cuts BTreeMap order.
Pattern semantics are represented only by the resulting multiple Spatial cuts;
projection never recomputes pattern coordinates or parameters.
Outside-body XY cuts remain present and can extend view bounds. Coincident and
overlapping candidates remain present; there is no tessellation, Arc,
centerline, deduplication, clipping, merging, nearest-only selection, or
visibility/depth sorting. Construction circles are reference-only and
produce no candidate, source entry, or bounds contribution.

Each primitive is allocated with the frozen builder next_id allocator and
inserted immediately. Sources are exactly SpatialElementRef::Body or
SpatialElementRef::Cut { id }; SpatialCutId remains opaque and is never
parsed. The builder enforces one global source-map entry per primitive id.
finish() recomputes bounds from primitive geometry, and the entry point then
calls validate_against(&spatial).

## Validation and phase boundaries

Because SpatialModel is publicly deserializable, the entry point performs a
read-only preflight. The body must be a finite positive Rectangle whose
origin-plus-dimensions are finite and strictly increasing; its Z interval
must be finite, strictly increasing, and start at zero. Bounds must be finite,
strict, and exactly match the body footprint and Z interval. Every cut center,
radius, and Z endpoint must be finite; radius must be positive; and the cut
interval must be strictly valid and contained in the body Z interval.
Malformed input returns SCC-PROJ-007 InvalidSpatialModel.

Arithmetic overflow in projected geometry returns SCC-PROJ-002; overflow or
reversal in projected depth returns SCC-PROJ-003. All failures are
fail-closed: no partial ProjectionModel is returned.

Phase 9.4 does not implement manufacturability validation, visibility,
hidden-line classification, styles, centerlines, dimensions, SVG/DXF,
DrawingModel integration, or UI. Phase 9.5 owns visibility and hidden-line
classification.

## Consequences

- A valid spatial model produces exactly three raw views and no partial result:
  `Front = 8 + 2n`, `Top = 8 + 4n`, and `Left = 8 + 4n`, where `n` is the
  number of spatial cuts.
- Near/far body loops, coincident cut rims, overlapping cuts, different
  sources, and different depth evidence remain independently addressable.
  This gives Phase 9.5 the complete geometric evidence needed for
  classification.
- Projection is deterministic and rebuildable. Primitive IDs depend on the
  frozen `(view, source, ordinal)` allocator stream, not on coordinates,
  screen position, or visibility ordering; construction references consume no
  projection IDs.
- The private preflight adds a strict fail-closed gate for malformed
  `SpatialModel` snapshots. Its additive `SCC-PROJ-007` error distinguishes
  invalid input state from invalid projected geometry, depth, bounds, source,
  or identity.
- The stored spatial bounds remain an integrity witness only. View bounds are
  recomputed from all emitted primitive geometry, so an outside-body XY cut is
  retained and may expand a view envelope.

## Rejected alternatives

- Mapping `SpatialModel.bounds` directly into view bounds was rejected because
  the body footprint and resolved body interval are the geometric source;
  bounds are only their integrity witness.
- Re-reading `EngineeringModel`, pattern parameters, `ThroughAll`/`Blind`, or
  thickness was rejected because ADR-033 has already resolved spatial
  geometry. Doing so would duplicate upstream lowering and make patterns or
  blind cuts drift from `SpatialModel`.
- Keeping only the nearest candidate, deduplicating coincident geometry,
  clipping to the body, or merging overlapping cuts was rejected because all
  are visibility/manufacturability decisions owned by later phases.
- Tessellating a circle, adding arcs, or rendering SVG/DXF was rejected because
  the frozen raw vocabulary is analytic `Segment` + `Circle`; rendering and
  styling belong to Drawing.
- Skipping preflight on the assumption that all spatial models are valid was
  rejected because serde can construct values without calling `ZInterval` or
  `Bounds3D` constructors. A malformed snapshot must fail closed.

## Compatibility

This is an additive leaf-crate API. `ProjectionModel`, `ProjectionView`, the
9.3 frame table, primitive schema, source-map shape, and existing builder APIs
are unchanged. `project_principal_views` depends only on `&SpatialModel` and
does not add a dependency from `scc-spatial`, `scc-engineering`, or
`scc-drawing` back to projection. `SCC-PROJ-007` is an additive structured
error code; `SCC-PROJ-001` through `006` retain their existing meanings and
validation order. Phase 8 SVG/DXF and UI behavior are outside this API and are
unchanged.

## Migration plan

No migration or data rewrite is required. A consumer that needs principal raw
views calls `project_principal_views(&spatial)` and handles the structured
projection error. Phase 9.5 may consume the returned candidates for
hidden-line classification; Phase 9.7 may later place classified views into
Drawing/Sheet space. Until then, no caller should add view transforms,
visibility fields, or rendering behavior to this layer.
