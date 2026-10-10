# Phase G6A.1 — Associative Engineering Dimensions Acceptance Report

Date: 2026-09-26
Repository: `<repo>`
Branch: `codex/aicad-pre-rollback-recovery`
Starting HEAD: `6794e658e059914db7805bbdf2c1f27323ba2a83`

## Result

```text
PHASE_G6A_1 = PARTIAL
ASSOCIATIVE_DIMENSION_MODEL = PASS
FEATURE_REFERENCE_RESOLUTION = PASS
LINEAR_DIMENSION = PASS_WITH_NOTES
DIAMETER_DIMENSION = PASS_WITH_NOTES
RADIUS_DIMENSION = PARTIAL
DEPTH_DIMENSION = PARTIAL
HOLE_COUNTERBORE_CALLOUT = PASS_WITH_NOTES
ANNOTATION_REBUILD = PASS
SUPPRESSED_FEATURE_HANDLING = PASS
INVALID_REFERENCE_FAIL_CLOSED = PASS
DRAWING_MODEL_INTEGRATION = PASS
SVG_ANNOTATION_EXPORT = PASS
DXF_VISUAL_ANNOTATION = PASS
DXF_NATIVE_DIMENSION = DEFERRED
D1_D13_MATRIX = PARTIAL
GENERIC_ANNOTATION_FIXTURE = PASS_WITH_NOTES
COMPLEX_PART_ANNOTATION_SUBSET = PASS_WITH_NOTES
FULL_COMPLEX_PART_DRAWING = BLOCKED
SECTION_VIEW_REGRESSION = PASS_WITH_NOTES
PROJECT_V1_REGRESSION = PASS
G1_G6A_REGRESSION = PASS_WITH_NOTES
FORMAL_STEP_REGRESSION = PASS
BROWSER_ACCEPTANCE = NOT_RUN
REAL_DEEPSEEK_TEST = NOT_RUN
PUSHED = NO
```

The phase adds formal associative records to the existing derived `DrawingModel`. It does not create another editable geometry model: `EngineeringModel v2` remains the only editable geometry truth, and the dimensions are regenerated after a validated rebuild. The overall phase stays PARTIAL because a fillet radius cannot yet be tied to a unique projected/section arc, generic pocket/slot depth dimensions are absent, and native DXF `DIMENSION` entities are not emitted.

## Implemented contract

Each record has a stable `annotation_id`, dimension kind, semantic body/feature reference, view, nominal value and unit, source parameters, provenance, observability, status, diagnostic, and an optional geometry witness. A witness binds to the current rebuild digest and canonical exact face-evidence IDs, with separate geometry and drawing measurements and a stated tolerance. No OCP Face/Edge ordinal, SVG path, screen pixel, or nearest-edge rebinding is used.

Supported records are assembled from EngineeringModel v2 parameters and the validated parametric B-Rep/orthographic evidence. Overall body length and width are compared to their selected view spans; base thickness requires a source-attributed planar side face and a matching Top-view line. Hole/counterbore callouts require matching cylindrical B-Rep evidence and a projected Front-view circle/arc. A blind depth is labeled from its validated EngineeringModel feature parameter and kept distinct from the measured diameter. Each rebuild creates fresh associations; suppressing a feature removes its derived callout.

## D1–D13 matrix

