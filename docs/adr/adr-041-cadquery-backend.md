# ADR-041: SolidBuildSpec CadQuery Backend

Status: Accepted for Phase 10.2

## Context

Phase 10.1 freezes `SolidBuildSpec` v1 as the manufacturing truth for the
rectangle body and resolved cylindrical cuts. The existing Python
`CadDocument`/`FeatureGraph` worker is a separate compatibility backend and
must not become an intermediate editable model for the EngineeringModel path.

## Decision

Add a stateless `SolidBuildSpec` transport DTO and a direct CadQuery executor:

```text
SolidBuildSpec JSON → strict Python DTO → CadQuery/OCP solid → SolidBuildResult
```

The Python worker receives only the serialized v1 contract. It validates the
schema version and every required physical field, uses millimetres in the
PartLocal frame, constructs the rectangle at the supplied origin, and applies
each `cylindrical_cut` using its supplied center, radius, Z interval, and
`positive_z` axis. It does not evaluate expressions, re-lower Through/Blind,
regenerate patterns, inspect EngineeringModel/GeometryModel/SpatialModel, or
parse opaque operation IDs.

The result contains only deterministic measurable metadata: validity, body and
solid counts, operation count, bbox, volume, operation status/provenance, and
body provenance. CadQuery objects are ephemeral and never cross the transport
boundary. Builds are atomic: a failed operation returns a structured error and
never a partial success. An outside-body cut is recorded as
`no_material_change`; overlapping cuts execute in input order without volume
additivity assumptions; kernel exceptions are sanitized into stable errors.

The Rust `scc-cad-backend` adapter only serializes the frozen spec, invokes a
configured worker without a shell, validates the structured response, and
checks result cardinality/provenance. It performs no geometry calculation.

The tested reference runtime is Python 3.12.11, CadQuery 2.8.0, and OCP
7.9.3.1. STEP, BREP, STL, GLB export, reimport equivalence, UI integration,
and persistent backend state remain out of scope for Phase 10.2.

## Rejected alternatives

- Routing through `CadDocument` or `FeatureGraph` would add editable truth and
  reintroduce parameter semantics at the backend boundary.
- Recomputing hole depth or pattern positions in Python would create a second
  manufacturing truth.
- Centering the body at the origin would corrupt non-zero PartLocal origins.
- Returning a CadQuery/TopoDS object or backend handle would make transport and
  lifetime non-deterministic.
- Exporting STEP in this phase would collapse the Phase 10.3 delivery boundary.

## Consequences

Phase 10.2 proves real kernel execution from the frozen cross-language
contract. `scc-manufacturing` remains backend-neutral and unchanged
semantically. Phase 10.3 owns STEP delivery and reimport validation.
