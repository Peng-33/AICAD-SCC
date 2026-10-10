# Phase 9 Completion Audit

## Phase 9 Completion Status

`PASS_WITH_NOTES`. Phase 9.6, 9.7, and 9.8 implementation, integration, regression tests, and deterministic golden checks are complete. The browser visual gate was not executed in this environment, and fixtures C-E do not each have a dedicated formal DrawingModel golden.

## Phase 9 Freeze Decision

`FREEZE`. The notes are non-blocking. No Phase 10.1 implementation was started.

## Phase 9 Architecture Audit

The frozen chain is:

`EngineeringModel -> lower_and_solve -> lower_spatial -> project_principal_views -> classify_visibility -> build_three_view_drawing -> drawing_to_svg/DXF`

The three-view builder owns the single Projection Consistency Validator (PCV) gate. It copies upstream projected geometry and visibility decisions; it does not re-project, re-classify, clip, split, or infer manufacturing semantics. The Node adapter and Workbench only select a mode, request the artifact, and present the returned bytes.

## Cross-layer Truth Audit

| Layer | Responsibility | Truth consumed |
| --- | --- | --- |
| Engineering | semantic commands, constraints, solve result | EngineeringModel |
| Spatial | deterministic 2.5D extrusion and cut intervals | solved EngineeringModel |
| Projection | Front/Top/Left orthographic candidates and provenance | SpatialModel |
| Visibility | Visible/Hidden/Occluded classification | SpatialModel + ProjectionModel |
| PCV | fail-closed consistency validation | SpatialModel + ProjectionModel + VisibilityModel |
| Drawing | view layout, role mapping, render channels | validated upstream models |
| UI/Node | transport, mode and lifecycle | DrawingModel/SVG bytes |

No UI or CAD compatibility backend is an Engineering truth source. No SolidBuildSpec or EngineeringModel-to-CadQuery bridge is part of Phase 9.

## Canonical Fixture Matrix

The requested A-E family is covered by distributed frozen Rust fixtures and integration tests. This table records direct coverage without inventing a single fixture file where none exists.

| Fixture | Spatial | Projection | Visibility | Drawing / golden | Result |
| --- | --- | --- | --- | --- | --- |
| A: body-only 120 x 80 x 10 | base slice and deterministic lowering | eight body candidates per view | body counts `(4,0,4)` | three-view pipeline and layout tests | covered |
| B: through diameter 10 | through interval | exact candidate counts `10/12/12` | Front `(5,0,5)`, side/top `(4,2,6)` | hidden channel and PCV-gated drawing tests | covered |
| C: blind base, T10/D4 | blind interval from base | interval consumed without recomputation | Front `(4,1,5)`, sides `(4,3,5)` | upstream contract; no separate blind formal golden | covered with note |
| D: blind opposite, T10/D4 | opposite blind interval | direct interval provenance | side hidden-rim classification | upstream contract; no separate blind formal golden | covered with note |
| E: eight-hole through | circular pattern and cut propagation | pattern candidate expansion | Front `(12,0,12)`, sides `(4,16,20)` | multi-cut semantics; formal golden uses one-hole plate | covered with note |

Relevant tests include `base_slice_120x80x10`, `through_all_interval`, `blind_base_interval`, `blind_opposite_interval`, `circular_pattern_eight_cuts`, `body_generates_eight_segments_in_each_canonical_view`, `blind_interval_is_consumed_directly_and_kind_is_not_recomputed`, `front_through_and_blind_rules_are_explicit`, `builds_three_canonical_views_from_frozen_sidecars`, `visibility_mapping_is_object_hidden_or_omitted`, and `formal_three_view_goldens_match_renderer_output`.

## Frozen Goldens

| Artifact | SHA-256 |
| --- | --- |
| legacy SVG | `cadfcc705fe7a3c07cfaf3d6d8ef014172739d94b663927f6f06daee9b997d19` |
| legacy DXF | `fdbe293786547cbc11ede2c69f4beef844f0dbe3c47c009fdad2ce57c4be320b` |
| three-view SVG | `97360a8c696cb1f70d153e18fb2a12c3e5a13a06e22e1f7758fb1c58d11525c9` |
| three-view DXF | `52c921adddfad87403aa19b443f3d7313b05a4a69b343d4a0600e35f1cf32fee` |

The contract freezes canonical `front`, `top`, and `left` IDs, First-angle and Third-angle layout transforms, one uniform scale, fixed paper-space gap and 10 mm margin, stable source-map IDs, Visible/Hidden/Occluded mapping, and deterministic output. Hidden SVG uses a distinct dashed channel and DXF uses `AICAD_HIDDEN`.

## Tests Actually Executed

- `cargo test --locked --workspace --all-targets`: 951 passed, 0 failed, 2 ignored.
- `cargo fmt --all -- --check`: pass.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: pass.
- `cargo build --locked --release -p scc-cli`: pass.
- UI `npm test` with the release `scc` binary: 111 passed, 0 failed.
- Real-binary drawing API tests: 10 passed; focused drawing UI tests: 17 passed.
- Mutation gates A, B, and C observed their required acceptance failures; source bytes were restored.

