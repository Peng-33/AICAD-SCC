# ADR-038: Three-view Drawing Integration and Hidden-line Presentation

- Status: Accepted
- Scope: AICAD Phase 9.7
- Date: 2026-09-04

## Context

Phases 9.2, 9.4, 9.5, and 9.6 freeze a resolved `SpatialModel`, raw
Front/Top/Left projection evidence, visibility decisions, and a consistency
gate. Phase 9.7 must make those artifacts consumable by the existing drawing
model and SVG/DXF exporters without introducing another geometry or visibility
engine.

## Decisions

1. The formal entry point is `build_three_view_drawing(&ThreeViewDrawingInputs)`.
2. Inputs are read-only `EngineeringModel`, `SpatialModel`, `ProjectionModel`, and `VisibilityModel` references.
3. The builder begins with exactly one `validate_projection_consistency` call.
4. PCV errors are returned unchanged, including their `SCC-PCV-*` stage and code.
5. Projection and visibility builders are never called by the drawing builder.
6. `DrawingViewKind` remains additive: `Principal` is retained and `Front`, `Top`, and `Left` are added.
7. The new path emits exactly three views in canonical Front/Top/Left order.
8. The legacy principal builder and its serialized/rendered output remain separate.
9. `DrawingPrimitiveRole::Hidden` is a presentation role distinct from Construction.
10. `Visible` maps to Object, `Hidden` maps to Hidden, and `Occluded` emits no drawing primitive.
11. Projection primitives are copied only as Segment→Line and Circle→Circle.
12. Projected model coordinates are copied verbatim as drawing UV coordinates.
13. No Top vertical inversion or Left horizontal mirror is applied by the adapter.
14. `DrawingSource::Projected` preserves the complete opaque `ProjectionPrimitiveId`.
15. The source map stores projected provenance in an additive optional field.
16. Drawing primitive IDs derive deterministically from the complete projection ID and do not parse it.
17. Every emitted primitive has exactly one projected source and no source-map orphan is accepted.
18. Drawing-specific adapter errors use `SCC-DRW-019` through `SCC-DRW-023` only for reachable local failures.
19. One `DrawingScale` is shared by all three views.
20. Scale is applied once by `DrawingViewTransform` and is never auto-fit by a renderer.
21. `VIEW_GAP_MM` is one centralized paper-space constant.
22. `DrawingSheet::DEFAULT_SHEET_MARGIN_MM` is reused for the global sheet margin.
23. Shared axes align by part-local origins, not by bounding-box centers.
24. Third-angle placement puts Top above Front and Left left of Front.
25. First-angle placement puts Top below Front and Left right of Front.
26. Projection method changes placement only; view geometry, visibility, and serialization order do not change.
27. Sheet bounds are the transformed union of all view geometry plus the existing outer margin.
28. PartTitle remains a single sheet-level annotation with existing placement semantics.
29. Three-view core output has no dimensions, synthetic centerlines, or construction-circle presentation.
30. Hidden SVG uses a distinct thin dashed group and hidden DXF uses `AICAD_HIDDEN`; legacy empty groups/layers are not added.
31. Existing `drawing_to_svg(&DrawingModel)` and `drawing_to_dxf(&DrawingModel)` remain the only renderer entry points.

## Boundaries and compatibility

The adapter performs no re-projection, re-classification, clipping, splitting,
Boolean operation, parameter evaluation, feature-pattern expansion, or spatial
geometry generation. Renderers consume only `DrawingModel`; they do not accept
projection or visibility arguments. Existing Phase 8 fixtures remain the
compatibility oracle, while the formal three-view plate has additive SVG/DXF
goldens under `crates/scc-drawing/tests/goldens/`.

## Rejected alternatives

A second projection pass, a second hidden-line classifier, bbox-centering,
auto-fit scaling, synthetic construction strokes, view labels, and projection
symbols were rejected because each would create a second source of geometric or
presentation truth in this phase.
