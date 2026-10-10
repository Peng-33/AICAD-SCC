# Phase 13.5 — Manufacturing Domain Closure Audit

**Status: INCOMPLETE — DO NOT FREEZE**
**Baseline:** 40ab8066175dc0132fda171371f0dca7a9c34f64
**Branch:** codex/aicad-pre-rollback-recovery
**Audit date:** 2026-09-18

## Decision

The canonical Engineering manufacturing route is internally coherent and its required focused, workspace, Docker, CLI/UI, and static verification gates pass. The complete product Manufacturing/STEP domain cannot be frozen: a second, reachable CadDocument / feature_graph_v1 source route still builds and serves STEP artifacts outside the EngineeringModel → SpatialModel → SolidBuildSpec contract and without the same Phase 10.3 geometric-equivalence and exact-validated-byte delivery gate.

This violates the Phase 13.5 strict gates for a singular mutable manufacturing truth and no reachable legacy manufacturing bypass. No source changes were made. No accepted/frozen ADR-062 was created. A narrowly scoped follow-up is recommended below; this audit does not start it.

## Baseline and scope

- Repository: <repo>
- Baseline HEAD and branch matched the user-designated values; the worktree was clean before audit commands.
- Scope: authority chain, profile/operation semantics, SolidBuildSpec serde and provenance, analytic profile transport, CadQuery boundary, active legacy/compatibility paths, STEP validation/delivery, fixtures/adversarial coverage, documentation, and requested verification gates.
- SOURCE_CODE_CHANGED = NO; no test source was added or altered. Generated build output stayed under ignored target/ and Docker runtime output.
- No push, reset, clean, restore, stash, rebase, amend, or Phase 14 work was performed.

## Canonical authority chain

The Engineering path is:

- EngineeringCommand / DSL / tools
- EngineeringModel
- SolvedEngineeringSnapshot / GeometryModel / source map
- SpatialModel
- SolidBuildSpec v1
- Rust CadBackendClient
- stateless Python SolidBuildSpec parser + CadQuery/OCP
- STEP export
- reimport and witness comparison
- Rust artifact identity check
- UI/CLI delivery of those bytes

crates/scc-cli/src/engineering.rs::export_step deserializes only EngineeringModel, solves once, lowers through Spatial and Manufacturing, and invokes the STEP artifact API. /api/engineering/export/step rejects derived fields on source input, delegates to the Rust CLI adapter, and serves the returned artifact. Browser/Node code does not compute geometry truth. AicadProject persists source EngineeringModel and metadata, not SpatialModel, SolidBuildSpec, or STEP caches.

## Freeze blocker: second reachable source and STEP path

This route is separate, live, and user-reachable:

- POST /api/cad/build
- CadDocument 1.0 / feature_graph_v1
- cad-service job + downloadable artifact manifest
- Python FeatureGraph builder
- direct STEP export and artifact download

Evidence:

- apps/scc-ui/server.mjs exposes POST /api/cad/build and GET /api/cad/artifacts/:jobId/:name.
- apps/scc-ui/lib/cad-service.mjs accepts normalized CadDocument, invokes the CAD worker, retains the job, and serves its step artifact.
- apps/scc-ui/cad/aicad_cad/worker.py::build_document dispatches feature_graph_v1 to build_feature_graph and calls cq.exporters.export for STEP.
- apps/scc-ui/cad/aicad_cad/feature_graph.py rebuilds ordered additive/cutting box/cylinder/pattern features from mutable CadDocument parameters/features; its _circular_pattern function computes instance positions and creates cylinders inside the Python CAD builder instead of receiving upstream-resolved cuts.
- The Docker suite passes test_generic_document_exports_step_and_auditable_coverage, confirming this is not dead code.
- docs/capabilities.json::separate_cad_document_backend explicitly records step_export: true, step_reimport: true, imported_bbox_volume_equivalence: false, and engineering_model_bridge: false; its note says it is a separate compatibility backend and not the Phase 10.3 equivalence gate.
- README.md likewise describes it as an active generic 3D CAD foundation and says the two paths do not share editable truth.