## Phase 10.0 Status

`BASELINE_ESTABLISHED_WITH_NOTES`. Phase 10.0 is a reproducibility and architecture audit only. It adds this audit and `docs/capabilities.json`; it does not implement a manufacturing bridge.

## Current Test Inventory

| Area | Inventory |
| --- | --- |
| Rust workspace | 951 passed, 0 failed, 2 ignored |
| Focused Rust package runs | engineering 188; spatial 71; projection 72; visibility 16; PCV 27; drawing 160; CLI 87 |
| Node UI | 111 passed |
| Focused three-view UI/API | UI 17 passed; API 10 passed |
| Python CAD in reference Docker | 5 passed |
| Host Python CAD | 0 tests executed because cadquery import is unavailable |
| Playwright browser/E2E | not run |

## Current CadQuery / Python CAD Architecture

The Python entrypoint is `apps/scc-ui/cad/aicad_cad/worker.py`. It dispatches `precision_gearbox_housing_v1` or generic `feature_graph_v1`, builds an OCCT B-Rep, and exports JSON, STEP, BREP, STL, GLB, and SVG artifacts with volume and bounding-box metrics. The generic kernel in `feature_graph.py` supports ordered box, cylinder, and circular-hole-pattern add/cut features and rejects disconnected or no-op results.

Node normalizes `CadDocument` parameters before invoking the worker. The worker itself does not re-import STEP for metric equivalence. The Python tests re-import STEP and check validity; gearbox coverage also checks an imported X extent. Imported bbox/volume preservation is not currently asserted.

The reference runtime is Python 3.12.11, CadQuery 2.8.0, and OCP 7.9.3.1. The host is Python 3.14.7 without CadQuery/OCP. The requirements file pins CadQuery, while OCP is transitive; the Docker base/image tags are not digest-pinned.

## Python CAD Tests Actually Executed

`docker compose run --rm --no-deps cad-test` passed 5/5. The native host command `PYTHONPATH=apps/scc-ui/cad python3 -m unittest discover -s apps/scc-ui/cad/tests -v` was attempted and failed before test execution with two `ModuleNotFoundError: cadquery` import errors. `docker compose config --quiet` passed. The browser suite was not run.

## Current Engineering ↔ CAD Separation

`/api/engineering/*` and `/api/cad/*` are separate routes and separate data paths. EngineeringModel drives the deterministic 2D/2.5D/three-view chain. CadDocument/FeatureGraph drives the separate CadQuery compatibility backend. There is no EngineeringModel-to-SolidBuildSpec or EngineeringModel-to-CadQuery bridge, and no Solid change is included in this phase.

## Golden Manufacturing Fixture

No unified EngineeringModel-driven manufacturing fixture exists yet. The frozen Phase 9 spatial fixture is a 120 x 80 x 10 plate with a center diameter-10 through hole and is suitable as the future bridge seed. A future manufacturing baseline may add four corner diameter-8 through holes and retain Third-angle presentation, but this is a specification-only recommendation here, not an implemented SolidBuildSpec.

## Capability Matrix

The machine-readable current-facts matrix is [docs/capabilities.json](../capabilities.json). EngineeringModel rows mark Solid and STEP as unavailable. ConstructionCircle is explicitly reference-only: it can survive engineering/spatial lowering for bounds and legacy drawing purposes, but is not a projected or visibility-classified manufacturing edge. The separate CadDocument backend facts are recorded independently and must not be read as EngineeringModel capabilities.

## Dirty Worktree Audit

The worktree contains extensive tracked and untracked changes from prior AICAD phases, including Phase 9 crates, CLI/UI integration, CAD compatibility files, docs, fixtures, and tests. Exact attribution of every dirty path relative to HEAD cannot be proven from the current worktree, so pre-existing ownership is `UNCERTAIN`. All existing changes were preserved. No reset, clean, checkout, stash, commit, add, or push was run.

## README Reality Check

Root and UI README claims were minimally corrected: Phase 9.8 is described as frozen, Phase 10.0 as a baseline audit, and the real CadQuery/OCCT statement is scoped to the separate CadDocument/FeatureGraph compatibility backend. Neither README claims an EngineeringModel-to-solid bridge.

## Blocks Phase 10.1

No correctness blocker was found for starting a future Phase 10.1. Baseline notes requiring explicit planning are: host CadQuery is unavailable, browser visual/E2E was not executed, OCP is transitive and image tags are not digest-pinned, imported bbox/volume equivalence is not tested, and no unified EngineeringModel manufacturing fixture exists.

## Phase 10.0 Freeze Decision

`FREEZE_WITH_NOTES`. The reproducible baseline is documented and machine-readable. Stop here as required; do not implement Phase 10.1 in this task.

## Recommended Next Phase

Phase 10.1 — EngineeringModel → SolidBuildSpec
