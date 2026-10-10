# Phase G6A.2 — Acceptance Report

Execution dates: 2026-09-26–27 (America/Denver)

Reference checkout: `ceaead9ef2d253b63d8bafdfced873640592dbd0`

Branch: `codex/aicad-pre-rollback-recovery`

## Result

```text
PHASE_G6A_2 = PARTIAL
```

This phase adds bounded source-validated circular-boss diameter annotations and native DXF `DIMENSION` output. The wider requirement is not complete: operational references and measurements are not generalized across all requested source types; Fillet radius, arbitrary feature spacing, and generic pocket/slot depth are not all formally dimensioned. The UI browser test also found that a Project v2 Parametric Hole has no bindable diameter slot for Workbench edits. No production geometry Feature, CAD Skill, UI, or STEP path was added or replaced.

## Status matrix

| Requirement | Status | Evidence and boundary |
|---|---|---|
| `GENERAL_DIMENSION_REFERENCE` | `PARTIAL` | Typed Feature/Face/Edge/Datum/ValidatedGeometry variants exist. The circular boss profile EdgeReference and existing Hole sources are validated; general builders and rebuild validators do not cover every variant. |
| `GENERAL_GEOMETRIC_MEASUREMENT` | `PARTIAL` | No generic measurement engine across arbitrary Face/Edge/Datum references. Existing overall, Hole, and Pattern measures remain bounded to their source contracts. |
| `FILLET_RADIUS_DIMENSION` | `DEFERRED` | No unique source-attributed projected/section arc and legal radius layout. No R value is fabricated from the declared Fillet or torus alone. |
| `BOSS_DIAMETER_DIMENSION` | `PASS_WITH_NOTES` | Real Rust→CadQuery/OCP fixture: single-circle XY Sketch → normal-Z Extrude ADD; source-attributed cylinder/circle evidence and unique Front projected circle; Ø16 resolves, radius edit 8→10 rebuilds as Ø20, suppression removes the association. Other sketch planes, profiles, and ambiguous views fail closed. |
| `FEATURE_SPACING_DIMENSION` | `PARTIAL` | Existing HolePattern spacing/PCD callouts remain; arbitrary hole-axis, parallel-edge, feature-center-to-datum, and DatumPoint distances are not implemented. |
| `GENERAL_DEPTH_DIMENSION` | `PARTIAL` | Existing blind Hole depth callouts are supported. Generic finite Extrude CUT/Pocket/Slot depth and all requested support directions do not have the shared formal depth measure. A through cut is not labeled as a fixed blind depth. |
| `ASSOCIATIVE_REBUILD` | `PASS_WITH_NOTES` | Source model edits rebuild the boss diameter and existing Hole callouts; the real Workbench UI cannot edit the v2 Parametric Hole diameter because the v2 feature has no bindable diameter slot. |
| `SUPPRESSION_HANDLING` | `PASS` | Boss and Hole suppressions remove their stale associative output in Rust→OCP regressions. |
| `INVALID_REFERENCE_FAIL_CLOSED` | `PASS_WITH_NOTES` | Boss source/geometry/view checks reject inapplicable or incomplete witnesses; established missing/suppressed Hole source and unsupported-view cases remain covered. General semantic Face/Edge/Datum measurements are still partial. |
| `DRAWING_MODEL_INTEGRATION` | `PASS_WITH_NOTES` | Resolved boss and Hole dimensions are actual DrawingModel dimensions and carry source/witness diagnostics. |
| `SECTION_VIEW_INTEGRATION` | `PARTIAL` | Actual Section A-A geometry rebuild/update passed; general boss/Fillet associative dimensions are not integrated into arbitrary section curves. |
| `SVG_ANNOTATION_EXPORT` | `PASS` | Actual DrawingModel SVG contains boss Ø16→Ø20 in the Rust→OCP fixture and the blind-hole Ø10/depth6 browser fixture. |
| `DXF_VISUAL_ANNOTATION` | `PASS_WITH_NOTES` | SVG/DXF visual representation remains from DrawingModel. Composite callouts use visual primitives; independent acceptance does not count these as native entities. |
| `DXF_NATIVE_DIMENSION` | `PASS_WITH_NOTES` | Independent ezdxf 1.4.4 parse found real linear (80), diameter (Ø20), and radial (R5) DIMENSION entities; `$INSUNITS=4`, `$DIMLUNIT=2`, `AICAD_MM`, populated anonymous blocks, and no audit errors/fixes. Native coverage is limited to simple resolved dimensions. |
| `GENERIC_FULL_FEATURE_FIXTURE` | `PARTIAL` | Existing generic v2 real-kernel fixtures cover plate, circular boss, holes, patterns, counterbore, blind pocket, Fillet and Chamfer, and generate formal drawings. These are not yet one fixture where every requested feature receives a legal associative dimension across Front/Top/Left/Section A-A. |
| `G6A1_D1_D13_REGRESSION` | `PASS_WITH_NOTES` | All Rust workspace tests, including the established G6A.1 associative-dimension cases, passed. This regression does not imply the G6A.2 deferred dimensions are complete. |
| `COMPLEX_PART_ANNOTATION_SUBSET` | `PARTIAL` | Generic complex-part source and supported-subset regressions remain; not all requested left/right boss, side-hole, recess, and slot dimensions have a uniquely observed formal view. |
| `FULL_COMPLEX_PART_SUPPORT` | `BLOCKED` | `AICAD-COMPLEX-001` remains blocked by the previously identified ambiguous C1 chamfer target edge set. No target was guessed. |
| `PROJECT_V1_REGRESSION` | `PASS` | Existing canonical STEP and project regression completed in the isolated same-filesystem Rust→CadQuery/OCP integration. |
| `PROJECT_V2_REGRESSION` | `PASS` | M1–M16 16/16 and the formal Project v2 integration suite passed. |
| `FORMAL_STEP_REGRESSION` | `PASS` | Lowering, OCP body build, STEP, reimport, equivalence, and canonical byte identity passed. No STEP implementation was changed. |
| `BROWSER_ACCEPTANCE` | `PARTIAL` | Real Chromium with fake Provider passed the shared `/`→`/engineering.html` Workbench, language switching, three-view and Section A-A drawing, SVG/DXF downloads, formal STEP download/hash identity, and failed-export no-download check. The G6A.2 browser fixture displayed/exported a resolved Ø10/depth6 callout, but a bound v2 Hole diameter input is not exposed; attempting a `bind_parameter` to the v2 Parametric Hole was rejected as `SCC-TOOL-010`. |
| `REAL_DEEPSEEK_TEST` | `NOT_RUN` | Fake Provider only. No real API key was read, printed, or used. |

