# Phase 13.1 Circular Manufacturing Closure Audit

Status: PASS

Freeze decision: FREEZE

Baseline: `8bfb3dc` Phase 12.4 Polygon closure
Branch: `codex/aicad-pre-rollback-recovery`
Pushed: NO

## Commits

- `8c6e90d` feat(manufacturing): support circular base solid specs
- `b7d78a5` feat(cad): build circular base solids
- `b96c59e` test(step): validate circular flange roundtrip

## Manufacturing architecture

EngineeringModel remains the sole engineering truth. SpatialModel is the
resolved 2.5D truth and expands Hole/CircularPattern cuts. SolidBuildSpec is
the backend-neutral manufacturing snapshot. CadQuery consumes only its typed
serialized contract. STEP is derived and is not project truth.

The Rust lowering path accepts SpatialFootprint::Circle and emits exact typed
SolidFootprint::Circle. Rectangle serialization remains unchanged.
Composite, Polygon, RoundedRectangle, and Slot still fail closed without
envelope approximation.

## Circle contract

Circle stores resolved center and radius plus the existing Z interval and
SpatialModel bounds witness. The contract rejects non-finite or non-positive
radius, invalid thickness, arithmetic overflow, and mismatched bounds.

```text
xmin = cx - radius
xmax = cx + radius
ymin = cy - radius
ymax = cy + radius
zmin = 0
zmax = thickness
```

Golden:
`crates/scc-manufacturing/tests/goldens/circular_disk_solid_spec.json`.

The historical Rectangle golden is unchanged.

## CadQuery and STEP

The Python DTO parser accepts typed Rectangle or Circle bodies under schema
version 1. Circle uses an exact CadQuery Workplane circle extrusion at the
resolved center and Z plane. Existing cylindrical operations are applied
without reconstructing engineering features or pattern metadata.

STEP delivery builds once, exports that shape, imports the exact file, and
validates single-solid, bbox, volume, material probes, provenance, and
artifact identity. No second export occurs after validation.

## Fixtures

- Disk: OD 160, thickness 20, centered at the origin.
- Translated disk: OD 160, thickness 20, center (30,-20), bbox
  [-50,110] x [-100,60] x [0,20].
- Annular flange: OD 160 with a resolved center Hole diameter 60.
- Flagship flange: OD 160, center Hole 60, thickness 20, and eight resolved
  diameter-14 cuts on PCD 120.
- Expression-derived diameter: `outer_diameter := pcd + 40`; manufacturing
  receives only resolved SpatialModel values.

## Tests actually executed

- `cargo test --offline --locked -p scc-manufacturing --all-targets`: 18
  passed, 0 failed.
- `docker compose config --quiet`: passed.
- `docker compose run --rm --build cad-test`: 35 passed, 0 failed.
- `docker compose run --rm --build cad-rust-test`: 2 passed, 0 failed,
- `SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui`: 125 passed, 0 failed, 2 skipped (Rust STEP tests are covered by cad-rust-test).
- `cargo check --offline --locked --workspace --all-targets`: passed.
- `cargo test --offline --locked --workspace --all-targets`: passed.
- `cargo fmt --all -- --check`, `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`, and `cargo build --offline --locked --release -p scc-cli`: passed.
  0 skipped, including the Circle flagship chain.
- Python syntax compilation: passed.
- Host Python CadQuery was unavailable; the official Docker runtime is the
  authoritative Python gate.

## Required boundaries

```text
Circle             = SUPPORTED
Rectangle          = SUPPORTED
RoundedRectangle   = FAIL_CLOSED
Composite          = FAIL_CLOSED
Polygon            = FAIL_CLOSED
Slot               = FAIL_CLOSED

PYTHON_CONSUMES_ENGINEERING_MODEL = NO
CIRCLE_SOLID_APPROXIMATION = NO
PATTERN_RECONSTRUCTED_IN_MANUFACTURING = NO
STEP_REEXPORTED_AFTER_VALIDATION = NO
```

## Freeze

Phase 13.1 is frozen complete for Circle manufacturing and STEP support.
No Phase 13.2 work is started by this change.
