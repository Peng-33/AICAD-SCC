# Phase 13.5A — Canonical STEP Authority Consolidation Closure Audit

**Status: PASS_WITH_NOTES — FREEZE**
**Baseline:** `4205a3278f67e3190df2bed56855ff2d4ffddbda`
**Implementation checkpoint:** `7d45d80`
**Branch:** `codex/aicad-pre-rollback-recovery`
**Audit date:** 2026-09-19

## Decision

Phase 13.5A selected Strategy A: retire formal STEP generation and download
from the CadDocument/FeatureGraph compatibility route while preserving its
bounded non-STEP CAD editing and preview capabilities. The public compatibility
route no longer declares or writes a STEP artifact, and the service rejects both
client requests for `step` and any worker manifest that attempts to return one.

Formal STEP remains available through the EngineeringModel route only. That
route continues to solve and lower through SpatialModel and SolidBuildSpec,
execute the stateless CadQuery/OCP backend, reimport the exported STEP,
validate geometric/material/topology witnesses, verify artifact size and hash,
and deliver those exact validated bytes.

This closes the Phase 13.5A objective. It does not by itself re-audit or freeze
Phase 13.5 Manufacturing Domain Closure, and it does not claim that every
product editing surface has EngineeringModel as its mutable truth.

## Scope and evidence

- The pre-remediation Phase 13.5 audit and ADR-057 through ADR-061 were read.
- The actual `/api/cad/build` callers, artifact route, Node service, Python
  worker, FeatureGraph pattern code, UI buttons, E2E scenario, canonical
  Engineering STEP route, CLI adapter, and project export path were enumerated.
- The compatibility route is still reachable and still accepts a separate
  mutable CadDocument/FeatureGraph. It is now a compatibility editing state,
  not a formal STEP manufacturing authority.
- No CadDocument-to-EngineeringModel reverse inference or fake bridge was added.
- No EngineeringModel, SpatialModel, SolidBuildSpec, ClosedProfile, typed
  Hole/Slot, or canonical STEP validator implementation was rewritten.

## Compatibility route inventory

| Entry | Actual caller/use | Input authority | Output after Phase 13.5A | Formal STEP result |
|---|---|---|---|---|
| `POST /api/engineering/export/step` | Engineering UI and API clients | source-only EngineeringModel | canonical validated STEP bytes | sole public formal STEP route |
| `scc engineering export step` / `scc-cli::engineering::export_step` | CLI and UI adapter | EngineeringModel | canonical validated STEP artifact | canonical Rust/CadQuery/reimport path |
| `aicad_cad.step_delivery` | internal canonical worker invoked by Rust | SolidBuildSpec lowered from Engineering/Spatial | private validated artifact + witnesses | internal stage of canonical route |
| `POST /api/cad/build` | 3D compatibility UI and CAD clients | CadDocument / FeatureGraph | source JSON, BREP, STL, GLB, SVG, metrics, preview | no STEP artifact is generated or declared |
| `GET /api/cad/artifacts/:jobId/:name` | compatibility artifact downloads | bounded compatibility job manifest | retained non-STEP artifacts | `step` returns `SCC-CAD-010` and HTTP 400 |
| `aicad_cad.worker` | compatibility worker/tests | CadDocument / legacy CadSpec | non-STEP derivatives only | no `cq.exporters.export(...step)` call |
| compatibility UI download controls | browser 3D CAD workspace | compatibility job manifest | JSON/BREP/STL/GLB controls | STEP button removed; no hidden alias |
| project export UI | Engineering Workbench | EngineeringModel | validated STEP download | canonical route only |

The FeatureGraph `_circular_pattern` implementation remains reachable for
retained compatibility B-Rep/preview behavior. It cannot reach a formal STEP
artifact because the compatibility worker no longer exports STEP and the Node
service rejects a STEP manifest. Canonical Engineering patterns remain
resolved upstream before manufacturing lowering.

## Selected consolidation strategy

**Strategy A — close the compatibility formal STEP boundary.**

Strategy B was rejected because no small, lossless, semantically unambiguous
CadDocument-to-EngineeringModel conversion exists. Building such a bridge would
reconstruct the FeatureGraph semantic system and expand this phase beyond the
safe authority boundary.

The implementation is deliberately at the delivery boundary: it preserves
legacy editing and preview behavior without pretending that the compatibility
model has become EngineeringModel truth.

## Canonical STEP authority

```text
source EngineeringModel
  → solve / GeometryModel
  → SpatialModel
  → SolidBuildSpec
  → Rust CadBackendClient
  → stateless CadQuery/OCP worker
  → STEP export
  → STEP reimport
  → geometric/material/topology equivalence
  → artifact size/SHA-256 identity
  → exact validated-byte delivery
```

No formal STEP request accepts a CadDocument, reconstructs an EngineeringModel,
rebuilds a pattern in the compatibility route, or downloads an unvalidated
worker file. The canonical UI and CLI continue to deliver exactly the bytes
whose witness and artifact identity were checked.

## Preserved non-STEP compatibility features

- editable CadDocument/FeatureGraph parameters and requirement coverage;
- real OCCT B-Rep construction and validity/solid-count metrics;
- source JSON artifact and in-memory SVG preview;
- BREP, STL, GLB, and SVG derivative artifacts;
- interactive GLB browser preview;
- legacy precision gearbox compatibility construction;
- non-STEP FeatureGraph circular-pattern behavior for compatibility preview/B-Rep.

