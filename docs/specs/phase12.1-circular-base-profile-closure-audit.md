# Phase 12.1 — Circular Base Profile & Circular-part Three-view Support

- Date: 2026-09-14
- Task type: LARGE_FEATURE
- Decision: PASS_WITH_NOTES
- Freeze: FREEZE

## Phase 12.1 Status

Phase 12.1 is complete for the drawing-first boundary. AICAD now supports
rectangular and circular 2D/2.5D engineering base profiles through the same
typed source-to-drawing chain. The circular flagship is represented as a
formal EngineeringModel, not as a drawing-only circle or a rectangle-shaped
fallback.

The selected scope is deliberately narrower than a general profile kernel:
only `BaseProfile::Circle` was added. Arc, ellipse, rounded rectangle, slot,
oblong, polygon profile, arbitrary sketch, ring feature, boss, pocket, and
general BRep remain deferred.

## Subagent / Skill Execution

`SUBAGENT_EXECUTED = UNAVAILABLE`.

The requested explorer-A and explorer-B read-only subagent attempts timed out
during repository-wide scanning and were closed. No subagent result is claimed
in this audit. `LOCAL_EQUIVALENT_EXECUTED = YES`: the same architecture and
downstream assumption audits were performed locally with repository search,
source inspection, focused tests, and full regression gates.

`SKILL_EXECUTED = NONE`: no available skill matched this Rust/CAD repository
change.

## Pre-implementation Base Profile Audit

The local audit found the original rectangle truth and migration points in:

- `crates/scc-engineering/src/feature.rs`: `EngineeringFeature::BaseProfile`
  originally owned `width`/`height` directly.
- `crates/scc-engineering/src/binding.rs`: rectangle slots were
  `Width`/`Height`; circular slots needed `CenterX`/`CenterY`/`Diameter`.
- `crates/scc-engineering/src/lowering.rs`: the base feature lowered to a
  rectangle geometry entity and supplied the body anchor.
- `crates/scc-spatial/src/lowering.rs`: the body footprint and bounds were
  rectangle-only.
- `crates/scc-projection/src/principal.rs`: body projection was rectangle-only
  before the explicit circular branch.
- `crates/scc-drawing/src/three_view_annotations.rs`: overall body dimensions
  were width/height-owned and circular primitives could not be treated as
  body-vs-cut semantics without provenance.
- `crates/scc-manufacturing/src/lowering.rs`: `SolidBuildSpec` v1 was a
  rectangle-body contract and was audited before choosing the boundary.

The existing GeometryModel already had a typed `Circle` primitive, but it was
used primarily for holes and construction geometry. The migration therefore
adds a typed source/provenance path rather than reusing circle size or
“largest radius” as a semantic guess.

## Rectangle Assumption Audit

The downstream assumptions were classified as follows:

- `GENERIC_2D_PROFILE_READY`: source maps, solved circle geometry, projection
  primitives, visibility decisions, center geometry, SVG/DXF rendering, and
  source-only project persistence now accept the typed circle branch alongside
  the existing rectangle branch.
- `RECTANGLE_ONLY_INTENTIONAL`: `SolidBuildSpec` v1 remains intentionally
  rectangle-only in this phase. Circular manufacturing returns a structured
  unsupported error.
- `RECTANGLE_ONLY_NEEDS_GENERALIZATION`: Engineering source, binding,
  spatial footprint, principal projection, visibility, drawing dimension
  ownership, and UI/tool/DSL manifests were generalized in this phase.

The audit specifically checked `width`, `height`, body footprint, projection
silhouette, overall dimensions, sheet bounds, SolidBuildSpec, project JSON,
CLI/tool manifest, and the UI natural-language entry points. No second circular
model or frontend geometry calculation was introduced.

## Selected BaseProfile Architecture

The frozen source architecture is:

```text
BaseProfile
├─ Rectangle { width, height }
└─ Circle { center: Point2D, diameter }
```

The implementation is a serde-compatible typed sum in
`crates/scc-engineering/src/feature.rs`. Legacy rectangle JSON remains
readable. Circle JSON is inferred by its typed field set under the existing
untagged compatibility representation; there is no string `shape_type` truth.

## Circular Engineering Contract

`BaseProfile::Circle` is EngineeringModel truth. It owns the center and the
outer diameter. A center hole remains an ordinary `Hole`; it is not an inner
radius of the base profile. A bolt circle remains the existing
`HolePattern::Circular`; no flange-specific pattern type was added.

