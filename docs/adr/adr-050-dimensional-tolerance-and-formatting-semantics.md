# ADR-050: Dimensional Tolerance, Precision, and Basic Drawing Semantics

- **Status:** Accepted for Phase 11.5
- **Date:** 2026-09-13
- **Scope:** `EngineeringModel` dimensional specifications and derived three-view presentation

## Context

Phase 11.4 produced nominal engineering hole callouts and deterministic Front/Top/Left annotations. A production drawing also needs typed dimensional requirements such as `120 ±0.10`, `80 +0.20/-0.05`, diameter tolerance, hole location, pattern spacing, PCD, and blind depth. These requirements must survive Project Save/Open without becoming geometry or renderer-owned state.

## Decision

1. `EngineeringModel` is the only authority. It stores an optional `BTreeMap<EngineeringDimensionRef, DimensionalSpecification>`. The typed reference covers feature slots, part thickness, hole location X/Y, and blind-hole depth. Empty/default values are omitted from Project v1 JSON; the schema version does not change.
2. `DimensionalTolerance` is typed as `None`, `Symmetric { plus_minus }`, or signed `Bilateral { upper, lower }`. Values must be finite; symmetric values are non-negative; bilateral values satisfy `upper >= 0`, `lower <= 0`, and `upper >= lower`. Zero symmetric or bilateral input is canonicalized to `None`.
3. `DecimalPlaces(0..=4)` is separate from tolerance. It affects presentation only. Without an explicit precision, nominal-only dimensions retain the legacy adaptive formatter; tolerance values use the frozen profile default.
4. The frozen internal drawing profile is `AICAD_INTERNAL_V1`: single-line tolerance text, Unicode `Ø`, `↧`, and `×`, and deterministic fixed-decimal rounding. This is an internal contract, not a claim of GB, ISO, or ASME compliance.
5. Pattern quantity is not a tolerance target. ThroughAll holes may carry diameter tolerance but cannot carry depth tolerance. A `HoleDepth` target is valid only for a declared Blind `HoleExtent`.
6. `DrawingModel` receives a validated derived specification copy with its typed reference. It never becomes authoritative and never persists formatted strings. The Phase 11.2 layout pass remains the only collision/layout authority; its text envelope is calculated from the actual formatter output.
7. SVG and DXF consume the same `DrawingModel` and formatter semantics. Drawing scale and First/Third-angle projection change placement only, not tolerance or precision text.
8. Tolerance and precision do not modify feature geometry, spatial lowering, `SolidBuildSpec`, CadQuery, or validated STEP output. STEP PMI tolerance export is explicitly unsupported (`STEP_PMI_TOLERANCE = NO`). Natural-language tolerance editing remains deferred; the formal EngineeringCommand/API is sufficient for this phase.

## Rejected alternatives

- Storing tolerance on `DrawingDimension` as source truth would make derived output editable and stale.
- Applying tolerance to geometry or spatial extents would corrupt nominal manufacturing/build semantics.
- Persisting display strings would make locale/profile changes and renderer parity non-deterministic.
- Introducing a second layout engine for long callouts would violate ADR-047 and make SVG/DXF disagree.
- Claiming a full national/international drafting standard profile is premature; advanced GD&T, datums, fits, threads, surface finish, title blocks, and formal compliance remain future work.

## Compatibility and migration

Old Project v1 documents deserialize with an empty specification map. Existing Phase 11.4 no-tolerance dimensions and goldens remain unchanged. New toleranced output is opt-in through typed engineering commands and is covered by separate `tolerance_plate.svg` and `tolerance_plate.dxf` goldens.

## Verification

The implementation is covered by engineering validation tests, Project Save/Open round trips, formatter examples, stale/tampered DrawingModel rejection, scale/First/Third invariance tests, SVG/DXF parity, and the SolidBuildSpec nominal golden guard.
