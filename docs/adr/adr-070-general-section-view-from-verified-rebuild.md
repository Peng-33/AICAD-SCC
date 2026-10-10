# ADR-070 — General Section Views from a Verified Parametric Rebuild

- Status: Accepted for the bounded G6A principal-plane section slice
- Date: 2026-09-26
- Scope: Engineering Drawing only; no change to the formal STEP authority

## Context

The existing DrawingModel already represented Front/Top/Left views, legacy annotations, dimension provenance, and SVG/DXF output. EngineeringModel v2 parametric models could be rebuilt through the CadQuery/OCP backend, but there was no formal SectionView or section geometry in the DrawingModel. The parametric path also did not produce linked manufacturing dimensions.

## Decision

1. A principal-plane Section A-A request is an explicit, read-only operation on the existing EngineeringModel v2 feature history. The same stateless OCP worker returns its validated rebuild witness and the section loops from that rebuilt solid.
2. Support X/Y/Z principal axes, finite offsets, and a signed view direction. Reject malformed planes, empty intersections, open/invalid loops, unsupported sampling, and degenerate loops with stable drawing-stage codes.
3. Keep sampled section loops as derived drawing geometry. The DrawingModel stores one Section A-A view alongside Front/Top/Left; section primitives carry the EngineeringModel digest and semantic loop/hatch/cutting-plane provenance. OCP Face/Edge indices are not persisted.
4. Preserve holes and material islands using closed section loops and even-odd material hatching. SVG and DXF are rendered independently from this same DrawingModel. DXF hatch is exported as line entities on `AICAD_HATCH`; existing Drawing dimensions remain line/arrow/text geometry rather than native DXF DIMENSION entities, per ADR-030.
5. Return drawing-completeness and annotation status separately from geometry/export status. Since EngineeringModel v2 feature-linked dimensions are not implemented, a valid section export may pass artifact validation while drawing completeness and annotation remain `PARTIAL`.

## Consequences

- Section geometry is rebuilt on each request and cannot mutate or replace the EngineeringModel.
- The OCP section curves are sampled with a declared 0.01 mm chord tolerance; hatching uses the current bounded 3.0 mm diagonal spacing convention. This is not a claim of full national or international drawing-standard compliance.
- The current section view can expose internal geometry, but it does not create formal dimensions for base extents, hole diameters/depths, counterbores, or edge treatments.
- Arbitrary oblique, stepped, and rotated sections; robust parametric dimension associations; automatic dimension collision layout; GD&T/PMI; and native DXF DIMENSION export remain deferred.
- The manufacturing rebuild and formal STEP re-import/equivalence/delivery authority are unchanged.

## Verification evidence

See [Phase G6A acceptance report](../testing/phase-g6a-drawing/acceptance-report.md) for OCP, Rust→OCP, SVG/DXF, browser, timing, and regression evidence and its limitations.
