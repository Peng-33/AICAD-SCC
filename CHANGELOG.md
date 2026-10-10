# Changelog

## [0.4.0-alpha.1] - 2026-10-10

Public alpha taken from private baseline `0568fac256178ab9b344995c1eab91861c292a7d`. The original source is Apache-2.0. Evidence is in `docs/release/TEST_REPORT.md`. Historical acceptance reports were not all re-run. The gated G7C Sweep and Loft cases pass inside their narrow domains. A sweep hole that consumes the end rim still fails closed. The Docker image is not part of this release.

### Changed

- Aligned the 18 Rust crates, `Cargo.lock` path packages, `@scc/ui`, the e2e package, the production image tag, and `scripts/smoke-test.py` on `0.4.0-alpha.1`.
- Set crate and package license fields to the intended Apache-2.0 identifier. The copyright holder is not named. See `NOTICE`.
- Replaced the root README with an English document and added `README.zh-CN.md`.
- Corrected the Workbench README: bounded Revolve, fixed-binormal Sweep, ruled aligned Loft, and formal v2 STEP exist in source. The gated G7C Sweep and Loft cases pass inside that narrow domain. A sweep hole the same diameter as the tube end fails closed. This is not a selector for every generated face.
- Completed the ADR index through ADR-076 without rewriting historical ADR bodies.
- Removed internal agent notes from this snapshot. The private backup repository was not modified.
- Formal Section A-A on a non-parametric model now fails closed as `SCC-DRW-024`, with `details.parametric_code` `SECTION_PARAMETRIC_REBUILD_REQUIRED`. A missing OCP section is `SCC-DRW-025` / `SECTION_GEOMETRY_MISSING`. The previous raw semantic codes violated the CLI error contract and were rewritten as `SCC-CLI-009`.

### Unchanged on purpose

- Historical acceptance text that recorded `scc 0.3.0` or image tag `0.4.0`.
- Test-only image names `g3-test` and `g4-test`.
- Manufacturing equivalence checks. Nothing in this version bump relaxes them.

- Phase 13.5 Manufacturing Domain Closure Re-Audit: **PASS_WITH_NOTES / FROZEN** for the declared Rectangle/Circle/RoundedRectangle/Polygon/Composite/Hole/Slot domain. All formal STEP delivery remains on the canonical EngineeringModel → SpatialModel → SolidBuildSpec → CadQuery/OCP → reimport/equivalence → validated-byte route; compatibility CadDocument/FeatureGraph remains non-STEP only. See [ADR-062](docs/adr/adr-062-manufacturing-domain-closure.md) and [the closure re-audit](docs/specs/phase13.5-manufacturing-domain-closure-reaudit.md). Product-wide editing states remain separate and Phase 14 is not started.

- Phase 13.5A Canonical STEP Authority Consolidation (ADR-063): selected Strategy A and retired formal STEP generation/download from the CadDocument/FeatureGraph compatibility route. Non-STEP compatibility editing, BREP/STL/GLB/SVG/source JSON, preview, and coverage remain available; formal STEP remains exclusively on the EngineeringModel → SpatialModel → SolidBuildSpec → validated-byte route. See the [ADR](docs/adr/adr-063-canonical-step-authority-consolidation.md) and [closure audit](docs/specs/phase13.5a-canonical-step-authority-closure-audit.md).
- Phase 13.5 Manufacturing Domain Closure Audit (pre-13.5A): **INCOMPLETE / DO NOT FREEZE** because the public CadDocument/FeatureGraph compatibility route was a second mutable CAD source and reachable STEP path. The Phase 13.5A remediation is implemented; Phase 13.5 must be re-audited separately. See the [historical audit record](docs/specs/phase13.5-manufacturing-domain-closure-audit.md).
- Phase 13.4 Slot manufacturing and STEP support (ADR-061): Engineering/Spatial-owned typed Slot semantics lower to a typed `SlotCut` carrying the exact resolved stadium contour as generic `ClosedProfileSpec`; CadQuery reuses the exact Line/Arc profile face builder for B-Rep subtraction, then STEP reimport/equivalence validates the artifact before returning those exact bytes. Translation, arbitrary orientation, Z, provenance, and coexistence with typed cylindrical Hole cuts are covered. No Slot-specific geometry reconstruction, tessellation, bbox substitution, or Slot pattern support. See the [Phase 13.4 closure audit](docs/specs/phase13.4-slot-manufacturing-closure-audit.md).

