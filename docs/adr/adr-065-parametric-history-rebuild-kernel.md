# ADR-065: Generic Parametric History Rebuild Kernel

- Status: Accepted for Phase G1B
- Decision owners: SCC architecture
- Scope: derived geometry for ordered `EngineeringModel` Sketch and Extrude history

## Context

ADR-064 establishes `EngineeringModel` as the sole editable design truth and adds
ordered typed datum, sketch, and extrude features. It intentionally deferred the
real Boolean rebuild and downstream drawing path. The existing formal
SolidBuildSpec v1 and STEP route are tied to the legacy spatial profile contract;
parametric history cannot be pushed through that contract without a new additive
manufacturing design.

The repository already has a stateless CadQuery/OCP process boundary and formal
Spatial → Projection → Visibility → Drawing stages. G1B must reuse those stages
without persisting a kernel shape or introducing a second editable model.

## Decision

### 1. Stateless rebuild request

- Rust lowers one validated, ordered `EngineeringModel` history into a dedicated
  versioned parametric rebuild request. The request contains only active Sketch
  and Extrude operations, exact Line/Arc/Circle profile data, resolved plane
  frames, directions, and extents.
- The Python worker creates CadQuery/OCP shapes only for the duration of that
  process call. It returns one validated body's bounds, volume, exact analytic
  edge/face evidence, and three HLR view records. It does not return or persist a
  B-Rep file, editable document, STEP file, or renderer artifact.
- Suppressed operations are omitted from the complete rebuild. Any failure
  rejects the entire output; partial bodies are never surfaced as success.

### 2. Supported G1B geometry

- Profiles contain closed Line/Arc/Circle loops. Circle is one entity per loop;
  self-intersecting, open, malformed, or unsupported loops fail closed.
- Sketch support is XY, XZ, YZ, or an offset of those principal datums. Custom
  planes and generated-face support are reserved and fail closed.
- One body uses a common principal extrusion axis. Operations are ordered
  Extrude ADD/CUT. The first material operation must be ADD; all results must
  remain one valid solid. Disconnected ADD, no-intersection CUT, whole-body
  removal, and invalid booleans have stable structured errors.
- DISTANCE and SYMMETRIC extents are supported. THROUGH_ALL is supported for
  CUT by extending the tool beyond the current body's measured bounds. Other
  extent modes and mixed-axis histories remain unsupported.

### 3. Spatial and drawing lowering

- `SpatialModel` carries a boxed, read-only `ParametricSpatialEvidence` value:
  source-model digest, body frame, measured bounds/volume, source-operation IDs,
  exact analytic edge/face descriptors, and HLR Front/Top/Left curves with
  visibility classifications.
- The existing Rust ProjectionModel, VisibilityModel, consistency validator,
  DrawingModel, SVG, and DXF pipeline consumes this evidence. The HLR result is
  not replaced by renderer-side geometry or a 2D silhouette shortcut.
- OCCT may expose degree-one intermediate curves for HLR output. The worker
  restores these to an analytic segment or circle/arc only after validating the
  control-point locus; unsupported splines fail closed. No tessellated path is
  accepted as an exact drawing primitive.
- Parametric features do not receive geometry-derived dimensions whose source
  semantics are not established by the current feature contract.

### 4. Manufacturing and product boundaries

- Parametric `SpatialModel` values are rejected by SolidBuildSpec v1 lowering.
  Generic parametric STEP support remains deferred. The existing canonical STEP
  authority and legacy projects remain unchanged.
- G1B adds no UI feature authoring, natural-language feature planning, Hole,
  Pattern, Mirror, Revolve, Sweep, Loft, Fillet, Chamfer, face/edge selectors,
  assemblies, or multi-body support.

## Rejected alternatives

- Persisting CadQuery B-Reps would create a second model lifecycle and stale
  derived state.
- Converting the feature history into compatibility `CadDocument` state would
  create a second editable authority and would not establish canonical STEP
  semantics.
- Routing parametric history into frozen SolidBuildSpec v1 would silently
  reinterpret a contract that does not represent the history.
- Emitting HLR polylines as final DrawingModel curves would degrade exact circle
  and arc geometry and bypass the formal analytic projection contract.

## Verification contract

G1B acceptance requires real CadQuery/OCP boolean evidence for the generic
feature matrix and fixture; Rust CLI integration through Spatial, Projection,
Visibility, Drawing, SVG, and DXF; and regression coverage for existing Project
v1 and canonical STEP delivery. The isolated image is tagged
`aicad-scc-ui:g1b-test`; it is not the production `0.4.0` image.
