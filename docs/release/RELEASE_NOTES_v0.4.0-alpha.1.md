# AICAD v0.4.0-alpha.1

AICAD is an alpha CAD compiler. SCC, the Semantic Constraint Compiler, turns an editable EngineeringModel into drawings and validated STEP. Natural language can propose that model. A language model does not write the solid, the drawing, or the STEP file.

This is a public source alpha, not a stable 1.0 release.

## What is in this release

- Eighteen Rust workspace crates, version `0.4.0-alpha.1`
- The `scc` CLI
- The Engineering Workbench at `/engineering.html`
- The Node service and the Python CadQuery/OCP adapter
- Six CAD skills under `skills/cad/`
- ADR 001–076, English `README.md`, and Chinese `README.zh-CN.md`
- Examples and the test suites in this tree

The private backup history is not in this repository. `Peng-33/AICAD` and `Peng-33/AICAD-w11-backup-20261003` stay private.

## Architecture

Engineering intent becomes a typed EngineeringModel. A deterministic chain then solves or rebuilds that model, projects orthographic drawings, and exports STEP only after CadQuery/OCP build, re-import, and equivalence checks. The compatibility CAD route cannot return formal STEP.

## Verified CAD scope

Bounded extrude, revolve, hole, pattern, constant-radius fillet, equal-distance chamfer, fixed-binormal sweep, and ruled aligned loft. Front, Top, and Left drawings, and a principal Section A-A when the model is EngineeringModel v2. Formal STEP uses the engineering export path.

Gated G7C Sweep and Loft cases passed inside that narrow domain. An interior sweep hole and a 0.25 mm rim fillet passed. A sweep hole the same diameter as the tube end fails closed with `TOPOLOGY_REFERENCE_STALE`. Associative tolerance stays 1e-5 mm. Hidden-line chord tolerance stays 0.01 mm. Spatial curve deviation stays 0.05 mm and 0.1 rad.

Local evidence on this source: Rust fmt and clippy passed; workspace tests passed 1166, failed 0, ignored 2 performance baselines; gated Node tests passed 191, failed 0, skipped 0; Python unittest passed 119, failed 0. Playwright with a fake provider passed G3, G4, G5, G6B, G7A, and G7B. G6A.2 and G6A.3 failed and were not weakened. `browser-g6a.mjs` was not run. Real DeepSeek and Qwen calls were not run.

GitHub Actions on `main` is a separate result. `npm test` in that workflow does not set the gated CadQuery variables, so it does not repeat the G7C run.

## Known limits

- Not a general CAD replacement, and not a complex-part generator
- No assemblies, multi-body parts, complete NURBS, or freeform modeling
- No complete GD&T/PMI, title block, or standard sheet frame
- No verified FreeCAD interchange
- No published Docker image. OCCT notices for redistributed binaries are still incomplete
- This release uploads no CLI binaries and no container images

## Install

Rust 1.96.0, Node.js 20 or newer, and, for formal STEP, Python 3.12 with `cadquery==2.8.0`.

```bash
cargo build --locked --release -p scc-cli
npm ci --prefix apps/scc-ui
SCC_BIN="$PWD/target/release/scc" npm start --prefix apps/scc-ui
```

Open `http://127.0.0.1:4173`. Load the built-in mounting plate for the offline demo. Export STEP from the engineering export action. See `docs/getting-started/quickstart.md`.

## Contributing

See `CONTRIBUTING.md` and `SECURITY.md`. Report vulnerabilities privately. A skipped test is not a pass.

## License

Apache License 2.0 for original AICAD / SCC source. See `LICENSE`, `NOTICE`, and `THIRD_PARTY_NOTICES.md`. Third-party components, including Open CASCADE Technology, keep their own licenses.
