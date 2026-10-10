# Phase G7B — General Parametric Loft Acceptance Report

- Date: 2026-09-27
- Baseline HEAD: `21154fcdd95cd5d999d3e2596d20f7f441129f3f`
- Branch: `codex/aicad-pre-rollback-recovery`
- Overall: `PASS_WITH_NOTES`
- `LOFT_GEOMETRY`: `PASS`
- `LOFT_TOPOLOGY_REFERENCE`: `PARTIAL`
- `LOFT_INNER_LOOPS`: `DEFERRED`
- `CLEAN_TEST_IMAGE_BUILD`: `PASS`
- `BROWSER_ACCEPTANCE`: `PASS`
- `REAL_DEEPSEEK_TEST`: `NOT_RUN`

## Baseline and Preservation

The current branch and actual starting HEAD were checked before changes. The tracked tree was clean at baseline. The pre-existing untracked `docs/testing/mmf204-real-user-acceptance/` directory is user-owned historical evidence; it was not opened, modified, staged, or deleted. Workspace governance and the project rules were loaded. ADR-064 through ADR-075, the G7A report, feature/rebuild/backend contracts, Drawing pipeline, six CAD Skills, and the sole Engineering Workbench were reviewed.

## Delivered Contract

- Added typed `ParametricFeature::Loft` referencing 2–32 earlier, unique Sketch IDs, with ADD/CUT, `RULED`, and `ALIGNED` fields. Sketch geometry remains owned by the original features.
- The supported plane frames are parallel, co-oriented principal/Datum planes with matching local axes and strict monotonic offsets along one common axis. An X-axis geometry case proves that the executor is not limited to world Z. Non-parallel frames, lateral plane movement, and twist are rejected.
- Each section has one closed outer loop. The validator checks finite supported analytic curves, unique curve identity, closure, non-degenerate area, self-intersection, winding, section order, and one-to-one ordered curve-kind/count correspondence before OCP is called.
- The shared schema-v5 Rust→CadQuery/OCP protocol lowers transient wire/face section inputs to real solid ruled `BRepOffsetAPI_ThruSections` geometry. No OCP Shape is persisted as editable state.
- ADD initializes the first body or fuses with the current body; it must leave one valid positive-volume solid. CUT must intersect the current body, reduce measured material, and leave one valid solid. Detached ADD, no-intersection CUT, whole-body removal, invalid geometry, and protocol/reference failures remain distinct fail-closed diagnostics.
- Full deterministic history rebuild handles section edits, suppression/restore, and dependent features. Hole after Loft is tested. G7B does not claim that every generated Loft edge can be referenced by Fillet/Chamfer.
- The existing Spatial → Projection → Visibility → DrawingModel → SVG/DXF and Section A-A path is reused. Formal STEP continues through the sole project v2 Manufacturing/STEP API and CLI, STEP serialization, re-import, equivalence, and byte-identity checks.
- The six existing CAD Skills and Workbench property/diagnostic surfaces were updated; no new Skill, UI, exporter, fixture-specific Builder, NURBS feature, or guide-curve feature was introduced.

## L1–L24 Test Matrix

