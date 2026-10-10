# AICAD / SCC Phase 13.3A — General Closed Profile Foundation Closure Audit

Status: PASS_WITH_NOTES (Completion Pass; all strict freeze gates passed)

Freeze decision: FREEZE

Date: 2026-09-17

Branch: `codex/aicad-pre-rollback-recovery`

Implementation baseline: `2d0f2c9`

Completion-pass baseline: `5671773`

Completion-pass commits: `9e7b13c`, `c048254`

Implementation commits: `1139e0b`, `6edd958`, `a458801`, `2ee4556`

## Closure result

Phase 13.3A is complete as an internal manufacturing foundation. It establishes exact ClosedProfile transport, defensive Line/Arc validation, native CadQuery construction, and stronger STEP equivalence. It does not claim public Polygon, Composite, or Slot manufacturing.

The frozen manufacturing matrix is:

~~~text
Rectangle        = SUPPORTED
Circle           = SUPPORTED
RoundedRectangle = SUPPORTED
ClosedProfile    = INTERNAL_FOUNDATION_ONLY
Polygon          = FAIL_CLOSED
Composite        = FAIL_CLOSED
Slot             = FAIL_CLOSED
~~~

## Scope and authority

The implemented path is:

~~~text
validated SolidBuildSpec
→ Rust manufacturing defensive validation
→ Python strict DTO decode
→ native CadQuery/OCP Line/Arc edges
→ ordered single outer wire
→ planar face
→ +Z linear extrusion
→ cylindrical cuts only
→ STEP export
→ real STEP reimport
→ strengthened equivalence witnesses
→ validated artifact
~~~

The public `SpatialModel → SolidBuildSpec` lowering remains unchanged for unsupported profile sources: Spatial `Composite` and `Polygon` are still rejected with `UnsupportedManufacturingFeature`, and non-empty `slot_cuts` are still rejected. No SlotCut, generic cut profile, inner loop, multiple contour, or general sketch system was introduced.

## Implemented contract

### Rust transport and validation

`SolidFootprint::ClosedProfile` carries one ordered `Vec<SolidProfileSegment>` with exact `Line` and `Arc` variants. Arc direction is explicit. The manufacturing validator checks finite values, non-degenerate segments, endpoint continuity and closure, arc radius/endpoints, directed sweep, analytic signed area, and analytic cardinal extrema for bounds. Bounds remain a witness copied into the build specification and must match the contour-derived envelope exactly.

No tessellation, convex-hull approximation, bounding-box replacement, source-ID parsing, or backend semantic reconstruction is present. Existing schema-version-1 goldens remain unchanged.

### Python/CadQuery backend

The strict DTO accepts only `closed_profile` with exact segment keys and one outer segment array. The backend uses native CadQuery analytic edges, an OCP wire builder that receives segments in supplied order, a planar face, and `Solid.extrudeLinear` along +Z. Invalid wire/solid results fail closed with structured backend errors. Cylindrical cuts retain their existing typed operation path.

### STEP equivalence

The solid witness now includes:

- single-solid validity and cardinality;
- exact rounded bounding-box coordinates;
- volume;
- surface area;
- center of mass;
- edge, face, and vertex counts;
- deterministic material probes.

`all_match` requires bounds, volume, surface area, center of mass, topology counts, and material witnesses to match. A real same-bbox/same-volume pair with different L-contours proves that the center-of-mass witness rejects a contour substitution even when the older bbox/volume/topology checks agree.

## Completion Pass (2026-09-17)

The earlier closure entry at implementation baseline `2d0f2c9` declared `FREEZE` before all strict gates in the completion prompt had evidence. That decision was premature and is superseded by this section. The two focused test commits above close the missing golden, translation fixture, and adversarial coverage; every required strict gate was then rerun and passed. No production geometry/backend behavior or public capability was enabled or changed.

### Fixture and adversarial evidence

