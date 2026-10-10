# Phase 13.2 RoundedRectangle Manufacturing Closure Audit

Status: PASS

Freeze decision: FREEZE

Baseline: `2f1ec6c` Phase 13.1 circular manufacturing closure
Branch: `codex/aicad-pre-rollback-recovery`
Pushed: NO

## Commits

- `b6fadcc` feat(manufacturing): support rounded rectangle solid specs
- `f850cfd` feat(cad): build exact rounded rectangle solids
- `6859310` test(step): validate rounded rectangle roundtrip
- closure commit contains the compatibility fix, explicit Polygon fail-closed test, and this audit

## Manufacturing architecture

EngineeringModel remains the sole engineering truth. SpatialModel is the
resolved 2.5-D input and now carries typed `SpatialBaseProfile` semantics in
addition to its exact boundary footprint. SolidBuildSpec is the backend-neutral
derived snapshot. CadQuery consumes only serialized SolidBuildSpec JSON. STEP
is derived, reimported, equivalence-checked, and never becomes source truth.

## RoundedRectangle SolidBuildSpec

`SolidFootprint::RoundedRectangle` preserves center, width, height, and
corner_radius. The body interval remains `Z=[0,T]`; bounds are a witness copied
from SpatialModel and checked against center plus/minus half extents.

```text
ROUNDED_RECTANGLE_INFERRED_FROM_SEGMENTS = NO
ROUNDED_RECTANGLE_BBOX_SUBSTITUTION = NO
ROUNDED_RECTANGLE_TESSELLATION = NO
EXPRESSION_REEVALUATED_IN_MANUFACTURING = NO
```

## Geometry and placement contract

The simple fixture is center `(0,0)`, `120 x 80`, radius `10`, thickness `8`,
with bbox `[-60,60] x [-40,40] x [0,8]`. The translated fixture is center
`(25,-15)` with the same dimensions and bbox `[-35,85] x [-55,25] x [0,8]`.
The asymmetric fixture is `140 x 60`, radius `12`, thickness `15`, proving
width/height are not swapped.

CadQuery uses an exact rectangle extrusion followed by native vertical-edge
fillets. The independent test witness is
`V=(W*H-(4-pi)*R^2)*T`; it does not become a second solid authority.

## Hole and pattern behavior

Existing resolved `CylindricalCut` operations are applied unchanged. The
flagship fixture uses a translated `120 x 80 R10 T8` body with a resolved
center `Ø12 THRU` hole. Pattern definitions are not reconstructed:

```text
PATTERN_RECONSTRUCTED_IN_MANUFACTURING = NO
NEW_ROUNDED_RECTANGLE_HOLE_TYPE = NO
```

## Parameter-derived fixture

The real Rust integration fixture binds `plate_width=120`,
`plate_height:=plate_width-40`, and `corner_radius=10` before export. The
manufacturing layer receives only resolved SpatialModel values; it never
reads those expressions.

## Invalid geometry and deferred profiles

Width, height, radius, thickness, NaN, Inf, and oversized radius mutations fail
closed before a STEP artifact is produced. Composite, Polygon, and Slot remain
fail-closed; no bbox or ignored-slot fallback is present.

```text
Composite = FAIL_CLOSED
Polygon   = FAIL_CLOSED
Slot      = FAIL_CLOSED
```

## STEP and artifact delivery

The generic CLI/API path performs one solve, lowers to SolidBuildSpec, invokes
the CadQuery worker, exports one private STEP file, reimports it through OCP,
and validates single-solid count, bbox, volume, material probes, and
provenance. The returned bytes are the validated bytes.

```text
PYTHON_CONSUMES_ENGINEERING_MODEL = NO
STEP_REEXPORTED_AFTER_VALIDATION = NO
```

## Capability matrix

Rectangle, Circle, and RoundedRectangle manufacturing/STEP are supported.
Composite, Polygon, and Slot remain deferred. Existing Rectangle and Circle
goldens are unchanged; `rounded_rectangle_solid_spec.json` is the new golden.

## Tests actually executed

- `cargo fmt --all -- --check`: pass.
- `cargo check --offline --locked --workspace --all-targets`: pass.
- `cargo test --offline --locked --workspace --all-targets`: 1090 passed, 0 failed, 0 ignored across 48 suites.
- `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`: pass.
- `cargo build --offline --locked --release -p scc-cli`: pass.
- `docker compose config --quiet`: pass.
- `cargo test --offline --locked -p scc-manufacturing --all-targets`: 22 passed, 0 failed.
- `docker compose run --rm --build cad-test`: 39 passed, 0 failed.
- `docker compose run --rm --build cad-rust-test`: 3 passed, 0 failed.
- `SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui`: 125 passed, 0 failed, 3 skipped (Rust STEP tests are covered by cad-rust-test).
- `git diff --check`: pass.
- Browser E2E: NOT_EXECUTED; non-blocking for this phase.

## Freeze

Phase 13.2 is frozen complete only when the final worktree is clean after
this audit commit. Phase 13.3 is not started automatically.