| Case | Result | Executed evidence |
|---|---|---|
| D1 Overall length | PASS | G2 OCP fixture: 140 mm nominal, current B-Rep envelope witness, linked DrawingModel dimension and layout. |
| D2 Overall width | PASS | G2 OCP fixture: 100 mm nominal, current B-Rep envelope witness, linked DrawingModel dimension and layout. |
| D3 Base thickness | PASS | G2 OCP fixture: 20 mm from the active base Extrude, exact source-attributed planar face, and matching Top-view line. |
| D4 Through-hole diameter | PASS | Actual G2 DrawingModel callout includes `Ø20 THRU`; exact cylindrical face and Front circle evidence are validated. |
| D5 Blind-hole depth | PASS | Actual G2 DrawingModel callout includes `Ø8 ↧5`; diameter is checked against cylinder evidence and depth remains declared-feature provenance. |
| D6 Counterbore diameter/depth | PASS | Actual G2 callout includes the Ø8 blind bore and `Ø14 × 5` counterbore; source parameters and cylindrical witnesses are checked. |
| D7 Fillet radius | PARTIAL | G4/Revolve fixture and complex subset retain an explicit `unsupported` association with no DrawingDimension. The validated radius lacks a unique feature-linked arc in the current section/projection evidence. No radius is claimed on SVG/DXF. |
| D8 Update after edit | PASS | OCP-backed edit changes the blind hole from Ø8/depth 5 to Ø7/depth 6; a fresh DrawingModel reports nominal 7 and formats `Ø7 ↧6`. |
| D9 Suppressed feature | PASS | Suppressing the blind-hole source removes its callout. A suppressed active pattern seed fails model validation rather than rebinding. |
| D10 Invalid FeatureReference | PASS | `unresolved_feature_parameter_reference_fails_closed` returns `SCC-DRW-009` for a missing feature parameter source. |
| D11 Inapplicable view | PASS | `body_length_dimension_on_inapplicable_view_fails_closed` rejects an X envelope dimension placed in Left; the observation validator reports an inapplicable view (`SCC-DRW-014`). |
| D12 SVG | PASS | Actual G2 SVG is checked for Front/Top/Left view groups and dimension text; the test also checks linked DrawingModel values before export. |
| D13 DXF | PASS_WITH_NOTES | Actual DXF contains the visual dimension layer/callout text (including `THRU`) and has no group-code `0 / DIMENSION` entity. This proves visual markup only; native entity status is DEFERRED. |

D1–D6 numeric checks use the actual emitted DrawingModel report and dimension list, not only the existence of an association record. OCP geometry witnesses are independently recomputed from the validated rebuild. D12/D13 inspect renderer output bytes/text separately.

## Capability boundaries

- **Linear:** overall X/Y envelope and base Extrude thickness are covered. Arbitrary feature-to-feature distances, boss-location dimensions, and general feature extent dimensions are not implemented.
- **Diameter:** supported for validated cylindrical Hole/Counterbore callouts whose axis produces a unique Front projected circle/arc. Cylindrical additive-boss diameter dimensions are not implemented. Side-axis holes without that witness remain `unsupported`.
- **Radius:** the existing DrawingModel renderer can display legacy radius dimensions, but the new parametric association does not emit a fillet radius yet. Exact analytic surface data alone is insufficient without a unique feature-linked drawing/section witness.
- **Depth:** blind Hole depth is embedded in the typed Hole callout. General depth dimensions for pockets, slots, and Revolve/CUT features remain unsupported; ordinary outlines do not prove those internal values.
- **Hole/pattern callouts:** supported Z-axis Hole/Counterbore and tested linear/circular pattern counts and spacing/PCD. Unsupported axis/view cases are retained as diagnostics and do not get a fabricated rendered dimension.
- **DXF:** visual lines, arrows, and text are exported. They are not native CAD `DIMENSION` entities; unit/style/definition-point support for native dimensions remains deferred.

## Generic annotation fixture

`GENERIC_ANNOTATION_FIXTURE=PASS_WITH_NOTES`.

The real OCP G2 fixture covers a rectangular base, through hole, blind hole, counterbores, hole patterns, and slots; D1–D6 are asserted from its DrawingModel, edits update a blind callout, and suppression removes it. A separate G3 fixture covers a circular boss, through hole, pocket, Fillet, and Chamfer with Front/Top/Left. Section A-A is exercised on a separate base/through-hole/blind-hole fixture. These feature sets were not combined into the single suggested all-features-plus-section fixture, so coverage is not reported as an unqualified one-fixture PASS.

## AICAD-COMPLEX-001

`COMPLEX_PART_ANNOTATION_SUBSET=PASS_WITH_NOTES`; `FULL_COMPLEX_PART_DRAWING=BLOCKED`.