- `closed_profile_solid_spec.json` is an internal manufacturing fixture only: one ordered outer loop, five exact segments, four Lines plus one explicitly counter-clockwise Arc, and a +Z interval from 0 to 10. Its full parsed `SolidBuildSpec` matches the golden. Rectangle, Circle, and RoundedRectangle goldens remain unchanged.
- The translated fixture applies (+30, -20) to the mixed Line/Arc profile. Rust→CadQuery→STEP reimport preserves source and reimported bounds `(30, 130, -20, 40, 0, 10)` and the matching translated center of mass; surface area, topology counts, and material probes also match.
- Bbox substitution uses a real concave profile and a box extrusion with identical bounds; the actual equivalence validator rejects it on volume and material witnesses.
- Arc loss replaces an analytic Arc with its chord while retaining the same broad bounds; the actual equivalence validator rejects the changed solid.
- Translation loss compares the translated profile with the same profile at the origin; bbox and center-of-mass witnesses reject it.
- The retained same-bounds/same-volume L-contour adversary confirms equal bbox, volume, surface area, and topology counts but different center of mass; `all_match` is false.
- Open-loop validation fails before Face construction: Rust manufacturing validation reports a non-closed loop, and the Python backend test verifies `_build_closed_profile` is never called and returns `CAD-BACKEND-004`. No topology repair is attempted.

### Equivalence witness stability

Real CadQuery/OCP construction and STEP reimport passed for the concave and mixed Line/Arc profiles, including the translated fixture. For the exercised STEP fixtures, bbox, volume, surface area, center of mass, edge count, face count, vertex count, and deterministic material probes all remain stable. The translated Rust adapter test explicitly compares edge/face/vertex counts and all material witness results across source and reimport. No witness was removed or weakened.

### Exact verification results

| Required command | Result |
|---|---|
| `cargo check --offline --locked --workspace --all-targets` | PASS, exit 0 |
| `cargo test --offline --locked --workspace --all-targets` | PASS, 1096 passed, 0 failed, 2 ignored, exit 0 |
| `cargo test --offline --locked -p scc-manufacturing --all-targets` | PASS, 26 passed, 0 failed, 0 ignored, exit 0 |
| `cargo test --offline --locked -p scc-cad-backend --all-targets` | PASS, 5 passed, 0 failed, 0 ignored, exit 0; the two environment-conditional integration bodies return early when the worker variable is absent |
| `docker compose run --rm cad-test` | PASS, 48 passed, 0 failed, exit 0 |
| `docker compose run --rm cad-rust-test` | PASS, 3 passed, 0 failed, 0 skipped, exit 0 |
| `SCC_CAD_PYTHON=/tmp/aicad-cadquery-worker cargo test --offline --locked -p scc-cad-backend --test closed_profile_step -- --nocapture` | PASS, 2 passed, 0 failed, 0 ignored, exit 0; real Rust adapter → CadQuery → STEP/reimport |
| `docker compose config --quiet` | PASS, exit 0 |
| `cargo fmt --all -- --check` | PASS, exit 0 |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | PASS, exit 0 |
| `cargo build --offline --locked --release -p scc-cli` | PASS, exit 0 |
| `SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui` | PASS, 125 passed, 0 failed, 3 skipped, exit 0 |
| `git diff --check` | PASS, exit 0 |

The exact focused CAD-backend command reports the two conditional test functions as passed because they return early without `SCC_CAD_PYTHON`; the separate worker-backed command above executes both tests against the real CadQuery/STEP path and passes 2/2. UI skips are the three existing Rust→CadQuery cases outside the integration runtime; all three execute and pass in `cad-rust-test`.

### Freeze invariants

