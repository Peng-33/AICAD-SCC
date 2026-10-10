# ADR-042: STEP Delivery and Reimport Validation

Status: Accepted for Phase 10.3

## Context

Phase 10.2 freezes a direct, stateless `SolidBuildSpec` → CadQuery backend.
STEP is needed as an exchange artifact, but it must not become project truth or
an alternate EngineeringModel input.  A successful export alone does not prove
that the delivered artifact preserves the source solid.

## Decision

Add an isolated delivery boundary:

```text
SolidBuildSpec → CadQuery Solid → STEP → CadQuery/OCP reimport → witnesses
```

The delivery worker builds the source solid exactly once, writes a generated
`.step` file inside a private `TemporaryDirectory`, computes size and SHA-256,
reimports the actual file through `cq.importers.importStep`, and returns only
structured metadata.  No user path, artifact bytes, CadQuery object, or
persistent handle crosses the boundary.

Acceptance requires one valid solid on both sides, finite positive measurements,
per-axis bbox agreement, scaled volume agreement, and material probes performed
on both the source and reimported shapes.  Through, blind-base, blind-opposite,
non-zero-origin, negative-coordinate, overlapping, outside-body, and pattern
fixtures are covered.  STEP bytes are hashed for traceability but byte identity
is not a determinism gate; semantic witnesses are the gate.

The Rust `scc-cad-backend` adapter reuses the Phase 10.2 no-shell subprocess
transport and validates the STEP artifact/witness/provenance DTO.  It performs
no geometry calculation.  `scc-manufacturing` remains unchanged and
backend-neutral.

## Rejected alternatives

- Treating STEP text or topology counts as geometry validation would not prove
  a kernel reimport and would create a fragile format parser.
- Returning a caller-controlled output path would create traversal, symlink,
  and cleanup hazards.
- Rebuilding independently for export and validation could hide a build-path
  divergence, so the same source shape is exported and then reimported.
- Converting STEP back into `EngineeringModel` would make an exchange format
  a second project truth and is explicitly prohibited.
- Adding STEP to the frozen `SolidBuildSpec` would couple a backend-neutral
  manufacturing snapshot to delivery concerns.

## Consequences

Phase 10.3 provides a real Docker/CadQuery STEP closure with bounded,
structured failure behavior.  It does not add UI integration, HTTP download,
product export controls, or new CAD feature vocabulary.  Browser delivery and
persistent artifact management remain later work.
