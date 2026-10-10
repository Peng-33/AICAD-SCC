# Phase 13.4 — Slot Manufacturing & STEP Closure Audit

## Decision

**PASS_WITH_NOTES / FREEZE** — all mandatory strict gates pass; Browser E2E is the sole explicitly non-blocking item. The closure documentation is committed locally, the final worktree is clean, and nothing from Phase 13.5 is implemented.

## Git baseline and commit chain

- Repository: `<repo>`
- Branch: `codex/aicad-pre-rollback-recovery`
- Phase baseline: `f82f6e3e6f66cada0d5e92cb0499862076388346` (Phase 13.3C closure)
- Implementation commits, in order:
  - `92d0fed` — `feat(manufacturing): add typed slot cut operations`
  - `240c483` — `feat(cad): subtract exact closed profile slot cuts`
  - `d3b8d17` — `test(step): validate slot manufacturing roundtrip`
  - `ccfa7bc` — `refactor(manufacturing): derive closed profile spec for slot cuts`
  - `5f150c4` — `style(manufacturing): format slot lowering`
  - `8b3bde3` — `test(step): cover slot base and hole combinations`
- Documentation/freeze commit: this closure record and synchronized project status docs; see final report for its commit hash. Final worktree: clean after the closure commit.
- Push: NO.

## Engineering and Spatial contract

- `EngineeringFeature::Slot` remains the source of truth: canonical overall length (end-to-end, not center spacing), width, center, and orientation.
- Engineering resolves one exact ordered stadium contour: Line, directed semicircle Arc, Line, directed semicircle Arc. The semicircles remain analytic arcs, not a polygonal approximation.
- Orientation is canonicalized upstream to `[0°, 180°)`; center translation and world-coordinate geometry are retained.
- Spatial emits a typed `SlotCut` with overall-length/width/center/orientation metadata, exact ordered segment geometry and source entities/indexes, ThroughAll kind, and resolved Z interval.
- Manufacturing does not reconstruct the stadium from semantic parameters. It validates the typed cut and copies its exact segment coordinates, radii, arc directions, order, and Z. Duplicate/invalid provenance or malformed contour fails closed.
- Segment-level provenance is retained and validated in Spatial/lowering; the Manufacturing operation source map links the operation to its Spatial cut. The exact contour itself is the derived transport witness.

## Manufacturing representation and builder

```text
EngineeringFeature::Slot
  → Spatial typed SlotCut
  → exact ordered stadium Line/Arc contour + resolved Z
  → SolidOperation::SlotCut { profile: ClosedProfileSpec { segments }, z, axis }
  → shared generic exact ClosedProfile face/tool builder
  → exact B-Rep Boolean subtraction
```

`SlotCut` remains an operation and is not a base footprint. The `ClosedProfileSpec` is a derived geometry value, not a new editable feature. Manufacturing and Python do not contain a second Slot geometry builder. The CadQuery backend strictly parses the nested exact `profile.segments` transport and reuses `_build_profile_tool`, which consumes analytic Line/Arc segments and the explicit Z interval. No tessellation, bbox substitution, profile repair, or Arc direction inference is used. Hole continues to lower/build as a typed cylindrical cut.

The SolidBuildSpec v1 schema envelope is unchanged. Slot is an additive tagged operation; no breaking serde change or schema bump is required. The golden `crates/scc-manufacturing/tests/goldens/slot_solid_spec.json` records its exact serialized shape and operation-source mapping.

## Fixture and behavior coverage

- Rectangle base + translated/rotated multiple Slots + typed cylindrical Hole: Engineering→Spatial→Manufacturing lowering, exact profile equality, source mapping, resolved `[0,10]` Z, deterministic golden, and 100 repeated serialization checks.
- ClosedProfile-backed Polygon base + Slot and Composite base + Slot: typed Manufacturing lowering keeps the base ClosedProfile and the cut a separate typed SlotCut. The production Rust CLI/UI→CadQuery→STEP integration also adds a translated 30° Slot to the exact Composite base and validates reimport/equivalence.
- Slot + Hole: the production Rectangle fixture contains two independent Slot features and one Hole; the Rust CLI→CadQuery→STEP chain validates the result. The SolidBuildSpec retains two typed SlotCut operations plus one typed CylindricalCut, deterministic order, and provenance.
- Multiple independent Slots: upstream supports and the focused integration test verifies two slots. No Slot pattern contract exists; pattern status is OUT OF SCOPE.
- Arbitrary orientation and translation: production Rust→CLI/UI→CadQuery→STEP includes a translated rotated Slot (30°), a second independent translated horizontal Slot, and a translated 30° Slot on the exact Composite base; exact origin/rotation are not regenerated in Python.
- Z is carried from the resolved Spatial interval; the real SolidBuildSpec and CadQuery consume `[0,10]` for the through fixture, with no Python inference.

## STEP delivery and equivalence

The real CLI/UI integration path lowers the Engineering source to the SlotCut transport, invokes the CadQuery worker, exports STEP, reimports it, and runs the strengthened equivalence checks before delivery. Slot tool void/material probes are sampled at three Z levels. The delivery bytes are hashed/size-checked and returned unchanged after validation. No second export occurs after validation.

The wrong-orientation fixture deliberately has the same bounding box, volume, and surface area as the source but fails the material-witness comparison. Width mutation, Arc-to-chord substitution, omission, and bbox-cut substitution likewise fail their required volume/surface/material/equivalence witnesses.

## Adversarial results

| Mutation | Expected | Result |
|---|---|---|
| Omit Slot operation | Reject by volume/material witnesses | PASS |
| Rotate contour to wrong orientation while preserving bbox/volume/area | Reject by material witnesses | PASS |
| Increase Slot width | Reject by volume/equivalence | PASS |
| Replace semicircular Arcs with chords | Reject by surface/equivalence | PASS |
| Replace Slot with bbox rectangular cut | Reject by volume/material witnesses | PASS |
| Malformed/open/disconnected profile or wrong Z interval | Fail closed before repair/backend acceptance | PASS |

