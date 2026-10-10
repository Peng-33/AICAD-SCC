# Phase 13.3C — Composite Manufacturing & STEP Closure Audit

Status: PASS
Freeze: FREEZE
Date: 2026-09-18

## Scope and Git record

Phase 13.3C enables only typed Composite base-profile manufacturing and validated STEP delivery through the existing generic exact `ClosedProfile` path. Slot, inner loops, multiple contours, islands, and generalized sketch/manufacturing features remain outside scope.

- Official repository: `<repo>`
- Branch: `codex/aicad-pre-rollback-recovery`
- Original recovered implementation baseline: `6b0fb9795f4b31cff467fab6a985f4dffbe30711`
- Prior Phase 13.3C verification checkpoint: `564db4e`
- Starting HEAD for this safe-closure pass: `fe6c18dde1c21d71a70ac61a5fbce2691a9140f6`
- Owner documentation preservation commit: `fe6c18d` — `docs(workspace): update AICAD paths and restore diagnostic plans` (separate from Composite implementation)
- Composite implementation commits: `e221f5a` (preserve typed segment order), `4252df1` (lower typed Composite to ClosedProfile), `9401c47` (STEP round-trip validation)
- Line-only Composite identity test commit: `a6a6e32` — `test(manufacturing): preserve line-only composite identity` (test-only)
- Final documentation commit: `docs(manufacturing): close phase 13.3c composite support` (this audit, ADR-060, and synchronized project status files)
- Push: NO

The worktree was clean at the start of this pass after the owner documentation had been preserved separately. No owner recovery/planning file was mixed into the implementation or closure commits.

## Owner documentation resolution

- `docs/specs/recovery-closure-audit.md`: coherent historical recovery audit; the separately preserved change corrects its repository path to the current Work/AICAD location. The report remains explicitly scoped to the recovery baseline and does not claim Phase 13 completion.
- `docs/plans/AICAD-工程化架构技术方案.md`: dated 2026-08-11, complete 1,549-line technical plan; Markdown code fences are balanced.
- `docs/plans/AICAD-愿景差距诊断报告.md`: dated 2026-08-11, complete 486-line gap assessment and recommendations; code fences are balanced.
- `docs/plans/AICAD-操作界面诊断报告.md`: dated 2026-08-11, complete 126-line UI diagnostic; no code fences or editor-temp artifacts.
- `docs/plans/AICAD-混乱诊断报告.md`: dated 2026-08-11, complete 143-line project-organization diagnostic; no code fences or editor-temp artifacts.
- `docs/plans/AICAD-项目介绍与架构说明.md`: dated 2026-08-11, complete 327-line architecture overview; code fences are balanced.

The five plan files identify themselves as dated diagnostic/plan snapshots, not current capability truth. Their historical August 2026 statements are therefore not treated as current status. All six owner files were already preserved together in the separate commit `fe6c18d`; they were not restaged, rewritten, or included in the Phase 13.3C commits.

## Typed identity and exact geometry

Engineering `BaseProfile::Composite` owns an ordered segment vector. Engineering source-map semantics identify each segment as `CompositeSegment` with its feature and index. Spatial lowering preserves the distinct `SpatialBaseProfile::Composite` identity and an exact `SpatialFootprint::Composite`; each Line/Arc carries source entity and contiguous `segment_index`, with explicit Arc direction. Manufacturing gates on the typed profile identity, validates the corresponding exact contour and provenance, then copies segments in source order to generic `SolidFootprint::ClosedProfile`.

No type is inferred from contour shape. In particular, an all-Line contour does not become Polygon. The line-only regression starts from a typed Engineering Composite, runs the real solve and Spatial lowering path, confirms Spatial Composite identity and per-segment Composite provenance, and then proves that manufacturing accepts it as generic ClosedProfile with body provenance still pointing to feature `main` / `main_segment_0`.