Validation rejects non-finite centers and non-positive or non-finite diameter.
Parameter binding supports `CenterX`, `CenterY`, and `Diameter`, and the
expression evaluator resolves diameter before geometry lowering.

The controlled DSL now accepts:

```text
BASE_CIRCLE main
  CENTER 30 -20
  DIA 160
```

with the existing `BIND main DIAMETER <- parameter` path. The formal tool
surface also exposes `create_base_circle(id, center_x, center_y, diameter)`.

## Diameter / Radius Canonical Policy

Diameter is the canonical engineering source because the mechanical and
drawing vocabulary is outer-diameter-first. Geometry lowering derives exactly
one radius (`diameter / 2`) for the typed `GeometryPrimitive::Circle`. Radius
is never an independently editable second truth.

## Circular Center Contract

The source supports `(0, 0)`, non-zero centers, and negative coordinates. The
center is carried through parameter bindings, solved geometry, exact spatial
footprint, projection provenance, center marks, and persistence. No arbitrary
transform layer was added.

## Geometry Lowering

`lower_base_profile` emits a formal circle entity for the circle source. The
parameterized fixture `OD = PCD + 40` resolves to `160` before lowering and
produces radius `80`; drawing code consumes the solved witness and does not
re-evaluate the expression.

Invalid diameter, unresolved binding, expression, and geometry/source mismatch
paths fail closed.

## Provenance

Body circle geometry maps to the `BaseProfile` feature/entity. Hole circles map
to `Hole` or `HolePattern` cut entities. Projection source maps and drawing
resolved witnesses preserve that distinction. Body-vs-hole semantics do not
depend on radius ordering or “largest circle” heuristics.

## Spatial Circular Prism

`SpatialFootprint` now has an exact `Circle { center, radius }` variant. The
body occupies that footprint over the existing `[0, thickness]` interval;
`Bounds3D` is only the derived envelope used for range bookkeeping. A circular
body is never converted into a `160 × 160` rectangle as its spatial truth.

## Front Projection

The circular body produces two source-backed body circles in Front (the two
depth boundary witnesses of the prism). Hole circles remain cut-boundary
primitives. The Front drawing owns the single typed overall body diameter.

## Top Projection

Top is generated from the circular prism side silhouette, with source-backed
segment geometry representing the diameter/thickness extent. It is not a copy
of the Front circle. Through-hole evidence remains available for hidden-line
classification.

## Left Projection

Left uses the same true side-projection contract with thickness on its depth
axis and the circular diameter extent on its profile axis. The Left view owns
the thickness dimension and does not repeat the Front body diameter.

```text
TOP_VIEW_COPIED_FROM_FRONT = NO
LEFT_VIEW_COPIED_FROM_FRONT = NO
```

## Visibility

Visibility continues to be derived from Projection depth and typed spatial
cut witnesses. Front body/cut circles and Top/Left hidden hole evidence are
classified by source role and depth; Drawing does not invent hidden lines.

## Outer Diameter Annotation

`DrawingDimensionKind::OverallDiameter` is a typed body-owned annotation. For
the circular flagship, Front contains `Ø160` and does not contain
`overall_width` or `overall_height` for the body. Hole `Ø60` remains a separate
cut-owned diameter callout.

## Centerlines

Circular body and hole center marks/centerlines use the existing center
geometry subsystem. Center source identity remains typed, including the
non-zero-center flange case.

## Center Hole Reuse

The flagship center bore is the existing `Hole` feature with a `ThroughAll`
extent. It resolves to `Ø60 THRU` through the existing hole-callout semantic
and witness path.

## Circular Pattern Reuse

The eight bolt holes are the existing `HolePattern::Circular` feature. No new
flange pattern or bolt-circle source type exists.

```text
NEW_FLANGE_PATTERN_TYPE = NO
```

## PCD Reuse

PCD comes from the existing circular-pattern semantic. The flagship renders
`8× Ø14 THRU PCD Ø120`; the formatter was extended only to ensure PCD remains
visible when the typed pattern has no explicit tolerance specification.

## Phase11 Layout Reuse

Circular body diameter, hole, pattern, center, and thickness annotations enter
the existing Phase 11.2 layout/reflow authority. No circular-specific collision
engine or second annotation layout was introduced.

```text
SECOND_LAYOUT_ENGINE = NO
```

## DrawingScale

Scale changes paper placement and layout envelopes only. The circular body
continues to format as `Ø160` at the tested scale/projection paths; the
measurement is never scaled to `80` or `320`.

