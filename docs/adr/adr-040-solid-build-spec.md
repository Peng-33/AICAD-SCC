# ADR-040: Backend-neutral SolidBuildSpec

Status: Accepted for Phase 10.1

## Context

`EngineeringModel` is the sole engineering truth. `GeometryModel` is solved XY
geometry and `SpatialModel` is resolved 2.5-D manufacturing geometry. The
existing CadQuery worker consumes a separate `CadDocument` compatibility
contract, so a formal backend boundary is required before any solid backend is
connected.

## Decision

Add `scc-manufacturing` as a derived leaf over `scc-spatial`. Its public entry
point is:

```rust
pub fn lower_solid_build_spec(
    spatial: &SpatialModel,
) -> Result<SolidBuildSpec, SolidBuildError>
```

`SolidBuildSpec` is read-only, deterministic, strongly typed, fail-closed, and
backend-neutral. It contains one resolved rectangular prismatic body, resolved
cylindrical subtractive operations, typed provenance, and schema version 1.
All dimensions are millimetres in the canonical PartLocal frame: XY is the
base plane and +Z is the thickness direction.

The lowerer consumes the solved rectangle, body Z interval, cut center/radius/Z
interval, and spatial provenance directly. It does not read EngineeringModel
parameters or expressions, re-resolve thickness, inspect Through/Blind to
recompute Z, regenerate pattern coordinates, or copy a parameter graph. One
spatial pattern may produce N operations; every operation maps to its opaque
`SpatialCutId` without parsing the ID. Construction circles produce no
operation.

The spec has no CadQuery vocabulary, Python calls, STEP/BREP/STL/GLB export,
BRep, mesh, Boolean kernel, drawing, projection, or visibility dependency.
Invalid finite/positive geometry, malformed body bounds, missing source
provenance, unsupported feature variants, and source cardinality violations
fail closed with `SCC-MFG-*` structured errors. Outside-body and overlapping
cuts are preserved as authoritative spatial operations; this phase adds no
Boolean topology policy.

Serialization is for deterministic transport and testing, not editable project
persistence. `BTreeMap` ordering and opaque numeric operation IDs make equal
spatial inputs byte-identical. A future backend may consume this contract via
CadQuery, OCCT, or another kernel, but may not read EngineeringModel directly.

## Rejected alternatives

- A CadQuery command DTO would make the formal boundary backend-specific.
- Re-reading engineering width, thickness, HoleExtent, PCD, or pattern layout
  would create a second manufacturing truth.
- Depending on Drawing, Projection, or Visibility would make physical output
  depend on presentation.
- Emitting a final solid or running Boolean operations in Rust would move the
  Phase 10.2 backend into this phase.

## Consequences

Phase 10.1 establishes `EngineeringModel → SpatialModel → SolidBuildSpec`.
Phase 10.2 owns `SolidBuildSpec → CadQuery Backend`; Phase 10.3 owns STEP
delivery and re-import equivalence. The current Python CadDocument worker is
unchanged and remains a legacy compatibility backend.
