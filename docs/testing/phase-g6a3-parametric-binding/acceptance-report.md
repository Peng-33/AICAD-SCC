# Phase G6A.3 — Acceptance Report

Execution date: 2026-09-27 (America/Denver)

Starting HEAD: `269fe5e1673abf673a4395a2ee9c9742baa93fd4`

Branch: `codex/aicad-pre-rollback-recovery`

## Result

```text
PHASE_G6A_3 = PASS_WITH_NOTES
```

The required Hole parameter-editing loop now works through the existing Engineering Workbench, formal EngineeringCommand, candidate OCP rebuild, associative DrawingModel, SVG/DXF, and canonical STEP validation. No geometry Feature, Project schema version, UI, or STEP authority was added. The bounded Counterbore slots are implemented but were not exercised by the real-browser fixture. General G6A.1/G6A.2 spacing, Fillet radius, and generic blind-cut depth gaps remain deferred.

## Status matrix

| Requirement | Status | Evidence and boundary |
|---|---|---|
| `PARAMETER_BINDING_ROOT_CAUSE` | `PASS` | v2 Parametric features had no editable `FeatureParameterSlot` capabilities; lowering and Hole dimensions read literals, and the Workbench had no v2 Hole inputs. The G6A.2 `SCC-TOOL-010` browser failure matched this path. |
| `GENERAL_TYPED_PARAMETER_REFERENCE` | `PASS` | Reference includes part, feature, slot, parameter identity, and expected type; deterministic identity derives from model IDs and slot; stale IDs, collisions, unsupported slots, and wrong types fail closed. |
| `PARAMETER_CAPABILITY_SCHEMA` | `PASS_WITH_NOTES` | Only v2 Hole diameter, applicable Blind depth, and supported Counterbore dimensions are editable. No arbitrary JSON paths or general edits to other feature kinds. |
| `HOLE_DIAMETER_BINDING` | `PASS` | Browser changed Ø10→Ø12 using `set_feature_parameter`; Rust model and real OCP volume changed from 215528.7611 to 215321.4160 mm³. |
| `HOLE_DEPTH_BINDING` | `PASS` | Blind depth changed 6→8 mm; OCP volume changed to 215095.2213 mm³; through-all Holes do not expose a blind-depth slot. |
| `COUNTERBORE_PARAMETER_BINDING` | `PASS_WITH_NOTES` | Capability schema, model validation, OCP lowerer, Workbench view, and associative value resolver include Counterbore diameter/depth. The browser fixture intentionally covers the required simple Blind Hole; Counterbore value edits were not browser-tested. |
| `ATOMIC_MODEL_UPDATE` | `PASS` | `SetFeatureParameter` resolves and validates a candidate model before commit; existing bound values retain stable parameter identity. Parameter updates through legacy SetParameter/SetExpression are also checked against Hole bindings. |
| `INVALID_UPDATE_ROLLBACK` | `PASS` | `-1` is refused at the tool input schema with `SCC-TOOL-005`; Chinese guidance preserves the raw failure. A body-removing cut failed with nested `CUT_REMOVES_BODY` (outer `SCC-SPAT-017` envelope); no candidate model returned, and the prior committed values/drawing remained visible. Subsequent valid editing succeeded. |
| `DETERMINISTIC_REBUILD` | `PASS` | Rust/OCP regression fixtures, deterministic STEP equivalence tests, and repeatable parameter edits passed; all geometry comes from the shared ordered feature history. |
| `DRAWING_ASSOCIATION_UPDATE` | `PASS` | The DrawingModel callout changed Ø10↧6→Ø12↧8 from bound EngineeringModel values; z=13 section loops changed 1→2 after the blind-depth edit. |
| `SVG_DXF_UPDATE` | `PASS` | Browser DrawingModel/SVG/DXF and downloaded SVG/DXF contain Ø12 and ↧8. Download sizes: SVG 8627 bytes; DXF 9502 bytes. |
| `FORMAL_STEP_UPDATE` | `PASS` | Browser downloaded STEP only after manufacturing, build, reimport, and equivalence passed. The downloaded 19338 bytes exactly matched the HTTP validated body and SHA-256 `9ebc10fca7e3b215fc588ca0137c14b32665ad839cb7f7247b75fcc73032902b`. |
| `GENERIC_PARAMETER_EDIT_FIXTURE` | `PASS` | Existing generic 120×90×20 mm base with one v2 blind Hole; no part-specific production builder or new geometry Feature. |
| `WORKBENCH_PARAMETER_EDIT` | `PASS` | Real selected Feature inspector exposes unit-bearing `直径` and `盲孔深度` inputs and submits formal typed tool calls. Failure restores the previous value and retains the prior drawing. |
| `BROWSER_ACCEPTANCE` | `PASS` | Real Chromium with isolated fake Provider covered `/`→`/engineering.html`, multiline Chinese intent, v2 plate/Hole creation, typed edits, three views, Section A-A, SVG/DXF downloads, validated STEP download, invalid-input rollback, OCP body-removal rollback, and recovery. No console/page or unexpected HTTP errors. Screenshot: [G6A.3 browser acceptance](g6a3-browser-acceptance.png). |
| `G6A1_G6A2_REGRESSION` | `PASS_WITH_NOTES` | Existing associative-dimension and Section A-A tests passed; browser verified source-linked Hole callout updates. Passing regression does not complete the already deferred general spacing, Fillet radius, or generic cut-depth dimensions. |
| `PROJECT_V1_REGRESSION` | `PASS` | Workspace Rust/Node project persistence and legacy source-only schema/API regression passed; no Project v1 format change. |
| `PROJECT_V2_REGRESSION` | `PASS` | Current Rust→OCP suite passed canonical Project v2 drawing/STEP, X2–X10, and M1–M16. Project v2 schema version and STEP authority are unchanged. |
| `FORMAL_STEP_REGRESSION` | `PASS` | Project v2 STEP reimport/equivalence and exact validated-byte delivery passed; X2–X10 returned no artifacts and each legal follow-up succeeded. |
| `REAL_DEEPSEEK_TEST` | `NOT_RUN` | Deterministic fake Provider only. No real API key was read, printed, or used. |

