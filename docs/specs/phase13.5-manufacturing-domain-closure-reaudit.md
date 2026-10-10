# Phase 13.5 — Manufacturing Domain Closure Re-Audit

**Status: PASS_WITH_NOTES — FROZEN**
**MANUFACTURING_DOMAIN_FROZEN: YES**
**Audit target HEAD:** `13d2a40516e644d93e880208c189ed160ac5f908`
**Baseline:** `4205a3278f67e3190df2bed56855ff2d4ffddbda`
**Branch:** `codex/aicad-pre-rollback-recovery`
**Audit date:** 2026-09-19
**SOURCE_CODE_CHANGED:** NO

## Scope and decision

This is a fresh Phase 13.5 re-audit after the Phase 13.5A remediation. It
does not re-implement manufacturing and does not start Phase 14. The audit
covers the declared Manufacturing/STEP domain only:

```text
Rectangle, Circle, RoundedRectangle, Polygon, Composite, Hole, Slot
```

All formal STEP generation and delivery paths were re-enumerated from actual
code, then the authority chain, SolidBuildSpec contract, exact CadQuery/OCP
boundary, equivalence witnesses, artifact identity, fail-closed inputs,
compatibility boundary, and required test gates were verified.

Decision:

```text
PHASE13_5_STATUS = PASS_WITH_NOTES
MANUFACTURING_DOMAIN_FROZEN = YES
FREEZE = YES
```

The notes are non-blocking: Browser E2E was not repeated under its explicit
phase exemption, and finite geometric witnesses are not a mathematical proof
of all pointwise geometry. Neither note creates a second STEP authority or
weakens the supported manufacturing contract.

## Previous blocker resolution

The original historical failure record remains unchanged in
`phase13.5-manufacturing-domain-closure-audit.md`.

| Previous blocker | Phase 13.5A remediation | Fresh result and evidence |
|---|---|---|
| Second mutable formal manufacturing source | Compatibility `CadDocument` remains editable but formal STEP was retired | CLOSED. `/api/cad/build` has no STEP output; only source JSON/BREP/STL/GLB/SVG and metrics/preview remain. |
| Independent compatibility STEP delivery | Worker STEP export was removed; service rejects STEP artifact requests and manifests | CLOSED. `cad-service.test.mjs`, `server.test.mjs`, `test_feature_graph.py`, and Docker `cad-test` pass. |
| Python compatibility pattern expansion reaching formal STEP | Compatibility pattern behavior is bounded to retained non-STEP BREP/preview; canonical patterns resolve in Spatial | CLOSED. `FeatureGraph._circular_pattern` has no STEP edge; canonical pattern tests and real STEP integration pass. |
| Missing equivalence validation on compatibility STEP | Compatibility STEP no longer exists; all formal STEP uses canonical validation | CLOSED. Canonical reimport, witness, provenance, hash/size tests pass. |
| Unvalidated public artifact delivery | Compatibility artifact route rejects `step`; canonical API returns Rust-verified bytes | CLOSED. HTTP 400 / `SCC-CAD-010` and exact-byte/header identity tests pass. |

## Official STEP entry-point inventory