All notable changes to SCC are documented in this file.

## [Unreleased]

### Added

- Phase 13.3C Composite manufacturing and STEP support (ADR-060): typed Engineering/Spatial Composite identity lowers to the existing generic exact Line/Arc `ClosedProfile`; segment order, explicit CW/CCW, major arcs, translation, legal concavity, and source provenance are preserved without tessellation, bbox substitution, or a Composite-specific DTO/backend. Typed cylindrical Hole cuts and upstream-resolved pattern cuts are reused; STEP is reimported and equivalence-validated, and the delivered artifact is unchanged after validation. Full-circle and exactly-180-degree ambiguous arcs remain forbidden; Slot remains fail-closed. See the [Phase 13.3C closure audit](docs/specs/phase13.3c-composite-manufacturing-closure-audit.md).

- Phase 13.2 RoundedRectangle manufacturing and STEP support (ADR-056): typed RoundedRectangle lowering through SpatialModel and SolidBuildSpec, exact CadQuery rounded extrusion with cylindrical-cut reuse, STEP export/reimport validation, and validated artifact delivery. At Phase 13.2, Composite, Polygon, and Slot were still deferred; Polygon and Composite are enabled in Phases 13.3B/13.3C, while Slot remains fail-closed.

- Phase 13.1 circular manufacturing and STEP support (ADR-055): typed Circle SolidBuildSpec lowering, exact CadQuery circle extrusion, cylindrical-cut support, STEP export/reimport equivalence validation, and validated artifact delivery. At Phase 13.1, non-circular manufacturing profiles were still deferred; RoundedRectangle, Polygon, and Composite are enabled by later phases, while Slot remains fail-closed.

- Phase 12.4 Polygon closure (ADR-054): typed canonical Polygon with implicit closure, exact Spatial/Projection/Visibility/Drawing pipeline, `BASE_POLYGON` DSL and `create_base_polygon` tool support, source-only project persistence, and deterministic goldens. Polygon manufacturing and STEP remain fail-closed deferred.

- Phase 12.3 slot / oblong through-cut feature semantics (ADR-053): one typed `EngineeringFeature::Slot` with canonical overall length, width, center, and `[0°, 180°)` orientation; exact Line+Arc lowering; exact spatial `SlotCut` footprint; Front `SlotSegment` provenance; Top/Left hidden silhouettes; typed `SLOT L × W THRU` callout on the Phase 11 layout authority; `SLOT` DSL and `add_slot` tool; source-only project persistence; and `phase123_slot_horizontal` / `phase123_slot_rotated` goldens. Slot manufacturing is fail-closed deferred; rectangle/circle/rounded/composite behavior remains unchanged.

- Phase 12.2 arc, rounded, and composite 2D profile foundation (ADR-052): typed `BaseProfile::RoundedRectangle` and one-loop Line/Arc `Composite`, exact spatial footprints, Front `BodySegment` provenance, native SVG/DXF arcs, Width/Height/`R10` on the existing Phase 11 layout authority, source-only project persistence, and `phase122_*` goldens. Rounded/Composite manufacturing is explicitly deferred; rectangle/circle behavior remains unchanged.

- Phase 12.1 circular base profile and circular-part three-view support (ADR-051): typed `BaseProfile::Circle` with parameterized center/diameter, exact circular spatial provenance, true Front/Top/Left projection and visibility semantics, body `Ø` annotations, `BASE_CIRCLE` DSL, `create_base_circle` tool, source-only Project v1 persistence, and deterministic disk/flange SVG/DXF goldens. Circular manufacturing is explicitly deferred in `SolidBuildSpec` v1; rectangle behavior remains unchanged.

