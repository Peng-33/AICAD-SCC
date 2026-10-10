# HTTP API

The server is `apps/scc-ui/server.mjs`. Defaults: `127.0.0.1:4173`.
`GET /` and `GET /index.html` redirect to the Engineering Workbench.

Routes below are the ones the server matches. Request bodies are JSON unless
noted. Engineering mutations that change a project check the browser origin.

## Health and catalogs

| Method | Path | Role |
| --- | --- | --- |
| GET | `/api/health` | `{ status, version, intentConfigured }`. `version` is the CLI version string, for example `scc 0.4.0-alpha.1`. |
| GET | `/api/examples` | Listed examples. |
| GET | `/api/examples/:id` | One example. |
| GET | `/api/cad-skills` | The six CAD skills under `skills/cad/`. |
| GET | `/api/model-presets` | Endpoint suggestions. Model ids are still user input. |
| GET | `/api/model-configs` | In-memory provider configs. Secrets are not returned in full. |
| POST | `/api/model-configs` | Create or replace an in-memory config. |
| GET, PATCH | `/api/intent-config` | Older single-config surface. Still local memory. |

## Formal engineering

| Method | Path | Role |
| --- | --- | --- |
| POST | `/api/engineering/dsl` | Body `{ dsl }`. Solves through the Rust CLI. |
| POST | `/api/engineering/tool` | Apply one typed tool call to a model. |
| POST | `/api/engineering/intent` | New or edit intent. Requires a configured provider for model calls. |
| POST | `/api/engineering/intent/confirm` | Commit a preview that still matches the fingerprint. |
| POST | `/api/engineering/workbench` | Derived panels for a model. |
| POST | `/api/engineering/drawing` | Principal drawing JSON. |
| POST | `/api/engineering/drawing/three-view` | Front/Top/Left drawing JSON. |
| POST | `/api/engineering/drawing/section` | Section A-A. `sectionPlane.axis` is `x`, `y`, or `z`; `offset_mm` is finite; `view_side` is `positive` or `negative`. |
| POST | `/api/engineering/project/save` | Source-only project JSON from a model plus a name. |
| POST | `/api/engineering/project/open` | Canonicalize a project and return the model. |
| POST | `/api/engineering/export/step` | Formal STEP. This is the manufacturing authority. |
| POST | `/api/engineering/skills/plan` | Skill plan for a request. A plan is not a solid. |

`POST /api/engineering/export/step` reads `model` and an optional
`projectName`. The CLI adapter runs `scc engineering export-step`. The
response is the validated artifact, not a promise that some other CAD system
will edit it. Success is HTTP 200 with `content-type: application/step` and
`x-aicad-step-manufacturing`, `x-aicad-step-build`, `x-aicad-step-reimport`,
and `x-aicad-step-equivalence` all set to `passed`.

`POST /api/engineering/drawing/section` requires an EngineeringModel v2
parametric history. A v1 model fails closed with HTTP 422, code `SCC-DRW-024`,
and `details.parametric_code` `SECTION_PARAMETRIC_REBUILD_REQUIRED`. The
mounting-plate fixture is v1. Its three-view route can still return SVG.

## Compatibility and legacy

| Method | Path | Role |
| --- | --- | --- |
| POST | `/api/compile`, `/api/execute`, `/api/inspect`, `/api/solve`, `/api/export` | Legacy SCC proposal pipeline. Not the Engineering Workbench. |
| POST | `/api/intent` | Older intent compiler. |
| POST | `/api/cad/build` | CadDocument / FeatureGraph build. Non-STEP artifacts only. |
| POST | `/api/cad/intent` | Compatibility CAD intent. |
| GET | `/api/cad/examples/gearbox-housing`, `/api/cad/examples/generic-flange` | Compatibility fixtures. |
| POST | `/api/documents/extract` | Document text extraction for intent input. |

`POST /api/cad/build` calls `cadService.build`. If the requested format name
is `step`, or the worker manifest contains a `step` artifact, the service
throws `SCC-CAD-010` and tells the caller to use the EngineeringModel export.
That rejection is the formal-STEP boundary.

## CLI equivalents

| CLI | Corresponds to |
| --- | --- |
| `scc engineering dsl <file>` | Model JSON from DSL. `--solve` adds proposal and geometry. |
| `scc engineering drawing-three-view <model.json>` | Three-view drawing JSON. |
| `scc engineering drawing-section <model.json> --axis <x\|y\|z> --offset-mm <n>` | Section A-A. |
| `scc engineering export-step <model.json>` | Formal STEP on stdout. Status lines on stderr. |
| `scc engineering project-save` / `project-open` | `.aicad.json` round trip. |
| `scc engineering manifest` | The 37 typed tools. |

Legacy `scc compile`, `execute`, `solve`, and `export` remain for proposal
files under `examples/`. They do not emit formal STEP.