## Implementation

### Root cause and typed capability

The previous v2 Hole path had no schema-backed parameter slots. `FeatureParameterReference` now identifies `part_id`, `feature_id`, `slot`, `parameter_id`, and `expected_type=length_mm`. Deterministic IDs derive from part/feature identity and the slot; collisions and stale references are rejected. The capability list depends on actual Hole termination and kind: a through-all Hole has no `blind_depth`, while a Counterbore exposes only its existing diameter/depth semantics.

`EngineeringCommand::SetFeatureParameter` is the single formal edit route. First edit creates a bound parameter; subsequent edits retain the same parameter ID. The Rust executor applies edits on a clone and validates before commit. The server returns no changed model when the candidate solve/OCP rebuild fails. No separate parameter database or UI geometry state was introduced.

### Real browser evidence

The browser ran on a private internal-only Docker network with no published ports, read-only root filesystems, and a deterministic fake Provider. The final run reported:

```text
hole_diameter=10->12
blind_depth=6->8
volume_mm3=215528.761101962->215321.415986825->215095.221315766
section_z13_loops=1->2
SVG_BYTES=8627
DXF_BYTES=9502
STEP_BYTES=19338
STEP_SHA256=9ebc10fca7e3b215fc588ca0137c14b32665ad839cb7f7247b75fcc73032902b
invalid_rollback=PASS
OCP_rebuild_rollback=PASS
geometry_failure_code=CUT_REMOVES_BODY
geometry_failure_envelope=SCC-SPAT-017
REAL_PROVIDER=NOT_RUN
```

`SCC-TOOL-005` is the actual negative-value code because the registered tool schema rejects a non-positive scalar before EngineeringCommand execution. `SCC-ENG-028` remains the typed EngineeringModel binding-validation code for command-level validation. The OCP body-removal error is carried as nested `CUT_REMOVES_BODY` under the existing spatial error envelope; its diagnostic stage is not misreported as successful model generation.

### Limits

- No general Feature parameter editor was added. Extrude extent, Fillet radius, Chamfer distance, Revolve angle, Pattern spacing/count, and Sketch dimensions remain unsupported for this phase.
- Counterbore value editing is in the schema and shared consumer path but lacks browser/OCP edit acceptance beyond existing unedited Counterbore fixtures.
- G6A.1/G6A.2 gaps remain: arbitrary feature spacing, generic cut depth, and Fillet radius annotation are not completed by parameter binding.
- Existing AICAD-COMPLEX-001 full-part support remains blocked by its unresolved semantic C1 edge set; no dimensions were guessed and no complete-part artifact is claimed.
- The independent four-slot STEP timeout issue remains outside this phase.
- Real DeepSeek remains `NOT_RUN`.

## Regression runs

| Command or suite | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace --all-targets` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `npm test --prefix apps/scc-ui` | PASS: 182 total, 161 passed, 21 host-gated OCP skips, 0 failed |
| CadQuery/OCP Python `unittest` | PASS: 106 tests |
| Rust→CadQuery/OCP and canonical STEP integration | PASS: 20/20 tests |
| G5 X2–X10 formal HTTP negatives | PASS: 9/9; legal follow-ups 9/9; no artifacts delivered |
| Project v2 M1–M16 manufacturing matrix | PASS: 16/16 |
| G5.1 independent DrawingModel/STEP observations and G6A Section A-A | PASS |
| Playwright real Chromium fake-provider flow | PASS; screenshot retained above |
| `docker compose config --quiet` | PASS |
| `git diff --check` | PASS |

The isolated OCP integration included real CadQuery B-Rep generation, semantic edge treatments, Revolve, formal Project v2 STEP build/reimport/equivalence, G5 negative cases, M1–M16, G5.1 independent Drawing/STEP evidence, and G6A section updates. `npm test` skips its host-gated OCP cases; those were executed separately in the isolated runtime (20/20 passed).

## Git and safety

Starting tracked tree was clean. The pre-existing untracked `docs/testing/mmf204-real-user-acceptance/` was not modified or staged. Only audited G6A.3 source, test, skill, state, ADR, report, and screenshot files will be committed. No push is performed. Final commit and worktree state are recorded after local commit below.

```text
FINAL_GIT_HEAD = recorded in the final delivery message
WORKTREE_STATUS = all phase files committed; protected pre-existing untracked acceptance evidence remains
PUSHED = NO
```