## Regression and freeze gates

Required public manufacturing matrix after closure:

| Feature | Status |
|---|---|
| Rectangle | SUPPORTED |
| Circle | SUPPORTED |
| RoundedRectangle | SUPPORTED |
| Polygon | SUPPORTED |
| Composite | SUPPORTED |
| Slot | SUPPORTED |
| Hole (typed cylindrical cut) | SUPPORTED |

Structural gates A–AJ: PASS. Specifically, A/B/C/D/E/F/G/H/I/J/K/L/M/N/O/P/Q/R/S/T/U/V/W/X/Y/Z/AA/AB/AC/AD/AE/AF/AG/AH/AI/AJ all pass through the production Engineering→Spatial→Manufacturing chain, exact contour/provenance assertions, generic builder reuse, deterministic golden, STEP adversarials, and the full feature regression matrix. Command gates AK/AL/AM/AN/AO/AP/AQ/AR/AS/AT/AU/AV/AW/AX also pass. AY capability/docs synchronization passes; AZ final clean-worktree gate passes after the closure commit.

Exact command outcomes:

| Gate | Command | Result |
|---|---|---|
| Focused Manufacturing | `cargo test --offline --locked -p scc-manufacturing --all-targets` | PASS — 39 passed, 0 failed, 0 ignored |
| Focused Engineering | `cargo test --offline --locked -p scc-engineering --all-targets` | PASS — 213 passed, 0 failed, 0 ignored |
| Focused Spatial | `cargo test --offline --locked -p scc-spatial --all-targets` | PASS — 76 passed, 0 failed, 0 ignored |
| Rust CAD backend | `cargo test --offline --locked -p scc-cad-backend --all-targets` | PASS — 5 passed, 0 failed, 0 ignored |
| Docker CadQuery/STEP | `docker compose run --rm cad-test` (image freshly rebuilt for the final source) | PASS — 58 passed, 0 failed |
| Real Rust→CadQuery→STEP/UI | `docker compose run --rm cad-rust-test` | PASS — 6 passed, 0 failed, 0 skipped; includes rotated Rectangle Slot+Hole and translated Composite ClosedProfile-base+Slot |
| Workspace check | `cargo check --offline --locked --workspace --all-targets` | PASS — exit 0 |
| Workspace tests | `cargo test --offline --locked --workspace --all-targets` | PASS — 49 result summaries; 1111 passed, 0 failed, 2 ignored |
| Formatting | `cargo fmt --all -- --check` | PASS — exit 0 after `cargo fmt --all` |
| Strict Clippy | `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | PASS — exit 0 |
| Release CLI | `cargo build --offline --locked --release -p scc-cli` | PASS — exit 0 |
| UI regression | `SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui` | PASS — 131 total, 125 passed, 0 failed, 6 skipped (worker-only; covered by Docker integration) |
| Compose config | `docker compose config --quiet` | PASS — exit 0 |
| Whitespace | `git diff --check` | PASS — exit 0 on final staged/committed changes |
| Documentation/capability sync | README, CHANGELOG, plan/todo, capabilities, ADR index, state, ADR-061 | PASS |
| Browser E2E | one bounded attempt only | NOT RUN / NON-BLOCKING |
| Final worktree | committed documentation and closure audit | PASS — clean |

Browser E2E is the only non-blocking gate and was not run, per the prompt’s instruction to avoid spending significant time. No mandatory gate remains failed or unrun.

## Required report constants

```text
SLOT_MANUFACTURING = SUPPORTED
SLOT_STEP = SUPPORTED
SLOT_IS_BASE_PROFILE = NO
SLOT_OPERATION_IS_TYPED = YES
SLOT_WORKS_ON_TYPED_BASE_AND_CLOSEDPROFILE_BASE = YES
SLOT_ON_CLOSEDPROFILE_BASE = PASS
RECTANGLE_PLUS_SLOT = PASS
SLOT_PLUS_HOLE = PASS
SLOT_USES_GENERAL_CUT_PROFILE = YES
GENERAL_CUT_PROFILE_IS_DERIVED = YES
MANUFACTURING_RECONSTRUCTS_SLOT_GEOMETRY = NO
PYTHON_RECONSTRUCTS_SLOT = NO
SLOT_TESSELLATION = NO
SLOT_BBOX_SUBSTITUTION = NO
SLOT_ARC_DIRECTION_RECONSTRUCTION = NO
ARBITRARY_SLOT_ORIENTATION = PASS
SLOT_TRANSLATION_PRESERVED = YES
SLOT_Z_INFERRED_BY_PYTHON = NO
DUPLICATE_SLOT_GEOMETRY_BUILDER = NO
SLOT_BOOLEAN_IS_EXACT_BREP = YES
HOLE_REMAINS_TYPED_CYLINDER = YES
PRIMARY_SLOT_VALIDATION_OWNER = ENGINEERING_SPATIAL
MANUFACTURING_DEFENSIVE_VALIDATION = YES
DETERMINISTIC_SLOT_OPERATION = YES
SERDE_BREAK_REQUIRED = NO
STEP_REEXPORTED_AFTER_VALIDATION = NO
MULTIPLE_SLOT_CUTS = PASS
SLOT_PATTERN = OUT_OF_SCOPE
```

## Closure decision

Closure result: `PASS_WITH_NOTES`, `FREEZE`. All code and documentation commits are local; no push was performed. Stop at Phase 13.4. Recommend Phase 13.5 — Manufacturing Domain Closure Audit; it is not implemented or started.
