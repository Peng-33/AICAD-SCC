# ADR-039: Three-view Workbench Integration

- Status: Accepted
- Scope: AICAD Phase 9.8
- Date: 2026-09-05

## Context

Phase 9.7 froze the formal Front/Top/Left DrawingModel, hidden-line
presentation, First/Third-angle sheet placement, and SVG/DXF renderer output.
The Engineering Workbench previously displayed only the legacy Principal
drawing. Phase 9.8 connects the formal pipeline to that existing panel without
creating a second engineering, projection, visibility, or drawing truth.

## Decisions

1. The legacy Principal preview remains the default.
2. Three-view is an additive UI presentation mode.
3. Preview mode is UI state, not EngineeringModel state.
4. The backend Three-view path uses the complete formal Rust pipeline.
5. Node remains a transport-only CLI/HTTP bridge.
6. The frontend never renders projection geometry.
7. The frontend displays renderer SVG through Blob, object URL, and img.
8. The Workbench does not use innerHTML for the drawing preview.
9. Three-view failures never silently fall back to Principal.
10. Structured upstream SCC error codes and messages are preserved.
11. Mode switches and model refreshes are protected by one generation counter.
12. Preview mode is excluded from undo and redo history.
13. Every committed model change refreshes the currently selected drawing mode.
14. Blob object-URL creation and revocation remain deterministic.
15. The Principal endpoint and CLI command remain backward-compatible.
16. The Three-view response returns presentation metadata only.
17. Spatial, Projection, and Visibility JSON do not reach the normal UI response.
18. ProjectionMethod layout logic remains in the frozen Rust drawing layer.
19. Three-view geometry logic remains in the frozen Rust crates, not Node.
20. Missing spatial semantics fail closed.
21. No automatic ThroughAll, thickness, or projection-method defaults are introduced.
22. Formal Three-view renderer output is displayed unchanged.
23. The Phase 9.7 Three-view SVG/DXF goldens remain frozen.
24. This phase adds no UI editing controls for spatial semantics.
25. Phase 9 Completion Audit follows Phase 9.8.

## Integration contract

The additive CLI command is:

    scc engineering drawing-three-view model.json

The additive endpoint is:

    POST /api/engineering/drawing/three-view

Both return one machine-readable JSON document containing svg,
sheet_width_mm, sheet_height_mm, scale, part_title, projection_method, and
drawing_mode. The Rust orchestration is:

    deserialize EngineeringModel
    -> lower_and_solve
    -> lower_spatial
    -> project_principal_views
    -> classify_visibility
    -> build_three_view_drawing
    -> drawing_to_svg

build_three_view_drawing owns the single validate_projection_consistency gate.
The CLI must not call PCV a second time.

The UI controller owns principal | three-view, the selected model snapshot,
the generation guard, error banner, and Blob URL lifecycle. A failed current
mode preserves the last valid image and displays the current mode plus the
structured SCC code/message. No model field, command, dirty bit, or history
entry is written by a mode switch.

## Compatibility and security

The additive POST /api/engineering/drawing/three-view and
engineering drawing-three-view command leave the existing
POST /api/engineering/drawing and engineering drawing behavior untouched. The
Node bridge continues to call execFile with an argument array and shell:false;
it does not use exec() or construct command strings. CSP hashes cover the
existing inline Workbench script. No browser SVG DOM is selected or rewritten,
and no screenshot golden is introduced.

The independent apps/scc-ui/test/fixtures/three-view-plate.json fixture is a
serialized EngineeringModel with a 120 x 80 x 10 mm plate, one ThroughAll
diameter-10 hole, explicit Third-angle projection method, and an explicit part
title. It is consumed through the formal pipeline; it is not a hardcoded SVG.