- Phase 11.5 dimensional tolerance and precision semantics (ADR-050): EngineeringModel-owned typed symmetric/bilateral tolerances, fixed decimal precision 0–4, zero canonicalization, width/height/thickness/hole-location/pattern-spacing/PCD/blind-depth coverage, shared SVG/DXF `AICAD_INTERNAL_V1` formatting, Project v1 persistence, fail-closed validation, and separate tolerance goldens; geometry, SolidBuildSpec, CadQuery, and STEP remain nominal.

- Phase 11.4 blind-hole depth and engineering hole-callout semantics (ADR-049): typed `HoleCalloutSemantic` values for direct and Circular/Grid/Corner pattern callouts, `THRU`/`↧D BASE`/`↧D OPP` formatting, resolved SpatialModel Z witnesses, typed pattern quantities, hidden-circle witness support without projection inference, SVG/DXF parity, and fail-closed side/depth/radius/cardinality/layout validation; Phase 11.2 remains the sole layout authority and AicadProject/SolidBuildSpec/STEP contracts remain unchanged.

- Phase 11.3 hole location and pattern dimension semantics (ADR-048): typed Front-view hole locations, resolved pattern-center/spacing witnesses, semantic Circular/Grid/Corner quantity/diameter/PCD/offset values, SVG/DXF parity, and fail-closed provenance/radius/cardinality validation; Phase 11.2 remains the sole layout authority.

- Phase 10.6 single-solve snapshot/revision/cache boundary (ADR-045): complete Rust solved snapshots, unified model fingerprints, monotonic session revisions, bounded opaque drawing caches, and no incremental or persistent solving/cache.

