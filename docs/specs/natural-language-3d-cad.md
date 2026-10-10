# Spec: Natural-language 3D CAD generation

- Status: Accepted and implemented as a bounded foundation
- Target milestone: v0.4.0
- Date: 2026-08-10
- Primary acceptance part: multi-cavity precision gearbox housing

## Objective

Extend AICAD from a two-dimensional semantic-constraint demonstrator into a local web application that converts bounded natural-language mechanical requirements into a validated parametric 3D CAD document, builds a real OCCT B-Rep solid, previews the result, and exports files that can be edited in mainstream CAD software.

The geometry kernel, not the language model, is authoritative. The model may produce only versioned `CadSpec` JSON. It may never emit or execute Python, shell, CadQuery, FreeCAD, or JavaScript source.

The first acceptance family is `precision_gearbox_housing_v1`. The implemented geometry includes the base, housing shell/main cavity, top opening and seal groove, mounting ears/counterbores, shaft and stepped bearing bores, front cover patterns, circular clearance cavities, side service openings and bolt patterns, dowel holes, and keyways. Lubrication definitions, incompletely positioned ribs, edge treatments, and manufacturing annotations remain structured source data rather than complete production geometry. It does not mean arbitrary future mechanical parts are already supported.

## Product behavior

### User flow

1. The user selects **3D CAD** and enters a natural-language specification.
2. The configured Intent API returns one JSON `CadSpec` document plus a short summary.
3. The local service validates units, ranges, references, conflicts, and required family parameters.
4. Critical ambiguity stops the build and is shown as a review item. Non-critical assumptions are explicit and editable.
5. The user can edit the normalized `CadSpec` JSON and build or rebuild the model.
6. The CAD worker constructs and validates a B-Rep solid.
7. The UI shows an isometric CAD preview, model metrics, warnings, and download actions.
8. The user can download the normalized `CadSpec`, STEP, OCCT BREP, STL, GLB, and SVG preview.

### Geometry truth and editability

- STEP and OCCT BREP contain the real boundary-representation result.
- `CadSpec` is the editable parametric source. Changing a parameter and rebuilding must change the solid deterministically.
- STL and GLB are tessellated viewing/manufacturing derivatives, not editable geometric truth.
- SVG is a generated isometric preview, not the CAD model.
- A STEP re-import must produce a valid solid with the same bounding box within 0.01 mm.

### Manufacturing semantics

Fits, dimensional tolerances, datum definitions, GD&T, roughness, thread designations, drawing-view definitions, and scale are retained as structured manufacturing data. Form features that affect the solid—holes, counterbores, pockets, chamfers, fillets, ribs, slots, and cavities—must be built geometrically.

Version 0.4 models tapped holes using their validated tap-drill/cylindrical geometry and retains the ISO thread designation as semantic data. It does not generate helical thread faces or claim STEP AP242 PMI support. Drawing sheets, associative dimensions, section hatching, and FCStd feature history are later milestones unless separately approved.

## CadSpec 1.0 contract

The top-level shape is intentionally bounded:

```json
{
  "version": "1.0",
  "kind": "precision_gearbox_housing_v1",
  "units": "mm",
  "parameters": {},
  "manufacturing": {
    "fits": [],
    "tolerances": [],
    "datums": [],
    "geometric_controls": [],
    "surface_finishes": [],
    "threads": []
  },
  "assumptions": [],
  "unresolved": []
}
```

Rules:

- All top-level fields are required.
- Unknown fields and unknown part families are rejected.
- Dimensions are finite millimetre values inside configured bounds; counts have small fixed maxima.
- Referenced axes, faces, holes, and patterns use stable IDs.
- `unresolved` entries have `severity`, `field`, `message`, and optional `choices`.
- Any `severity: "critical"` entry prevents geometry generation.
- Derived values record their equation and source fields so an edit can be traced.
- The normalized document and artifact manifest contain a SHA-256 source hash.

The gearbox family schema must include named groups for envelope, base, shell, axis system, stepped bearing seats, service openings, threaded patterns, lubrication ports, internal clearances, reinforcement, locating features, edge treatments, and manufacturing semantics. The complete JSON Schema is an implementation artifact and must reject an incomplete or contradictory model before invoking OCCT.

## Gearbox acceptance interpretation