~~~text
GENERAL_CLOSED_PROFILE_FOUNDATION = IMPLEMENTED
GENERAL_PROFILE_IS_DERIVED = YES
GENERAL_PROFILE_IS_EDITING_TRUTH = NO
GENERAL_PROFILE_TESSELLATION = NO
GENERAL_PROFILE_BBOX_SUBSTITUTION = NO
CADQUERY_GENERAL_PROFILE_SUPPORTS_CONCAVE = YES
ARC_DIRECTION_RECONSTRUCTION_FROM_ENDPOINTS = NO
BACKEND_SEGMENT_REORDERING = NO
PRIMARY_PROFILE_VALIDATION_OWNER = ENGINEERING_SPATIAL
MANUFACTURING_DEFENSIVE_VALIDATION = YES
SPEC_IDENTITY_IS_GEOMETRIC_EQUIVALENCE = NO
STEP_EQUIVALENCE_STRENGTHENED = YES
BBOX_SUBSTITUTION_DETECTED = YES
ARC_GEOMETRY_LOSS_DETECTED = YES
TRANSLATION_LOSS_DETECTED = YES
POLYGON_MANUFACTURING = FAIL_CLOSED
COMPOSITE_MANUFACTURING = FAIL_CLOSED
SLOT_MANUFACTURING = FAIL_CLOSED
~~~

Browser E2E was not executed and remains non-blocking. With all strict gates above passed, the current decision is `PASS_WITH_NOTES` / `FREEZE`.

## Historical verification (superseded)

The earlier verification counts below describe the implementation closure only and are not evidence for the strict Completion Pass above: 24 focused manufacturing tests, 43 CadQuery tests, and one real ClosedProfile adapter fixture. Those results have been superseded by the exact rerun counts in the Completion Pass table.


| Gate | Result | Evidence |
|---|---:|---|
| `cargo fmt --all` | PASS | clean |
| `cargo test --workspace` | PASS | all workspace tests passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | clean |
| `cargo test -p scc-manufacturing` | PASS | 24 tests |
| `cargo test -p scc-cad-backend` | PASS | 3 unit tests + adapter integration test; host run safely skips only when CadQuery env is absent |
| Rust adapter → Compose CadQuery → STEP/reimport | PASS | concave ClosedProfile integration, 1/1, real worker |
| Docker `cad-test` | PASS | 43 Python/CadQuery tests |
| Docker `cad-rust-test` | PASS | 3 existing Rust→CadQuery→STEP tests |
| Node/UI `npm test` | PASS_WITH_NOTES | 128 tests: 125 passed, 3 existing Rust STEP tests skipped outside the CadQuery integration flag |
| Browser E2E | NOT_EXECUTED | non-blocking; no browser claim made |

The real cross-layer ClosedProfile test was executed with a temporary wrapper that routed the host Rust adapter into the current Compose CadQuery runtime; the wrapper was deleted immediately and no repository file or product interface was changed.

## Regression and freeze gates

- Rectangle, Circle, and RoundedRectangle remain supported.
- Existing cylindrical Hole/cut semantics are unchanged.
- Polygon, Composite, and Slot remain fail-closed.
- No SlotCut or inner-loop/multiple-contour support is present.
- No public product/UI capability matrix was expanded.
- STEP artifacts remain ephemeral and validated before delivery.
- No push was performed.

## Known notes

1. The new ClosedProfile foundation is intentionally not wired from Engineering/Spatial Polygon or Composite sources. Those profile-specific enablement phases must add source provenance goldens and product-level acceptance separately.
2. Browser E2E was not executed in this closure; it is not used as evidence for manufacturing correctness.
3. The conditional Rust adapter tests return early without `SCC_CAD_PYTHON`; the explicit worker-backed completion command exercised both ClosedProfile integration cases against the real CadQuery/STEP path. The temporary worker wrapper was removed after testing.

## Documents

- [ADR-057 architecture decision](../adr/adr-057-general-closed-profile-manufacturing-foundation.md)
- [ADR-058 implementation decision](../adr/adr-058-general-closed-profile-solid-and-step-equivalence.md)
- [Phase 13.3 architecture audit](phase13.3-manufacturing-architecture-audit.md)

## Next phase

Phase 13.3B — Polygon Manufacturing & STEP, Line-only ClosedProfile with concave fixtures. Composite and Slot remain deferred behind their own gates.

## Source code changed

YES

## PUSHED

NO