- Phase 10.3 STEP delivery validation (ADR-042): build-once SolidBuildSpec-to-CadQuery execution, private temporary `.step` lifecycle, real CadQuery/OCP reimport, bbox/volume/material witnesses, structured atomic errors, semantic determinism, and Rust adapter DTO validation; no STEP truth, UI endpoint, or product download path.
- Phase 10.2 direct stateless CadQuery backend (ADR-041): strict SolidBuildSpec v1 transport validation, real rectangle/cylindrical Boolean execution, bbox/volume/validity witnesses, opaque provenance, structured errors, Rust subprocess adapter, and no STEP/product export.
- Phase 10.1 backend-neutral `scc-manufacturing` crate (ADR-040): derived `SolidBuildSpec` lowering from resolved `SpatialModel`, typed provenance, deterministic serde, fail-closed `SCC-MFG-*` errors, and no CadQuery/Python/solid-kernel execution.
- Phase 9.8 Three-view Workbench integration (ADR-039): additive Principal/Three-view mode, formal Rust CLI/API path, backend-derived metadata, stale-response/error safety, Blob img lifecycle, and independent spatial-ready fixture; legacy Principal behavior remains unchanged.
- Phase 9.7 three-view drawing integration (ADR-038): additive Front/Top/Left `DrawingModel` builder gated by the frozen projection-consistency validator, Visible/Object + Hidden/Hidden + Occluded omission, opaque projected provenance, First/Third-angle sheet placement, scale-invariant paper gap/margin, conditional hidden SVG/DXF presentation, and formal SVG/DXF goldens.
- New `scc-projection` leaf crate (Phase 9.3, ADR-034): deterministic orthographic projection model derived from a `SpatialModel` — three canonical principal views (`Front < Top < Left`, frozen named-axis frames with `right × up = toward_viewer`), projected `Segment`/`Circle` primitives carrying embedded depth ranges (`min == max` legal, larger = nearer viewer), stored-validated per-view bounds, and a `ProjectionSourceMap` tying every primitive back to its spatial element — no matrices/camera, no visibility, no sheet/scale semantics, and no `lower_projection` yet (9.4).
- Fail-closed projection builder: `add_view` (idempotent, frozen frames) and `add_primitive` with deterministic gate order (view → geometry → depth → duplicate id → source integrity; zero-length segments exact `==`, non-finite via `is_finite`, unknown cut ids rejected at insert time), deterministic gapless `next_id` (`{view}:{source}:p:{index:04}`), `finish()` bounds recomputation, and the post-build/post-deserialize gates `validate_integrity()` / `validate_against()` — 6 structured `SCC-PROJ-001…006` codes under the new `ErrorStage::Projection` (docs/error-codes.md).
- Deterministic `BTreeMap`-ordered views/primitives/source map, canonical `PrincipalViewKind` Ord (`Front < Top < Left` contractual), byte-stable serialization locked by a 100-run pretty-JSON determinism test, and provenance reverse queries derived on demand (never stored) — 40 unit + 21 integration tests in `crates/scc-projection`.
- ADR-034 recording the orthographic projection boundary (frame table, `Segment`/`Circle`-only geometry with a frozen additive `Arc` path, separate provenance map, two-gate validation, `ProjectionMethod` never read — first/third angle is sheet placement, not per-view geometry).
- New `scc-spatial` leaf crate (Phase 9.2, ADR-033): deterministic 2.5-D spatial lowering from a spatial-ready `EngineeringModel` + solved `GeometryModel` + `EngineeringSourceMap` into a derived, read-only `SpatialModel` — base body (`solved rectangle footprint × [0, thickness]`), witnessed cylindrical cuts (`ThroughAll` = `[0,T]`, blind base = `[0,D]`, blind opposite = `[T−D,T]`), construction circles recorded reference-only, and bounds derived from the body footprint only.
- Fail-closed spatial lowering integrity: Phase 9.1 readiness gate runs first (issues carried verbatim); then pattern cardinality cross-checks, 1 nm diameter witnesses, claim integrity (every solved entity claimed exactly once), foreign/missing provenance rejection, non-finite/defensive depth gates, and a defensive diameter gate for unresolved bound diameters — 16 structured `SCC-SPAT-001…016` codes under the new `ErrorStage::Spatial` (docs/error-codes.md).
- Deterministic `SpatialCutId` allocation (`{feature_id}:cut:{index:04}`), `BTreeMap`-ordered collections, byte-stable serialization locked by a 100-run end-to-end determinism test, and a construction-circle inverse-provenance cross-check (double claims fail closed) — 48 integration + 23 unit tests in `crates/scc-spatial`.
- ADR-033 recording the 2.5-D representation boundary (no B-Rep, no mesh, no boolean kernel, no projection semantics) and the leaf-crate dependency direction (`scc-engineering` gains no Phase 9.2 changes).
- New `scc engineering drawing` CLI subcommand: solved model JSON in, one `{svg, sheet_width_mm, sheet_height_mm, scale, part_title}` JSON document out via `scc-drawing` (Phase 8 renderer).
- Engineering workbench "Engineering Drawing" preview panel fed by `/api/engineering/drawing`, with stale-request guard, structured error banner (`SCC-DRW-*`), Blob/objectURL lifecycle discipline, and the former 2D view renamed to "Geometry Debug" (ADR-031).
- CSP fix: `script-src`/`style-src` now whitelist the sha256 hashes of engineering.html's inline script and style (previously silently blocked: page never rendered / unstyled); regression-guarded by `test/csp-inline-scripts.test.mjs`.
- Fixed the Phase 7 legacy workbench redo no-op (`createModelHistory` sentinel handling) and added unit tests for the full undo/redo/baseline/truncation behavior.