| Entry | File / caller | Input | Public? | Generates STEP? | Delivers STEP? | Canonical Engineering authority? | Reimport/equivalence? | Final result |
|---|---|---|---:|---:|---:|---:|---:|---|
| `POST /api/engineering/export/step` | `apps/scc-ui/server.mjs`; Engineering Workbench | source-only `EngineeringModel` | YES | YES | YES | YES | YES | sole public formal STEP API |
| `scc engineering export step` | `crates/scc-cli/src/main.rs`, `engineering.rs` | source-only `EngineeringModel` JSON | YES | YES | YES | YES | YES | canonical CLI delivery |
| `aicad_cad.step_delivery` | `crates/scc-cli/src/engineering.rs` invokes Python module | `SolidBuildSpec` lowered from Engineering/Spatial | NO, internal | YES | only through Rust-owned validated artifact | YES | YES | canonical backend stage |
| `CadBackendClient::execute_step_delivery_artifact` | `crates/scc-cad-backend/src/lib.rs` | `SolidBuildSpec` | NO, internal | reads worker artifact | returns only checked bytes to Rust | YES | YES | exact validated bytes |
| `POST /api/cad/build` | `apps/scc-ui/server.mjs`; compatibility UI/clients | `CadDocument` / `FeatureGraph` | YES | NO | NO | NO by design | N/A | bounded non-STEP compatibility route |
| `GET /api/cad/artifacts/:job/:name` | `server.mjs` → `cad-service.mjs` | compatibility job manifest | YES | NO | `step` rejected | NO | N/A | no STEP alias or download bypass |
| generic `POST /api/export` | `server.mjs` → `scc-cli.mjs` | legacy `GeometryModel` | YES | NO | NO | N/A | N/A | format allowlist is SVG/DXF only |
| compatibility `worker.py` | `apps/scc-ui/cad/aicad_cad/worker.py` | `CadDocument` / legacy `CadSpec` | indirectly | NO | no STEP manifest | NO | N/A | source/BREP/STL/GLB/SVG only |

The scan also covered `.step`/`.stp`, `exportStep`/`export_step`, artifact
URLs, manifest fields, download controls, test/debug entrypoints, direct
worker paths, and case/format aliases. The only `step` references in the
compatibility surface are explicit rejection tests or the canonical
Engineering route. No public unvalidated STEP path remains.

## Canonical manufacturing authority

```text
source EngineeringModel
  → one solved Engineering snapshot / GeometryModel
  → SpatialModel
  → SolidBuildSpec v1
  → Rust CadBackendClient
  → stateless CadQuery/OCP
  → STEP export to private validated.step
  → STEP reimport as a distinct solid
  → bbox/volume/surface/center/topology/material witnesses
  → Rust artifact size and SHA-256 identity check
  → exact validated bytes returned by CLI/UI/API
```

Fresh code evidence:

- `crates/scc-cli/src/engineering.rs::export_step` deserializes only
  `EngineeringModel`, solves once, lowers Spatial, lowers Manufacturing, and
  invokes `aicad_cad.step_delivery` through `CadBackendClient`.
- `crates/scc-manufacturing/src/model.rs` defines derived `SolidBuildSpec`
  v1; it has no backend, kernel, persistence, or editable-engineering state.
- `apps/scc-ui/cad/aicad_cad/step_delivery.py` builds once, exports once,
  reimports, compares witnesses, and returns artifact metadata.
- `crates/scc-cad-backend/src/lib.rs` reads the same private artifact after
  validation and rejects size/SHA-256 mismatches.
- `apps/scc-ui/lib/scc-cli.mjs` verifies metadata against the bytes received
  from the release CLI; `server.mjs::sendStepArtifact` verifies bytes, size,
  and SHA-256 again before HTTP delivery.

Therefore:

```text
FORMAL_STEP_HAS_CANONICAL_ENGINEERING_AUTHORITY = YES
FORMAL_STEP_BYPASSES_SOLIDBUILDSPEC = NO
FORMAL_STEP_BYPASSES_EQUIVALENCE = NO
UNVALIDATED_STEP_PUBLIC_DELIVERY = NO
REACHABLE_LEGACY_FORMAL_STEP_PATH = NO
FORMAL_MANUFACTURING_HAS_SECOND_MUTABLE_TRUTH = NO
```

## Compatibility CAD boundary

```text
COMPATIBILITY_EDITING_STATE_EXISTS = YES
COMPATIBILITY_INDEPENDENT_STEP_DELIVERY = NO
FORMAL_MANUFACTURING_HAS_SECOND_MUTABLE_TRUTH = NO
ALL_AICAD_EDITING_STATES_UNIFIED = NO
```