## First/Third

First- and Third-angle semantics remain placement-only. They do not change
outer diameter, hole diameter, pattern count/PCD, or thickness semantics. The
existing rectangle projection tests remain green.

## SVG

SVG is rendered from the derived DrawingModel. New byte-level goldens:

- `crates/scc-drawing/tests/goldens/phase121_circular_disk.svg`
- `crates/scc-drawing/tests/goldens/phase121_circular_flange.svg`

## DXF

DXF is rendered from the same DrawingModel and has matching new goldens:

- `crates/scc-drawing/tests/goldens/phase121_circular_disk.dxf`
- `crates/scc-drawing/tests/goldens/phase121_circular_flange.dxf`

The flagship integration test checks SVG/DXF semantic parity for diameter,
hole/pattern callouts, hidden lines, and center marks.

## AicadProject Persistence

Project v1 persists the source `BaseProfile::Circle` center and diameter. It
does not persist a derived GeometryModel circle, SpatialModel, projection,
DrawingModel, or renderer text. Save/open round-trip coverage verifies the
typed source and non-zero center.

## Rectangle Backward Compatibility

Existing rectangle construction, legacy rectangle JSON, rectangle projects,
rectangle drawing goldens, and the rectangle SolidBuildSpec v1 contract remain
unchanged. The full workspace and UI suites passed after the circular changes.

## Natural Language

The safe formal natural-language boundary is now available through both the
controlled `BASE_CIRCLE` new-design DSL and the registered
`create_base_circle` edit tool. The prompt explicitly instructs the provider
to keep center holes as `HOLE` and bolt circles as `HOLE_PATTERN CIRCULAR`.
The Rust tool adapter test verifies that the tool creates typed Circle source,
not generated coordinates or a rectangle.

## NL Preview == Commit

The existing Ask → Clarify → Preview → Confirm → Commit API remains the only
mutation route and its full UI/API regression suite passed. The new circular
DSL path and typed tool path both feed the same EngineeringModel/solve route;
no separate circular preview or commit semantics were added. A dedicated
browser-level circular Workbench scenario was not executable in this host (see
Browser E2E), so this closure is `PASS_WITH_NOTES` rather than unconditional
browser PASS.

## Manufacturing Boundary

The architect audit selected Strategy B (drawing-first): the current
SolidBuildSpec v1 body schema is not silently changed. Circular spatial
lowering returns a structured unsupported-manufacturing error with the reason
that no rectangle approximation is allowed.

```text
CIRCULAR_MANUFACTURING = DEFERRED
```

## Circular SolidBuildSpec

Not implemented in Phase 12.1 by design. There is no circular
`SolidBuildSpec` golden and no circular body is accepted by the rectangle-only
v1 manufacturing contract.

## CadQuery

Not invoked for circular bodies. Python continues to receive only
`SolidBuildSpec` and never receives EngineeringModel or Circular Base source
semantics.

## STEP

No circular STEP claim is made. Existing rectangle validated STEP/CadQuery
contracts remain covered by their prior regression suites; circular STEP is a
future manufacturing-boundary decision.

## Flagship Flange

The canonical flange fixture is:

```text
Outer Diameter = 160
Center Hole = 60
Thickness = 20
Bolt Holes = 8×14
PCD = 120
```

Results:

- Front: body `Ø160`, center `Ø60 THRU`, `8× Ø14 THRU PCD Ø120`, body/hole
  center marks and centerlines.
- Top: source-backed diameter/thickness silhouette with hidden through-hole
  geometry.
- Left: source-backed silhouette with hidden through-hole geometry and owned
  thickness `20`.
- Drawing: deterministic SVG/DXF with the same annotation/provenance model.
- Manufacturing: explicit deferred error; no fake rectangular solid.

## Existing Rectangle Goldens

Existing rectangle and prior Phase 11 SVG/DXF golden assertions remain in
place and pass. No rectangle golden was replaced by a circular fixture.

## New Circular Goldens

The disk and flange SVG/DXF goldens listed above are newly added and checked by
permanent byte-level tests in `three_view_integration.rs`.

## SolidBuildSpec Compatibility

Rectangle SolidBuildSpec v1 and its existing golden remain unchanged. Circular
manufacturing is not represented as a new v1 variant, so there is no schema
compatibility migration to hide.

## Mutation Tests

The fail-closed matrix covers the circular boundary through:

- invalid/zero/negative/non-finite diameter tests in engineering lowering;
- typed base/solved-geometry mismatch and exact circle footprint tests in
  spatial lowering;