```text
COMPOSITE_MANUFACTURING = SUPPORTED
COMPOSITE_STEP = SUPPORTED
COMPOSITE_USES_GENERAL_PROFILE = YES
COMPOSITE_SOLID_SPEC_VARIANT = NO
COMPOSITE_ARC_TESSELLATION = NO
COMPOSITE_BBOX_SUBSTITUTION = NO
ARC_DIRECTION_RECONSTRUCTION_FROM_ENDPOINTS = NO
MANUFACTURING_REORDERS_COMPOSITE_SEGMENTS = NO
PROFILE_TYPE_INFERRED_FROM_SEGMENTS = NO
LINE_ONLY_COMPOSITE_MANUFACTURING = PASS
SOURCE_PROFILE_IDENTITY_PRESERVED = YES
PYTHON_KNOWS_COMPOSITE = NO
PYTHON_COMPOSITE_HANDLER = NO
HOLE_REMAINS_TYPED_CYLINDER = YES
PATTERN_RECONSTRUCTED_IN_MANUFACTURING = NO
STEP_REEXPORTED_AFTER_VALIDATION = NO
POLYGON_MANUFACTURING = SUPPORTED
SLOT_MANUFACTURING = FAIL_CLOSED
SLOT_STEP = FAIL_CLOSED
MAJOR_ARC_SUPPORT = YES
FULL_CIRCLE_COMPOSITE_ARC = FORBIDDEN
EXACT_180_DEGREE_AMBIGUOUS_ARC = FORBIDDEN
```

The SolidBuildSpec body source map identifies the originating feature and first source entity; detailed segment-by-segment provenance remains in Spatial and is not duplicated in the generic DTO.

## Geometry, arc, and cut contract

- Lines preserve exact endpoints; Arcs preserve endpoints, center, radius, and explicit CW/CCW direction.
- Major directed arcs are supported. Full-circle Composite arcs are forbidden. Exactly-180-degree arcs are rejected as ambiguous.
- Segment order is source order, including indices beyond lexical ID ordering; no manufacturing reorder occurs.
- Translation is preserved; no origin normalization or translation loss.
- Valid concave Composite contours are supported.
- No arc tessellation, arc-to-chord substitution, or bounding-box substitution.
- `Hole` remains a typed cylindrical cut. Resolved hole-pattern instances are expanded upstream and arrive as separate cuts; Manufacturing does not rebuild pattern semantics.
- Composite combined with Slot remains fail-closed. Slot Manufacturing and Slot STEP remain fail-closed independently.
- Inner loops, multiple contours, islands, and arbitrary sketch profiles remain deferred.

## Solid and STEP route

The existing generic CadQuery backend consumes the ClosedProfile DTO, builds native analytic Line/Arc edges in ordered-wire form, makes the planar face, and extrudes along +Z over the resolved body interval. No Composite-specific SolidBuildSpec variant or Python handler was added. The real STEP path exports and reimports one valid solid, then checks bounds, volume, surface area, center of mass, stable edge/face/vertex counts, and deterministic material occupancy probes (25 probes for the closed-profile contour, with typed depth-aware operation probes). Product delivery returns the exact artifact bytes that passed validation; it does not re-export after validation. The Rust CLI and UI endpoint integration verifies delivered bytes against artifact size/hash metadata.

`COMPOSITE_USES_GENERAL_PROFILE = YES`
`COMPOSITE_MANUFACTURING = SUPPORTED`
`COMPOSITE_STEP = SUPPORTED`
`COMPOSITE_ARC_TESSELLATION = NO`
`COMPOSITE_BBOX_SUBSTITUTION = NO`
`ARC_DIRECTION_RECONSTRUCTION_FROM_ENDPOINTS = NO`
`HOLE_REMAINS_TYPED_CYLINDER = YES`
`PATTERN_RECONSTRUCTED_IN_MANUFACTURING = NO`
`STEP_REEXPORTED_AFTER_VALIDATION = NO`
`MAJOR_ARC_SUPPORT = YES`
`FULL_CIRCLE_COMPOSITE_ARC = FORBIDDEN`
`EXACT_180_DEGREE_AMBIGUOUS_ARC = FORBIDDEN`

## Adversarial and regression evidence