- New `scc-geometry` M3 crate with a read-only `GraphState` boundary, deterministic `GeometryModel`, and an explicit solver support matrix.
- New `scc solve` command for legacy or schema-1.x GraphState input and legacy or enveloped GeometryModel output.
- New `geometry` structured error stage with stable `SCC-GEOM-001` through `SCC-GEOM-007` codes and CLI exit code `4`.
- Initial deterministic construction for Point, Line, Circle, Arc, and Rectangle primitives with Fixed-origin handling.
- Two-axis `CenterOf` solving with deterministic dependency resolution, cycle detection, and conflicting-position diagnostics.
- Preserved anchor roles and formal coordinate conventions through ADR-005.
- End-to-end centered plate/hole fixture covering `compile → execute → solve`.
- Point-to-Point `Distance` verification for independently positioned points, plus `SCC-GEOM-007` for underconstrained distance components.
- Optional deterministic structured-error `details` for stage-specific machine context; `SCC-GEOM-007` uses it for `remaining_degrees_of_freedom`.
- Machine-readable `solve_report` with fully constrained status, direct-method convergence, evaluated-constraint count, and remaining degrees of freedom.
- ADR-006 defining distance direction semantics and degrees-of-freedom reporting.
- ADR-007 and per-axis position resolution for partial-axis `CenterOf`, including explicit-axis composition and selected-axis conflict detection.
- Partial-center Proposal and mutation fixtures covering underconstrained and fully constrained CLI pipelines.
- ADR-008 and deterministic Line-to-Line `Align` validation for collinear explicit lines, including anti-parallel support and stable conflict diagnostics.
- Aligned-line Proposal and mutation fixtures covering successful and conflicting `compile → execute → solve` pipelines.
- ADR-009 and deterministic Line-to-Line `Parallel` validation for explicit lines, including offset and anti-parallel support.
- Parallel-line Proposal and mutation fixtures covering successful and conflicting `compile → execute → solve` pipelines.
- ADR-010 and deterministic Line-to-Line `Perpendicular` validation using normalized direction dot products.
- Perpendicular-line Proposal and mutation fixtures covering successful and conflicting `compile → execute → solve` pipelines.
- End-to-end anchored-point fixture covering `compile → execute → solve` and verifying that underconstrained input emits no GeometryModel.
- Deterministic Polygon construction from ordered `vertex_count` and `x_N`/`y_N` RuntimeEntity parameters.
- Deterministic linear, grid, and circular array expansion with stable `array_id[index]` identities.
- New `scc export` command with byte-stable SVG and ASCII DXF AC1027 output.
- ADR-011 defining Polygon validation, array instance semantics, unitless world coordinates, and the supported CAD subsets.
- Release-mode M3 performance and convergence baseline harness for 10,000-entity models.
- End-to-end Polygon and array fixtures covering `compile → execute → solve`.
- Local v0.3.0 acceptance report with test, release-build, smoke-test, determinism, and performance evidence.

### Fixed

- SVG drawing renderer: dimension and annotation `<text>` elements now carry explicit `stroke="none"` paint isolation, so 3.5 mm text is no longer overstroked by the dimensions group-level 1 mm stroke (visual ink 0.1134 → 0.0422); golden SVG rebuilt (`2f428276…` → `cadfcc70…`), DXF golden unchanged (Phase 8.10.1).

### Changed

- Workspace crate versions advanced to `0.3.0` for M3 development.
- Runtime constraints now preserve numeric ConstraintIR parameters in GraphState; empty maps remain omitted for backward-compatible JSON.
- Lowered dimensions retain both the generic `value` and a semantic key (`diameter`, `radius`, `width`, `height`, `length`, or `offset`).
- M2 execution emits deterministic `SetParameter` commands for self-Distance dimension carriers so M3 can consume dimensions from GraphState only.
- Centered-axis markers and optional anchor roles now survive ConstraintIR → RuntimeCommand → GraphState; absent roles remain backward-compatible.
- Nonzero `Distance` no longer invents positive-X placement; only zero-distance coincidence can propagate a resolved point position.
- `aligned_with` analysis now requires Line entities, matching the supported M3 geometry contract.
- Array pattern parameters now survive SemanticIR → ConstraintIR → GraphState; deterministic roots and affected Golden fixtures are updated accordingly.
- Array-derived ID collision checks use deterministic ordered-set membership instead of quadratic scans.
- Arc construction now rejects zero-span and full-turn angle pairs; full turns use Circle geometry.

## [0.2.0] - 2026-07-23

SCC v0.2.0 contract-governance release candidate.

### Added