The compatibility CadDocument remains a second product-level editable state in
its bounded workspace. That is intentional and is not reported as a unified
product mutable truth.

## Public manufacturing matrix

| Capability | Canonical Engineering STEP result |
|---|---|
| Rectangle | PASS |
| Circle | PASS |
| RoundedRectangle | PASS |
| Polygon | PASS |
| Composite | PASS |
| Hole | PASS |
| Slot | PASS |

The matrix is covered by the existing Rust manufacturing tests, Docker
CadQuery/STEP tests, and real Rust/CadQuery/UI integration tests. No capability
was downgraded by removing compatibility STEP.

## Closure gates

| Gate | Result | Evidence |
|---|---|---|
| A. Formal STEP entries enumerated | PASS | route inventory and static route scan |
| B. No independent compatibility STEP delivery | PASS | worker removal; service/API tests |
| C. Server cannot bypass canonical validation | PASS | canonical route unchanged; compatibility STEP rejected |
| D. No unvalidated formal STEP download alias | PASS | artifact route rejects `step`; UI button removed |
| E. Python patterns do not reach formal manufacturing | PASS | compatibility path has no STEP output; canonical patterns remain upstream-resolved |
| F. Formal STEP inputs have Engineering authority | PASS | only Engineering export API/CLI delivers formal STEP |
| G. STEP reimport/equivalence remains valid | PASS | Docker CAD and six real integration cases |
| H. Validated bytes equal delivered bytes | PASS | canonical artifact hash/size contract and UI delivery test |
| I. Non-STEP compatibility has no regression | PASS | Docker 58/58; Node route/service tests |
| J. Rectangle/Circle/RoundedRectangle | PASS | canonical manufacturing and real integration suites |
| K. Polygon/Composite | PASS | canonical manufacturing and real integration suites |
| L. Hole/Slot | PASS | canonical manufacturing and real integration suites |
| M. Real API bypass rejection | PASS | server/service tests, including forbidden worker manifest |
| N. Workspace checks/tests | PASS | 49 summaries, 1111 passed, 0 failed, 2 ignored |
| O. Docker CAD/STEP integration | PASS | `cad-test` 58/58; `cad-rust-test` 6/6 |
| P. fmt/Clippy/release/UI/Compose | PASS | all required commands exit 0 |
| Q. ADR-063 and closure audit | PASS | ADR-063 and this document |
| R. P0/P1 Phase13.5A blocker | PASS | no unresolved Phase13.5A blocker; Phase13.5 re-audit remains separate |
| S. Git worktree clean | PASS | final documentation commit verified clean |
| T. No automatic push | PASS | no push performed |

## Tests actually executed

| Command | Result |
|---|---|
| `node --test apps/scc-ui/test/cad-service.test.mjs apps/scc-ui/test/server.test.mjs` | PASS — 13 passed, 0 failed, 0 skipped; exit 0 |
| `python3 -m unittest apps.scc-ui.cad.tests.test_feature_graph apps.scc-ui.cad.tests.test_gearbox` | NOT_AVAILABLE locally — Python environment lacks CadQuery; Docker equivalent passed |
| `SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui` | PASS — 132 total, 126 passed, 0 failed, 6 skipped; exit 0 |
| `docker compose run --rm cad-test` | PASS — 58 passed, 0 failed, 0 skipped; exit 0 |
| `docker compose run --rm cad-rust-test` | PASS — 6 passed, 0 failed, 0 skipped; exit 0 |
| `cargo check --offline --locked --workspace --all-targets` | PASS; exit 0 |
| `cargo test --offline --locked --workspace --all-targets` | PASS — 49 summaries, 1111 passed, 0 failed, 2 ignored; exit 0 |
| `cargo fmt --all -- --check` | PASS; exit 0 |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | PASS; exit 0 |
| `cargo build --offline --locked --release -p scc-cli` | PASS; exit 0 |
| `docker compose config --quiet` | PASS; exit 0 |
| `git diff --check` | PASS; exit 0 |
| Browser E2E | NOT RUN; non-blocking by phase contract. E2E source was updated to assert BREP download and no compatibility STEP button. |

## Remaining architecture debt

1. CadDocument/FeatureGraph remains a separate product-level editable state for
   the generic 3D compatibility workspace. This phase does not claim that all
   product editing truth is EngineeringModel.
2. Phase 13.5 Manufacturing Domain Closure must be re-audited after this
   remediation; this document must not be treated as ADR-062 or a Phase 13.5
   freeze decision.
3. Inner loops, multiple contours, general sketch/history, richer operations,
   Slot patterns, and NL Slot remain deferred as documented domain boundaries.
4. Existing geometric witness equivalence is a strong deterministic contract,
   not a mathematical proof of all possible pointwise geometry.

`BLOCKS_PHASE13_5A_FREEZE = NO`
`PHASE13_5_STATUS = NOT_REAUDITED`
`PHASE13_5_FREEZE = DO_NOT_FREEZE_PENDING_REAUDIT`
`PUSHED = NO`

## Next phase

If the owner elects to continue, the next separately authorized phase is:

**Phase 13.5 — Manufacturing Domain Closure Re-Audit**

This phase stops here and does not automatically execute that re-audit.