This is not a bypass inside the canonical Engineering route, and that route remains correct. It is nevertheless a second mutable CAD input and a second reachable STEP manufacturing/delivery authority within the complete product domain the audit was asked to close. Its Python-side circular-pattern expansion also violates the whole-domain upstream-pattern-expansion gate. The strict whole-domain gates for singular truth, no legacy path, upstream expansion, and uniform STEP validation/delivery therefore fail. Documentation cannot convert this runtime path into a dead path.

Smallest remediation phase: Phase 13.5A — Canonical STEP Authority Consolidation. Decide and implement one explicit product boundary: either retire/disable STEP delivery from the compatibility route, or converge it on the canonical validated delivery/authority contract. Update route tests and capability/status docs, then rerun the Phase 13.5 audit. Do not start this remediation implicitly as part of the audit.

## Canonical manufacturing model audit

| Concern | Observed contract | Result |
|---|---|---|
| Base profiles | Rectangle, Circle, RoundedRectangle are typed footprints; Polygon and Composite preserve upstream identity and lower to one exact ClosedProfile loop | PASS |
| Operations | CylindricalCut and SlotCut; Hole is not inferred from profile geometry, and Slot remains a typed operation | PASS |
| Source of resolved geometry | Manufacturing consumes SpatialModel; lower_solid_build_spec reads no EngineeringModel values | PASS |
| SolidBuildSpec | Serde v1, backend-neutral geometry/operations, resolved Z/bounds, deterministic maps and opaque operation IDs | PASS |
| ClosedProfile | Derived analytic transport only; not an editable sketch system or source of profile identity | PASS |
| Polygon/Composite lowering | Typed Spatial profile discriminator is checked; exact segment order/source identity is checked upstream and preserved | PASS |
| Arc semantics | Explicit CW/CCW, center, radius, endpoints; no tessellation/chord inference | PASS |
| Pattern semantics | Engineering expands hole patterns upstream; manufacturing receives resolved cylindrical cuts | PASS |
| Slot semantics | Spatial carries exact ordered stadium Line/Arc contour and resolved Z; backend uses generic profile-tool builder, not slot-dimension reconstruction | PASS |
| Profile builder | One shared _build_profile_tool handles ClosedProfile body and SlotCut; no Polygon/Composite/Slot-specific builder in Python | PASS |
| Boolean | CadQuery/OCP exact B-Rep extrusion and Boolean cut; no mesh Boolean | PASS |
| Z | XY profile, canonical +Z extrusion/cuts; body and operation intervals are transported, not guessed by Python | PASS |
| Fail-closed DTO | Strict version/field/variant, finite-number, interval, bounds, arc, continuity, closure, and unsupported-operation checks | PASS for canonical DTO |

Operation ordering is deterministic: Rust forms the union of Spatial cylindrical/slot cut IDs, sorts by SpatialCutId, allocates opaque ordinal IDs in that order, and serializes ordered maps. Python iterates the ordered JSON operation object and retains IDs/source IDs; it does not sort or infer their meaning. Profile segment order is preserved exactly; canonical typed Polygon/Composite/Slot ordering is established upstream.

## Canonical STEP validation and delivery

The product Engineering route builds one source solid, exports it to a private controlled .step, reimports that exact file, verifies one valid solid, and compares bbox, volume, surface area, center of mass, edge/face/vertex counts, and deterministic material probes. It hashes and sizes the artifact; Rust reads those bytes and compares size/SHA-256 to the worker witness before returning ValidatedStepArtifact. The UI/CLI serves those same bytes; it does not re-export after validation.

The equivalence check is geometric witness comparison, not source identity or fingerprint equality. Its material probes are deterministic finite witnesses, not a formal proof of all pointwise geometry. OCP topology-count equality is strict and tested in the pinned CadQuery/OCP runtime.

The separate CadDocument route is the exception: it is still reachable and produces STEP, but the capability manifest explicitly says imported bbox/volume equivalence is false and it is not guarded by the canonical SolidBuildSpec witness/artifact contract. This is the freeze blocker, not a defect in the canonical validator.

## Golden and adversarial coverage