- body/cut role, geometry, radius, source-map, and depth mutation tests in
  projection-consistency/visibility/drawing;
- body overall-diameter ownership and absence of rectangular body dimensions;
- explicit circular-manufacturing rejection without rectangle approximation.

The disk integration test also rejects the observable Mutation B/C outcomes:
side views must be segment silhouettes and the body must expose only
`overall_diameter`, not duplicated width/height dimensions. Typed provenance
tests reject Mutation D; the manufacturing boundary rejects Mutation E.

## Adversarial Tests

Covered adversarial inputs include zero/negative/non-finite diameters, finite
non-zero and negative centers, parameter/expression-driven diameter, resolved
center bindings, no-hole disk, center-hole flange, eight-hole circular pattern,
hidden side cuts, drawing renderer parity, project round-trip, deterministic
serialization, and the explicit manufacturing unsupported path. Existing
rectangle, blind-hole, scale, first-angle, third-angle, and cache/fingerprint
regressions also remain green.

## Browser E2E

`BROWSER_E2E = NOT_EXECUTED`.

The dedicated CUA attempt failed before browser state discovery because the
desktop sandbox could not create the read-only visualization mount metadata:
`bwrap: Can't create file .../.agents: Read-only file system`. No Front/Top/Left
browser screenshot or click path is claimed.

## Tests Actually Executed

Final gates and focused evidence:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo check --workspace --all-targets --offline --locked` | exit 0 |
| `cargo test --offline --locked --workspace --all-targets` | exit 0 after the 17-tool assertion sync |
| `cargo test --offline --locked -p scc-engineering --all-targets` | 196 unit + 2 back-compat passed |
| `cargo test --offline --locked -p scc-dsl --all-targets` | 35 unit + 11 integration passed |
| `cargo test --offline --locked -p scc-project --all-targets` | 6 passed |
| `cargo test --offline --locked -p scc-drawing --all-targets` | 20 unit + 131 integration + 36 three-view passed |
| `cargo test --offline --locked -p scc-manufacturing --all-targets` | 14 integration passed |
| `cargo test --offline --locked -p scc-tools --all-targets` | 2 unit + 26 integration passed |
| `npm test` in `apps/scc-ui` | 125 passed, 0 failed, 1 Docker-only STEP test skipped |
| `docker compose config --quiet` | exit 0 |
| `git diff --check` | exit 0; existing `.gitignore` CRLF warning only |

The workspace test command was run again after the DSL `BASE_CIRCLE` addition;
its final exit code was 0. The UI test was run after restoring the locked local
`three@0.184.0` dependency with `npm ci --offline --ignore-scripts`.

## Reviewer Verdict

No second-truth route was found. EngineeringModel remains authoritative;
geometry, spatial, projection, visibility, drawing, and renderers remain
derived; Python/CadQuery remains downstream of SolidBuildSpec only; and the
circle-to-rectangle manufacturing fallback is explicitly prohibited.

## Remaining Debt

### BLOCKS_12_2

None for the frozen Phase 12.1 drawing boundary. Circular manufacturing is a
declared deferred capability, not a hidden blocker.

### NON_BLOCKING

- Browser Workbench visual/E2E was not executable in the current CUA sandbox.
- Circular SolidBuildSpec/CadQuery/STEP is intentionally unsupported.
- Full natural-language flagship thickness/STEP browser proof remains future
  work; the controlled DSL and typed tool entry points are available.

### PRODUCT_POLISH

- Add a dedicated circular-flange Workbench fixture and visual affordances
  after browser automation is available.
- Decide whether circular pattern PCD construction geometry should be rendered
  as a visible construction circle in a future presentation pass; current typed
  PCD annotation is complete and deterministic.

### FUTURE_2D_DOMAIN

- Arc Profile
- Rounded Rectangle
- Slot / Oblong Hole
- Polygon Profile
- General Closed Profile

### FUTURE_DRAWING

- GD&T
- Datum systems
- Hole tables
- Full external drawing-standard conformance

## Phase 12.1 Freeze Decision

`FREEZE` with `PASS_WITH_NOTES`. The note is limited to unavailable browser
coverage and the explicitly deferred circular manufacturing boundary. No
Phase 12.2 implementation was started.

## Recommended Next Phase

Recommend exactly:

```text
Phase 12.2 — Arc, Rounded & Composite 2D Profile Foundation
```

Planning only. Do not start it under this task.
