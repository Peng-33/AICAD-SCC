# Phase G5.1 — Formal Manufacturing Acceptance Closure

- Date: 2026-09-26
- Branch at start: `codex/aicad-pre-rollback-recovery`
- HEAD at start: `bfca3a9f47e6407fee002fd148c57f18552a0eae`
- Historical reference: `bfca3a9f47e6407fee002fd148c57f18552a0eae` (the actual starting HEAD matched it)

## Result

```text
PHASE_G5_1 = PASS_WITH_NOTES
G5_CORE_MANUFACTURING = PASS
G5_ACCEPTANCE_COVERAGE = PASS_WITH_NOTES
FULL_COMPLEX_PART_SUPPORT = BLOCKED
HTTP_NEGATIVE_MATRIX_X2_X10 = PASS
DRAWING_STEP_GEOMETRIC_CONFORMANCE = PASS_WITH_NOTES
INTERNAL_FEATURE_VERIFICATION = PASS_WITH_NOTES
GENERIC_MACHINING_FIXTURE = PASS
GENERIC_REVOLVED_FIXTURE = PASS
COMPLEX_PART_REQUIREMENT_COVERAGE = PASS_WITH_NOTES
FULL_COMPLEX_PART_DRAWING = BLOCKED
FULL_COMPLEX_PART_STEP = BLOCKED
STEP_REIMPORT = PASS
GEOMETRIC_EQUIVALENCE = PASS
SAME_VALIDATED_BYTES = PASS
PROJECT_V1_STEP_REGRESSION = PASS
PROJECT_V2_STEP_REGRESSION = PASS
BROWSER_ACCEPTANCE = PASS
REAL_DEEPSEEK_TEST = NOT_RUN
PUSHED = NO
```

G5's shared manufacturing path is intact and all previously missing X2–X10 formal HTTP negatives now have evidence. Independent feature observations cover actual `DrawingModel` primitives and the actual API-delivered, OCP-reimported STEP bytes. The exact, unambiguous features of AICAD-COMPLEX-001 were exercised through Drawing and formal STEP as a clearly named subset. The whole part remains blocked: the original C1 requirement does not identify the edge set, and the current three views omit or do not dimension several machining details. No feature was guessed or silently omitted from the subset history.

## Starting-state and source audit

- Read workspace rules and the repository's current branch, HEAD, status, ADR-064 through ADR-069, G5 acceptance report, X1–X10 definitions, M1–M16 matrix, formal STEP HTTP/CLI/equivalence code, and Drawing/Projection/Visibility geometry contracts.
- Starting worktree had only the user-owned untracked `docs/testing/mmf204-real-user-acceptance/` directory. It was not edited or staged. No reset, clean, restore, stash, or push was performed.
- The original complex-part specification is not stored in the repository. It was recovered from the supplied user attachment and checked before mapping:

```text
SPEC_IN_REPO = NO
SPEC_SOURCE_RECOVERED = YES
ATTACHMENT_SHA256 = bd72eb0694afabee54f4444457f659c0b837b24b426ac7fb3f05f926605565b0
```

The attachment's full original dimensions and feature descriptions were used; this report does not replace the source with an abbreviated specification. The only unresolved geometry instruction is `其余指定底板外轮廓锐边 C1`: the source does not identify which top/bottom/perimeter edges are intended. No edge selector was invented.

## Formal HTTP negative matrix X2–X10

Every case POSTed an actual EngineeringModel v2 to `POST /api/engineering/export/step`. Each returned JSON HTTP 422, a machine-readable cause code and failure class/stage, no STEP/download body, no artifact object, no content-disposition, no hash/size/success-stage headers, and no server path or credential leakage. A legal STEP API request immediately after each rejection passed, including hash/size and manufacturing/build/reimport/equivalence headers (9/9).

