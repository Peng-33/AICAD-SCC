# AICAD / SCC Phase 13.3B — Polygon Manufacturing & STEP Closure Audit

Status: PASS_WITH_NOTES (all strict freeze gates pass; the optional browser E2E attempt timed out)

Freeze decision: FREEZE

Date: 2026-09-18

Branch: `codex/aicad-pre-rollback-recovery`

Baseline: `8b60817b5337bea2654603614d32f7d19b862b75`

Implementation commits:

- `3854ac45a20f8bd0af393bdbcf613264648266ce` — preserve canonical Polygon edge order in Spatial lowering.
- `767d9ac0efec3ab667043b36d92be132cf2fa0b9` — lower typed Polygon through the existing ClosedProfile manufacturing path.
- `753c4ca86007b387fe310a8b333c9718a3f9d8c6` — validate Polygon delivery through CadQuery, STEP reimport, and UI artifact delivery.

## Closure result

Phase 13.3B enables one typed source path and no broader profile inference:

~~~text
Engineering BaseProfile::Polygon
→ SpatialBaseProfile::Polygon
→ exact canonical, line-only Spatial contour
→ SolidFootprint::ClosedProfile
→ existing CadQuery native wire / face / +Z extrusion
→ existing typed cylindrical cuts
→ STEP export
→ real STEP reimport
→ strengthened geometric equivalence
→ exact validated artifact delivery
~~~

The manufacturing boundary is frozen as:

~~~text
Rectangle        = SUPPORTED
Circle           = SUPPORTED
RoundedRectangle = SUPPORTED
Polygon          = SUPPORTED
Composite        = FAIL_CLOSED
Slot             = FAIL_CLOSED
~~~

A line-only contour is not sufficient to infer Polygon identity. The gate uses the typed source profile, so a line-only Composite remains rejected.

## Contract and evidence

- **Source identity and canonical order:** the Engineering Polygon source remains authoritative. Spatial lowering uses typed `PolygonEdge { index }` semantics to preserve canonical order (including indices with two decimal digits); manufacturing validates contiguous indices and source provenance and copies the exact endpoints without recanonicalization.
- **ClosedProfile bridge:** only the typed Spatial Polygon branch emits the existing `SolidFootprint::ClosedProfile`. It uses the existing `SolidBuildSpec`; no Polygon-specific transport variant or backend was added.
- **Line-only:** the Polygon bridge rejects Arc segments and malformed, discontinuous, open, misordered, or identity-mismatched contours. The backend receives generic exact contour segments and performs no tessellation, bounding-box substitution, or semantic reconstruction.
- **Shape coverage:** the canonical pentagon `(0,0), (100,0), (120,40), (80,90), (0,60)` is covered by the manufacturing golden. A concave Polygon fixture passes. A translated Polygon at `(+30,-20)` passes while preserving its translated extents.
- **Hole and patterns:** the existing typed cylindrical through-hole path works on Polygon. Circular patterns are expanded upstream into resolved cuts and are consumed as typed cylinders; manufacturing does not rebuild a pattern. The real integration fixture exercises a translated T8 Polygon, a Ø10 through-hole and a resolved 4×Ø8 / PCD20 cut pattern.
- **Source map:** edge indices and source edge IDs survive Engineering→Spatial→manufacturing lowering and are checked against the ordered line segments; no IDs are inferred from the output geometry.
- **CadQuery / STEP / reimport:** the existing generic native edge→ordered wire→face→+Z extrusion path is used. STEP is reimported and checked by the existing enhanced witnesses: bounds, volume, surface area, center of mass, topology counts, and deterministic material probes. Artifact hash and byte size correspond to the validated bytes delivered by the CLI/UI routes.
- **Golden and deterministic build:** `crates/scc-manufacturing/tests/goldens/polygon_solid_spec.json` captures the flagship build specification. The Rust test compares the exact golden and repeats the build 100 times for deterministic equality.
- **Adversarial coverage:** same-bounds bbox substitution is rejected; a wrong vertex/contour with the same bounding box is rejected by the real STEP equivalence validation. Rust tests also reject malformed order, unexpected Arc, open contours, footprint/source mismatch, and altered bounds.
- **Feature boundary:** Polygon+Slot remains rejected. Composite remains rejected even when its spatial contour is structurally line-only. Rectangle, Circle, RoundedRectangle, and the Phase 13.3A ClosedProfile foundation regressions pass.

## Required freeze gates

