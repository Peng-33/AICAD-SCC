# SCC v0.3.0 Local Acceptance Report

- Date: 2026-08-09
- Branch: `codex/v0.3.0-geometry-foundation`
- Scope: M3 geometry primitive completion, array expansion, deterministic CAD export, and local release readiness
- Result: **Locally accepted; remote Windows/Linux CI accepted**

## Accepted functionality

- Polygon GeometryModel construction from explicit ordered vertices, including finite-value, unique-vertex, count, and nonzero-area checks.
- Linear, grid, and circular array pattern preservation through ConstraintIR and GraphState.
- Deterministic array expansion with stable `array_id[index]` identities and strict paired-relation validation.
- Deterministic SVG export for all GeometryPrimitive variants.
- Deterministic ASCII DXF AC1027 export for Point, Line, Circle, Arc, Rectangle, and Polygon.
- `scc export` with SVG/DXF format selection, atomic output, and existing `--force` protection.
- Legacy and schema-1.x GeometryModel input support for export.
- Proposal-to-CAD acceptance coverage for Polygon and array examples.
- Reproducible release-mode performance and convergence baseline tests.

## Verification evidence

The following checks completed successfully with Rust 1.96.0:

```bash
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release --workspace
SCC_BIN=<release-binary> python3 scripts/smoke-test.py
```

Results:

- 335 tests passed.
- 2 manual release-mode performance tests were ignored during the ordinary suite by design and passed when invoked explicitly.
- Clippy passed with warnings denied.
- Rustfmt check passed.
- Release workspace build passed.
- The smoke test passed compile determinism, overwrite protection, execute determinism, conflict reporting, solve, SVG export, DXF export, and temporary-file cleanup.
- SVG and DXF outputs matched byte-for-byte Golden fixtures.
- `git diff --check` and Python smoke-script bytecode compilation passed.

## Remote CI evidence

- Draft PR [#3](https://github.com/Peng-33/AICAD/pull/3) targets the released v0.2.0 `master` baseline.
- SCC CI run [31320569355](https://github.com/Peng-33/AICAD/actions/runs/31320569355) passed on commit `71624ab`.
- Format, Clippy, Coverage, Ubuntu Test, Ubuntu Smoke, Windows Test, and Windows Smoke all passed.
- The first Windows run exposed CRLF conversion in SVG/DXF Golden fixtures; commit `71624ab` fixed the checkout contract with explicit LF attributes, and the rerun passed.

## Performance and convergence evidence

On the recorded Fedora 44 / Intel Core i9-13980HX host in release mode:

- 10,000 explicit Point entities solved in 14,316 µs.
- 10,000 linear array instances expanded in 3,630 µs.
- Both cases reported `fully_constrained`, `exact`, and zero remaining degrees of freedom.

These are regression references rather than CI latency thresholds. Full details and reproduction steps are in [M3_Performance_Baseline.md](M3_Performance_Baseline.md).

## Compatibility assessment

- M2 remains independent of `scc-geometry`; solver and CAD code consume GraphState read-only.
- Existing non-array ConstraintIR and GraphState JSON remain compatible.
- Array ConstraintIR and GraphState now carry pattern parameters, intentionally changing deterministic roots and affected Golden snapshots.
- GeometryModel fields are unchanged; export is an additive API and CLI command.
- Legacy array states without pattern parameters fail explicitly with `SCC-GEOM-005` instead of producing guessed geometry.

## External release gates

The implementation and cross-platform CI are complete. The following release actions remain external and require repository-owner authority:

1. Review Draft PR #3 and mark it ready when appropriate.
2. Merge PR #3 into the released `master` lineage.
3. Apply required status checks if the repository plan supports branch protection.
4. Approve and push the `v0.3.0` release tag from the reviewed merge commit.
5. Download, checksum, and smoke-test the published Windows/Linux archives.

The implementation and CI-fix commits were pushed through Draft PR #3. No merge, tag, or release was created during this acceptance run.
