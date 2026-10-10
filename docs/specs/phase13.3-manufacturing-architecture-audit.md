# Phase 13.3 — General Closed Profile Manufacturing Architecture Audit

Status: PASS_WITH_NOTES

Freeze decision: FREEZE

Date: 2026-09-17

Baseline: 88fbb0c75d3571818e852ce14b722f738a0106c8

Branch: codex/aicad-pre-rollback-recovery

Task type: ARCHITECTURE_AUDIT_ONLY

SOURCE_CODE_CHANGED: NO

## Scope and evidence

This audit inspected the relevant Engineering, Spatial, Manufacturing, CadQuery, STEP adapter, test, ADR, and closure-audit layers. No functional source code, SolidBuildSpec implementation, CadQuery builder, or STEP path was changed.

The current verified manufacturing state is:

~~~text
Rectangle        = SUPPORTED
Circle           = SUPPORTED
RoundedRectangle = SUPPORTED
Composite        = FAIL_CLOSED
Polygon          = FAIL_CLOSED
Slot             = FAIL_CLOSED
~~~

The existing authority chain remains:

~~~text
EngineeringModel
→ GeometryModel
→ SpatialModel
→ SolidBuildSpec
→ Rust manufacturing layer
→ Python CadQuery/OCP
→ STEP export
→ STEP reimport
→ equivalence validation
→ exact validated artifact delivery
~~~

## Current Manufacturing Model

The actual Rust model at the audit baseline is:

~~~rust
struct SolidBuildSpec {
    schema_version: u32,
    part: SolidPartSpec,
    operations: BTreeMap<SolidOperationId, SolidOperation>,
    source_map: SolidBuildSourceMap,
}

struct SolidPartSpec {
    body: SolidBodySpec,
    bounds: SolidBounds3D,
}

struct SolidBodySpec {
    footprint: SolidFootprint,
    z: SolidZInterval,
}

enum SolidFootprint {
    Rectangle { origin, width, height },
    Circle { center, radius },
    RoundedRectangle { center, width, height, corner_radius },
}

enum SolidOperation {
    CylindricalCut { center, radius, z, axis },
}
~~~

The current model has no BaseSolidSpec, CutSpec, ClosedProfile, or Slot operation variant. SpatialModel already has a typed SpatialBaseProfile, an exact Composite footprint, typed CylindricalCut records, and typed SlotCut records.

## Composite Findings

| Contract question | Result | Evidence |
|---|---|---|
| Ordered exact Line/Arc segments | YES | Engineering BaseProfile::Composite stores Vec<ProfileSegment>; Spatial stores Vec<SpatialProfileSegment>. |
| One simple closed outer loop | YES at Engineering; PARTIAL defensively at Spatial | Engineering checks continuity, closure, non-zero area, and self-intersection. Spatial checks segment validity, continuity, and bounds but does not independently repeat all pairwise self-intersection/area checks. |
| Self-intersection rejection | YES at Engineering; PARTIAL at Spatial | Engineering has analytic line-line, line-arc, and arc-arc checks. Spatial relies on upstream validated source plus defensive segment/bounds checks. |
| Arc direction preserved | YES | ArcDirection is explicit in Engineering and Spatial and is matched during lowering. |
| Source ordering preserved | YES | Segment order and segment_index are retained; source-to-solved endpoint/arc witnesses are checked. |
| Exact Spatial retention | YES | SpatialFootprint::Composite carries exact line/arc segments with source entity and index. |

Conclusion: Composite is structurally ready for a derived exact ClosedProfile manufacturing payload, subject to a defensive SolidBuildSpec contour validator.

## Polygon Findings

Polygon currently has typed vertices, implicit closure, canonical CCW winding, lexicographically smallest start vertex, finite/duplicate/zero-area/self-intersection rejection, exact Line lowering, and typed PolygonEdge provenance.

Its resolved Spatial form is an exact Line-only SpatialFootprint::Composite with SpatialBaseProfile::Polygon identity. No bounding-box approximation is used. Polygon and Composite are therefore compatible after Spatial resolution while remaining distinct EngineeringModel source types.

CONCAVE_POLYGON_SUPPORTED_AT_ENGINEERING = YES

The current validation rejects self-intersection but does not impose convexity. Therefore:

CADQUERY_GENERAL_PROFILE_MUST_SUPPORT_CONCAVE = YES

