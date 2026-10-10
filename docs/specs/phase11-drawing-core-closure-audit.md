# Phase 11 Drawing Core Closure Audit

- Date: 2026-09-14
- Task type: AUDIT
- Scope: Phase 11.1–11.5 formal Front/Top/Left drawing core
- Decision: PASS_WITH_NOTES
- Freeze: FREEZE

## Scope and boundary

This audit closes the Phase 11 drawing core. It does not add a new drawing
feature and does not start Phase 12.1. The audited chain is:

EngineeringModel -> solved Spatial/Projection/Visibility artifacts -> derived DrawingModel -> SVG/DXF

EngineeringModel remains the source of dimensional requirements. Spatial and
projection artifacts remain resolved witnesses. DrawingModel owns only
validated derived annotation semantics and presentation geometry. SVG and DXF
consume the same DrawingModel; neither renderer re-evaluates engineering
parameters or invents drawing semantics.

## Closure findings

### 1. Annotation ownership and coverage — PASS

Phase 11.1 establishes Front/Top/Left ownership for overall width, overall
height, thickness, hole diameter, center marks, and centerlines. Phase 11.3
adds typed hole-location and pattern semantics for direct, Circular, Grid, and
Corner holes. Phase 11.4 adds typed ThroughAll and Blind/Base or Blind/Opp
callouts, with depth taken from resolved Z intervals and side retained from
typed engineering intent. Phase 11.5 adds typed tolerance and precision for
linear dimensions, diameter, location, spacing, PCD, and blind depth.

All of these remain derived annotations with typed provenance. IDs are used as
identity only; semantic values are not recovered by parsing strings.

### 2. Layout authority and fail-closed behavior — PASS

annotation_layout::optimize_three_view_layouts is the only formal three-view
collision/reflow authority. Linear dimensions may reflow outward with bounded,
deterministic stacking. Witness-attached diameter, pattern, and hole-callout
geometry is not detached; unresolved crowded layouts fail closed. The layout
pass uses the actual formatted text envelope, including Phase 11.5 tolerance
text.

### 3. Renderer and view invariants — PASS

SVG and DXF share the same derived dimensions, formatter, center geometry,
provenance, and sheet bounds. Drawing scale and First/Third-angle projection
change placement only; measured values and formatted tolerance text remain
invariant. Legacy Principal output and the historical Phase 9 three-view
goldens remain available for byte-level regression comparison.

### 4. Persistence and downstream boundaries — PASS

Project v1 persists source EngineeringModel data, including typed dimensional
specifications, but not DrawingModel, formatted strings, HoleCalloutSemantic,
SpatialModel, SolidBuildSpec, CadQuery objects, or STEP artifacts. The
SolidBuildSpec golden and manufacturing/CadQuery/STEP contracts remain nominal
and unchanged by tolerances or drawing presentation.

## Verification evidence

The current worktree was checked without reset, clean, checkout, stash, commit,
add, or push operations.

| Gate | Result |
| --- | --- |
| cargo test workspace all-targets offline locked no-fail-fast | exit 0 |
| scc-drawing three-view integration | 33 passed |
| scc-drawing drawing integration | 131 passed |
| scc-engineering unit/back-compat tests | 194 + 2 passed |
| scc-project tests | 5 passed |
| strict workspace Clippy (-D warnings) | exit 0 |
| cargo fmt all check | exit 0 |
| git diff check | exit 0; existing .gitignore CRLF warning only |
| Docker CadQuery/OCCT discovery | 31 passed |
| Docker Rust -> real CadQuery/STEP integration | 1 passed |
| Compose configuration | exit 0 |

Frozen artifact hashes remain:

- historical Phase 9 three-view SVG: 97360a8c696cb1f70d153e18fb2a12c3e5a13a06e22e1f7758fb1c58d11525c9
- historical Phase 9 three-view DXF: 52c921adddfad87403aa19b443f3d7313b05a4a69b343d4a0600e35f1cf32fee
- current three-view SVG: e1be2bb05d1e73ab1bb30f01349c386848acd2b5654fdae1d9ac17fdda232cd5
- current three-view DXF: 087f2683953d0cdca642741d42868025a0ec470136be6e5064cbe93ac1807a17
- tolerance SVG: 9409ab06adff5744cf053f12da8755455b8cbde3f9e98c789452dc358708c99f
- tolerance DXF: 659d86a1bdc9d892892ac9d8051930dceb259846e79c51466e303b52984bbb03
- SolidBuildSpec golden: 0198994fc3875850b4e692024529c7a9c86d6daf754c11e02ef1a9504d52e122

## Non-blocking notes

1. Browser visual/E2E coverage was not executed because the current desktop
   visualization/CUA environment cannot start its read-only .git mount.
2. Host Python does not provide CadQuery; real kernel evidence is therefore
   from the reference Docker runtime, which passed the CAD and STEP gates.
3. The repository already contains a large dirty worktree from the preceding
   phases. Exact pre-existing attribution is not recoverable in this audit;
   all changes were preserved.

These notes prevent an unconditional PASS, but do not identify a Phase 11
functional blocker.

## Freeze decision and next gate

Phase 11 drawing core is frozen with notes. Do not add NL tolerance editing,
GD&T/PMI, datums, fits, surface roughness, threads, counterbore/countersink,
hole tables, title blocks, or formal GB/ISO/ASME profiles under this phase.

The next planned feature gate is Phase 12.1 — Circular Base Profile &
Circular-part Three-view Support. It should begin as a separately scoped
phase after review of this audit.