The compatibility state remains intentionally separate. `CadDocument` and
`FeatureGraph` can still build valid OCCT BREP and derivatives, expose
coverage/metrics, and drive GLB preview. `cad-service.mjs::artifact` rejects
the name `step` with HTTP 400 / `SCC-CAD-010`; `validateManifest` rejects a
worker that tries to return a `step` entry. The compatibility UI exposes
source JSON, BREP, STL, and GLB controls and no STEP control. The real API
test exercises both the build and the attempted STEP artifact download.

## Public manufacturing matrix

| Capability | Engineering/Spatial | SolidBuildSpec/CadQuery | Real STEP/reimport | Result |
|---|---:|---:|---:|---|
| Rectangle | PASS | PASS | PASS | SUPPORTED |
| Circle | PASS | PASS | PASS | SUPPORTED |
| RoundedRectangle | PASS | PASS | PASS | SUPPORTED |
| Polygon | PASS | PASS | PASS | SUPPORTED |
| Composite | PASS | PASS | PASS | SUPPORTED |
| Hole | PASS | PASS | PASS | SUPPORTED |
| Slot | PASS | PASS | PASS | SUPPORTED |

This does not claim arbitrary 3D modeling, general feature history, inner
loops, multiple contours, Slot patterns, or natural-language Slot support.

## SolidBuildSpec contract

The actual Rust contract is `SolidBuildSpec { schema_version, part,
operations, source_map }` with `SOLID_BUILD_SPEC_SCHEMA_VERSION = 1`.

- `part.body.footprint` is one of typed `Rectangle`, `Circle`,
  `RoundedRectangle`, or derived `ClosedProfile { segments }`.
- `part.body.z` and `part.bounds` are resolved finite witnesses; bounds are
  checked against the exact footprint and are not an alternate geometry source.
- `operations` is a deterministic `BTreeMap<SolidOperationId,
  SolidOperation>` containing typed `CylindricalCut` and `SlotCut` only.
- `source_map` preserves body feature/entity and opaque SpatialCutId
  provenance; operation/source keys must match exactly and source IDs are
  unique.
- Rust validation rejects unknown schema versions, malformed maps, non-finite
  values, invalid intervals, invalid bounds, unsupported axes/variants, open
  or disconnected profiles, invalid arcs, and invalid Slot containment.

`SolidBuildSpec` is backend-neutral and derived. It is not editable truth and
is never persisted as the product source model.

## ClosedProfile / Hole / Slot architecture

Engineering Rectangle/Circle/RoundedRectangle retain typed body semantics.
Engineering Polygon and Composite retain independent upstream identity; their
typed Spatial markers select exact lowering to the same generic ordered
Line/Arc `ClosedProfile`. No downstream inference from “contains arcs” is used.

Hole lowers to typed `SolidOperation::CylindricalCut` with center, radius,
resolved Z interval, and +Z axis. Slot lowers to typed
`SolidOperation::SlotCut { profile, z, axis }`, where the profile is copied
from Spatial's exact four-segment contour. Slot is never a body footprint.

`solid_backend.py::_build_profile_tool` is the single native Line/Arc builder
used for ClosedProfile bodies and Slot tools. It creates OCCT-backed Line and
Arc edges, an ordered wire, a face, and an exact +Z extrusion. There is no
second stadium builder, tessellation truth, bbox cut, or Slot reconstruction.

## Python / CadQuery boundary

Python receives only resolved SolidBuildSpec DTOs. `solid_build_spec.py` has
strict exact-key, schema, finite-number, interval, bounds, continuity,
closure, area, radius, sweep, direction, and containment validation. The
CadQuery executor consumes the typed DTO and performs defensive validity,
single-solid, volume, and bounds checks. It does not read EngineeringModel,
expressions, FeatureGraph semantics, or source DSL.

Compatibility Python pattern expansion remains in `feature_graph.py`, but the
worker that owns that builder has no STEP exporter or STEP manifest entry. It
therefore cannot affect formal STEP geometry. Formal patterns are expanded by
Engineering/Spatial into resolved cylindrical cuts; manufacturing receives the
resolved cuts and preserves their opaque source IDs.

## Provenance / serde / golden matrix