| Case | Expected | Actual | Result | Evidence |
|---|---|---|---|---|
| L1 — two circular sections, ADD | Two compatible circles form one solid. | A two-circle OCP ADD produced one valid solid; the test also covers the X principal axis (L6). | PASS | `test_g7b_ruled_rectangle_loft_and_non_z_principal_axis` |
| L2 — three circular sections, ADD | All three ordered circles affect the solid. | Real OCP Loft used Ø20 at Z0, Ø40 at Z30, and Ø26 at Z70; one solid with bounds Z0–70. | PASS | `test_g7b_ruled_three_section_loft_add_uses_all_sections_and_rebuilds_deterministically` |
| L3 — non-monotonic section size | Intermediate and final size changes affect actual geometry. | Radii 10→20→13; section probes at Z30 and Z55 verify the middle section and ruled transition toward the final section. | PASS | Same OCP test; inside/outside B-rep probes at Z30 and Z55. |
| L4 — section centers move | Section transforms and centers remain in the geometry. | Centers change from (0,0) to (5,0) to (-3,2); the Z45 station probes use the displaced center and bounds. | PASS | OCP station material probes; final Drawing/STEP front bounds. |
| L5 — compatible rectangles | Same-topology rectangular profiles form a valid transition. | 20×10 rectangle at Z0 to 30×14 at Z20, offset center, one positive-volume solid. | PASS | `test_g7b_ruled_rectangle_loft_and_non_z_principal_axis` |
| L6 — non-Z principal direction | The executor transforms section planes and local geometry on another axis. | Two circular sections on YZ frames form a one-solid Loft from X0 to X25 with correct bounds. | PASS | Same OCP test; X-axis bounds asserted. |
| L7 — ADD to existing Body | Fused result remains exactly one body and adds material. | An Extrude base plus a three-section Loft ADD fused to one solid with volume above the base volume. | PASS | `test_g7b_loft_add_to_body_cut_cavity_and_real_section_geometry` |
| L8 — Loft CUT | Loft tool makes a real variable-section cavity and reduces material. | Three-section circular Loft CUT removed material, left one valid solid, and center/outside probes distinguished cavity from remaining material. | PASS | OCP CUT test and final STEP re-import material probes. |
| L9 — Loft + Extrude | Features coexist in ordered history. | `Extrude ADD → three-section Loft CUT` reaches Drawing and STEP. | PASS | `makeLoftCutHistory`; final-image G7B integration. |
| L10 — Loft + Revolve | Loft is composed with another generic body source in one history. | `Revolve ADD → three-section Loft ADD → Hole` succeeds through Drawing and STEP. | PASS | `makeLoftAddHistory`; final-image G7B integration. |
| L11 — Hole after Loft | Later dependent machining re-resolves after Loft. | Through Hole after Loft ADD is included in the active rebuilt operation history and STEP. | PASS | Final-image `rust-parametric.integration.test.mjs`. |
| L12 — missing Sketch reference | Fail closed with stable API error and no successful artifact. | Real browser request to formal STEP API returned HTTP 422 with `LOFT_SECTION_REFERENCE_NOT_FOUND`; no STEP hash header, download header, or artifact. | PASS | Chromium fake-provider browser test asserts status, code, and artifact absence. |
| L13 — too few sections | Reject a one-section request before kernel execution. | `LOFT_SECTION_COUNT_INVALID`. | PASS | `test_g7b_loft_validation_and_boolean_failures_are_closed` |
| L14 — degenerate section | Reject zero-area profile geometry. | Collinear closed loop returns `LOFT_PROFILE_DEGENERATE`. | PASS | Same OCP validation test. |
| L15 — invalid section ordering | Reject reversed or non-monotonic section positions. | Reversed list returns `LOFT_SECTION_ORDER_INVALID`. | PASS | Same OCP validation test; model references must also be earlier history nodes. |
| L16 — incompatible profile topology | Do not guess correspondence between different curve topology. | Circle-to-rectangle profile returns `LOFT_PROFILE_INCOMPATIBLE`. | PASS | Same OCP validation test. |
| L17 — self-intersection | Reject invalid contour before it becomes a solid. | Bow-tie contour returns `SELF_INTERSECTION`; collinear degenerate contour is separately rejected. | PASS | Same OCP validation test; assertions run in final 119-test suite. |
| L18 — detached ADD | Reject an unfused/additionally disconnected material body. | Detached sections return `LOFT_ADD_DETACHED`. | PASS | Same OCP validation test. |
| L19 — CUT without intersection | Reject a Loft tool that misses the body. | No-hit CUT returns `LOFT_CUT_NO_INTERSECTION`. | PASS | Same OCP validation test. |
| L20 — edit a section | Rebuild from edited Sketch and update downstream result. | Editing the middle radius 20→17 changes volume; the browser creates valid section geometry from the submitted model. | PASS | OCP edit assertion and browser workflow. |
| L21 — suppress/restore | Suppress Loft, then restore the same rebuilt result. | Suppression removes the Loft operation; restore reproduces the original spatial summary and volume. | PASS | Final-image G7B Rust→OCP integration. |
| L22 — deterministic history | Identical EngineeringModel histories yield equivalent derived geometry. | Repeated rebuilds match bounds, volume, and face-count evidence; restored history matches the complete original spatial summary. | PASS | OCP and final-image integration tests. |
| L23 — STEP re-import | Delivered STEP is a valid single solid with equivalent geometry/material. | ADD (19,373 bytes) and CUT (27,124 bytes) STEP artifacts re-import as one valid solid and pass the unchanged equivalence gate with section-station material probes. | PASS | Final-image integration; SHA-256 recorded below. |
| L24 — Drawing/STEP conformance | Extracted DrawingModel and re-imported STEP evidence agree within tolerance. | Actual typed DrawingModel Front XY bounds match re-imported STEP XY bounds at `1e-5 mm`; the separate source-OCP versus STEP volume check agrees within `1e-5 mm³` and is not a DrawingModel measurement. Station-specific section dimensions are not recoverable from the existing three-view Drawing and are not inferred from its outline. | PASS_WITH_NOTES | `G7B_DRAWING_STEP_FRONT_XY`; independent DrawingModel primitives and re-import observer. |

