# Architecture

AICAD compiles an engineering model into drawings and a checked STEP file.
The Rust workspace is the authority. Node.js is the local service and the
only place that talks to an intent provider.

## Crates

All 18 workspace members are part of the public snapshot:

| Crate | Responsibility |
| --- | --- |
| `scc-core` | Shared ids, errors, and primitives |
| `scc-proposal` | Legacy constraint proposals |
| `scc-analysis` | Static proposal diagnostics |
| `scc-lowering` | Proposal to execution IR |
| `scc-execution` | IR execution |
| `scc-geometry` | Bounded deterministic solver |
| `scc-dsl` | AICAD DSL front end |
| `scc-tools` | 37 typed edit tools |
| `scc-engineering` | `EngineeringModel`, v1 features, v2 parametric history |
| `scc-spatial` | 2.5-D spatial model for the formal pipeline |
| `scc-projection` | Front / Top / Left projection |
| `scc-visibility` | Hidden-line classification |
| `scc-projection-consistency` | Fail-closed cross-check of the three models |
| `scc-drawing` | Derived drawing, SVG, and DXF |
| `scc-manufacturing` | `SolidBuildSpec` and formal manufacturing input |
| `scc-cad-backend` | CadQuery/OCP adapter and STEP re-import |
| `scc-project` | Source-only `.aicad.json` projects |
| `scc-cli` | `scc` binary |

## Two model paths

```text
v1 BaseProfile + Hole / Pattern / Slot
  -> geometry solver
  -> Spatial -> Projection -> Visibility -> Drawing
  -> SolidBuildSpec -> CadQuery -> STEP re-import

v2 ordered ParametricFeature history
  -> CadQuery/OCP rebuild (legacy Proposal solve is skipped)
  -> same drawing consumers
  -> FormalManufacturingInput::ProjectV2 -> same STEP re-import
```

`scc-project` writes schema version 2 when parametric features are present.
The file holds metadata and the `EngineeringModel`. It does not hold a mesh
or a provider transcript.

## What an LLM is allowed to do

`ADR-020` keeps the provider in Node.

- A new design must return DSL. The DSL lowers only to engineering commands.
- An edit must return typed tool calls. The registry is `standard_definitions()` in `crates/scc-tools/src/registry.rs`.
- The provider is untrusted. Repair is bounded. A second bad reply is a rejection, not a guessed solid.

## STEP authority

Formal delivery:

- CLI: `scc engineering export-step <model.json>`
- HTTP: `POST /api/engineering/export/step`

Both call the Rust exporter. The exporter builds a solid, writes a private
temporary STEP, re-imports it with CadQuery/OCP, checks witnesses, and
returns those bytes only when equivalence matches.

Compatibility delivery:

- HTTP: `POST /api/cad/build`
- Worker: `python -m aicad_cad.worker` via `execFile` with `shell: false`
- A requested STEP format, or a worker that emits a STEP artifact, fails with `SCC-CAD-010` (`apps/scc-ui/lib/cad-service.mjs`).

Drawing SVG/DXF are presentations of `DrawingModel`. They are not a STEP source.

## Fail-closed geometry

The v1 solver does not search a general constraint space. Supported relations
are Fixed, Datum, CenterOf, a point Distance check, Align, Parallel,
Perpendicular, ArrayRelation, and InstanceOf.

v2 Sweep is fixed-binormal, open, planar, and C1 (`ADR-075`). v2 Loft is
ruled, aligned, and parallel (`ADR-076`). G7C adds selectors for sweep and
loft caps and lateral faces. Those tests stay skipped unless
`AICAD_RUST_PARAMETRIC_INTEGRATION=1`. On this snapshot the gated Loft case
passed and the gated Sweep case failed closed. The set is not accepted.

## Where to read next

- Decision history: [../adr/README.md](../adr/README.md). An ADR is true for the day it was accepted. Later ADRs narrow or extend it.
- HTTP surface: [../api/http.md](../api/http.md)
- Bounds: [../limitations/current.md](../limitations/current.md)
