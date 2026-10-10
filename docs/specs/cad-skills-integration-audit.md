# CAD Skills integration audit and boundary

Updated: 2026-09-24

## Audit classification

| Classification | Existing or added assets | Evidence and boundary |
|---|---|---|
| `ALREADY_IMPLEMENTED` | `crates/scc-tools/src/registry.rs`, `crates/scc-engineering/src/command.rs`, `crates/scc-engineering/src/executor.rs`, `crates/scc-dsl/` | Rust owns the 22-tool registry, `EngineeringCommand`, DSL parsing, atomic application, and EngineeringModel validation. |
| `ALREADY_IMPLEMENTED` | `crates/scc-spatial/`, `crates/scc-projection/`, `crates/scc-visibility/`, `crates/scc-projection-consistency/`, `crates/scc-drawing/` | Formal spatial lowering, projection, visibility, consistency validation, DrawingModel, typed annotations, SVG, and DXF already exist. The new layer does not replace these gates. |
| `ALREADY_IMPLEMENTED` | `apps/scc-ui/lib/intent-service.mjs`, `provider-openai-compatible.mjs`, `engineering-intent.mjs`, `public/js/engineering-model-manager.js` | Provider profiles, activation, connection checks, Engineering Intent protocol checks, safe key redaction, and provider request snapshots already exist. The Skills work reuses these services. |
| `REUSABLE` | `apps/scc-ui/public/engineering.html`, `public/js/engineering-page.js`, `public/js/engineering-workbench.js`, `public/js/engineering-i18n.js` | Existing single Workbench, Chinese default, English switch, drawing controller, project workflow, and error mappings are the user surface for Skill progress and evidence. |
| `MISSING` | `skills/cad/*/skill.json`, `apps/scc-ui/lib/cad-skills/` | The repository had no built-in CAD Skill contracts, checked registry/router, request analysis, one-use plan binding, Skill diagnostics report, or independent export report. Those are added here. |
| `MISSING` | CLI/UI evidence transport | The formal pipeline returned SVG/DXF but not typed view/annotation evidence or the exact compiled `EngineeringCommand` list. The CLI now returns additive read-only evidence from the same solved command batch and DrawingModel. |
| `UNKNOWN_REQUIRES_VERIFICATION` | External Provider and container state | Browser acceptance used a local fake Provider. External DeepSeek/Qwen calls and current live-container build identity were not verified in this task. No key was read or used. |
| `UNSUPPORTED` | Advanced geometry and annotation | The current EngineeringModel/ToolRegistry has no general solid boss/rib/draft/chamfer/countersink/counterbore feature chain. General GD&T/control frames, datum symbols, surface roughness notation, fit-class resolution such as H7, and standards-compliance validation are not claimed. |

## Six built-in Skills

1. **Mechanical Part Analysis** extracts source spans for explicit dimensions, reads derived values only from the actual `EngineeringContext`, and separates ambiguity, conflicting labels, missing inputs, and recognized unsupported requirements. It is deterministic and does not modify the model.
2. **Engineering Model Planning** snapshots the current model fingerprint and Rust manifest. A two-minute, single-use process-memory plan is bound to mode, effective input, and fingerprint. Edit tool schemas are the intersection of declared permissions and the actual Rust registry; `create_part` is removed for an existing part.
3. **Orthographic Drawing** calls the existing formal three-view endpoint. Front, Top, and Left evidence comes from the same DrawingModel that feeds both renderers.
4. **Drawing Annotation** reports only typed, feature-linked dimensions and center geometry in that DrawingModel. Unsupported, ambiguous, and recognized-but-not-rendered requirements stay separate.
5. **CAD Export Validation** checks the actual SVG root and unique view groups independently from DXF's HEADER units, ENTITIES section, entity count, EOF, and model fingerprint.
6. **Engineering Diagnostics** maps actual codes and supplied model facts to bilingual recovery guidance. It does not run recovery commands or mutate the EngineeringModel.

The definitions are declarative JSON, schema-checked when the service starts, and have no script or executable fields. The router invokes only the fixed in-process implementations; it does not load arbitrary Skill code.

## State, provider, and geometry safety

- `EngineeringModel` remains the only mutable engineering truth. DrawingModel is transient derived evidence and is not added to project files.
- New-design DSL is parsed and converted to real `EngineeringCommand` values by Rust. Edit tool calls are checked against this plan's allowlist before the existing Rust ToolRegistry/CommandExecutor runs.
- The server keeps a bounded set of one-use plans in process memory for two minutes. Plans contain no Provider credentials. They are discarded after use, expiry, or process restart.
- Provider configuration continues through the existing Provider Manager. No parallel provider store was added, no system-level client configuration is modified, and no API key is returned by these endpoints.
- Both drawing exports are generated by the existing formal Rust DrawingModel renderers. The CLI's new report exposes typed annotation provenance and view counts; it does not create another drawing model.
- Formal supported annotation evidence currently includes typed linear/diameter/overall-diameter/pattern/slot callouts, hole location callouts, center marks/centerlines, and numeric tolerance specifications when already attached to the model. The existence of an annotation does not establish ISO/ASME standards compliance.

## Acceptance boundary

A fake-provider browser run and Rust-backed tests exercise the application pipeline without making a paid model request. The full MMF-204 input is only analyzed against the capability map in automated tests; it is not generated. A real DeepSeek request, real MMF-204 provider run, and browser acceptance with a live provider remain `NOT_RUN`.

## Verification results (2026-09-24)

| Gate | Result | Evidence |
|---|---|---|
| Six declarative Skill contracts, registry, router, request analysis, plan binding, diagnostics, and export checks | `PASS` | `npm test`; the dedicated contract/integration suites pass, including tests A–F. |
| Node/API suite | `PASS` | 151 passed, 0 failed, 6 existing formal STEP delivery tests skipped. |
| Rust workspace | `PASS` | `cargo test --workspace -q` passed; 2 existing ignored tests remained ignored. |
| Typed numeric tolerance transport | `PASS` | A Rust CLI drawing test attaches a bilateral tolerance to a typed EngineeringModel dimension and observes it in the report from the formal DrawingModel. A Node regression also verifies that an equal-valued linear dimension cannot satisfy a requested diameter annotation. |
| Chromium browser acceptance | `PASS` | Real Chromium with an isolated fake Provider exercised `/` → Engineering Workbench, zh-CN/en, two saved local profiles, activation, multiline Chinese Engineering Intent, actual Rust-generated orthographic drawing and SVG/DXF downloads. Screenshot: `/tmp/aicad-cad-skills-workbench-final.png`. |
| Real DeepSeek/Qwen connectivity and live-provider Engineering Intent | `NOT_RUN` | No live or paid provider request was made; no API key was read, printed, migrated, or changed. |
| MMF-204 full-part generation with a real Provider | `NOT_RUN` | The request was analyzed against the declared capabilities only. The integration test confirms it is not sent to a paid Provider and not reported complete. |
| Current running 4173/4175 container identity | `NOT_RUN` | Both pre-existing services were left untouched. The fake-provider acceptance server used port 4178 and was stopped after the browser run. |

The Node/API tests exercise real Provider-independent route logic with a deterministic fake Provider. This establishes the code path and protocol validation behavior; it does not establish an external Provider's availability, credentials, model compatibility, or service-side persistence. Existing Provider profiles remain governed by the existing runtime service and its memory-backed restart contract.

The formal STEP delivery route and CAD backend were not modified. Their existing tests include environment-dependent skips; this task's acceptance focused on Skills, Engineering Intent, three-view drawing, and SVG/DXF.
