# Phase 12.2 — Arc, Rounded & Composite 2D Profile Foundation Closure Audit

- Date: 2026-09-15
- Task type: LARGE_FEATURE
- Decision: PASS_WITH_NOTES
- Freeze: FREEZE
- Status: IN_PROGRESS at takeover; closed in this audit

## Phase 12.2 Status

`PASS_WITH_NOTES`. The only note is `BROWSER_E2E = NOT_EXECUTED`.

`FULL_REAUDIT_REPEATED = NO`. This task resumed the dirty worktree and completed remaining P0 provenance, tests, goldens, and freeze gates. It did not re-run the Sol/Grok/Luna architecture audit and did not start Phase 12.3.

## Architecture

ADR-052 remains the frozen architecture contract. `EngineeringModel` is source truth.

```text
BaseProfile
├─ Rectangle
├─ Circle
├─ RoundedRectangle
└─ Composite
```

RoundedRectangle stores `center`, `width`, `height`, and `corner_radius`. Composite stores one ordered outer loop of typed `Line` and `Arc` segments. Geometry and spatial may lower RoundedRectangle to Line+Arc; the engineering source identity is never discarded into an anonymous composite.

## RoundedRectangle

Typed semantic = YES. Fixtures 120×80 R10 T=8 at origin and at center `(30,-20)` pass Front exact profile, Top/Left real projections, Width/Height/`R10`/Thickness, and CornerPattern `4×Ø8 THRU`. Radius annotation is one `R10` from the typed RoundedRectangle slot, not `4×R10` inferred from equal arcs.

## Composite

Typed semantic = YES. One simple closed outer loop of Line/Arc only. Inner loops, islands, Bezier, Spline, Ellipse, and general sketch constraints remain out of scope. Generic Composite does not emit `three_view.corner_radius`.

## Line

Typed segment = YES. Front Line primitives use `SpatialElementRef::BodySegment { index }` matching the spatial `segment_index`.

## Arc

Typed segment = YES. Native `ProjectedGeometry::Arc` / `DrawingPrimitive::Arc`. Engineering rejects `radius <= 0`, coincident endpoints, invalid/non-finite center, radius mismatch, zero sweep, ambiguous 180° sweep, NaN, and Inf. Ambiguous semicircles stay rejected.

## Closure Validation

CONNECTED = YES, CLOSED = YES, NON_DEGENERATE = YES. Tests cover a valid line-arc loop plus disconnected, open, and zero-length rejection.

## Self-intersection

NON_SELF_INTERSECTING = YES. Tests cover line-line, line-arc, and arc-arc crossings using the existing analytic intersection helpers. No new geometry kernel was added.

## Typed provenance

```text
SEMANTIC_ID_PARSING = NO
GEOMETRY_INFERRED_PROVENANCE = NO
```

Front Line/Arc sources are `BodySegment { index }` from the spatial segment index, never from parsing primitive IDs or matching geometry after the fact.

## BodySegment provenance

P0 at takeover: Front composite projection already emitted `BodySegment`, but drawing `source_for_projected` did not match it (compile break) and PCV grouped Front segments under `Body`, so a Body-only Front would still validate.

Minimal patch:

- Drawing maps `BodySegment` to the body feature/entity for engineering annotations.
- Envelope/radius witnesses may query the Body family so Width/Height/`R10` still attach without collapsing provenance.
- PCV Front Composite validates each `BodySegment { index }` against that exact near/far pair and rejects Front `Body` Line/Arc sources.

Top/Left silhouettes remain `Body`.

## Spatial footprint

Exact composite footprint = YES.

```text
BBOX_USED_AS_SPATIAL_TRUTH = NO
```

RoundedRectangle spatial is `SpatialFootprint::Composite` of Line+Arc, not a bounding rectangle. Bounds are an envelope only.

## Front

Exact native Line/Arc = YES. Front segment provenance = YES.

## Top

Real projection from exact profile extrema plus thickness = YES.

```text
TOP_VIEW_COPIED_FROM_FRONT = NO
```

## Left

Real projection = YES.

```text
LEFT_VIEW_COPIED_FROM_FRONT = NO
```

## Visibility

PASS. Front Composite uses `2 * segment_count`, not a rectangle/circle magic count. `Body | BodySegment` is treated as body evidence. No second classifier.

## Projection consistency

PASS. Expected Front Composite geometry comes from the supplied `SpatialFootprint::Composite` segments. PCV does not re-lower EngineeringModel and does not guess profiles. Front Body-only provenance is rejected.

## Drawing

PASS. Width, Height, and one corner Radius for RoundedRectangle. Composite keeps native Line/Arc and does not invent a rounded identity.

## Radius annotation

PASS. Policy remains one `R10`. Equal-radius arc counting is not used to infer RoundedRectangle.

## Layout

```text
SECOND_LAYOUT_ENGINE = NO
```

Radius/Width/Height enter the existing Phase 11 annotation layout / collision authority. No `rounded_profile_layout.rs` or `arc_layout.rs`.

## SVG

PASS. Native arc paths (`A` commands). Same DrawingModel as DXF.

## DXF

PASS. Native `ARC` entities. Same DrawingModel as SVG.

## SVG/DXF parity

PASS on the Phase 12.2 fixtures and existing renderer parity tests.

