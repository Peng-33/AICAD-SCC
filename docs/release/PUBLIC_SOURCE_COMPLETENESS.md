# Public source completeness

The private baseline is commit `0568fac256178ab9b344995c1eab91861c292a7d`.
`git ls-files` there lists 568 tracked files. This snapshot keeps every
tracked application, test, skill, ADR, workflow, and example, except the
19 paths named in [EXCLUDED_FILE_MANIFEST.md](EXCLUDED_FILE_MANIFEST.md)
before one test fixture was restored.

Restored after the first exclusion pass:

- `SCC_UI_UX_Design_Specification.docx` is opened by `apps/scc-ui/test/documents.test.mjs`. Removing it failed that test. An XML text scan of the file found no home path, email, private-key block, or cloud key marker. The other eight root `.docx` files stay out.

## Modules that were not removed

| Surface | Present |
| --- | --- |
| Rust workspace crates | 18: `scc-analysis`, `scc-cad-backend`, `scc-cli`, `scc-core`, `scc-drawing`, `scc-dsl`, `scc-engineering`, `scc-execution`, `scc-geometry`, `scc-lowering`, `scc-manufacturing`, `scc-project`, `scc-projection`, `scc-projection-consistency`, `scc-proposal`, `scc-spatial`, `scc-tools`, `scc-visibility` |
| EngineeringModel v1 and v2 | `crates/scc-engineering` |
| Constraint solver | `crates/scc-geometry` |
| Spatial, projection, visibility, drawing | the four matching crates, plus `scc-projection-consistency` |
| Parametric history, Sweep, Loft, Revolve | `crates/scc-engineering/src/parametric.rs` and the CadQuery rebuild under `apps/scc-ui/cad` |
| STEP validation | `crates/scc-manufacturing`, `crates/scc-cad-backend`, `scc engineering export-step` |
| Workbench and Node service | `apps/scc-ui` |
| Python CAD worker | `apps/scc-ui/cad` |
| CAD skills | six directories under `skills/cad` |
| CLI | `crates/scc-cli` |
| Examples and tests | `examples/`, crate tests, `apps/scc-ui/test`, `apps/scc-ui/cad/tests` |
| CI | `.github/workflows/scc-ci.yml`, `.github/workflows/release.yml` |
| ADRs | ADR-001 through ADR-076, with no missing number |

No crate or skill was dropped to hide a commercial core. The files that are
absent are agent notes, internal plans, and office exports that the test
suite does not open.
