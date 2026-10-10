# ADR-013: Bounded 3D CAD kernel and CadSpec boundary

- Status: Accepted
- Date: 2026-08-10
- Owners: SCC architecture

## Context

M3 intentionally solves deterministic two-dimensional primitives and exports SVG/DXF. A production mechanical housing requires solid booleans, stepped bores, pockets, ribs, chamfers, fillets, B-Rep validity checks, tessellation, and STEP interchange. Extending the current 2D `GeometryPrimitive` enum cannot provide those semantics without replacing the M3 contract.

Natural-language providers are also untrusted. Letting a provider emit executable CadQuery or FreeCAD scripts would turn ordinary design input into arbitrary code execution.

## Decision

Add a parallel 3D CAD path for v0.4.0:

1. `CadSpec 1.0` is a strict versioned, data-only parametric contract.
2. The first supported family is `precision_gearbox_housing_v1`.
3. Node validates and derives the document, owns subprocess and artifacts, and never evaluates model code.
4. A pinned Python 3.12/CadQuery 2.8.0 worker uses the OCCT kernel to construct and validate B-Rep solids.
5. STEP and BREP are geometry truth; JSON is parametric source; mesh and SVG outputs are previews/derivatives.
6. M1→M2→M3 remains unchanged for the existing 2D workflow.

## Why CadQuery

CadQuery provides a parameter-oriented API over OCCT, supports the required construction operations, and officially supports STEP plus tessellated and SVG exports. Its Python boundary also allows the heavy CAD dependency to remain container-only.

## Rejected alternatives

- **Treat 2D circles as a 3D housing:** false success and no editable solid.
- **Execute language-model Python:** unacceptable arbitrary-code boundary.
- **OpenSCAD as the geometry truth:** mesh/CSG workflow does not meet the editable STEP B-Rep requirement.
- **FreeCAD GUI automation:** larger GUI/runtime surface and brittle UI automation; it can remain an optional downstream STEP editor.
- **Rewrite OCCT modeling in Rust immediately:** much larger unsafe FFI and topology-management effort before validating the product workflow.
- **Embed the CAD kernel in the browser:** excessive payload and weaker resource isolation for the first release.

## Consequences

- The UI image becomes materially larger and its first build downloads pinned Python/OCP dependencies.
- Linux container execution becomes the reference CAD environment; the browser remains cross-platform.
- A family schema is less superficially general than model-generated scripts, but it is testable, editable, and safe.
- Each new part family or generic feature type requires schema, builder, and real-kernel tests.

## Compatibility

Existing Proposal, ConstraintIR, GraphState, GeometryModel, CLI commands, and 2D exports do not change. New routes and artifacts are additive.

## Approval record

The repository owner approved the container-only CadQuery/OCP dependency, STEP + BREP + CadSpec artifact set, and interactive GLB preview on 2026-08-10. The accepted coordinate/default interpretations are recorded in the feature specification.