## Project persistence

Rounded Save/Open = PASS. Composite Save/Open = PASS. Segment order and arc direction are preserved. Project JSON stores Engineering source only.

## Backward compatibility

Rectangle, Circle, Phase 11, and Phase 12.1 goldens were not modified. Old hashes remain the frozen files already in tree.

## Manufacturing boundary

```text
ROUNDED_MANUFACTURING = DEFERRED
COMPOSITE_MANUFACTURING = DEFERRED
```

Requests fail closed with `UnsupportedManufacturingFeature`. No bounding-rectangle SolidBuildSpec fallback.

## Goldens

Existing unchanged:

- `crates/scc-drawing/tests/golden/engineering_plate.svg`
- `crates/scc-drawing/tests/golden/engineering_plate.dxf`
- `crates/scc-drawing/tests/goldens/three_view_plate.svg/.dxf`
- `crates/scc-drawing/tests/goldens/tolerance_plate.svg/.dxf`
- `crates/scc-drawing/tests/goldens/phase9_three_view_plate.svg/.dxf`
- `crates/scc-drawing/tests/goldens/phase121_circular_disk.svg/.dxf`
- `crates/scc-drawing/tests/goldens/phase121_circular_flange.svg/.dxf`

New Phase 12.2 goldens:

- `crates/scc-drawing/tests/goldens/phase122_rounded_rectangle.svg`
- `crates/scc-drawing/tests/goldens/phase122_rounded_rectangle.dxf`
- `crates/scc-drawing/tests/goldens/phase122_composite_profile.svg`
- `crates/scc-drawing/tests/goldens/phase122_composite_profile.dxf`

## Mutation tests

| ID | Result |
| --- | --- |
| A RoundedRectangle spatial → bbox rectangle | FAIL as required (`rounded_rectangle_spatial_is_exact_composite_not_bbox_rectangle`) |
| B Arc → provenance-less lines | FAIL as required (Front Arc stays `ProjectedGeometry::Arc` with BodySegment) |
| C Composite segment reorder | FAIL/identity change as required (project round-trip inequality) |
| D ID-string Arc detection | FAIL as required (IDs do not contain `arc`; typed geometry+source is required) |
| E Infer RoundedRectangle from 4 equal arcs | FAIL as required (generic Composite has no `R10`) |
| F Front Line/Arc provenance is only Body | FAIL as required until BodySegment; BodySegment then PASS |

## Determinism

100-run drawing SVG/DXF loops for Composite; existing 100-run harnesses for projection, spatial, manufacturing, and engineering lowering remain green. No second determinism harness.

## Browser E2E

`BROWSER_E2E = NOT_EXECUTED`. `timeout 25 docker compose run --rm e2e` started the UI to healthy and created the e2e container, then exited 124. No browser result is claimed.

## Tests actually executed

| command | passed | failed | ignored/skipped | exit |
| --- | --- | --- | --- | --- |
| `cargo check --offline --locked --workspace --all-targets` | compile | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-engineering --all-targets` | 204 unit + 2 back-compat | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-spatial --all-targets` | 25 unit + 49 integration | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-projection --all-targets` | 42 unit + 13 principal + 21 model | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-visibility --all-targets` | 5 unit + 14 integration | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-projection-consistency --all-targets` | 2 unit + 26 integration | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-drawing --all-targets` | 21 unit + 131 drawing + 41 three-view | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-project --all-targets` | 8 | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-dsl --all-targets` | 35 unit + 12 integration | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-manufacturing --all-targets` | 15 integration | 0 | 0 | 0 |
| `cargo test --offline --locked --workspace --all-targets` | 1047 | 0 | 2 | 0 |
| `cargo fmt --all -- --check` | pass | 0 | 0 | 0 |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | pass | 0 | 0 | 0 |
| `git diff --check` | pass | 0 | 0 | 0 (existing `.gitignore` CRLF warning only) |
| `cargo build --offline --locked --release -p scc-cli` | pass | 0 | 0 | 0 |
| `SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui` | 125 | 0 | 1 Docker-only STEP | 0 |
| `docker compose config --quiet` | pass | 0 | 0 | 0 |
| `docker compose run --rm cad-test` | 31 | 0 | 0 | 0 |
| `docker compose run --rm cad-rust-test` | 1 | 0 | 0 | 0 |
| `timeout 25 docker compose run --rm e2e` | not completed | n/a | n/a | 124 |

## Remaining debt

### BLOCKS_PHASE12_2_FREEZE

NONE.

### NON_BLOCKING

- Browser Workbench visual/E2E was not completed in this environment.
- Host CadQuery remains unavailable; Docker is the reference manufacturing runtime.
- Generic Composite auto-dimensioning of arbitrary arc radii remains deferred; RoundedRectangle emits one `R10`.

### FUTURE_2D_DOMAIN

```text
Slot
Oblong
Polygon
Multiple contours
Inner contours
General sketch constraints
```

### FUTURE_MANUFACTURING

```text
Circle
RoundedRectangle
Composite
→ SolidBuildSpec
→ CadQuery
→ STEP
```

## Phase 12.2 Freeze Decision

```text
FREEZE
```

`BLOCKS_PHASE12_2_FREEZE = NONE`. Required drawing-first gates passed. Do not start Phase 12.3 in this task.
