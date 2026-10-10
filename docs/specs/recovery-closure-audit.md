# AICAD Recovery Closure Audit

## Scope

This audit closes the pre-rollback recovery work on branch
`codex/aicad-pre-rollback-recovery`. It verifies the recovered repository,
records the actual architecture and capability boundaries, and freezes the
recovered baseline for future work.

This is a recovery and verification document. It does not claim Phase 12.4
completion and it does not implement Phase 12.4 or Phase 13.

## Recovery Closure Status

`PASS_WITH_NOTES`

The recovered repository is buildable, testable, formatted, lint-clean, and
audited for recovery junk and ownership boundaries. The notes are deliberate:
browser E2E was not executed in this recovery audit, and polygon/general
profile support is partial rather than a completed Phase 12.4 slice.

## Recovery Freeze Decision

`FREEZE`

The pre-rollback recovery baseline is safe to freeze. Future feature work must
start from this baseline and must not reinterpret the recovery commits as a
claim that polygon manufacturing, polygon DSL/tool entry points, or the full
Phase 12.4 closure exists.

## Git State

- Repository: `<repo>`
- Branch: `codex/aicad-pre-rollback-recovery`
- Recovery base tag: `recovery-base-12e7d0a`
- Recovery base commit: `12e7d0a375bfa559751b160b2874747db0d1ed68`
- `git merge-base recovery-base-12e7d0a HEAD`: `12e7d0a375bfa559751b160b2874747db0d1ed68`
- Recovery worktree: clean before this audit and verified clean after the closure commit
- Remote push: not performed

The preserved pre-recovery backup was not modified:
`<home>/aicad-pre-rollback-j2Ykeo`.

Backup evidence hashes:

| Artifact | SHA-256 |
| --- | --- |
| `head.txt` | `3fb1ade1d8ff9cd82390a66d30467e8b85a57161c74a50a7ebc844d737ee4042` |
| `unstaged.diff` | `4003a222d72042a7af40a211ec0b95c95a8810ec1df9b1889e1290b75ab3a439` |
| `untracked-files.tar.gz` | `a70b16439a5a2b774f2d57430ca7e03c922c80a4a8d2d898ab556ae054485778` |

## Recovery Commit Chain

The recovery history is linear, append-only, and based on the recovery tag.
No rebase, amend, or history rewrite was used.

1. `55a2212` — `recovery: restore engineering model foundation`
2. `3a9b95d` — `recovery: restore engineering DSL and tool layer`
3. `f2ca9e4` — `recovery: restore spatial and principal projection layers`
4. `e11d07f` — `recovery: restore visibility and projection consistency validation`
5. `4ed5d04` — `recovery: restore drawing and three-view annotation engine`
6. `67d8276` — `recovery: restore backend-neutral solid build specification`
7. `26cbcc6` — `recovery: restore CadQuery backend and validated STEP delivery`
8. `f62a65a` — `recovery: restore project persistence and intent preview workflow`
9. `20995a8` — `recovery: restore engineering workbench and drawing UI integration`
10. `f34bb54` — `recovery: record profile closure evidence and deferred polygon plan`

## Architecture Ownership

`EngineeringModel` is the source-of-truth owner. Its command layer,
dependency evaluator, bindings, and solved snapshot own engineering intent and
source provenance. Spatial, projection, visibility, consistency, drawing,
manufacturing, CAD, STEP, project persistence, and UI caches consume explicit
derived models or validated contracts.

The recovered architecture preserves these boundaries:

- `scc-engineering`: source model, commands, evaluation, bindings, snapshots,
  and source maps.
- `scc-spatial`: derived spatial semantics; it does not mutate the source
  engineering model.
- `scc-projection`, `scc-visibility`, and
  `scc-projection-consistency`: view geometry, hidden-line classification, and
  validation.
- `scc-drawing`: one `DrawingModel` consumed by SVG and DXF, with one
  three-view annotation-layout authority.
- `scc-manufacturing`: backend-neutral `SolidBuildSpec`; unsupported shapes
  fail closed instead of being approximated.
- `scc-cad-backend` and the CadQuery worker: explicit solid/STEP delivery and
  artifact validation.
- `scc-project`: source-only persistence; derived geometry and STEP artifacts
  are not serialized as source truth.
- UI intent/preview code: non-mutating previews followed by fingerprint-checked
  formal command application.

## Restored Workspace Crates

The actual offline Cargo workspace contains:

`scc-analysis`, `scc-cad-backend`, `scc-cli`, `scc-core`, `scc-drawing`,
`scc-dsl`, `scc-engineering`, `scc-execution`, `scc-geometry`, `scc-lowering`,
`scc-manufacturing`, `scc-project`, `scc-projection`,
`scc-projection-consistency`, `scc-proposal`, `scc-spatial`, `scc-tools`, and
`scc-visibility`.

The dependency direction inspected during the audit is consistent with the
ownership model: engineering feeds spatial; spatial feeds projection and
visibility; drawing consumes the derived/view layers; manufacturing consumes
engineering and spatial semantics; UI and CLI sit at the orchestration edge.