All seven SolidBuildSpec fixtures exist under crates/scc-manufacturing/tests/goldens/ and are exercised by the Rust manufacturing suite:

| Capability | Golden | Rust coverage | Real CadQuery/STEP coverage |
|---|---|---|---|
| Rectangle | installation_plate_solid_spec.json | installation plate and typed hole/pattern operation cases | Docker STEP body/hole round trips |
| Circle | circular_disk_solid_spec.json | exact circle lowering/golden | disk, translation, annulus, flagship flange |
| RoundedRectangle | rounded_rectangle_solid_spec.json | typed lowering/golden | translated rounded rectangle + hole round trip |
| ClosedProfile foundation | closed_profile_solid_spec.json | exact concave/mixed-arc, open-loop rejection | real STEP and translated real worker integration |
| Polygon | polygon_solid_spec.json | canonical line-only profile and source identity | golden STEP round trip and same-bounds/wrong-vertex adversaries |
| Composite | composite_solid_spec.json | ordered Line/Arc, direction, translation, concavity | real round trip, wrong direction/chord, bbox and translation adversaries |
| Slot operation | slot_solid_spec.json | typed operation, exact contour/Z/provenance and deterministic serde | exact round trip plus omission/orientation/width/chord/bbox-cut adversaries |

Representative existing adversarial tests include:

- Same-bounds contour changes: test_same_bbox_and_volume_different_contours_fail_centroid_witness, test_polygon_wrong_vertex_with_same_bbox_is_rejected.
- Bbox substitution: test_bbox_extrusion_substitution_is_rejected_by_actual_equivalence, test_polygon_bbox_extrusion_substitution_is_rejected, test_composite_bbox_substitution_and_translation_loss_are_rejected.
- Arc-to-chord/direction/radius: test_arc_to_chord_substitution_is_rejected_by_actual_equivalence, test_composite_wrong_arc_direction_and_chord_are_rejected_by_equivalence.
- Translation loss: test_translation_loss_to_origin_is_rejected_by_actual_equivalence, test_translated_closed_profile_preserves_cadquery_step_bbox_and_center_of_mass.
- Slot omission, wrong orientation, wrong width, arc-to-chord, and bbox-cut substitution: test_slot_step_roundtrip_and_adversarial_geometry_witnesses.
- Transport fail-closed: test_unknown_or_missing_schema_fails_closed, test_slot_profile_is_strictly_validated_without_repair, test_open_closed_profile_is_rejected_before_face_builder, and Rust manufacturing malformed numeric/source/open/disconnected contour tests.
- Artifact/reimport failure: test_mutation_fake_reimport_and_witness_mismatch_fail, test_export_import_empty_and_exception_errors_are_structured, plus Rust byte identity checks in the artifact API.

The separate FeatureGraph path has its own input/build/manifest tests, but those tests do not close the product-level equivalence/artifact-identity gap above.

## Executed verification gates

All executed gates passed. They prove regression health of the current code, but do not override the structural freeze blocker.

| Gate | Result |
|---|---|
| cargo test --offline --locked -p scc-engineering --all-targets | PASS — 213 passed |
| cargo test --offline --locked -p scc-spatial --all-targets | PASS — 76 passed |
| cargo test --offline --locked -p scc-manufacturing --all-targets | PASS — 39 passed |
| cargo test --offline --locked -p scc-cad-backend --all-targets | PASS — 5 passed; worker-conditional real STEP functions are exercised in Docker |
| cargo check --offline --locked --workspace --all-targets | PASS |
| cargo test --offline --locked --workspace --all-targets | PASS — 49 result summaries; 1,111 passed, 0 failed, 2 ignored |
| docker compose run --rm cad-test | PASS — 58/58 |
| docker compose run --rm cad-rust-test | PASS — 6/6 real Rust CLI/UI → CadQuery/OCP → STEP integrations |
| docker compose config --quiet | PASS |
| cargo fmt --all -- --check | PASS |
| cargo clippy --offline --locked --workspace --all-targets -- -D warnings | PASS |
| cargo build --offline --locked --release -p scc-cli | PASS |
| SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui | PASS — 131 total, 125 passed, 0 failed, 6 worker-dependent skips |
| Browser E2E | NOT RUN — explicitly non-blocking per task prompt |
| git diff --check | PASS — documentation diff clean |