Polygon manufacturing should use the general ClosedProfile path, not PolygonSolidSpec.

## Slot Findings

Slot is always EngineeringFeature::Slot. It is a through material-removal feature, never BaseProfile::Slot. Engineering stores center, overall length, width, and canonical [0°,180°) orientation. Spatial lowering produces a separate typed slot_cuts map containing SlotCut semantic fields, exact Line/Arc stadium segments, Z interval, ThroughAll kind, and source feature identity.

Engineering does not provide a base-slot variant, and Spatial does not merge SlotCut into the body footprint. Slot is therefore a typed cut feature.

The selected future form is a typed CutSpec::Slot carrying an exact derived ClosedProfile contour and resolved Z interval. The backend consumes that contour; it does not rebuild the stadium from semantic parameters.

## Spatial Compatibility

PARTIAL → YES for the selected derived manufacturing convergence.

Polygon and Composite both lower into:

~~~text
SpatialFootprint::Composite { segments: Vec<SpatialProfileSegment> }
~~~

with exact ordered Line/Arc geometry, finite coordinates, explicit arc center/radius/direction, continuity, and source entity/index provenance. Their parallel SpatialBaseProfile identity keeps Polygon versus Composite semantics available upstream.

The compatibility is not a license to move source validation downstream. Engineering and Spatial remain semantic owners; SolidBuildSpec must add defensive contour checks before transport acceptance.

## Selected Architecture

Selected Option B:

~~~text
General exact ClosedProfile base
+
typed Slot cut
+
typed cylindrical Hole cut
~~~

Decisions:

- GENERAL_CLOSED_PROFILE_FOUNDATION = YES
- POLYGON_USES_GENERAL_PROFILE = YES
- COMPOSITE_USES_GENERAL_PROFILE = YES
- SLOT_IS_TYPED_CUT = YES
- SLOT_USES_GENERAL_CUT_PROFILE = YES
- HOLE_REMAINS_TYPED_CYLINDER = YES
- GENERAL_PROFILE_IS_DERIVED = YES
- SOURCE_PROFILE_IDENTITY_PRESERVED = YES
- MANUFACTURING_PROFILE_RECONSTRUCTION = NO
- GENERAL_PROFILE_TESSELLATION = NO
- ARC_DIRECTION_RECONSTRUCTION_FROM_ENDPOINTS = NO
- INNER_LOOPS = DEFERRED
- MULTIPLE_CONTOURS = DEFERRED
- GENERAL_SKETCH_SYSTEM = DEFERRED
- STEP_EQUIVALENCE_EXTENSION_REQUIRED = YES

The Slot decision means the operation remains typed as Slot for authority and provenance, while its geometry payload is an exact general contour produced upstream. It does not mean all cuts become anonymous profiles.

## Rejected Alternatives

### Option A — Dedicated variants

PolygonSolidSpec, CompositeSolidSpec, and SlotSolidSpec would duplicate validators, DTO parsers, CadQuery handlers, STEP tests, and provenance plumbing. They would make each profile a special manufacturing path and increase regression surface. Rejected.

### Option C — One universal closed-profile operation system

Using one generic profile-plus-operation abstraction for every base and cut would weaken the distinction between additive bodies and typed Hole/Slot semantics and would become a second sketch/topology system. Rejected.

### Backend semantic reconstruction

Passing EngineeringModel, DSL, parameter expressions, Polygon canonicalization, or Slot dimensions to Python violates the current authority chain and duplicates geometry construction. Rejected.

## Required Decisions

~~~text
GENERAL_CLOSED_PROFILE_FOUNDATION = YES
POLYGON_USES_GENERAL_PROFILE = YES
COMPOSITE_USES_GENERAL_PROFILE = YES
SLOT_IS_TYPED_CUT = YES
SLOT_USES_GENERAL_CUT_PROFILE = YES
HOLE_REMAINS_TYPED_CYLINDER = YES
INNER_LOOPS = DEFERRED
MULTIPLE_CONTOURS = DEFERRED
GENERAL_SKETCH_SYSTEM = DEFERRED
STEP_EQUIVALENCE_EXTENSION_REQUIRED = YES
~~~

## Proposed SolidBuildSpec Evolution

The current Rust model should evolve additively, conceptually toward:

~~~rust
struct SolidBuildSpec {
    schema_version,
    part: SolidPartSpec,
    operations: BTreeMap<SolidOperationId, SolidOperation>,
    source_map: SolidBuildSourceMap,
}

enum SolidFootprint {
    Rectangle { ... },
    Circle { ... },
    RoundedRectangle { ... },
    ClosedProfile { segments: Vec<SolidProfileSegment> },
}

enum SolidOperation {
    CylindricalCut { ... },
    SlotCut { profile: ClosedProfileSpec, z: SolidZInterval, ... },
}
~~~

This is a future contract, not an implementation. Existing Rectangle, Circle, and RoundedRectangle JSON must remain accepted unchanged. The existing body/operation source maps remain the opaque provenance boundary. No backend dispatch may parse IDs or infer Polygon versus Composite from segment patterns.

## Proposed CadQuery Backend Evolution

The future adapter should have:

~~~text
body dispatch:
  Rectangle
  Circle
  RoundedRectangle
  ClosedProfile

cut dispatch:
  CylindricalCut
  SlotCut carrying exact ClosedProfile geometry
~~~

ClosedProfile construction should use native analytic Line and Arc edges, assemble one ordered closed wire, build one planar face, and extrude along +Z over the resolved interval. CadQuery 2.8.0 is pinned, and its official documentation exposes native wire, face, and extrude APIs:

- https://cadquery.readthedocs.io/en/stable/free-func.html
- https://cadquery.readthedocs.io/en/latest/classreference.html

The existence of those APIs is capability evidence only. Phase 13.3A must define deterministic edge orientation, wire assembly, invalid-wire errors, and single-solid validation before enabling any profile.

## STEP Equivalence Analysis

Current STEP equivalence checks:

- exactly one valid solid;
- bounding box;
- volume;
- sparse material probes;
- operation and body provenance;
- artifact identity and no second export.

Current result: STEP_EQUIVALENCE_EXTENSION_REQUIRED = YES.

A shape with the same bbox and volume but a different contour can pass the current geometric witnesses if the sparse material probes happen to agree. A concave polygon and a rectangle-like profile are a direct risk class. Material probes improve detection but are not a complete contour proof.

Before Polygon, Composite, or Slot STEP delivery, Phase 13.3A must define a stronger, deterministic witness set. The minimum recommended extension is surface area, center of mass, stable edge/face counts, and contour-derived occupancy probes that include vertices, arc extrema, and deterministic interior/exterior samples. If the acceptance contract requires contour identity rather than strong equivalence, a canonical contour/topology fingerprint must also be specified. This is a blocker, not a deferred warning.

## Concave Polygon

CONCAVE_POLYGON_SUPPORTED_AT_ENGINEERING = YES

CADQUERY_GENERAL_PROFILE_MUST_SUPPORT_CONCAVE = YES

The general wire/face path must not assume convexity or use convex-hull/envelope fallback. Spatial containment currently uses a winding test for line-only composite bodies, consistent with concave support, while the future manufacturing validator must independently reject invalid/self-intersecting contours.

## Arc Direction

Engineering stores explicit Clockwise or CounterClockwise direction. Spatial lowers and verifies direction against solved arc endpoints, center, and radius. The future ClosedProfile transport must retain direction explicitly.

ARC_DIRECTION_RECONSTRUCTION_FROM_ENDPOINTS = NO

The backend may use the explicit direction to construct the analytic arc, but may not infer it from endpoint order or feature IDs.

## Backward Compatibility

SERDE_BREAK_REQUIRED = NO for existing payloads, provided the extension is additive and both Rust and Python readers are updated together.

Existing schema-version-1 Rectangle, Circle, and RoundedRectangle payloads can remain byte-compatible. The new tagged ClosedProfile and SlotCut variants are additive transport cases. A legacy reader that does not know the new variants will reject those new payloads, so profile rollout must be version-gated operationally; this is not a breaking change to existing supported payloads. A schema bump is required only if field meanings, authority, or existing variant shapes change.

## Validation Ownership

PRIMARY_PROFILE_VALIDATION_OWNER = Engineering + Spatial

DEFENSIVE_BACKEND_VALIDATION = YES

CadQuery remains a final executor/solid-integrity gate. It must not become the semantic owner of closure, self-intersection, source identity, or engineering interpretation.

## Coordinate and Transform Contract

