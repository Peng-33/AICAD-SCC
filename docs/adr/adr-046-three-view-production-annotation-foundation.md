# ADR-046: Three-view production annotation foundation

- Status: accepted
- Scope: Phase 11.1 Front/Top/Left production annotations
- Date: 2026-09-11

## Decision

1. Dimensions remain derived `DrawingModel` semantics; they are not a second
   editable engineering model.
2. Dimensions never drive `EngineeringModel`; the dependency remains
   `EngineeringModel → solved geometry → DrawingModel`.
3. Annotation geometry is derived presentation data and is not persisted as
   editable engineering state.
4. Front owns the single overall-width dimension.
5. Front owns the single overall-height dimension.
6. Left owns the single thickness dimension.
7. Front owns hole-diameter callouts.
8. Production dimension values come from already-resolved Spatial/Projection
   geometry witnesses.
9. The drawing path does not re-evaluate engineering parameters.
10. The drawing path does not re-lower `HoleExtent`.
11. The drawing path does not regenerate pattern geometry.
12. A centerline is distinct from a Hidden projected edge.
13. A centerline is distinct from Construction geometry.
14. Every production dimension and center element carries typed feature/entity
    provenance.
15. No semantic meaning is recovered by parsing dimension, primitive, or
    feature ID strings.
16. `DrawingScale` changes graphical spans and placement, not measured mm
    values or displayed dimension text.
17. First- and Third-angle projection change view placement only; annotation
    ownership and measured values remain equal.
18. Sheet bounds include view geometry, dimension layouts, center geometry, and
    the existing deterministic margins.
19. Phase 8 dimension semantics, formatter, layout geometry, and SVG/DXF
    emitters are reused.
20. Phase 11.1 intentionally changes the formal three-view SVG/DXF goldens to
    include production annotations.
21. Legacy single-view Principal SVG/DXF goldens remain byte-identical.
22. Manufacturing artifacts, SolidBuildSpec, CadQuery output, and STEP delivery
    remain unaffected.
23. Advanced dimension collision avoidance, stacking, and crowded-layout
    policy belong to Phase 11.2.
24. Drawing standards, tolerances, units beyond mm, and standards-specific
    formatting remain deferred.

## Annotation source and ownership contract

| Annotation | Owner | Resolved source |
| --- | --- | --- |
| Overall width | Front | projected body X extent |
| Overall height | Front | projected body Y extent |
| Thickness | Left | projected body Z extent in Left (`u`) |
| Hole diameter | Front | presented object-role circle radius × 2 |
| Center mark | Front | presented object-role circular hole center |
| Hole centerline | Front | presented object-role circular hole center/radius |

A circular hole pattern receives one representative diameter callout and one
center mark plus horizontal/vertical centerlines for each presented object-role
circle. Blind base, blind opposite, and through-all extents share the same
resolved circle-radius diameter contract; depth annotation is deferred.

## Coordinate and presentation contract

Primitive geometry remains in model/projection coordinates. The integrated view
transform maps model coordinates to sheet coordinates once. Dimension offsets,
extension gaps, arrows, text gaps, center mark lengths, and centerline overhang
are paper-space constants. The annotation adapter lays out after the initial
view placement, then translates view transforms and sheet-space annotation
geometry together so no generated element clips. SVG exposes `data-role` for
center marks/centerlines; DXF uses `AICAD_CENTER` and `AICAD_CENTERLINE`,
separate from `AICAD_HIDDEN`.

## Consequences

The formal three-view renderer now emits existing Phase 8 dimension and center
geometry groups for every formal view. The old Phase 9 formal goldens are
retained under `tests/goldens/phase9_three_view_plate.*` for historical hash
comparison; the canonical `three_view_plate.*` files are the Phase 11.1
fixtures. No Phase 11.2 capability is implemented by this decision.
