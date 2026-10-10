# ADR-036: Hidden-line classification over raw principal-view projection

- Status: Accepted
- Date: 2026-09-04
- Related: ADR-032, ADR-033, ADR-034, ADR-035,
  crates/scc-visibility/

## Background

Phase 9.4 freezes a deterministic ProjectionModel containing every raw
Front/Top/Left Segment or Circle candidate, its depth evidence, deterministic
ProjectionPrimitiveId, and a separate ProjectionSourceMap. A raw candidate is
geometric evidence, not yet a drawing decision. The next boundary must decide
whether each candidate contributes a visible, hidden, or no independent stroke
without changing that evidence.

The current spatial domain is intentionally small: a rectangular body over
the canonical +Z axis and resolved cylindrical cuts. It has no BRep, Boolean
result, boundary arrangement, clipping, or candidate splitting.

## Decision

Phase 9.5 adds the independent leaf crate scc-visibility. Its only production
inputs are:

    SpatialModel + ProjectionModel
              -> VisibilityModel

The crate has no dependency on EngineeringModel, GeometryModel, DrawingModel,
rendering, UI, or pattern parameter APIs. It does not call
project_principal_views; it consumes the ProjectionModel supplied by its
caller.

ProjectionModel remains raw and frozen. VisibilityModel is a derived sidecar
with no geometry, depth, source, feature, entity, style, dash, centerline,
dimension, sheet, scale, or view-placement fields. The public result is:

    VisibilityModel {
        views: BTreeMap<PrincipalViewKind, VisibilityView>
    }

Each VisibilityView contains:

    decisions: BTreeMap<ProjectionPrimitiveId, VisibilityDecision>

VisibilityDecision contains exactly one VisibilityClass:
Visible, Hidden, or Occluded.

Serde is transport/debug support only. Deserialized VisibilityModel values
must explicitly pass validate_against(&ProjectionModel) before use.

## Validation gates

classify_visibility first calls ProjectionModel::validate_integrity and
validate_against(&SpatialModel), then validate_visibility_domain, then checks
the frozen candidate shape contract. It returns no partial result.

The classifier requires exactly Front, Top, and Left views. Every view must
contain eight body-boundary Segment candidates in four unoriented coincident
near/far pairs. Every cut must have two Front Circle candidates and four Top
and four Left Segment candidates. Source and role must agree: Body maps to
BodyBoundary and Cut maps to CutBoundary. These are structural input gates,
not a second projection implementation. Cross-view geometry/depth consistency
remains Phase 9.6 work.

## Visibility semantics

Visible is a raw candidate that contributes a visible outline or material
boundary. Hidden is a real material boundary that is behind nearer material
from the current view. Occluded means that the raw candidate contributes no
independent final stroke, including a farther coincident candidate or a rim
already represented by a visible body boundary. Occluded does not assert that
every point is physically covered by material.

The classifier never moves, normalizes, reverses, merges, splits, clips,
quantizes, deletes, or rewrites a raw candidate. A successful result contains
exactly one decision for every ProjectionPrimitiveId.

## Classification rules

Body candidates are grouped by unoriented Segment endpoint coincidence. In
each four-edge group, the candidate with the unique greatest depth is Visible;
all farther coincident candidates are Occluded. Equal-depth winners fail
closed as SCC-VIS-003. No insertion order or identifier spelling is semantic.

For Front cut circles, the two coincident circles are ordered by their raw
depth evidence. The nearest circle is Visible when cut.z.max equals the
SpatialModel body top, otherwise Hidden. The farther circle is Occluded.
The interval is consumed as already resolved; SpatialCutKind is never used to
re-lower it.

For Top and Left cut segments, a non-point depth range identifies a rim and a
point depth identifies a tangent. Tangents are Hidden. A rim fully contained
in a visible body boundary segment is Occluded; an interior rim is Hidden.
Containment and coincidence are orientation-independent, and containment
allows a proper rim subsegment.

The classifier vocabulary contains no PCD, grid, corner, row, column,
spacing, offset, or pattern-specific branch. Multiple cuts are processed as
the BTreeMap of resolved SpatialModel cuts.

## Supported domain and fail-closed policy

The analytic MVP supports a rectangular body, canonical +Z cylindrical cuts,
finite positive dimensions, cuts strictly inside the XY rectangle, pairwise
strictly disjoint and non-tangent XY disks, and resolved Z intervals that
touch the body base, top, or both. Construction circles are reference-only
and do not affect the result.

The following are unsupported visibility configurations and return
SCC-VIS-002 without a VisibilityModel:

- a cut outside or tangent to the rectangle;
- overlapping, tangent, or coincident cut disks;
- a cut with unsupported Z topology;
- any candidate shape or topology that would require clipping, splitting,
  partial classification, Boolean union, or a new boundary.

This support-domain gate is not EngineeringModel manufacturability
validation and does not modify or reject the SpatialModel.

## Numeric and determinism policy

All comparisons use one absolute VISIBILITY_TOLERANCE_MM of 1.0e-9 mm:
coincidence, depth equality, body-surface equality, containment, strict
rectangle containment, and cut separation. Distance uses hypot. Finite
intermediate values are required; arithmetic overflow fails closed. No
rounded strings or hash keys are used. Authoritative decisions and queries
use BTreeMap/BTreeSet; the classifier is serial and deterministic.

## Error contract

Visibility errors use ErrorStage::Visibility and SCC-VIS-001 through
SCC-VIS-005. They are structured, severity Error, recoverable false:

- SCC-VIS-001: invalid spatial/projection input or structural shape;
- SCC-VIS-002: unsupported visibility topology;
- SCC-VIS-003: ambiguous coincident depth winner;
- SCC-VIS-004: missing decision;
- SCC-VIS-005: invalid view/decision coverage or orphan decision.

Projection validation errors can remain as the structured source of
SCC-VIS-001. No visibility error returns a partial derived model.

## Boundaries and consequences

Phase 9.5 does not add Drawing style, dash semantics, centerlines,
dimensions, SVG, DXF, sheet space, First/Third angle placement, UI, or
Phase 9.6 consistency validation. Phase 9.7 owns Drawing integration.

The sidecar preserves complete raw projection evidence while enabling a future
Drawing layer to map Visible to object-line semantics, Hidden to hidden-line
presentation, and Occluded to omission. That presentation mapping is
intentionally deferred.

## Rejected alternatives

- Adding a visible or hidden boolean to ProjectionPrimitive would mutate the
  frozen Phase 9.4 schema and conflate evidence with interpretation.
- Removing farther candidates or merging coincident geometry would destroy raw
  provenance and prevent later validation.
- Re-projecting SpatialModel or reading EngineeringModel/GeometryModel would
  create a second geometry truth and violate the input boundary.
- Running Boolean, clipping, or candidate splitting would pretend to support
  topology that this phase cannot represent; those configurations fail closed.
- Parsing ProjectionPrimitiveId or relying on ordinal order would make
  classification depend on allocator spelling rather than geometry/depth.

## Compatibility and migration

Existing ProjectionModel and SpatialModel serde payloads remain unchanged.
VisibilityModel is an additive derived artifact and must be validated against
the exact projection instance from which it was produced. A future Drawing
integration may consume both artifacts without changing this phase's model.

## Verification

The implementation locks body, through-hole, blind-base, blind-opposite,
surface-rim containment, tangent, negative-coordinate, multi-cut, ordering,
serde, raw-model immutability, malformed-input, and deterministic behavior in
scc-visibility integration tests. It also preserves the Phase 8 SVG/DXF
goldens and existing Phase 9.2-9.4 regression gates.
