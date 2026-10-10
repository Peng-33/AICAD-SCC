# ADR-053: Slot and Oblong Feature Semantics

Status: Accepted for Phase 12.3

## Context

Phase 12.2 froze typed RoundedRectangle and one-loop Line/Arc Composite
profiles with BodySegment provenance. Phase 12.3 adds the smallest exact
through-slot cut: an oblong / long hole as a reusable material-removal
feature. This is not a general sketcher, pocket/groove system, slot pattern
system, or manufacturing expansion.

An `EngineeringFeature::Slot` stub already existed (`length`, `width`) and
failed closed at lowering and spatial readiness. Phase 12.3 promotes that
single variant into the formal Slot semantic. Oblong hole and long hole are
user-language synonyms only.

## Decision

1. One engineering truth: `EngineeringFeature::Slot`. No `OblongFeature` or
   `LongHoleFeature`. Slot is a cut feature, never `BaseProfile::Slot`.
2. Canonical dimensions are `overall_length` (persisted as existing `length`),
   `width`, `center`, and `orientation_degrees`. Center-distance `L - W` is
   derived geometry and is never a second editable truth.
3. `end_radius = width / 2`. Validity: finite values, `width > 0`, and
   `overall_length > width`. `overall_length == width` is rejected; a round
   hole remains `Hole`.
4. Orientation is finite and canonicalized to `[0°, 180°)`. `0°` and `180°`
   are the same engineering identity.
5. Extent is Through only. Blind slot, pocket, and groove depth are deferred.
   Slot does not reuse or reconstruct the `HoleExtent` map.
6. Geometry lowers to exact native `Line + Arc + Line + Arc` (two parallel
   sides and two directed semicircular ends), then rotated by orientation and
   translated to center. No polyline tessellation.
7. Spatial stores an exact slot footprint (`SlotCut`) extruded on `Z = [0, T]`.
   Bounding-rectangle cuts are forbidden (`SLOT_BBOX_CUT = NO`). A slot that
   leaves or intersects the body outline fails closed and is never clipped.
   Containment covers Rectangle, Circle, RoundedRectangle, and Composite
   bodies.
8. Front projection emits the exact four profile primitives with typed
   `SlotSegment { cut_id, index }` provenance. Top/Left emit the true
   Z-extrusion silhouette (interval × thickness) with `SlotCut` provenance.
   They do not copy Front primitives.
9. Visibility owns hidden/occluded classification. Drawing never invents
   hidden geometry and never infers Slot from two arcs and two lines.
10. Drawing emits one typed `SlotCalloutSemantic` formatted as
    `SLOT {L} × {W} THRU`. The formatted string is not engineering truth.
    Phase 11 remains the sole layout authority. Slot center location
    dimensions and angle dimensions are deferred.
11. Project v1 persists only typed Slot source: id, center, overall length,
    width, orientation, and bindings. Derived segments, spatial cuts,
    drawing primitives, and formatted callouts are not stored.
12. DSL `SLOT` and tool `add_slot` produce `EngineeringCommand::AddFeature`.
    Natural-language Slot is deferred (`NL_SLOT = P2`). Slot patterns are
    deferred. Manufacturing is fail-closed (`SLOT_MANUFACTURING = DEFERRED`)
    with no bounding-box SolidBuildSpec fallback.

## Frozen scope

Included: through Slot/oblong as a typed cut, exact Line/Arc lowering, exact
spatial footprint, Front/Top/Left presentation, typed provenance, one
production callout, DSL/tool entry, project round-trip, and compatibility
with Rectangle/Circle/RoundedRectangle/Composite bodies.

Deferred: Blind Slot, pocket, groove, slot pattern, angle annotation, slot
center location dimensions, NL Slot, and Slot SolidBuildSpec/CadQuery/STEP.

## Consequences

Existing stub Slot JSON (`id`, `length`, `width`) remains readable; missing
center/orientation default to the origin and `0°`, then run the new
validation and spatial path. Fillet remains unsupported. Hole's circular cut
pipeline is unchanged. No FeatureProfile mega-abstraction is introduced:
Hole and Slot stay two typed features.