## Profile Capability Matrix

The matrix uses `SUPPORTED`, `FAIL_CLOSED`, `PARTIAL`, and `NOT_SUPPORTED`.
“DSL” and “Tool” refer to formal entry points, not natural-language intent
coverage.

| Profile | Engineering | Spatial | Projection | Visibility | Drawing | Project | Manufacturing | DSL | Tool | Tested |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Rectangle | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED |
| Circle | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | FAIL_CLOSED | SUPPORTED | SUPPORTED | SUPPORTED |
| RoundedRectangle | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | FAIL_CLOSED | SUPPORTED | SUPPORTED | SUPPORTED |
| Composite | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | FAIL_CLOSED | NOT_SUPPORTED | NOT_SUPPORTED | SUPPORTED |
| Polygon | SUPPORTED | PARTIAL | PARTIAL | PARTIAL | PARTIAL | PARTIAL | FAIL_CLOSED | FAIL_CLOSED | NOT_SUPPORTED | PARTIAL |
| Hole | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED |
| Slot | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | SUPPORTED | FAIL_CLOSED | SUPPORTED | SUPPORTED | SUPPORTED |

Polygon is not absent: typed profile canonicalization and generic segment-based
spatial paths exist. It is still partial because the public DSL/tool boundary,
dedicated project and drawing closure evidence, and manufacturing/STEP path are
not complete. This distinction is the principal Phase 12.4 recovery finding.

## Circular Status

`SUPPORTED` through engineering, solve, spatial, principal projection,
visibility, drawing, project persistence, formal DSL/tool entry points, and
focused tests. Circular base manufacturing and STEP remain explicitly
fail-closed; the supported solid backend is rectangle-based with cylindrical
cuts, not a general circular-base solid backend.

## Rounded Status

`SUPPORTED` through the engineering, spatial, projection, visibility, drawing,
and project layers, with formal DSL/tool construction and phase-specific SVG/
DXF goldens. Manufacturing and STEP are fail-closed by design.

## Composite Status

`SUPPORTED` through the typed engineering and derived drawing pipeline, with
phase-specific goldens and integration coverage. There is no public generic
composite DSL statement or tool entry point, and manufacturing/STEP are
fail-closed. This is a supported derived-profile capability, not a supported
solid-delivery capability.

## Slot Status

`SUPPORTED` through engineering, spatial, projection, visibility, drawing,
project persistence, formal DSL/tool entry points, and Phase 12.3 horizontal /
rotated goldens. Slot manufacturing and STEP are fail-closed. Natural-language
slot intent is still deferred in the capability manifest; this does not negate
the formal DSL/tool support.

## Polygon Status

`PARTIAL`; `PHASE_12_4_COMPLETE=NO`.

The typed `BasePolygon` profile and canonicalization path are present, and the
spatial layer can lower polygon segments through its generic composite path.
However, `scc-dsl` explicitly returns `UnsupportedDslFeature` for
`BasePolygon`, no polygon tool is registered, manufacturing rejects composite
and polygon bases without envelope approximation, and there is no dedicated
full Phase 12.4 closure matrix or polygon STEP witness. No claim of polygon
solid delivery is allowed by this audit.

## Drawing

`SUPPORTED` for the recovered circular, rounded, composite, slot, hole, and
three-view paths covered by the existing tests and goldens. The drawing model
is the shared source for SVG and DXF, hidden-line strokes are represented as a
separate presentation channel, and three-view annotation layout has one
optimization authority. Polygon drawing is recorded as `PARTIAL` because the
generic segment path exists but a dedicated Phase 12.4 closure proof is absent.

## Project Persistence

`SUPPORTED` for the recovered source model and the completed profile families.
Persistence tests assert that `GeometryModel`, `SpatialModel`,
`SolidBuildSpec`, and other derived artifacts are not serialized as source
truth. Circle, rounded, composite, slot, tolerance, and blind-extent round
trips are covered. Polygon has generic typed serialization support but no
dedicated Phase 12.4 project closure test, so it remains `PARTIAL`.

## Intent / Preview

`SUPPORTED` for the recovered workflow. Intent creates a non-mutating preview;
confirmation requires the current model fingerprint to match the proposal base,
applies only the formal command plan, verifies the candidate fingerprint, and
then advances the revision. Derived-cache entries are limited to principal and
three-view drawings and are process-local; source models and STEP artifacts are
not treated as cache truth.

## Manufacturing

`SUPPORTED` for the recovered rectangle body plus cylindrical-hole cut path.
Circle, rounded rectangle, composite, polygon, and slot manufacturing are
explicitly `FAIL_CLOSED` with typed unsupported-feature errors. No envelope or
bounding-box approximation is silently emitted as a solid.

## STEP

`SUPPORTED` for the validated rectangle/cylindrical-cut delivery path. The
CadQuery worker builds from `SolidBuildSpec`, exports a temporary STEP artifact,
reimports it, validates witness data, size, extension, and SHA-256, and only
then exposes the artifact. Rust and UI layers repeat the artifact contract
checks. `STEP_REEXPORTED_AFTER_VALIDATION=NO` for this audit: no new STEP was
re-exported because the audit made no model or backend changes.

