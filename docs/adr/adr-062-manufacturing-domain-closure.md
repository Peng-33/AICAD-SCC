# ADR-062: Manufacturing Domain Closure

- Status: Accepted / FROZEN for Phase 13.5
- Date: 2026-09-19
- Decision owners: SCC architecture
- Scope: the currently declared Manufacturing / STEP domain only

## Context

The pre-remediation Phase 13.5 audit found a reachable
`CadDocument`/`FeatureGraph` compatibility route that could independently
build and deliver STEP. That route had a separate mutable source, Python-side
pattern expansion, and no canonical geometric-equivalence and validated-byte
contract. Phase 13.5A closed that boundary under ADR-063 while preserving the
compatibility workspace's non-STEP editing and preview features.

This re-audit verifies the current implementation from the actual HTTP, CLI,
Rust, Python, CadQuery/OCP, artifact, and test entrypoints. The decision is
limited to the supported seven-item manufacturing matrix; it is not a claim
of general-purpose mechanical CAD coverage or product-wide editing-state
unification.

## Decision

1. Formal manufacturing authority is the source-only `EngineeringModel`.
2. `SpatialModel` is the derived, validated resolved manufacturing geometry.
3. `SolidBuildSpec` v1 is the derived, backend-neutral manufacturing contract.
4. Rectangle, Circle, and RoundedRectangle remain typed body footprints.
5. Polygon and Composite preserve their upstream engineering identity and
   lower to one exact generic Line/Arc `ClosedProfile` representation.
6. Hole remains a typed `CylindricalCut` operation.
7. Slot remains a typed `SlotCut` operation carrying an exact derived
   `ClosedProfile` contour; Slot is not a base profile.
8. Python consumes resolved `SolidBuildSpec` geometry and does not reconstruct
   Engineering semantics, Slot dimensions, or formal pattern intent.
9. Formal STEP is created only through:

   ```text
   EngineeringModel → solved snapshot → SpatialModel → SolidBuildSpec
   → Rust CadBackendClient → CadQuery/OCP → STEP export → STEP reimport
   → geometric/material/topology witnesses → artifact identity → delivery
   ```

10. Compatibility `CadDocument`/`FeatureGraph` remains a separate editable
    product state, but cannot generate or deliver formal STEP.
11. Tessellation, bounding-box substitution, inferred Arc direction, and
    segment reordering are not manufacturing truth.
12. Only the bytes whose size and SHA-256 identity passed STEP validation are
    returned to the canonical API and UI.
13. A finite witness contract is a strong deterministic regression contract,
    not a mathematical proof of pointwise equivalence for every possible
    shape.
14. Inner loops, multiple contours, general Sketch/history, Slot patterns,
    NL Slot, and broader operations remain deferred.
15. All AICAD editing states are not unified; only the formal
    Manufacturing/STEP authority is frozen.

## Compatibility boundary

`POST /api/cad/build` remains available for source JSON, BREP, STL, GLB, SVG,
preview, coverage, metrics, and bounded legacy compatibility behavior. The
worker manifest rejects a `step` artifact, and the artifact service rejects a
`step` request with `SCC-CAD-010`. Formal STEP users must use the Engineering
export route. No reverse `CadDocument → EngineeringModel` bridge is implied.

Compatibility circular-pattern expansion may remain inside the retained
non-STEP FeatureGraph BREP/preview path. Canonical Engineering patterns are
resolved upstream into Spatial cylindrical cuts before manufacturing lowering;
the compatibility expansion cannot reach a formal STEP artifact.

## Consequences

- The product has one reachable formal STEP authority and one validated-byte
  delivery contract.
- Canonical manufacturing remains exact and typed without adding a second
  Polygon, Composite, or Slot backend.
- Generic compatibility CAD remains useful but is explicitly not formal
  manufacturing authority.
- Future formal STEP capabilities must extend this authority chain rather
  than introduce a new export or download path.

## Verification

The evidence for this decision is recorded in the
[Phase 13.5 closure re-audit](../specs/phase13.5-manufacturing-domain-closure-reaudit.md).
It includes a fresh route scan, focused Rust tests, workspace gates,
CadQuery/OCP Docker tests, real Rust→STEP integration, API rejection tests,
golden fixtures, adversarial equivalence tests, and final clean-worktree
verification.