The existing G5.1 supported-subset EngineeringModel v2 fixture was rebuilt through OCP. This phase explicitly verified associations for overall length 180 mm, width 120 mm, base thickness 24 mm, the left center-hole callout, and the left PCD pattern callout. The test also confirms that both boss Fillet radii and the side blind counterbore are not silently represented as resolved callouts. Other feature dimensions and internal depths are not counted as covered by this acceptance.

Requirement C1 still says to chamfer the “remaining designated base outline sharp edges” without defining the edge set. No edge selection or geometry was guessed. The existing supported-subset STEP evidence is not a complete-part deliverable; no full-part STEP/drawing claim is made. See the [G5.1 acceptance report](../phase-g5.1-manufacturing-closure/acceptance-report.md).

## Section View regression

The actual Rust→OCP Section A-A regression passed: 2 closed loops, 12 section edges, 0.01 mm chord tolerance, repeatable export, and changed blind-hole depth changes the section. Blind-slot, counterbore, and revolved-body section cases remain `NOT_RUN` as in G6A; dimension support does not change that status. See the [G6A report](../phase-g6a-drawing/acceptance-report.md).

## Regression and execution evidence

| Verification | Result | Evidence |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | Workspace formatting. |
| `cargo check --workspace --all-targets` | PASS | Full Rust workspace. |
| `cargo test --workspace --all-targets` | PASS | Full Rust workspace: 1,147 passed, 0 failed, 2 ignored. |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | Strict workspace Clippy. |
| `npm test --prefix apps/scc-ui` | PASS | 178 tests: 157 passed, 21 host-gated OCP integration skips, 0 failed. |
| CadQuery/OCP | PASS | 106 tests in isolated `aicad-scc-ui:g1b-test` container. |
| Rust→CadQuery/OCP, Intent, drawing, Project v1/v2 STEP | PASS | Isolated integration suite: 34/34 passed, 0 skipped. After adding D1–D6 numeric assertions, targeted G2 integration rerun: 1/1 passed. |
| Project v2 M1–M16 STEP matrix | PASS | 16/16; reimport/equivalence exercised. |
| Formal HTTP X2–X10 negative matrix | PASS | 9/9 rejected without artifacts; legal follow-up requests passed 9/9. |
| G5.1 complex subset STEP and drawing regression | PASS_WITH_NOTES | Supported subset passed; full complex part remains blocked by C1. |
| Project v1 STEP regression | PASS | Existing canonical CLI/API delivery and reimport integration cases passed. |
| `docker compose config --quiet` | PASS | Compose configuration parses. |
| `git diff --check` | PASS | Whitespace/error check; rerun before commit. |
| Real browser with fake Provider | NOT_RUN | Browser execution runtime was not available for this task. API/OCP tests are not counted as browser acceptance. |
| Real DeepSeek | NOT_RUN | No live Provider request was made; no API key was read or changed. |

OCP containers were isolated: `network=none`, read-only root filesystem, temporary `/tmp`, no host ports, and source mounts read-only. The production image/tag was not rebuilt or changed.

## Skills updated

Four existing built-in CAD Skills were extended in place: Drawing Annotation, Orthographic Drawing, CAD Export Validation, and Engineering Diagnostics. The two other built-in CAD Skills were unchanged; no duplicate Skill or geometry Feature was added.

## Remaining limitations

1. Parametric Fillet radius association is unsupported until the drawing pipeline can link a unique projected/section arc to the source feature.
2. General linear distances, additive cylindrical feature diameters, and generic pocket/slot/revolve depth dimensions remain unsupported.
3. Native DXF `DIMENSION` entities are deferred; only visual DXF annotations are validated.
4. The proposed single composite generic fixture was not built; the existing OCP fixtures cover the required feature groups separately.
5. Full AICAD-COMPLEX-001 remains blocked pending an explicit C1 edge set and additional drawing callout coverage.
6. Browser acceptance was not run. Real DeepSeek remains not run.
7. No work on G6B was started.
