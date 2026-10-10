# ADR-068: General DatumAxis and Revolve

- Status: Accepted for Phase G4
- Decision owners: SCC architecture
- Scope: generic DatumAxis and Revolve features in the EngineeringModel v2 ordered rebuild history

## Context

ADR-064 through ADR-067 define one editable EngineeringModel v2 history, a stateless exact CadQuery/OCP rebuild, machining features, semantic face/edge references, and the derived Spatial → Projection → Visibility → Drawing → SVG/DXF pipeline. The formal Manufacturing → SolidBuildSpec → STEP chain is a separate authority and does not accept parametric histories.

G4 adds a general axis-based material operation. It must reuse the existing Sketch/Profile and coexist in the same history with Extrude, Hole, Pattern, Fillet, and Chamfer. G2 already stores vector axes for Hole and Pattern; changing those serialized contracts would break existing EngineeringModel v2 data.

## Decision

### 1. Authoritative model and typed features

- `EngineeringModel` remains the only editable engineering truth. DatumAxis definitions, AxisReference, Revolve dimensions, and stable Sketch entity IDs are typed data. Runtime axes, wires, OCP Shapes, Spatial evidence, and drawing exports remain derived and invocation-local.
- `DatumAxis` supports principal X/Y/Z axes and an axis through two finite, distinct points. Two-point directions are normalized deterministically with scale-safe arithmetic. Invalid or unresolved axes fail closed.
- `Revolve` references an earlier existing Sketch and an earlier `AxisReference::DatumAxis`. It has generic ADD/CUT semantics, `full_360` or a partial angle strictly between 0 and 360 degrees, and explicit forward/reverse orientation. No shaft-, flange-, bushing-, pulley-, or groove-specific feature was introduced.
- Existing G2 Hole and Pattern vector fields remain unchanged. CircularPattern angles now use a deterministic local basis perpendicular to the declared axis; the established global XY basis remains the convention for a Z axis.

### 2. Rebuild and geometry validation

- The transient strict Rust→CadQuery/OCP contract is versioned as schema 3. The lowerer resolves DatumAxis to a normalized origin/direction and transports the existing typed Sketch loops and entity IDs. It does not accept kernel code, persistent Shapes, or topology ordinals.
- Before calling OCP, the worker validates finite closed profile wires, nonzero face area, axis/profile-plane coplanarity, and exact line/circle/arc radial extrema. Profiles that cross the rotation axis fail with `REVOLVE_PROFILE_CROSSES_AXIS`; invalid axes, angles, missing references, and kernel geometry errors keep stable codes.
- The worker executes true `Solid.revolve` geometry. The first ADD can create the body; later ADDs fuse into one valid connected solid or fail with `DETACHED_ADD`. CUT requires an existing body and must remove material without deleting the complete body. Partial sweeps preserve their declared orientation.
- `Shape.clean()` may merge coplanar boolean faces after operation-level provenance has been recorded. Any final stable geometry fingerprint that has no prior source is attributed to the final active operation. Existing provenance is retained; no OCP index is used.

### 3. Semantic topology and drawing

- `RevolveProfileVertex` identifies circular/arc edges through a stable Sketch entity ID and endpoint. It is re-resolved against the source operation and current rebuild prefix on each run. Missing, ambiguous, stale, or duplicate matches fail closed.
- Exact Cylinder, Cone, Plane, and Torus evidence continues through the existing Spatial model. The drawing summary exposes a cylindrical-face count for review. Front, Top, and Left are generated through formal projection and visibility gates, not from a kernel screenshot or a standalone 2D section.
- Exact analytic projected edges are kept where available. Derived projected curves carry explicit positive tolerances. Current non-linear HLR spline restoration is bounded at 1e-4 mm and does not alter EngineeringModel dimensions or the OCP body.
- SVG and DXF are derived independently from the current validated DrawingModel. No parametric STEP endpoint, SolidBuildSpec, project persistence contract, or formal STEP route is changed.

### 4. Skills and natural-language tools

- The existing six CAD Skills and provider-neutral Rust ToolRegistry expose generic `create_sketch`, `create_datum_axis`, and `create_revolve` workflows. Skills describe axis, profile, angle, and boolean validation without hardcoding part names as geometry operations.
- G4 continues to use Engineering Intent. It does not add a second CAD document, route, or Provider configuration, and the acceptance flow uses an isolated fake Provider.

## Rejected alternatives

- Persisting OCP axes, faces, or edges would make model identity depend on a transient kernel run.
- Creating dedicated shaft/flange/groove features would split generic modeling into part-specific modes.
- Converting Revolve output to an editable drawing or parametric STEP would bypass the existing source-of-truth and formal manufacturing boundaries.
- Replacing G2 Hole/Pattern axis fields with DatumAxis references in place would break existing v2 model data.

## Verification contract

The G4 completion report records the actual CadQuery/OCP, Rust→CadQuery/STEP, Rust workspace, Node, fake-provider Playwright, and Compose results. The generic fixtures check full and partial revolve, hollow and stepped profiles, ADD/CUT, axis errors, deterministic edits, suppression, dependency/reorder guards, semantic Fillet/Chamfer, mixed Extrude/Hole/Pattern histories, three views, and SVG/DXF.

General complex-part capability, exact Countersink, explicit-placement patterns, Sweep, Loft, and parametric STEP remain outside this phase. Passing the bounded generic fixtures does not establish support for arbitrary complex mechanical parts.


## Completion evidence (2026-09-26)

- CadQuery/OCP suite: 99/99 passed in the isolated `aicad-scc-ui:g4-test` image. Rust→CadQuery/OCP plus formal STEP integration: 25/25 passed; combined workspace Rust tests: 1141 passed, 0 failed, 2 ignored. `cargo check`, strict Clippy, Rustfmt, Node, Compose, and diff checks passed.
- UI Node suite: 155 passed, 0 failed, 12 host-gated skips. The real OCP/STEP integration skipped on the host was separately run and passed in the isolated container. Existing Sketch serde remains compatible; unknown tool profile fields are rejected at the `create_sketch` boundary, and two-point axis difference overflow fails early with `INVALID_DATUM_AXIS`.
- Playwright acceptance passed using the isolated fake Provider. It verified `/`, `/engineering.html`, one Simplified Chinese Engineering Workbench, a typed Sketch + two-point DatumAxis + full Revolve ADD + Hole + CircularPattern history, formal Front/Top/Left drawing, SVG and DXF. Screenshot and full matrix: `docs/testing/phase-g4-browser/acceptance-report.md`.
- Five-run median OCP operation times were 1.565 ms (simple full revolve), 2.740 ms (stepped), 4.017 ms (hollow), and 6.708 ms for Revolve CUT. Final mixed-history worker rebuild was 1616.308 ms; Projection/Visibility 0.541 ms, three-view generation 1.028 ms, SVG 0.411 ms, DXF 0.272 ms on this test host.
- Real DeepSeek/API call: NOT_RUN. Countersink, explicit-placement patterns, Sweep, Loft, and parametric STEP remain deferred. Passing the bounded fixtures does not establish arbitrary complex-part support.