| TEST_CASE | INPUT | EXPECTED_FAILURE_STAGE | ACTUAL_FAILURE_STAGE | HTTP_STATUS | ERROR_CODE | FAILURE_CLASS | ARTIFACT_DELIVERED | RESULT |
|---|---|---|---|---:|---|---|---|---|
| X2 | 100×60×10 base; Ø10 ThroughAll cut centered at (100,0), no intersection | `manufacturing_build` | `manufacturing_build` | 422 | `CUT_NO_INTERSECTION` | `GEOMETRY_BUILD_FAILURE` | NO | PASS |
| X3 | 10×10×10 base; Ø200 ThroughAll cut removes the whole body | `manufacturing_build` | `manufacturing_build` | 422 | `CUT_REMOVES_BODY` | `GEOMETRY_BUILD_FAILURE` | NO | PASS |
| X4 | Hole diameter 0 on a valid base | `engineering_validation` | `engineering_validation` | 422 | `INVALID_HOLE_DIAMETER` | `ENGINEERING_VALIDATION_FAILURE` | NO | PASS |
| X5 | Hole Ø8 with invalid counterbore Ø6, depth 3 | `engineering_validation` | `engineering_validation` | 422 | `INVALID_COUNTERBORE` | `ENGINEERING_VALIDATION_FAILURE` | NO | PASS |
| X6 | Linear pattern references missing seed `x6_missing_seed` | `engineering_validation` | `engineering_validation` | 422 | `PATTERN_SEED_NOT_FOUND` | `ENGINEERING_VALIDATION_FAILURE` | NO | PASS |
| X7 | Three repeated blind pockets, followed by semantic entrance Fillet with ambiguous pattern-edge reference | `manufacturing_build` | `manufacturing_build` | 422 | `AMBIGUOUS_TOPOLOGY_REFERENCE` | `GEOMETRY_BUILD_FAILURE` | NO | PASS |
| X8 | Otherwise valid machining fixture with Fillet radius 0 | `engineering_validation` | `engineering_validation` | 422 | `INVALID_FILLET_RADIUS` | `ENGINEERING_VALIDATION_FAILURE` | NO | PASS |
| X9 | DatumAxis endpoints are identical (zero-length axis) | `engineering_validation` | `engineering_validation` | 422 | `INVALID_DATUM_AXIS` | `ENGINEERING_VALIDATION_FAILURE` | NO | PASS |
| X10 | Revolve rectangle profile crosses the specified Z axis | `manufacturing_build` | `manufacturing_build` | 422 | `REVOLVE_PROFILE_CROSSES_AXIS` | `GEOMETRY_BUILD_FAILURE` | NO | PASS |

The test preserves the existing SCC outer error envelope and exposes the original cause separately. Geometry-build, engineering-validation, manufacturing-lowering, and STEP-validation failures are not collapsed into one cause code.

## Drawing ↔ STEP feature conformance

The reference was the same generic EngineeringModel v2 per fixture. Drawing evidence came from the Rust-generated `DrawingModel`'s actual typed view primitives and official SVG/DXF pipeline. Manufacturing evidence came from the exact formal HTTP STEP response body written unchanged to a temporary file and reimported with CadQuery/OCP; the observer reads analytic surfaces and bounds without using transient Face/Edge ordinals or the source-shape builder.

Feature measurement tolerance in these cross-representation assertions is `1e-5 mm`. This is separate from the formal ADR-069 equivalence tolerances: bounds/center of mass `1e-6 mm`, volume/surface area `1e-5` absolute plus `1e-9` relative, analytic descriptor scalars `1e-5`, and material-probe tolerance `1e-7`.