Engineering source-map segment indices select Polygon/Composite order before
Spatial. Spatial carries `source_entity`, `segment_index`, explicit Arc
direction, and typed source profile identity. Manufacturing checks contiguous
indices, unique source entities, body identity, operation/source-map key
identity, and copies the ordered geometry. Operations are sorted from the
SpatialCutId union before deterministic opaque ordinal IDs are allocated.

The following seven SolidBuildSpec golden files exist and are loaded by Rust
manufacturing tests (and the Python backend loads representative fixtures):

| Golden | Capability | Loaded by tests |
|---|---|---|
| `installation_plate_solid_spec.json` | Rectangle + Hole/Pattern | Rust + Python |
| `circular_disk_solid_spec.json` | Circle | Rust |
| `rounded_rectangle_solid_spec.json` | RoundedRectangle | Rust |
| `polygon_solid_spec.json` | Polygon | Rust + Python STEP |
| `composite_solid_spec.json` | Composite | Rust + Python STEP |
| `slot_solid_spec.json` | Slot | Rust + Python |
| `closed_profile_solid_spec.json` | ClosedProfile foundation | Rust |

Serde round-trip and 100-run byte-determinism tests pass. Unknown schema,
unknown operation, missing fields, and malformed values fail closed.

## STEP export / reimport and equivalence

The canonical path exports one private `.step`, checks it is non-empty and
confined, imports it as a distinct object, and compares:

- exact finite bounding-box coordinates within `1e-6` mm;
- volume with absolute `1e-5 mm³` plus relative `1e-9` tolerance;
- surface area with absolute `1e-5 mm²` plus relative `1e-9` tolerance;
- center of mass within `1e-6` mm;
- exact edge, face, and vertex counts;
- deterministic material probes with exact boolean agreement.

No second export occurs after validation. Rust verifies the worker witness,
provenance, artifact size, and SHA-256 before returning the bytes. Node and
the HTTP response verify the same byte identity. Source-model fingerprints
are metadata and do not substitute for geometric equivalence.

## Adversarial coverage

Fresh Docker and Rust test runs cover actual tests for:

- same-bounds/different-contour and same-bounds/wrong-polygon vertices;
- bbox extrusion or bbox-cut substitution;
- Arc-to-chord, wrong Arc direction, wrong radius, major-arc preservation;
- translation loss to origin;
- Slot omission, wrong orientation, wrong width, chord substitution, and
  bbox-cut substitution;
- fake reimport, witness mismatch, empty/export/import failures, and artifact
  identity mismatch;
- open/disconnected/zero-area profiles, invalid radius/sweep, invalid Z,
  unknown schema/operation, non-finite numbers, and invalid Slot containment.

Representative test files are `apps/scc-ui/cad/tests/test_step_delivery.py`,
`apps/scc-ui/cad/tests/test_solid_backend.py`,
`crates/scc-manufacturing/tests/manufacturing_integration.rs`, and
`crates/scc-cad-backend/tests/closed_profile_step.rs`.

## Fail-closed evidence

Engineering and Spatial reject unresolved/non-finite/unsupported semantic
inputs before lowering. Manufacturing rejects identity, order, source-map,
interval, bounds, profile, arc, and operation corruption. Python rejects
unknown fields and variants rather than guessing. CadQuery rejects invalid or
non-single solids. STEP delivery rejects export/import failures, invalid
reimport, any witness mismatch, missing material agreement, invalid artifact
path, empty bytes, and metadata/hash mismatch. API compatibility STEP requests
are rejected rather than redirected or approximated.

## Compatibility non-STEP regression

The current Docker run passed `test_generic_document_exports_non_step_derivatives_and_auditable_coverage`,
`test_exports_non_step_compatibility_derivatives`, the real flange/bracket
valid-solid tests, gearbox compatibility tests, and the full Python backend
suite. The current Node run passed the allowlisted artifact, forbidden
manifest, API build/rejection, and canonical project STEP delivery tests.

## Test matrix — executed during this re-audit

