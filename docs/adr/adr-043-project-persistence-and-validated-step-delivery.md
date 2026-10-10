# ADR-043 — Source-only project persistence and validated STEP delivery

Status: Accepted for Phase 10.4

## Context

`EngineeringModel` is the authoritative editable design state. `SolidBuildSpec`,
`SpatialModel`, drawing data, CadQuery objects, and STEP bytes are derived or
exchange artifacts and must not become project truth. The product also needs a
local `.aicad.json` save/open flow and a real STEP download path without a
server-side artifact store.

## Decision

An AICAD project is versioned as `AicadProject v1`:

```text
{ schema_version: 1, metadata: { name, description? }, engineering: EngineeringModel }
```

The Rust `scc-project` crate owns validation, strict unknown-field rejection,
deterministic JSON serialization, and expression/pattern/source preservation.
The browser treats the project as a file transport and never reconstructs
semantic model state. Save is a canonical JSON download; open validates before
replacing the current snapshot. No project database or server persistence is
used.

`POST /api/engineering/export/step` accepts only an `EngineeringModel`. Rust
performs EngineeringModel → lowering → SpatialModel → SolidBuildSpec, then
`SccCadBackend` invokes the configured CadQuery Python worker. The worker builds
one source solid, exports a private `.step`, reimports it through CadQuery/OCP,
and returns witnesses. Rust reads the same validated file, checks its size and
SHA-256, and the Node endpoint delivers those exact bytes with matching headers.
There is no STEP→EngineeringModel path and no user-controlled output path.

The UI tracks a source-model fingerprint and a separate STEP export generation.
A stale export result is discarded when the model changes while the worker is
running; drawing mode, derived previews, and export status do not affect the
source fingerprint.

## Rejected alternatives

- Persisting `SolidBuildSpec` or STEP as project truth: couples editable state to
  a backend and loses source expressions/provenance.
- Re-exporting STEP after validation: breaks exact-byte identity.
- Accepting a browser output path or storing artifacts on the server: creates an
  unsafe and non-reproducible filesystem boundary.
- Reconstructing EngineeringModel from STEP: loses semantic intent and patterns.

## Impact and migration

The additive `scc-project` crate, CLI project-save/open commands, real CAD
backend artifact transport, UI file controls, and endpoint tests implement the
contract. Existing `CadDocument` compatibility routes remain unchanged. No
Phase 10.5 feature work is included.