The supplied specification establishes the following coordinate convention unless the user changes it during review:

- X: left to right over the 260 mm base length.
- Y: front to rear over the 190 mm base width.
- Z: upward from datum A, the bottom mounting plane.
- Front and rear shaft bores are parallel to Y.
- The main shaft is at X=0, Z=92.
- The second shaft defaults to X=+78, Z=92.
- The third shaft has Z=48 and a derived horizontal offset of `sqrt(62^2 - 44^2) = 43.6807 mm`; the approved default places it at X=+43.6807 mm (right).

The owner approved the following first-release interpretations:

1. Place the third shaft on the right and record the derived X value as an editable assumption.
2. Give the explicit centered outer/inner envelopes precedence over the nominal 10 mm wall value.
3. Center each internal circular clearance cavity over its shaft along Y.
4. Use STEP + BREP + CadSpec as the first editable artifact set; FCStd is not required.
5. Provide an interactive Three.js GLB viewer plus SVG fallback.

Rib attachment locations and external bearing-boss outlines remain underdimensioned. Their values are preserved, but the current foundation must not be described as a manufacturing-ready interpretation of those omitted forms.

## Architecture

### Pipeline

```text
Natural language
  -> bounded Intent API response
  -> CadSpec JSON Schema validation
  -> domain conflict/derivation pass
  -> CadQuery worker subprocess
  -> OCCT B-Rep validity and metric checks
  -> STEP/BREP/STL/GLB/SVG artifacts
  -> local artifact manifest and browser preview
```

### Application boundary

- The existing M1→M2→M3 Proposal pipeline remains available as **2D SCC** for compatibility.
- The 3D path uses a new `CadSpec` contract. It does not coerce 3D features into 2D `Proposal` entities.
- Node owns HTTP validation, request bounds, Intent API configuration, subprocess lifecycle, artifact lifecycle, and same-origin download routes.
- Python owns typed CAD construction and OCCT export only. It receives JSON on stdin and writes only to a request-scoped output directory supplied by Node.
- The CAD worker has no network access and never evaluates user or model source code.
- Artifacts are temporary, process-local, size-bounded, and deleted on expiry or service restart. No persistent volume is introduced in v0.4.

### API

- `POST /api/cad/intent`: natural language to validated `CadSpec`; shares the in-memory Intent provider configuration.
- `POST /api/cad/build`: validated `CadSpec` to artifact manifest.
- `GET /api/cad/artifacts/:jobId/:name`: same-origin artifact download using an opaque job ID and allowlisted filename.
- `DELETE /api/cad/artifacts/:jobId`: delete exactly one generated job.

Build requests have a 60-second timeout, a 1 MiB JSON limit, at most one active build per process, and a 64 MiB artifact budget per job. Structured errors use `SCC-CAD-NNN` codes.

### Project structure

```text
apps/scc-ui/lib/cad-service.mjs       Node worker and artifact boundary
apps/scc-ui/lib/cad-spec.mjs          CadSpec validation and derivation
apps/scc-ui/cad/requirements.txt      pinned Python dependencies
apps/scc-ui/cad/worker.py             bounded JSON worker entrypoint
apps/scc-ui/cad/aicad_cad/            typed builders and exporters
apps/scc-ui/cad/tests/                 Python geometry tests
apps/scc-ui/test/cad-*.test.mjs        Node/API contract tests
apps/scc-ui/public/js/cad-viewer.js    browser 3D viewer
examples/gearbox-housing.cadspec.json  reviewed acceptance input
docs/adr/adr-013-3d-cad-kernel.md      architecture decision
```

## Implemented tech stack

- CadQuery `2.8.0`, Apache-2.0, installed only inside the project container.
- OCCT through CadQuery's pinned OCP wheels; no host installation.
- Python 3.12 container/runtime, pinned to a concrete patch tag and image digest during implementation.
- Existing Node 24 service and Rust SCC CLI remain pinned and unchanged for the 2D pipeline.
- Node standard library for subprocess and artifact handling; no arbitrary-code sandbox package.

CadQuery was selected because its API builds parametric OCCT solids and exports STEP, BREP-compatible shapes, STL, SVG, and assembly glTF/GLB. The owner approved the material Docker image-size increase and container-only dependency boundary.

## Commands

Implemented verification commands:

```bash
# Focused schema and service tests
node --test apps/scc-ui/test/cad-spec.test.mjs apps/scc-ui/test/cad-service.test.mjs

# CAD kernel tests inside the pinned image
docker compose run --rm --no-deps cad-test

# Build the integrated UI image
docker compose build ui e2e cad-test

# Full existing Node/CLI regression
SCC_BIN="$PWD/target/release/scc" npm test --prefix apps/scc-ui

# Browser acceptance
docker compose run --rm --no-deps e2e
```

## Code style

Builders are pure with respect to the input document and return both a shape and stable feature records:

```python
def build_gearbox(spec: GearboxSpec) -> BuildResult:
    base = build_rounded_base(spec.base)
    shell = build_housing_shell(spec.shell)
    shape = base.union(shell)
    shape = cut_axis_system(shape, spec.axes)
    return validate_result(shape, feature_ids=spec.feature_ids())
```

- Python uses explicit dataclasses/type hints and `snake_case`.
- JavaScript remains ESM and uses small dependency-injected services.
- Stable IDs and sorted output are required wherever order affects artifacts or tests.
- Selector-heavy fillet/chamfer operations must be isolated and tested because topology naming can change after booleans.

## Testing strategy

### Small tests

- CadSpec schema, bounds, unknown fields, critical ambiguity gate, and deterministic derivations.
- Intent response parsing and one bounded correction request.
- Artifact filename, path, job ID, TTL, size, and timeout enforcement.

### Medium integration tests

- Build simple box, cut hole, counterbore, rounded pocket, rib, and circular/linear pattern fixtures through the real CadQuery worker.
- Re-import every STEP fixture and assert valid solids, volumes, bounding boxes, and expected hole/feature counts.
- Build the gearbox acceptance model and assert 260×190×165 mm bounds, valid solid count, nonzero volume, through bores, cavity cuts, and stable artifact manifest.
- Change one source parameter, rebuild, and verify the corresponding B-Rep metric changes.

### Large browser test

- Configure a fake deterministic Intent provider.
- Submit the gearbox natural language.
- Review normalized parameters and an explicit ambiguity.
- Resolve the ambiguity, build, display the SVG preview, edit one parameter, rebuild, and download JSON and STEP.
- Verify no browser errors at 1440, 1024, 768, and 320 px.

## Boundaries

### Always

- Validate model output before CAD execution.
- Build real B-Rep geometry and verify OCCT validity before reporting success.
- Distinguish geometric features from metadata-only manufacturing semantics.
- Expose every assumption and contradiction to the user.
- Preserve the existing 2D pipeline and its tests.

### Ask first

- Add or update CAD/Python/browser dependencies.
- Pull new remote images or publish materially larger images.
- Add persistent artifact storage, host mounts, external ports, or GPU access.
- Introduce a new supported part family or enable arbitrary feature scripts.

### Never

- Execute model-generated source code.
- Claim that meshes or previews are editable B-Rep CAD.
- Claim helical threads, GD&T/PMI, drawing sheets, or unresolved dimensions are implemented when they are metadata only.
- Return a successful model if OCCT reports an invalid or empty solid.
- Send generated geometry or source specifications to any endpoint except the explicitly configured Intent API.

## Success criteria

- Natural-language gearbox input yields a strict normalized `CadSpec`, not SCC 2D Proposal JSON.
- Critical conflicts are visible and block `manufacturing_ready` status.
- After resolving required assumptions, the real CAD worker creates a valid solid.
- The solid bounding box is 260×190×165 mm within 0.01 mm.
- STEP re-import succeeds and preserves the bounding box within 0.01 mm.
- STEP, BREP, STL, GLB, SVG, and source JSON downloads are non-empty and content-type correct.
- Changing `parameters.envelope.base_length` changes the rebuilt B-Rep bounding-box length and source hash.
- Existing 2D SCC tests remain green.
- Full Node/CLI, Python CAD, Docker build, and browser E2E suites pass without skipped acceptance tests.

## Approval record

On 2026-08-10 the owner approved CadQuery/OCP in Docker, the right-side third shaft, explicit-envelope precedence, STEP + BREP + CadSpec as the editable artifact set, and an interactive GLB viewer. Remaining form ambiguities are documented as implementation boundaries rather than silently claimed as solved.