## Generic Fixtures and Drawing ↔ STEP Evidence

**Fixture A — additive mixed history.** `Revolve ADD → three-section Loft ADD → Hole` uses the existing generic history and creates actual Front/Top/Left DrawingModel primitives, SVG/DXF, A-A section, and canonical STEP. The three section circles have radii 15/20/13 mm at Z20/45/70 mm with centers (0,0)/(5,0)/(-3,2). OCP station witnesses check the intermediate section and the interpolated mid-span. STEP artifact: 19,373 bytes, SHA-256 `52b7454faef0cf8ad7843cdced702a959d0a3bad6056e48b24f4c11e18cd340a`.

**Fixture B — subtractive mixed history.** `Extrude ADD → three-section Loft CUT` cuts a real cavity from a 100×80×40 mm body. The section radii are 4/8/5 mm at Z2/20/38 mm, with center changes in the end stations. OCP cavity/remaining-material probes pass; Section A-A returns at least two closed loops with a hatch in SVG and DXF. STEP artifact: 27,124 bytes, SHA-256 `d7117f5572e78ac4d1fb07cb6ee41ac3fac2a2ffff7bb64633949dab4cd739a4`.

**Independent comparison.** `DrawingModel` front primitives and bounds are extracted from a completed drawing API response. STEP bounds, volume, solid validity, and material probes are extracted by importing the actual STEP bytes in the independent observer. The emitted front XY bounds matched at `1e-5 mm`; Source OCP versus STEP re-import volume difference was bounded by `1e-5 mm³`; this measurement is not derived from DrawingModel. STEP equivalence retains its pre-existing tighter bounding-box tolerance `1e-6 mm`, volume absolute tolerance `1e-5 mm³` plus relative `1e-9`, surface area absolute tolerance `1e-5 mm²` plus relative `1e-9`, and center-of-mass tolerance `1e-6 mm`.

The three-view drawing does not uniquely expose every station’s internal section size as a dimensioned manufacturing requirement. G7B therefore marks those section dimensions `NOT_OBSERVABLE_IN_CURRENT_DRAWING`; use a suitable additional section/dimension view if a drawing must communicate them. STEP station probes are independent B-rep evidence and do not upgrade the Drawing claim.

## Numerical Representation and Topology Limits

- OCP Loft result remains an exact transient B-rep. For non-analytic B-spline surfaces, Spatial evidence is a bounded sample (0.05 mm linear deviation, 0.1 rad angular tolerance), not a persistent topology identity.
- HLR-projected non-analytic curves are approximated with declared 0.01 mm deviation. Section A-A uses 0.01 mm chord tolerance. DXF therefore contains the declared projected/sample geometry; it does not claim these B-spline curves are exact analytic circles/arcs.
- No OCP Face/Edge index is persisted. `LOFT_TOPOLOGY_REFERENCE=PARTIAL`: stable selection is not promised for every Loft side face, section edge, or generated side edge. G7B does not certify generic Fillet/Chamfer operations on all Loft edges.
- `RULED` makes no smooth/G1/G2 continuity claim. `SMOOTH`, inner loops, arbitrary/non-parallel planes, user twist, guide curves, and multi-solid output are deferred or rejected.