- Line-only Engineering Composite identity passes the new production-routing test; it is not reclassified as Polygon.
- Mixed Line/Arc order and provenance corruption, wrong source identity, discontinuity/open contour, wrong Arc direction, and invalid Arc radius fail closed at the relevant Rust boundary.
- Real STEP equivalence rejects Arc-to-chord substitution and wrong Arc direction; bbox substitution and translation loss are rejected.
- STEP validation adversarials reject fabricated reimport/witness mismatches and geometrically different contours that share bbox/volume evidence.
- Translated, asymmetric, concave geometry and valid major Arc pass exact ClosedProfile and STEP round-trip coverage.
- One Hole and multiple upstream-resolved pattern cuts remain typed and pass the real Composite STEP delivery integration.
- Polygon manufacturing/STEP regressions remain passing; Polygon is still identity-gated and line-only.
- Composite+Slot and Slot manufacturing remain fail-closed.

## Public manufacturing matrix

| Base profile | Manufacturing / STEP |
|---|---|
| Rectangle | SUPPORTED |
| Circle | SUPPORTED |
| RoundedRectangle | SUPPORTED |
| Polygon | SUPPORTED |
| Composite | SUPPORTED |
| Slot | FAIL_CLOSED |

Capability facts keep `Slot.manufacturing_spec`, `cadquery_solid`, `step_export`, and `step_reimport_validation` false. Composite has no specific DTO/backend; it uses the generic ClosedProfile bridge.

## Verification run for this closure

All results below were produced during this pass; counts are not carried forward from the prior checkpoint.

| Command / gate | Result |
|---|---|
| `cargo check --offline --locked --workspace --all-targets` | PASS, exit 0 |
| `cargo test --offline --locked --workspace --all-targets` | PASS, 49 suites; 1110 passed, 0 failed, 2 ignored |
| `cargo test --offline --locked -p scc-engineering --all-targets` | PASS, 213 passed, 0 failed, 0 ignored |
| `cargo test --offline --locked -p scc-spatial --all-targets` | PASS, 76 passed, 0 failed, 0 ignored |
| `cargo test --offline --locked -p scc-manufacturing --all-targets` | PASS, 38 passed, 0 failed, 0 ignored |
| `cargo test --offline --locked -p scc-cad-backend --all-targets` | PASS, 5 passed, 0 failed, 0 ignored (integration worker exercised by Docker below) |
| `docker compose run --rm cad-test` | PASS, 55 passed, 0 failed |
| `docker compose run --rm cad-rust-test` | PASS, 5 passed, 0 failed, 0 skipped; includes translated typed Composite with exact arcs and hole pattern through Rust CLI/UI |
| `docker compose config --quiet` | PASS, exit 0 |
| `cargo fmt --all -- --check` | PASS, exit 0 |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | PASS, exit 0 |
| `cargo build --offline --locked --release -p scc-cli` | PASS, exit 0 |
| `SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui` | PASS, 130 total; 125 passed, 0 failed, 5 skipped |
| `git diff --check` | PASS, exit 0 |
| `python3 -m json.tool docs/capabilities.json` | PASS, exit 0 |
| Browser E2E | NOT_PASSING_NON_BLOCKING; not rerun as instructed; the previous attempt timed out around solve completion |

The five UI skips are worker-dependent integration cases; the Docker `cad-rust-test` real worker suite passes all five, including Composite, Polygon, RoundedRectangle, Circle, and validated artifact delivery.

## Final documentation and freeze decision

ADR-060 records the typed identity gate, exact generic profile bridge, geometry restrictions, backend/STEP reuse, and rejected inference/special-backend alternatives. README, CHANGELOG, `tasks/plan.md`, `tasks/todo.md`, `docs/capabilities.json`, ADR index, and `.agent/STATE.md` agree that Phase 13.3C is complete, Composite is supported, Slot remains fail-closed, and Phase 13.4 is next but not started.

`PHASE_13_3C = COMPLETE`
`FREEZE = YES`
`WORKTREE = CLEAN`
`PUSHED = NO`
`NEXT_PHASE = Phase 13.4 — Slot Manufacturing & STEP`

No Phase 13.4 implementation was started. Browser E2E is the only non-blocking note.