| Command | Passed | Failed | Ignored/skipped | Exit |
|---|---:|---:|---:|---:|
| `cargo test --offline --locked -p scc-engineering --all-targets` | 213 | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-spatial --all-targets` | 76 | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-manufacturing --all-targets` | 39 | 0 | 0 | 0 |
| `cargo test --offline --locked -p scc-cad-backend --all-targets` | 5 | 0 | 0 | 0 |
| `cargo check --offline --locked --workspace --all-targets` | — | — | — | 0 |
| `cargo test --offline --locked --workspace --all-targets` | 1111 | 0 | 2 ignored | 0 |
| `docker compose run --rm cad-test` | 58 | 0 | 0 | 0 |
| `docker compose run --rm cad-rust-test` | 6 | 0 | 0 | 0 |
| `node --test apps/scc-ui/test/cad-service.test.mjs apps/scc-ui/test/server.test.mjs` | 13 | 0 | 0 | 0 |
| `SCC_BIN=$PWD/target/release/scc npm test --prefix apps/scc-ui` | 126 | 0 | 6 skipped | 0 |
| `docker compose config --quiet` | — | — | — | 0 |
| `cargo fmt --all -- --check` | — | — | — | 0 |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | — | — | — | 0 |
| `cargo build --offline --locked --release -p scc-cli` | — | — | — | 0 |
| `git diff --check` | — | — | — | 0 |

The six UI skips are worker-dependent tests; the required Docker CAD and real
Rust/CadQuery integration runs cover that worker path. Browser E2E was not run,
as it remains explicitly non-blocking under the phase contract.

## Freeze gates

```text
A1–A7 Authority Gate              PASS
M1–M13 Manufacturing Contract     PASS
S1–S8 STEP Assurance              PASS
C1–C7 Compatibility               PASS
V1–V19 Verification               PASS
```

The formal STEP authority is consolidated, the supported manufacturing matrix
is intact, the compatibility boundary is server-enforced, and the final
source worktree was clean before documentation changes. No source-code or
manufacturing changes were made during this re-audit.

## Remaining architecture debt

### Manufacturing / STEP Domain

Frozen for the seven declared capabilities and the canonical validated-byte
contract. This does not freeze unsupported geometry or permit new independent
STEP routes.

### Product-wide Editing Architecture

Not unified. The generic `CadDocument`/`FeatureGraph` compatibility workspace
remains a separate editable state for bounded non-STEP CAD. It is intentionally
not a second formal manufacturing truth and is not reverse-inferred into
`EngineeringModel`.

Deferred items include inner loops, multiple contours/islands, general Sketch
and feature history, richer operations, Slot patterns, NL Slot, and broader
mechanical-CAD standards.

## Closure constants

```text
FORMAL_STEP_HAS_CANONICAL_ENGINEERING_AUTHORITY = YES
FORMAL_STEP_BYPASSES_SOLIDBUILDSPEC = NO
FORMAL_STEP_BYPASSES_EQUIVALENCE = NO
UNVALIDATED_STEP_PUBLIC_DELIVERY = NO
REACHABLE_LEGACY_FORMAL_STEP_PATH = NO
FORMAL_MANUFACTURING_HAS_SECOND_MUTABLE_TRUTH = NO
COMPATIBILITY_EDITING_STATE_EXISTS = YES
COMPATIBILITY_INDEPENDENT_STEP_DELIVERY = NO
ALL_AICAD_EDITING_STATES_UNIFIED = NO
MANUFACTURING_DOMAIN_CLOSED = YES
MANUFACTURING_DOMAIN_FROZEN = YES
PHASE14_STATUS = NOT_STARTED
PUSHED = NO
```

## Historical record and next decision

The original `phase13.5-manufacturing-domain-closure-audit.md` is preserved as
the pre-remediation `INCOMPLETE / DO NOT FREEZE` record. ADR-063 records the
Phase 13.5A boundary decision. ADR-062 records this final Manufacturing/STEP
closure decision.

The next roadmap decision is whether and when to address the deferred product
editing architecture or broader manufacturing domain. Phase 14 is not started
by this audit.