- `SCC-{STAGE}-{NNN}` structured error contract across all crates, with stages `PROP`, `ANAL`, `LOW`, `EXEC`, `CLI` and `StructuredError::validate_contract()` enforcement.
- ADR-003 schema envelope with opt-in `--schema-version 1.0`; legacy unversioned JSON remains the default for backward compatibility.
- New CLI subcommands: `scc validate` and `scc inspect`.
- New CLI flags: `--json-errors`, `--quiet`, `--fail-on-conflict`, and `--schema-version`.
- Stable CLI exit codes: `0` success, `1` input/CLI/proposal/analysis/lowering errors, `2` business conflicts with `--fail-on-conflict`, `3` execution-stage errors.
- Atomic output writing for `compile`/`execute` with overwrite protection via `--force` and best-effort two-phase commit/rollback for trace/state pairs.
- Expanded test coverage: schema envelope integration, exit-code behavior, validate/inspect, structured error contract, and runtime error codes.
- Batch-preserving structured diagnostics through `SccError::StructuredBatch`; CLI JSON output no longer truncates multi-error analysis or validation results.
- `SCC-CLI-010` for machine-readable command-line argument failures.
- `ErrorStage::code_prefix()` (`const fn`) mapping Proposal/Analysis/Lowering/Execution/Cli to `PROP`/`ANAL`/`LOW`/`EXEC`/`CLI`.
- `ErrorContractViolation` enum (`EmptyCode`, `EmptyMessage`, `InvalidCodeFormat`, `StageMismatch`, `InvalidSource`) and `StructuredError::validate_contract()` for ADR-002 contract enforcement (no regex dependency; recursive `source` validation).
- `docs/error-codes.md` registry for stable `SCC-{STAGE}-{NNN}` codes (one code per problem; no duplicate assignment).

### Changed

- Workspace crates bumped from `0.1.0` to `0.2.0`.
- `SccError` extended with `Structured` and `StructuredBatch` variants for rich, machine-readable diagnostics.
- `scc compile` now uses `analyze_detailed`; unsafe semantic input returns `SCC-ANAL-*` with exit code `1`, while genuine MCS contradictions remain business ConflictReports.
- Execution implementation modules are crate-private; the supported public entry point remains `driver::execute`, with `RuntimeOutput` re-exported at the crate root.
- `docs/error-codes.md` registry synchronized with new execution, proposal, and analysis codes.
- `README.md` updated to document `validate`, `inspect`, envelope usage, and exit codes.
- **Compatibility (source risk):** `SccError` gains a `Structured` variant. Old variants and constructors remain. Callers that exhaustively `match` on `SccError` must add a `Structured` arm. This is **not** full source compatibility; v0.1.0 tag is unaffected.
- ADR-002 JSON examples use lowercase `stage` (`proposal`, `analysis`, `lowering`, `execution`, `cli`) and lowercase `severity` (`error`, `warning`) only — never PascalCase `Cli`/`Error`.

### BREAKING CHANGES

- `SccError::Structured` new variant — callers that exhaustively `match` on `SccError` must add a `Structured` arm.
- `SccError::StructuredBatch` new variant — batch-preserving diagnostics replace the previous single-error degradation behavior for multi-error analysis, lowering, and validation results; callers that exhaustively `match` on `SccError` must add a `StructuredBatch` arm.
- `RuntimeOutput` path migrated from `scc_execution::runtime::execution_runtime::RuntimeOutput` to `scc_execution::RuntimeOutput` (re-exported at crate root).
- Six internal modules made crate-private (no longer public API): `execution_plan`, `ir_patcher`, `mutation`, `overlay_canonicalizer`, `plan_generator`, `runtime`.
- `MutationProposal` and `Decision::Skip` removed (no remaining callers in workspace).

### Fixed