Profiles remain in canonical part-local XY with material over Z=[0,T]. Translation is preserved exactly; no origin normalization is allowed unless the transform is explicit and reversible. Composite arcs retain direction under translation. Polygon and Composite have no axis-aligned manufacturing restriction. Slot orientation remains arbitrary over its existing canonical half-turn.

## Provenance and Artifact Identity

Polygon, Composite, and Slot source identity remains in EngineeringModel and SpatialModel. The derived manufacturing contour may converge, but body source feature/entity and cut source IDs remain in SolidBuildSourceMap and STEP operation/body witnesses. STEP is still a validated derived artifact, not engineering truth. Complete semantic history inside STEP is not required.

## General Profile Golden Strategy

Future implementation phases should add separate goldens even when the transport variant is shared:

- polygon_solid_spec.json — Line-only ClosedProfile, Polygon body source.
- composite_solid_spec.json — Line/Arc ClosedProfile, Composite body source.
- slot_cut_solid_spec.json — typed SlotCut with exact stadium ClosedProfile contour.

The golden payloads may share the same ClosedProfile variant, but source feature/entity and operation provenance must remain distinguishable. Add adversarial same-bbox/same-volume/different-contour fixtures to the STEP validator before any delivery claim.

## Multiple Contours

INNER_LOOPS = DEFERRED

MULTIPLE_CONTOURS = DEFERRED

ISLANDS = DEFERRED

The scope is one simple outer loop only. No topology graph or inner-wire policy is introduced in this audit.

## Risk Register

| ID | Risk | Severity | Likelihood | Mitigation | Blocking phase |
|---|---|---:|---:|---|---|
| R1 | General profile becomes a second sketch system | High | Medium | Keep one outer loop, Line/Arc only, no constraints/topology editor | 13.3A |
| R2 | Backend infers engineering semantics | High | Medium | Send only resolved Spatial/SolidBuildSpec; typed source maps remain opaque | 13.3A |
| R3 | Bbox approximation sneaks in | High | Medium | Fail closed on unsupported/invalid contours; adversarial tests | 13.3A |
| R4 | Arc direction is lost | High | Medium | Explicit direction in Spatial and ClosedProfile DTO; round-trip tests | 13.3A |
| R5 | Concave polygon backend failure | High | Medium | Concave fixtures and native wire/face validation | 13.3B |
| R6 | SolidBuildSpec serde incompatibility | High | Low | Additive tagged variants; preserve old JSON goldens | 13.3A |
| R7 | STEP equivalence is too weak | Critical | High | Add topology/area/COM/contour probes before profile delivery | 13.3A |
| R8 | Slot semantics duplicated in backend | High | Medium | Typed SlotCut with exact contour generated upstream | 13.4 |
| R9 | Generic cut abstraction over-engineered | Medium | Medium | Keep Hole typed and Slot typed; defer arbitrary cut graph | 13.4 |
| R10 | Regression to Rectangle/Circle/RoundedRectangle | High | Medium | Preserve existing variants/goldens and run full regression gates | 13.3A |

## Future Phase Order

The exact recommended sequence is:

1. Phase 13.3A — General Closed Profile Manufacturing Foundation: additive transport, analytic Line/Arc wire/face/extrude, defensive contour validation, and stronger STEP witnesses.
2. Phase 13.3B — Polygon Manufacturing & STEP: Line-only ClosedProfile, including concave fixtures.
3. Phase 13.3C — Composite Manufacturing & STEP: Line/Arc ClosedProfile with explicit arc-direction coverage.
4. Phase 13.4 — Slot Manufacturing & STEP: typed SlotCut carrying exact general contour geometry.
5. Phase 13.5 — Manufacturing Domain Closure Audit: cross-profile provenance, compatibility, STEP equivalence, and regression closure.

No implementation phase is started by this audit.

## Documents Created

- docs/adr/adr-057-general-closed-profile-manufacturing-foundation.md
- docs/specs/phase13.3-manufacturing-architecture-audit.md

The ADR index is updated to include ADR-057. The task plan and task ledger are updated only to record this future sequence; no implementation item is marked complete.

## Source Code Changed

NO

## Commit

Pending until verification and documentation-only commit.

## Final Worktree

Pending until commit.

## PUSHED

NO

## Next Phase

Phase 13.3A — General Closed Profile Manufacturing Foundation.

Do not implement it as part of this audit.