## Verification Results

| Gate | Result | Evidence |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | Rust formatting check. |
| `cargo check --workspace --all-targets` | PASS | Final Rust source. |
| `cargo test --workspace --all-targets` | PASS | Final Rust source; 1,164 passed, 0 failed, 2 ignored across 50 suites. |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | Final Rust source, warnings denied. |
| `npm test --prefix apps/scc-ui` | PASS | 189 tests; 165 passed, 0 failed, 24 opt-in Rust/OCP tests skipped in the host-only run. Those integration tests were run separately in the image. |
| Final-image Rust→OCP parametric integration | PASS | `AICAD_RUST_PARAMETRIC_INTEGRATION=1`; 17/17, including G1B–G7B, G5.1, M1–M16, X2–X10 with 9/9 legal follow-ups, Project v2, G6A/G6B. |
| Final-image CadQuery/OCP Python suite | PASS | 119/119. |
| Project v1 STEP CLI regression | PASS | `AICAD_RUST_STEP_INTEGRATION=1`; 6/6 against `aicad-scc-ui:g7b-final-test`. |
| `docker compose config --quiet` | PASS | Compose configuration validation. |
| `git diff --check` | PASS | Whitespace/error check before commit. |
| `CLEAN_TEST_IMAGE_BUILD` | PASS | New isolated tag `aicad-scc-ui:g7b-final-test`; pinned production bases and locked Rust dependencies; existing `0.4.0` tag not replaced. |
| `BROWSER_ACCEPTANCE` | PASS | Real Chromium, fake Provider, private network, read-only product/test containers. Root `/` navigated to `/engineering.html`; `zh-CN`; no legacy SCC editor; three Sketch sections created through Engineering Intent; Front/Top/Left; A-A; SVG, DXF, and STEP downloads; exact downloaded bytes matched successful response bytes and SHA-256; invalid Loft returned 422/code/no artifact. Screenshot: [aicad-g7b-engineering-workbench.png](aicad-g7b-engineering-workbench.png), SHA-256 `6b3f4e1dfbb8790cff2050dd37eb3884e7853c9af2dc559cc06af19ab0bac624`. |
| `REAL_DEEPSEEK_TEST` | NOT_RUN | No real Provider call was made; no key was read, printed, changed, or stored. |

Final Chromium STEP response: 13,075 bytes, SHA-256 `1b9bf77d6e816849e09aa5f5fe78a6ead16868c4a7657cced786317515478fd7`. The browser test compared its exact response body to the downloaded file and matched the server’s validation hash and stage headers.

## Remaining Functional Limits

- Inner-loop and hollow Loft are deferred; the validator rejects these rather than dropping inner loops.
- Only aligned ruled transitions between compatible, single-loop analytic profiles on parallel co-oriented frames are supported. Smooth continuity, arbitrary section twist, guide curves, and non-parallel workplanes are not supported.
- Semantic topology selection on all Loft-generated edges/faces is partial. No promise is made that a Loft side edge can be a valid Fillet/Chamfer target.
- Three-view drawing does not separately dimension every Loft station. Station-specific internal sizes require additional formal Drawing views/evidence.
- Existing non-G7B limits remain: G7A Sweep semantic selectors, the previously recorded G6B Countersink/STEP and annotation gaps, arbitrary 3D pattern support, and unresolved AICAD-COMPLEX-001 C1 target ambiguity. G7B does not claim these as solved or claim complete support for the complex workpiece.
- A four-slot STEP timeout concern previously recorded outside the G7B scope remains an independent issue; this phase did not claim it resolved.

## Git Delivery

The audited G7B paths are committed locally by explicit path list; the final commit and HEAD are recorded in the delivery response. No push is performed. The user-owned `docs/testing/mmf204-real-user-acceptance/` directory remains unstaged and untouched.