| Gate | Result and evidence |
|---|---|
| A — real Polygon production lowering | PASS — typed Engineering/Spatial Polygon production path |
| B — existing ClosedProfile route | PASS — exact line contour, existing SolidBuildSpec |
| C — no Polygon SolidBuildSpec variant | PASS — no new profile DTO/variant |
| D — no Polygon Python backend | PASS — generic ClosedProfile worker only |
| E — line-only contour | PASS — Arc and malformed contours rejected |
| F — canonical order preserved | PASS — typed edge indices and 12-edge regression |
| G — no manufacturing recanonicalization | PASS — endpoint order copied as supplied |
| H — no bbox substitution | PASS — real STEP adversarial rejects |
| I — no tessellation | PASS — native analytic lines |
| J — concave Polygon | PASS — manufacturing fixture |
| K — translated Polygon | PASS — `(+30,-20)` fixture and integration |
| L — Polygon + Hole | PASS — typed cylindrical through-hole |
| M — expanded multiple Hole cuts | PASS — resolved 4×Ø8 pattern cuts |
| N — Polygon + Slot | PASS — remains fail-closed |
| O — Composite | PASS — remains fail-closed |
| P — line-only Composite is not inferred as Polygon | PASS — rejected by typed identity gate |
| Q — Polygon manufacturing golden | PASS — exact `polygon_solid_spec.json` |
| R — Rust→CadQuery→STEP integration | PASS — Compose `cad-rust-test` |
| S — STEP reimport | PASS — Compose `cad-rust-test` |
| T — enhanced equivalence | PASS — geometric and material witnesses |
| U — wrong-contour adversarial | PASS — same-bounds wrong-vertex case rejected |
| V — bbox-substitution adversarial | PASS — same-bounds box extrusion rejected |
| W — Rectangle regression | PASS — workspace and Compose CAD gates |
| X — Circle regression | PASS — workspace and Compose CAD gates |
| Y — RoundedRectangle regression | PASS — workspace and Compose CAD gates |
| Z — ClosedProfile foundation regression | PASS — workspace and Compose CAD gates |
| AA — workspace check | PASS — `cargo check --offline --locked --workspace --all-targets` |
| AB — workspace tests | PASS — 49 suites, 1103 passed, 0 failed, 2 ignored |
| AC — focused Manufacturing tests | PASS — 32 passed |
| AD — Docker CadQuery tests | PASS — `docker compose run --rm cad-test`, 51 passed |
| AE — Rust/CadQuery/UI STEP tests | PASS — `docker compose run --rm cad-rust-test`, 4 passed, 0 skipped |
| AF — Compose config | PASS — `docker compose config --quiet` |
| AG — Rustfmt | PASS — `cargo fmt --all -- --check` |
| AH — strict Clippy | PASS — workspace/all-targets with `-D warnings` |
| AI — release CLI | PASS — locked offline release build |
| AJ — UI regression | PASS — fresh release binary; 125 passed, 0 failed, 4 skipped |
| AK — git diff check | PASS — `git diff --check` |
| AL — clean worktree | PASS — recorded after final documentation commit |

## Focused and end-to-end test record

| Command / suite | Result |
|---|---|
| `cargo test --offline --locked -p scc-engineering --all-targets` | PASS — 213 passed |
| `cargo test --offline --locked -p scc-spatial --all-targets` | PASS — 75 passed |
| `cargo test --offline --locked -p scc-manufacturing --all-targets` | PASS — 32 passed |
| `cargo test --offline --locked -p scc-cad-backend --all-targets` | PASS — 5 reported passed; the two worker-conditional functions return early without the worker variable |
| `docker compose run --rm cad-test` | PASS — 51 passed |
| `docker compose run --rm cad-rust-test` | PASS — 4 passed, 0 skipped; real CLI→CadQuery→STEP→UI delivery |
| `SCC_BIN="$PWD/target/release/scc" npm test --prefix apps/scc-ui` | PASS — 129 total, 125 passed, 0 failed, 4 skipped |
| `docker compose run --rm e2e` | NON-BLOCKING NOTE — one attempt; UI became healthy, then existing browser smoke timed out waiting 60 seconds for `[data-stage="solve"].complete` at `apps/scc-ui/e2e/browser.mjs:189`; not retried |

The browser E2E is explicitly non-blocking. The required real Polygon STEP path is exercised by the passing `cad-rust-test`; no browser-E2E pass is claimed. The UI container started for the one E2E attempt was stopped afterward, restoring its previously stopped state.

## Frozen final constants

~~~text
POLYGON_MANUFACTURING = SUPPORTED
POLYGON_STEP = SUPPORTED
POLYGON_USES_GENERAL_PROFILE = YES
POLYGON_SOLID_SPEC_VARIANT = NO
POLYGON_TESSELLATION = NO
POLYGON_BBOX_SUBSTITUTION = NO
POLYGON_CLOSED_PROFILE_ARCS = FORBIDDEN
MANUFACTURING_RECANONICALIZES_POLYGON = NO
PROFILE_TYPE_INFERRED_FROM_SEGMENTS = NO
SOURCE_PROFILE_IDENTITY_PRESERVED = YES
PYTHON_KNOWS_POLYGON = NO
PYTHON_POLYGON_HANDLER = NO
HOLE_REMAINS_TYPED_CYLINDER = YES
PATTERN_RECONSTRUCTED_IN_MANUFACTURING = NO
STEP_REEXPORTED_AFTER_VALIDATION = NO
COMPOSITE_MANUFACTURING = FAIL_CLOSED
SLOT_MANUFACTURING = FAIL_CLOSED
~~~

## Freeze decision

`PASS_WITH_NOTES` / `FREEZE`. Every strict P0 gate A–AL is satisfied and the worktree is clean at the closure commit. The sole note is the failed optional browser E2E attempt; this does not weaken or replace the passing real Rust→CadQuery→STEP integration gate. No Phase 13.3C work was started. Do not push this branch.