- `scc validate` now exits with code `1` for invalid proposals instead of `0`.
- `--schema-version` no longer silently accepts unsupported values; it returns `SCC-CLI-006`.
- Any top-level Envelope marker now requires a complete, valid Envelope, preventing partial version metadata from being mis-routed as legacy payload.
- `LoweringError::NonFiniteParameter` JSON path now uses the correct `constraint_id` segment.
- `Outside` relations with `ConstraintKind::On` at `AuthorityLevel::L2` are now accepted by the lowering validator.
- Execution runtime and mutation validation errors use dedicated codes (`SCC-EXEC-009/010/011/012`) instead of overloading `SCC-EXEC-008`.
- L0 immutability enforced for `UnsuppressConstraint` mutations.
- Empty proposal/analysis error fallbacks use semantically matched codes (`SCC-PROP-009`, `SCC-ANAL-007`).
- Duplicate entity-ID detection in proposal validation is now deterministic (`BTreeSet`).
- Production `expect` in `scc-analysis` replaced with a structured `AnalysisError::InvalidDeclarationGraph`.
- Schema versions now require canonical two-part numeric `MAJOR.MINOR` syntax and the supported major; partial envelopes cannot fall back to legacy parsing.
- Analysis type-error JSON paths retain original Proposal relation indices, including when unsupported relations are filtered from Semantic IR.
- `--json-errors --fail-on-conflict` now emits `SCC-CLI-009` on stderr with clean stdout, and CLI syntax failures emit `SCC-CLI-010`.
- Runtime overlay application independently enforces L0 immutability, no L0 promotion, and non-negative mutation parameters.
- Atomic pair writes preflight no-clobber conditions, avoid loading backups for single-file writes, and atomically restore the first file if the second publish fails.
- CI path filters now include `examples/**` and `scripts/**`, ensuring fixture and smoke-test changes trigger validation.

## [0.1.0] - 2026-07-12

Official M1/M2 release (`b801306fcdcfd5be67bbb576c33d19f7a227604f`, SCC Release Run [29193133514](https://github.com/Peng-33/AICAD/actions/runs/29193133514)).

### Added

- Root project guide with workspace, CLI, validation, and release instructions.
- Pinned Rust toolchain (`1.96.0`) for reproducible local and CI builds.
- Windows/Linux CI matrix for tests, Clippy, Rustfmt, and coverage artifacts.
- Cross-platform release workflow for tags matching `v*`, with SemVer validation.
- CLI `--version` support and an end-to-end version regression test.
- Atomic file output for `compile` and `execute` via `NamedTempFile`.
- Global `--force` CLI flag to allow overwriting existing output files.
- Best-effort two-phase commit with rollback for Trace/State output pairs.
- ADR-002 documenting the structured error model for `v0.2.0`.
- ADR-003 documenting the Schema Envelope versioning strategy for `v0.2.0`.

### Changed

- CI and release workflows pin all third-party Actions to full commit SHAs.
- Release workflow verifies that the Git Tag, Cargo version, and `scc --version` agree before building.
- Release workflow generates SHA-256 checksums for the published archives.
- Release packages include `README.md`, `CHANGELOG.md`, `Cargo.lock`, and a `BUILDINFO` file.
- M2 release-readiness task documents now reflect the solver-free runtime architecture.
- Local verification baseline updated to the automated test suite passing on Windows and Linux CI.

### Fixed

- Runtime integrity validation no longer rejects valid unconstrained entities.
- Stale task documentation describing removed M2 solver behavior has been removed.
- CLI output no longer overwrites existing files unless `--force` is supplied.
- Failed CLI writes no longer leave partially written or truncated output files.

### Known limitations

- Trace/State are committed as a best-effort two-file transaction, not a strict atomic transaction.
- Windows does not provide a stable std API for syncing the parent directory after rename.
- Rollback restores file contents but does not preserve original permissions or timestamps.
- Release artifacts include Cargo metadata, not a formal SPDX/CycloneDX SBOM.
- Action SHA pinning is currently manual; an automated update mechanism is planned.

## Phase 10.5 — Natural-language clarification and change preview

- Added typed `Ask → Clarify → Preview → Confirm → Commit` workflow over the formal EngineeringCommand boundary.
- Added structured non-mutating ChangePreview with source/resolved parameter values, feature classification, diagnostics, downstream readiness, and stale fingerprint checks.
- Added confirmation replay, race/security tests, provider clarification JSON parsing, and CSP-safe formal clarification UI.
- Added ADR-044 and capability-matrix truth for `nl_intent`, `clarification`, and `change_preview`; Fillet and Slot remain unsupported.

## Phase 10.4 — Project persistence and validated STEP delivery

- Added `AicadProject v1` source-only persistence through the `scc-project` crate and CLI validation.
- Added real Rust-to-CadQuery/OCP STEP artifact transport with exact validated-byte identity at the HTTP endpoint.
- Added Workbench New/Open/Save/Export STEP controls, dirty fingerprints, and stale-export generation protection.