| Requirement | ENGINEERING_EXPECTED | DRAWING_OBSERVED | STEP_REIMPORT_OBSERVED |
|---|---|---|---|
| Base dimensions | 120×90 footprint, Z0..20 | Front bounds X[-60,60], Y[-45,45]; the 20 mm thickness is `NOT_OBSERVABLE_IN_CURRENT_DRAWING` in emitted typed primitives; a side dimension/detail is needed | Reimport bounds X[-60,60], Y[-45,45], Z0..30; horizontal base faces at Z0 and Z20 |
| Circular ADD boss | Center (-42,0), Ø28, Z20..30, top R0.5 | Front analytic circles R14 and R13.5; height is not dimensioned | R14 cylinder Z20..29.5, torus major R13.5/minor R0.5, max Z30 |
| Through hole and entrance | Z axis, center (38,0), Ø8 through, C0.5 entrance | Front Ø8 and Ø9 mouth circles | R4 cylinder Z0..19.5 and 45° cone to Z20, mouth R4.5 |
| Side blind hole | Entry (60,0,8), axis −X, Ø6, depth 10 | `NOT_OBSERVABLE_IN_CURRENT_DRAWING`: current named views emit no analytic entry-circle primitive; section/auxiliary view required | R3 analytic cylinder at Y0/Z8, X interval 50..60 |
| Counterbore | Center (42,-24), pilot Ø6 through, CB Ø10×3 from Z20 | Front pilot and hidden CB circles; CB depth `NOT_OBSERVABLE_IN_CURRENT_DRAWING` without section | R3 pilot Z0..17 and R5 CB Z17..20 |
| Circular pattern | 4×Ø5 on radius 24 about (0,0) | Front four analytic circles at the expected centers | Four R2.5 cylinders through Z0..20 at those centers |
| Linear pattern | 3×Ø4, 18 mm spacing along +X from (-28,-28) | Front three analytic circles at the expected centers | Three R2 cylinders through Z0..20 at those centers |
| Extrude CUT | 18×12 opening at (20,24), from Z20 toward −Z by 6 | Front opening edges at X11..29/Y18..30; cut depth `NOT_OBSERVABLE_IN_CURRENT_DRAWING` without section | Planar pocket floor Z14 with matching XY bounds |
| Revolve axis and profile radii | X axis; radii 20/15/10 over X[0,20]/[20,60]/[60,80] | Front silhouette has measurable X-axis radial levels R20/R15/R10 | Analytic X-axis cylinders with those radii and intervals |
| Revolve sweep angle | Full 360° | `NOT_OBSERVABLE_IN_CURRENT_DRAWING`; orthographic silhouettes cannot prove rotational coverage | Reimported cylindrical U spans are 2π |
| Fillet/chamfer sizes | Actual R/C sizes in the source feature history | `NOT_OBSERVABLE_IN_CURRENT_DRAWING` as dimensioned machining callouts; detail/section annotation needed | Analytic torus/cone measurements confirm the tested radii, angle, and extent |

The STEP geometric observer independently verified one valid solid and the delivered body surfaces. The model's `spatial_summary` was not used as a substitute for Drawing or STEP geometry evidence. A successful STEP reimport is not treated as proof that the drawing expresses every internal requirement.

## Generic fixtures and full complex-part coverage

Both reusable generic fixtures traversed DrawingModel → official SVG/DXF and formal HTTP STEP → exact-byte OCP reimport → equivalence. STEP response SHA-256 and size headers matched the response bytes; the same bytes were then imported by the independent observer.

- `GENERIC_MACHINING_FIXTURE = PASS`: sequential ADD/CUT, side blind Hole, through Hole, Counterbore, circular and linear patterns, semantic Fillet and Chamfer. STEP size 96,061 bytes in the formal M16 fixture; typed Front/Top/Left primitives and SVG/DXF were generated.
- `GENERIC_REVOLVED_FIXTURE = PASS`: full-turn DatumAxis/Revolve, mixed Extrude CUT, axial/radial holes and pattern, semantic Fillet and Chamfer. STEP size 70,891 bytes; typed Front/Top/Left primitives and SVG/DXF were generated.
- The fixture histories retain the existing parameterized-feature rebuild path; no part-specific production builder or new geometry feature was added.

Complex coverage used a test-only generic EngineeringModel v2 history made from existing Sketch, Extrude, Hole, Pattern, Fillet and Chamfer operations. It is not a production builder. All unambiguous feature rows below were active in the same one-solid subset. The subset was named `AICAD-COMPLEX-001-supported-subset`; it is not the complete part's STEP deliverable.

