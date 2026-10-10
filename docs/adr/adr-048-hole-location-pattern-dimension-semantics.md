# ADR-048 — Hole Location and Pattern Dimension Semantics

- Status: Accepted for Phase 11.3
- Scope: formal Front/Top/Left production drawing annotations
- Date: 2026-09-12

## Context

Phase 11.1/11.2 provide production dimensions, hole center presentation, and one deterministic layout authority. The next annotation slice must expose hole locations and pattern facts without turning expanded solved cuts into a second engineering model or adding a second collision engine.

## Decision

1. A direct `Hole` may receive Front-view X/Y location dimensions. The references are the resolved body left and bottom edges; they are drawing references, not GD&T datums.
2. A `HolePattern` receives one typed `Pattern` dimension. Count, grid pitch, corner offsets, and circular PCD come from resolved typed pattern semantics. Hole diameter is witnessed by one resolved representative cut and all pattern radii must agree.
3. Grid and corner patterns additionally receive witness-attached Front location/spacing dimensions. Circular patterns receive location dimensions for the resolved pattern center; PCD is never fitted from hole centers.
4. Every new number carries typed provenance (`ResolvedHoleLocation`, `ResolvedPatternLocation`, `ResolvedPatternSpacing`, `ResolvedPatternSemantic`, or `ResolvedGeometry`). Identifiers are identity only and are never parsed for meaning.
5. The existing Phase 11.2 `annotation_layout` pass remains the only collision/reflow authority. SVG and DXF consume the same `DrawingModel`; DXF uses ordinary line/arrow/text primitives and does not require native DIMENSION entities.
6. Unsupported or inconsistent provenance, counts, radii, witnesses, and semantic values fail closed. No depth, thread, tolerance, GD&T, datum, hole-table, ordinate, or new CAD-geometry semantics are introduced.

## Rejected alternatives

- Counting equal-radius circles to infer pattern quantity: loses semantic intent and accepts unrelated geometry.
- Fitting PCD from resolved centers: makes a semantic source depend on presentation geometry.
- Adding a second layout algorithm or embedding formatting into the engineering model: creates competing ownership and breaks deterministic reflow.

## Compatibility and verification

The Phase 11.2 three-view golden bytes were recorded before this phase and the formal fixture changes only by the two new centered-hole location dimensions. Explicitly offset holes and all three typed pattern variants are covered by real EngineeringModel → SpatialModel → ProjectionModel → VisibilityModel → DrawingModel integration tests. The SolidBuildSpec golden and all upstream spatial/projection/visibility artifacts remain unchanged.