## Documentation Reality

The audit inspected `README.md`, `CHANGELOG.md`, `docs/capabilities.json`,
`docs/SCC_UI_Implementation_Matrix.md`, the relevant phase closure specs, the
ADR index, and the task planning files.

No stale “Phase 12.4 complete” claim was found in the top-level product
documentation or capability manifest. The remaining broad planning language in
`tasks/plan.md` is treated as future work, not proof of implementation.
`apps/scc-ui/e2e/browser.mjs` contains a polygon-named example fixture; that is
test data, not a capability declaration.

## ADR Status

- ADR-051: accepted for Phase 12.1; consistent with circular-base closure.
- ADR-052: accepted for Phase 12.2; consistent with rounded/composite closure.
- ADR-053: accepted for Phase 12.3; consistent with slot semantics.
- ADR-054: `IMPLEMENTED_PARTIAL` / planning boundary for Phase 12.4. Its typed
  polygon and derived-profile design is present, while its DSL/tool and
  manufacturing/STEP boundaries remain deferred. ADR-054 is not treated as a
  Phase 12.4 completion certificate.

## Recovery Junk Audit

Passed. No tracked or working-tree matches were found for:

- `.~lock*`, `CONTEXT.md`, `.agent/STATE.md`
- empty `derived` or `solved` placeholder files
- editor backups (`*~`, `*.swp`), temporary files (`*.tmp`)
- files larger than 20 MiB outside build/cache directories
- tracked blobs larger than 20 MiB

The historical recovery intentionally skipped the non-formal control/editor
artifacts `.agent/STATE.md`, `CONTEXT.md`, `.~lock.README.md#`, and zero-byte
`derived`/`solved` placeholders. No tracked path collisions were found.

## Goldens

Present and exercised by the recovered drawing tests:

- Phase 9: `phase9_three_view_plate.svg/.dxf`
- Phase 11: `tolerance_plate.svg/.dxf`
- Phase 12.1: `phase121_circular_disk.svg/.dxf`,
  `phase121_circular_flange.svg/.dxf`
- Phase 12.2: `phase122_rounded_rectangle.svg/.dxf`,
  `phase122_composite_profile.svg/.dxf`
- Phase 12.3: `phase123_slot_horizontal.svg/.dxf`,
  `phase123_slot_rotated.svg/.dxf`

No polygon Phase 12.4 golden or validated polygon STEP artifact is claimed.

## Tests Actually Executed

The following closure gates passed at the recovered `f34bb54` source state:

- `cargo metadata --offline --no-deps --format-version 1`
- `cargo check --offline --locked --workspace --all-targets`
- `cargo test --offline --locked --workspace --all-targets`: 47 suites,
  1077 passed, 0 failed, 2 ignored
- `cargo fmt --all -- --check`
- `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`
- `cargo build --offline --locked --release -p scc-cli`
- `npm test --prefix apps/scc-ui`: 126 tests, 125 passed, 0 failed, 1 skipped
- `docker compose config --quiet`
- `docker compose run --rm cad-test`: 31/31 Python tests passed
- `docker compose run --rm cad-rust-test`: 1/1 Node integration test passed

The first UI test invocation only exposed a missing local `three` dependency;
`npm ci --ignore-scripts` restored the locked dependency and the required test
was rerun successfully. No source change was made for that dependency setup.

## Browser E2E

`NOT_EXECUTED`.

Browser E2E was not required to establish the recovery freeze because the
crate, UI, service, container, and artifact-contract gates passed. It remains
non-blocking recovery debt and must not be represented as a passing browser
result.

## Recovery Closure Commit

This document is committed as:

`docs(recovery): close pre-rollback recovery audit`

The final commit hash is authoritative in `git log -1` and in the completion
report. No other source or generated artifact is part of the closure commit.

## Final Worktree

Required final state: clean. The audit verifies no staged, unstaged, or
untracked files remain after the closure commit.

## PUSHED

`NO`

## BLOCKS_RECOVERY_FREEZE

`NONE`

## NON_BLOCKING_RECOVERY_DEBT

- Browser E2E has not been executed in this recovery closure.
- Polygon/general closed-profile Phase 12.4 closure remains partial.
- Polygon DSL/tool entry points remain deferred.
- Polygon, rounded, composite, circular-base, and slot solid/STEP delivery
  remains fail-closed until a future backend extension is implemented and
  validated.
- Natural-language slot and fillet intent coverage remains deferred.

## FUTURE_DEVELOPMENT

Future work may proceed from the frozen recovery baseline. The next feature
slice should add a dedicated Phase 12.4 polygon contract and tests across DSL,
tool, project, projection/visibility/drawing evidence, manufacturing, and STEP
validation before changing the capability manifest or ADR status. That work
belongs to a new feature commit/phase and is outside this recovery closure.
