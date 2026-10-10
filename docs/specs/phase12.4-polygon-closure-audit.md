# Phase 12.4 Polygon Closure Audit

Status: COMPLETE_WITH_DEFERRED_BOUNDARIES

Baseline: `af6cdf3` (pre-rollback recovery closure)

Implementation commits:

- `febebb8`  `BASE_POLYGON` / `POINT` DSL lowering.
- `acb5673`  bounded polygon tool value/adapter and canonical typed output.
- `3cc4fd0`  source-only project round-trip coverage.
- `2930594`  spatial containment, projection/visibility/PCV/drawing coverage and
  Phase 12.4 SVG/DXF goldens.

## Closure result

Phase 12.4 is complete for the declared engineering and presentation scope:

- Polygon is a typed `BaseProfile`, not an anonymous Composite.
- Closure is implicit; the final vertex is never duplicated.
- Ingest canonicalizes to finite, non-degenerate, counter-clockwise vertices
  with the lexicographically smallest vertex first.
- Fewer than three vertices, duplicates, zero-length edges, zero area,
  self-intersections, and non-finite coordinates fail closed.
- Engineering lowering emits exact Line segments with
  `ProfileSegmentSemantic::PolygonEdge { index }`.
- Front projection preserves one `BodySegment` per polygon edge.
- Top and Left are derived body silhouettes; they are not copies of Front.
- Spatial lowering uses the exact polygon boundary as a composite footprint.
  No bounding-box spatial approximation is used.
- Hole and Slot containment is validated and rejects outside, crossing, or
  touching cases.
- Projection consistency remains validator-only.
- Drawing uses the existing Phase 11 layout authority and deterministic SVG/DXF
  serialization.
- Project persistence stores source Polygon vertices only; derived geometry,
  spatial, projection, solid, and STEP artifacts are excluded.
- `BASE_POLYGON` / `POINT` DSL and `create_base_polygon` tool both emit
  typed engineering commands and apply the same canonicalization contract.

## Explicit deferred boundaries

These are intentional and fail closed:

- Polygon manufacturing / `SolidBuildSpec` / CadQuery / STEP.
- Polygon STEP export and re-import validation.
- Natural-language Polygon intent parsing and clarification.
- Automatic per-edge Polygon dimensions.
- Multiple contours, inner loops, islands, and general sketch/profile editing.
- Bezier, Spline, Ellipse, Fillet, Chamfer, Pocket, Boss, and similar features.

The manufacturing boundary does not replace Polygon with a bounding rectangle.

## Verification evidence

Dedicated Phase 12.4 test:

```text
cargo test --offline --locked -p scc-drawing --test phase124_polygon
4 passed
```

The dedicated test covers:

- canonical triangle and pentagon engineering sources;
- engineering!’ spatial ’ principal projection!’ visibility ’ PCV ’
  three-view drawing;
- Front BodySegment provenance and derived Top/Left silhouettes;
- deterministic SVG/DXF output across repeated runs;
- checked SVG/DXF goldens for triangle and pentagon;
- inside and fail-closed Hole/Slot containment, including rotated Slot input.

Other focused gates completed during implementation:

```text
cargo test --offline --locked -p scc-dsl --lib
cargo test --offline --locked -p scc-dsl --test vertical_slices
cargo test --offline --locked -p scc-tools
cargo test --offline --locked -p scc-project
cargo check --offline --locked -p scc-spatial
cargo test --offline --locked -p scc-spatial
```

Phase 12.4 adds only new goldens:

- `phase124_polygon_triangle.svg`
- `phase124_polygon_triangle.dxf`
- `phase124_polygon_pentagon.svg`
- `phase124_polygon_pentagon.dxf`

Pre-existing Phase 912.3 goldens remain untouched.

## Source-of-truth rule

Polygon source truth is the EngineeringModel/project source. SpatialModel,
principal views, visibility, PCV results, drawing artifacts, SolidBuildSpec,
and STEP artifacts remain derived and are not persisted as project truth.