## Risks and deferred domain

1. Critical — second source/STEP route and pattern authority: live CadDocument and EngineeringModel models can independently drive STEP, while the compatibility builder expands circular patterns inside Python.
2. Critical — unequal delivery assurance: compatibility STEP lacks the canonical geometric equivalence and exact validated-byte gate.
3. Medium — route ambiguity: UI exposes two CAD/STEP workflows with materially different validation guarantees.
4. Medium — witness limits: sampled material probes plus aggregate measures are strong regression witnesses, not mathematical equivalence proof.
5. Medium — kernel drift: strict topology counts can vary across CadQuery/OCP versions; current Docker runtime is pinned and tested.
6. Low — no-op generic cuts: backend DTO tests intentionally report no_material_change for outside/duplicate cuts; this is surfaced, not silently claimed as applied.
7. Low — compatibility contract: SolidBuildSpec schema v1 is strict; future schema/backend changes need explicit version and golden review.
8. Deferred — one outer loop only; inner loops, multiple contours/islands, general editable Sketch/feature history, Slot patterns, and broader manufacturing operations remain unsupported.

Natural-language coverage is not implied by manufacturing support; Slot NL intent and patterns remain outside this phase.

## Required closure constants

| Constant | Value |
|---|---|
| MANUFACTURING_DOMAIN_CLOSED | NO |
| MANUFACTURING_DOMAIN_FROZEN | NO |
| ENGINEERING_MODEL_IS_SOLE_MUTABLE_TRUTH | NO |
| SPATIAL_MODEL_IS_DERIVED_MANUFACTURING_TRUTH | YES |
| SOLIDBUILDSPEC_IS_BACKEND_NEUTRAL | YES |
| CLOSEDPROFILE_IS_DERIVED_TRANSPORT | YES |
| CLOSEDPROFILE_IS_SECOND_SKETCH_SYSTEM | NO |
| HOLE_REMAINS_TYPED_CYLINDER | YES |
| SLOT_REMAINS_TYPED_OPERATION | YES |
| PYTHON_READS_ENGINEERING_MODEL | NO |
| PYTHON_KNOWS_POLYGON | NO |
| PYTHON_KNOWS_COMPOSITE | NO |
| PYTHON_RECONSTRUCTS_SLOT | NO |
| DUPLICATE_CLOSED_PROFILE_BUILDER | NO |
| PROFILE_TYPE_INFERRED_FROM_SEGMENTS | NO |
| MANUFACTURING_SEGMENT_REORDERING | NO |
| MANUFACTURING_TESSELLATION_AS_TRUTH | NO |
| BBOX_SUBSTITUTION_AS_TRUTH | NO |
| PATTERN_RECONSTRUCTED_IN_MANUFACTURING | YES on reachable CadDocument compatibility route |
| PROVENANCE_CHAIN_PRESERVED | YES |
| STEP_REEXPORTED_AFTER_VALIDATION | NO |
| VALIDATED_BYTES_ARE_DELIVERED_BYTES | NO product-wide (canonical Engineering route only: YES) |
| MANUFACTURING_FAILS_CLOSED | NO product-wide (canonical SolidBuildSpec route: YES) |
| REACHABLE_LEGACY_MANUFACTURING_PATH | YES |
| MANUFACTURING_HAS_SECOND_MUTABLE_TRUTH | YES |
| INNER_LOOPS | DEFERRED |
| MULTIPLE_CONTOURS | DEFERRED |
| GENERAL_SKETCH_SYSTEM | DEFERRED |

## Freeze decision

**DO NOT FREEZE.** Do not create ADR-062 with Accepted/Frozen status. The canonical Engineering manufacturing chain is validated by the passing gates above, but the complete product domain fails strict gates C, K, Q, T/V at the product boundary because the live compatibility STEP route has a different mutable source, performs pattern expansion in Python, and has weaker delivery validation. Resume only with the separately scoped Phase 13.5A authority-consolidation decision/implementation, then rerun the closure audit.