| REQUIREMENT_ID | ENGINEERING_FEATURE | PARAMETERS | ENGINEERING_SUPPORTED | DRAWING_SUPPORTED | MANUFACTURING_SUPPORTED | VERIFICATION_EVIDENCE |
|---|---|---|---|---|---|---|
| CP-01 | `complex_base_add` | 180×120×24; four outer vertical R6 corners | YES | PARTIAL; three views generated, no complete dimension annotations | YES for subset | active feature + reimported rounded-corner cylinders and overall envelope |
| CP-02 | `left_boss_add` | center (-35,0), Ø66, Z24..48, fused | YES | PARTIAL; profile visible, internal heights not fully dimensioned | YES for subset | R33 cylinder and one-solid 180×120×48 envelope |
| CP-03 | `left_boss_top_fillet` | top outside R1 | YES | PARTIAL; blend has no manufacturing callout | YES for subset | OCP torus major R32/minor R1 |
| CP-04 | `left_center_hole`, `left_center_hole_chamfer` | center (-35,0), Ø32 through Z0..48; C0.5 at top | YES | PARTIAL; top profile is projected, no section/dimension callout | YES for subset | OCP R16 cylinder and 45° cone from Z47.5..48 |
| CP-05 | `left_pcd_seed`, `left_pcd_pattern` | 4×Ø5.5, PCD52 at 45/135/225/315°, through | YES | PARTIAL; named projections lack complete dimension callouts | YES for subset | four OCP R2.75 cylinders at exact centers through Z0..48 |
| CP-06 | `right_boss_add` | 68×80 R3, X14..82/Y±40, Z24..36, fused | YES | PARTIAL; profile projection generated | YES for subset | four OCP R3 vertical corners and one-solid reimport |
| CP-07 | `right_boss_top_fillet` | top outside R1 | YES | PARTIAL; blend has no manufacturing callout | YES for subset | OCP torus minor R1 on right boss |
| CP-08 | `right_pocket_cut` | center (48,0), 32×32 R2, Z36 to Z28 | YES | PARTIAL; pocket depth needs section | YES for subset | OCP R2 inside corners and planar floor at Z28 |
| CP-09 | `pocket_center_hole` | Ø10 Z0..28; CB Ø16 Z25..28 | YES | PARTIAL; CB depth needs section | YES for subset | R5 cylinder Z0..25; R8 cylinder Z25..28 |
| CP-10 | four `right_mount_hole_*` | four Ø6.6 through at (24,30),(72,30),(24,-30),(72,-30); CB Ø11 Z32..36 | YES | PARTIAL; counterbore depths need section/callout | YES for subset | four R3.3 and four R5.5 reimported cylinders at requested intervals |
| CP-11 | four `base_slot_*_cut` | 14×7, R3.5 ends, X-axis, centers (±75,±45), through Z0..24 | YES | PARTIAL; through extent is not dimensioned in the three views | YES for subset | eight OCP R3.5 end cylinders through base only |
| CP-12 | `side_blind_counterbore` | X90/Y0/Z16, axis −X; Ø8 to X72; CB Ø12×2 | YES | PARTIAL; side entry/depth needs section or auxiliary view | YES for subset | OCP R4 and R6 cylinders over X72..88 and X88..90 |
| CP-13 | `bottom_relief_cut` | bottom opening at (0,0), 24×40 R3, Z0..5 | YES | PARTIAL; bottom is not one of the three views; section/bottom view needed | YES for subset | OCP rounded corners and floor at Z5 |
| CP-14 | unresolved semantic Chamfer target | C1 on the “remaining designated base outline sharp edges”; exact edge set is unspecified | Generic Chamfer exists; this target is BLOCKED pending clarification | BLOCKED | NOT MODELED | no selector guessed; no edge set exists in the source wording |

```text
COMPLEX_PART_SUPPORTED_SUBSET_DRAWING = PASS_WITH_NOTES
COMPLEX_PART_SUPPORTED_SUBSET_STEP = PASS_WITH_NOTES
FULL_COMPLEX_PART_DRAWING = BLOCKED
FULL_COMPLEX_PART_STEP = BLOCKED
FULL_COMPLEX_PART_SUPPORT = BLOCKED
```

Subset formal API evidence: STEP size 275,799 bytes; reimported bounds `[-90,90,-60,60,0,48.0000001]`; one valid solid; API manufacturing/build/reimport/equivalence statuses all `passed`. The full-part statuses remain blocked by CP-14 and the drawing's unexpressed section/internal-dimension requirements. No complete-part artifact was named or reported as delivered.

## Failure closure and timing

