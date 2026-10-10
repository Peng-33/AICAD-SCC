# ADR-069: Project v2 Formal Manufacturing and Canonical STEP

- Status: Accepted for bounded Phase G5 implementation
- Decision owners: SCC architecture
- Scope: EngineeringModel v2 input to the existing single formal STEP authority

## Context

ADR-063 froze one product STEP authority at `POST /api/engineering/export/step` and `scc engineering export-step`. Project v1 reaches that authority through the derived `SolidBuildSpec` and its established CadQuery/OCP validator. The v1 contract describes a solved SpatialModel body and ordered cuts; it cannot retain Project v2 ADD/CUT order, sketch frames, extents, typed Hole/Pattern operations, semantic edge references, or Revolve history.

G1B–G4 already define a strict schema-v3, ordered Rust-to-CadQuery operation contract for EngineeringModel v2 drawing rebuild. Its OCP executor produces a transient, validated one-solid shape. Creating STEP from a DrawingModel or renderer Shape would make the drawing runtime a second manufacturing authority.

## Decision

### 1. Editable ownership and derived Manufacturing IR

- `EngineeringModel` remains the only editable feature tree and source of truth. Sketch, DatumPlane/DatumAxis, Feature History, parameters, dependencies, suppression, and stable semantic topology references remain in the EngineeringModel.
- `lower_parametric_manufacturing_spec()` produces a validated, immutable, invocation-local `ParametricRebuildRequest` in feature-history order. It resolves dependencies and patterns deterministically, skips suppressed operations, keeps ADD/CUT and feature parameters explicit, and does not retain OCP indices or Shapes.
- `SolidBuildSpec` v1 remains unchanged for Project v1. The new Project v2 DTO is a manufacturing transport plan, not another project format or editable model.

### 2. One formal STEP authority

- Both Project v1 and Project v2 enter the same CLI command and API route. Rust dispatch uses `FormalManufacturingInput::ProjectV1` or `ProjectV2`; there is no v2-only endpoint/exporter.
- Project v1 retains its `EngineeringModel → SpatialModel → SolidBuildSpec → CadQuery/OCP` lowering. Project v2 uses the history lowerer and the same versioned ordered OCP executor already used for drawing rebuild.
- The Project v2 STEP source is rebuilt from its operation request, never from DrawingModel, SVG, DXF, browser state, or LLM-generated geometry. The OCP Shape exists only for that worker invocation.
- Each operation and the final result must preserve one valid connected solid. Empty, invalid, detached ADD, no-intersection/full-body CUT, invalid topology, or unsupported input fails closed before an artifact is returned.

### 3. Reimport, equivalence, and exact-byte delivery

- The worker builds one source solid, exports one temporary STEP file, and reimports that same file with CadQuery/OCP. It checks one valid solid, bounds, volume, surface area, center of mass, material probes, and analytic feature geometry.
- Project v1 retains its stricter edge/face/vertex count check. Project v2 does not treat kernel topology counts as EngineeringModel identity; it compares stable analytic circular/elliptical edge and cylindrical/conical/toroidal surface descriptors in addition to mass, bounds, and material witnesses. No raw topology ordinal is persisted.
- STEP comparison tolerances are: bounds and center of mass `1e-6 mm`; volume `1e-5 mm³ + 1e-9 × max(|Vsource|, |Vimport|)`; surface area `1e-5 mm² + 1e-9 × max(|Asource|, |Aimport|)`; analytic descriptor scalar comparison `1e-5`; material `isInside` tolerance `1e-7`.
- Rust reads the worker-validated temporary artifact, recomputes SHA-256 and byte length, validates the worker witnesses/provenance/status, and returns those exact bytes. The API checks the same hash/size plus manufacturing, build, reimport, and equivalence statuses before sending the body. The browser acceptance hashes the file saved by Chromium and compares its bytes with the API body and validation hash.

### 4. Diagnostics and measurement

- The single API/CLI surface distinguishes lowering, manufacturing build, STEP serialization, reimport, and equivalence failure. Original backend stage/code is retained in structured diagnostics.
- CLI diagnostics record `manufacturing lowering`, build, serialization, reimport, equivalence, and total elapsed times. Legacy Project v1 and Project v2 timings remain separate measurements.
- Browser UI reports EngineeringModel/drawing readiness separately from the formal STEP lowering/build/reimport/equivalence result.

### 5. Deferred work

- Countersink geometry, explicit placement patterns, Sweep, Loft, NURBS, assemblies, multi-body manufacturing, and arbitrary complex-part support remain deferred or unsupported.
- AICAD-COMPLEX-001 is a generic-system acceptance target only. No named-part builder or special exporter is allowed. A partial or unrun complex-piece STEP test does not establish broad complex-part capability.
- Future Sweep/Loft work must add typed EngineeringModel features and the shared versioned rebuild/executor contract before it enters the canonical STEP input. It must not add another STEP route or bypass this validator.

## Consequences

- Project v1 and v2 share one public STEP authority, exporter, reimport boundary, error boundary, and byte-delivery gate while retaining source-appropriate lowering.
- Rebuild and STEP do not share cached kernel Shapes; each call starts from the source model and fails closed on unsupported history.
- The Phase G5 acceptance report records which manufacturing matrix cases, API failure cases, browser flows, and conformance checks actually ran. A passing bounded matrix does not imply support for every mechanical part.