## Implementation and evidence

### Circular additive boss

The supported association uses a stable Sketch profile entity ID and an exact source-attributed cylindrical face/circular-edge witness from the rebuilt solid. It also requires a unique Front projected circle. The test changes the circle radius from 8 mm to 10 mm, rebuilds through Rust→CadQuery/OCP, and confirms the DrawingModel/SVG/native DXF measurement changes from Ø16 to Ø20. Suppressing the source removes the association. It is not a general circular-cylinder finder and does not infer non-circular profiles as diameters.

### Native DXF entities

The exported file was parsed independently with ezdxf 1.4.4 in a temporary environment. It contained three native DIMENSION entities: linear 80 mm, diameter Ø20, and radial R5. Each references a populated anonymous graphics block; entity types, text, style, units, and definition points were inspected. The diameter definition points lie on opposite sides of the measured circumference; the radial entity uses its center and boundary points. Legacy non-parametric dimension output remains visual-only. Pattern and compound hole callouts also remain on that visual path.

Autodesk group-code references used to check the DXF definition-point semantics: [radial and diameter dimensions](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-165A992D-9017-4C1E-B8CC-E70A17191BFE.htm) and [common dimension group codes](https://help.autodesk.com/cloudhelp/2016/ENU/AutoCAD-DXF/files/GUID-EDD54EAC-A339-4EBA-AEA6-EC8066505E2B.htm).

### Real browser evidence

The browser tests ran in isolated containers on an internal-only Docker network with a deterministic fake Provider, no published port, and no real credentials.

- `browser-g5.mjs`: PASS. `/` and `/engineering.html` show the Engineering Workbench; Simplified Chinese is default and English toggling works. The browser created a machining v2 part and a Revolve v2 part, rendered Front/Top/Left, downloaded SVG/DXF and canonical STEP, matched browser-saved STEP bytes and SHA-256 to the API validated bytes, and confirmed a failed 422 export produced no download.
- `browser-g6a.mjs`: PASS. Three-view and Section A-A were generated, SVG/DXF downloads contained the section output, and no browser console/page or HTTP error was recorded.
- `browser-g6a2.mjs`: PARTIAL. The browser created a v2 plate with a blind Hole and displayed/downloaded the resolved Ø10/depth6 DrawingModel annotation. The feature inspector exposed no editable slot. A fixture attempt to bind `diameter` to the v2 Parametric Hole failed closed with `SCC-TOOL-010` because that slot is not valid for the v2 Parametric feature. Thus an actual browser parameter edit and browser-observed annotation update did not pass.

Screenshots are captured from the real Chromium session:

- [Machining Workbench](artifacts/engineering-workbench-machining.png)
- [Section A-A Workbench](artifacts/engineering-workbench-section.png)
- [G6A.2 blind-hole callout](artifacts/engineering-workbench-g6a2-callout.png)

### Regression runs

| Command or suite | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace --all-targets` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `npm test --prefix apps/scc-ui` | PASS: 178 total, 157 passed, 21 host-gated OCP tests skipped, 0 failed |
| Isolated CadQuery/OCP unit suite | PASS: 106 tests |
| Rust→OCP / canonical manufacturing integration | PASS: 14/14 tests, including X2–X10 (9/9, no delivered artifacts, legal follow-up 9/9), Project v1/v2, geometry reimport/equivalence, and Section A-A update |
| Project v2 manufacturing matrix | PASS: M1–M16, 16/16 |
| `docker compose config --quiet` | PASS |
| `git diff --check` | PASS |

X2–X10 retained their actual G5.1 definitions and were sent through the formal HTTP STEP API. Each returned the expected class-specific failure stage and stable error code with HTTP 422, no STEP body/metadata delivery, and a subsequent legal request succeeded. The run distinguishes engineering validation from manufacturing lowering, geometry build, and STEP validation failures.

## Limits and follow-up boundary

- Do not claim general associative dimensions, Fillet R, arbitrary feature spacing, or universal blind-cut depth are supported.
- A Project v2 Parametric Hole currently cannot expose its own diameter as a Workbench-editable parameter through `feature_parameters`; the browser evidence confirms this as a real product limitation rather than substituting an API test.
- The one-fixture all-feature dimension coverage remains incomplete. Existing geometry/manufacturing fixtures and drawing cases are complementary, not a claim that every machining requirement has a legal current view.
- C1 is still ambiguous, so complete AICAD-COMPLEX-001 drawing/STEP acceptance is blocked. No Sweep/Loft/G6B work was started.
- The historical four-slot STEP timeout remains its independent issue; this phase does not relabel it as resolved.

## Git and safety

No production API key was accessed. No user DeepSeek request was made. The protected `docs/testing/mmf204-real-user-acceptance/` directory was kept untouched and excluded from staging. No automatic push was performed. The implementation and this report are committed locally; see the final task summary for commit, final HEAD, and worktree state.