- Added short backend tests: a 50 ms worker timeout kills the child and returns `Timeout` with no response bytes; a worker that prints STEP-like bytes and is then signal-terminated returns a process error and discards stdout. Existing OCP exception cleanup tests also pass. These test the Rust backend boundary; no claim is made that a synthetic 60-second timeout was driven through the HTTP server.
- The separate G5 historical four-slot STEP timeout remains unresolved. It was not reclassified as a pass. The more complex subset's successful timing is a different model and does not close that issue.
- Complex subset Drawing sample (test run): history/kernel rebuild 6.2 s; projection/visibility 0.8 ms; three-view generation 1.5 ms; SVG export 0.6 ms; DXF export 0.6 ms.
- Same-model formal CLI stage sample: manufacturing lowering 0.252 ms; OCP build 4,852.991 ms; STEP serialization 15.533 ms; reimport 232.524 ms; equivalence 184.001 ms; total 5,285.476 ms. These are observed timings, not an SLA. Formal API success-stage headers and exact response bytes were checked independently.

## Browser acceptance

Isolated Chromium/Playwright used the local fake Provider and an internal-only Docker network. There were no host port mappings and no real Provider calls. Browser actions verified:

- `/` and `/engineering.html` lead to the same Simplified Chinese Engineering Workbench; English switching remains available.
- Engineering Intent creates Project v2 machining and Revolve histories, produces Front/Top/Left, and exports SVG/DXF.
- Downloaded machining STEP (75,479 bytes) and Revolve STEP (32,726 bytes) exactly equal their API response bodies and the server-validated SHA-256.
- A browser-triggered STEP request receiving a 422 JSON failure has no STEP magic, download headers, artifact headers, or browser download event.

```text
BROWSER_ACCEPTANCE = PASS
REAL_DEEPSEEK_TEST = NOT_RUN
SCREENSHOT = aicad-g5.1-engineering-workbench.png (1440×1677)
SCREENSHOT_SHA256 = 5536c3a23e5d72438bbe9b67846ee25609afcf1049683f9e62311874d8309533
```

## Regression results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace --all-targets` | PASS, including timeout and interrupted-worker fail-closed tests |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `npm test --prefix apps/scc-ui` | PASS: 175 total, 155 passed, 0 failed, 20 environment-gated skips |
| Isolated CadQuery/OCP unittest suite | PASS: 102 tests |
| Isolated Rust→CadQuery/OCP + formal STEP/API + Engineering Intent | PASS: 33/33, 0 skipped |
| Project v1 formal STEP | PASS within the isolated Rust→OCP regression |
| Project v2 formal STEP M1–M16 | PASS: 16/16 |
| HTTP API X2–X10 | PASS: 9/9; legal follow-up after each: 9/9 |
| Browser Chromium/Playwright | PASS |
| `docker compose config --quiet` | PASS |
| `node --check` and Python observer execution | PASS |
| `git diff --check` | PASS |

The host Node suite intentionally skips environment-gated real OCP cases; those were not counted as passing there and were run separately in the isolated container above.

## Changes made and boundaries

- `crates/scc-cli/src/engineering.rs`: carries original parametric cause codes and explicit failure class/stage into formal STEP diagnostics; exposes typed DrawingModel view primitives for observation.
- `apps/scc-ui/lib/scc-cli.mjs`: preserves structured failure class/stage/cause in HTTP JSON while retaining the outer SCC error envelope.
- `crates/scc-cad-backend/src/lib.rs`: timeout/interruption fail-closed regression tests only.
- `apps/scc-ui/cad/tests/step_feature_observer.py`: generic test-only OCP observer for actual reimported STEP bytes; no production shape construction.
- `apps/scc-ui/test/rust-parametric.integration.test.mjs`: HTTP negative cases, generic conformance, and test-only generic-history complex subset.
- `apps/scc-ui/e2e/browser-g5.mjs`: browser no-download assertion for an HTTP 422 failure.
- No new CAD feature, production part-specific builder, exporter, API route, STEP format, or geometry implementation was added.
- No API key was read, printed, reset, or committed; no Provider request was made.

The final local commit and post-commit HEAD/worktree status are recorded in the delivery message. No push was performed.
