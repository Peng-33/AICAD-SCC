# Test report for 0.4.0-alpha.1

Date: 2026-10-10. Tree: the public-prep snapshot, after the G7C closure
pass. This is not a public release, and it is not a claim that every
historical acceptance report was re-run.

Ignored tests and skipped tests are not passes. The remaining G7C Sweep
failure is not a pass.

## Rust

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Exit 0 |
| `cargo test --workspace --all-targets --locked` with `SCC_CAD_PYTHON` set | Exit 0. 1166 passed, 0 failed, 2 ignored. The closed-profile STEP tests ran inside this command (2 passed, 2.53s) because `SCC_CAD_PYTHON` was set. They return immediately when that variable is unset. |
| `cargo build --release -p scc-cli` | Not rebuilt in this pass. The existing `./target/release/scc --version` still prints `scc 0.4.0-alpha.1`. Python and Node called that binary. |

The two ignored tests are `explicit_point_solve_baseline` and
`linear_array_expansion_baseline`. Both are marked as manual release-mode
performance baselines. They were not run.

Clippy required three scoped `large_enum_variant` allows on public serde
enums. Those enums were not boxed.

## Python / CadQuery

`python -m unittest discover` from the snapshot root, with
`PYTHONPATH=apps/scc-ui/cad` and `/tmp/aicad-cad/bin/python` (CadQuery 2.8.0,
host Python 3.12.3):

119 tests, 0 failures, 21.610s. This run is after the sweep lateral-arc
resolution and the loft projected-extrema fix. Manufacturing assertions
were not loosened. Chord tolerance stayed 0.01 mm. Associative tolerance
stayed 1e-5 mm.

## Node, with the gated CAD tests enabled

Command, from the snapshot root, concurrency 1, per-test timeout 180s:

```bash
AICAD_RUST_PARAMETRIC_INTEGRATION=1 \
AICAD_RUST_STEP_INTEGRATION=1 \
SCC_BIN="$PWD/target/release/scc" \
SCC_CAD_PYTHON=/path/to/venv/bin/python \
PYTHONPATH="$PWD/apps/scc-ui/cad" \
node --test --test-timeout=180000 --test-concurrency=1 \
  apps/scc-ui/test/*.test.mjs
```

`npm test` does not set those two `AICAD_RUST_*` variables and does not set
`--test-timeout`. A run without them skips the gated tests. Skipped is not
this result.

| Count | Value |
| --- | --- |
| Tests | 191 |
| Passed | 190 |
| Failed | 1 |
| Skipped | 0 |
| Wall time | 231.6s |
| Process exit | 1 |

Passed cases that exercise the real kernel include G1B, G2, G3, G4, G5 /
G5.1, G6A, G6B, G7A, G7B, the Project v2 M1–M16 STEP matrix, and all six
`rust-step.integration.test.mjs` cases (rectangle, rounded rectangle,
circle, polygon, composite, slot). G6A is a v2 principal-plane Section A-A
rebuilt from OCP and checked against an edit.

### G7C

Both gated G7C tests ran. Assertions, tolerances, and successful fixture
geometry were not weakened. The Loft stale check now writes
`selector.second.selector.end_section_id`. The previous path put
`end_section_id` on the generated-face object, which `deny_unknown_fields`
rejected as `SCC-CLI-002` before topology ran. The Sweep stale path was
corrected the same way and was not reached.

- Loft: pass. Drawing, formal STEP re-import within 1e-5 mm of the drawing
  solid, section-radius edit, and a missing section identity fails closed.
  The old front-span miss was a tessellated HLR polyline that stayed inside
  the 0.01 mm chord tolerance and outside the 1e-5 mm dimension contract.
  The extreme is now a polyline vertex. The chord tolerance was not raised.
- Sweep: fail. `SCC-SPAT-017`, `parametric_code` `TOPOLOGY_REFERENCE_STALE`,
  message `referenced edge was changed or removed by an upstream feature`.
  The earlier `Sweep lateral face boundary no longer exists` error was a
  fuse splitting the start profile circle. The resolver now accepts the
  remaining arc of that same circle when the full edge is gone, and still
  requires one qualifier-matching lateral face. After that, the fillet
  source edge is the full end rim (radius 2). The fixture hole has diameter
  4 on that cap, so the current body keeps only a semicircle between the
  hole cylinder and the sweep torus. ADR-067 requires the same exact curve.
  The semicircle is not used. STEP export for this fixture was not reached.

G7C is not accepted as a set. The Loft case passed. The Sweep fillet did not.

## Mounting plate

`examples/mounting-plate.dsl` is a v1 rectangle, 120 mm by 80 mm, 10 mm
thick, four corner holes. No provider is required.

CLI `export-step` on the DSL solve wrote 29967 STEP bytes. Stderr reported
`manufacturing=passed build=passed reimport=passed equivalence=passed`.
The CLI payload sha256 was
`6338d27ffa8281092957531455db487cc009c840410072457086abfd6cbe44e5`.

The same DSL through the live Workbench on `127.0.0.1:4199`:

| Call | Result |
| --- | --- |
| `POST /api/engineering/dsl` | HTTP 200. Model fingerprint `834453afc8c3b877fca1221097a589eefab4cd8074436ecbf046a84d70b7929d`. |
| `POST /api/engineering/drawing/three-view` | HTTP 200. SVG 11770 bytes with `view_front`, `view_top`, and `view_left`. Skills report `PASS`. Same fingerprint. |
| `POST /api/engineering/export/step` | HTTP 200. 29967 bytes, starts `ISO-10303-21`. All four stage headers `passed`. sha256 `0e6ed3adb0a171619b5fc14d0470f49cbf909c163265b56f473f33cef36223af`. |
| `POST /api/engineering/project/save` then `/project/open` | Both HTTP 200. Opened fingerprint matches the DSL solve. |

The CLI and HTTP STEP payloads have the same size and different hashes.
Each one passed its own equivalence check. They are not recorded as one
golden file.

`POST /api/engineering/drawing/section` on this v1 model is HTTP 422,
`SCC-DRW-024`, with `parametric_code` `SECTION_PARAMETRIC_REBUILD_REQUIRED`.
The model is not mutated. That is the intended fail-closed result. Before
the contract fix, the same rejection was rewritten as `SCC-CLI-009`.

## Browser

Playwright 1.60.0 drove headless Chromium against the real Engineering
Workbench. Each script used its isolated fake provider. `REAL_PROVIDER=NOT_RUN`.
A fake-provider pass is not a DeepSeek or Qwen test. `browser-g6a.mjs` has no
server source in this tree and was not run.

| Script | Result |
| --- | --- |
| `browser.mjs` | PASS. Chinese and English Workbench, fake intent, fillet/chamfer, three-view, SVG/DXF. |
| `browser-g4.mjs` | PASS. Revolve ADD, hole, circular pattern, three-view, SVG/DXF. |
| `browser-g5.mjs` | PASS. Formal STEP re-import stages passed. A 422 export produced no download. |
| `browser-g6a.mjs` | NOT_RUN. No matching fake-provider server in this tree. |
| `browser-g6a2.mjs` | FAIL. The zh-CN feature row is `center_hole 孔`. After selecting it, the inspector has 2 `.wb-prop-input` controls. The script requires 0. |
| `browser-g6a3.mjs` | FAIL. The same row is selectable. An invalid diameter edit returns `SCC-TOOL-010`. The script requires `SCC-TOOL-005`. |
| `browser-g6b.mjs` | PASS. Countersink, explicit placement, section, SVG/DXF, formal STEP. |
| `browser-g7a.mjs` | PASS. Sweep ADD, three-view, section, SVG/DXF, formal STEP. Fake provider. |
| `browser-g7b.mjs` | PASS. Ruled loft, three-view, section, SVG/DXF, formal STEP. Fake provider. |

The G6A.2 and G6A.3 assertions were not changed to match the new inspector or
error code.

## Docker

`docker compose build ui` exit 1. The daemon timed out during the TLS
handshake to `registry-1.docker.io` for `rust:1.96.0-bookworm`,
`python:3.12.11-slim-bookworm`, and `node:24.18.1-bookworm-slim`. The image
`aicad-scc-ui:0.4.0-alpha.1` was not built. It was not pushed. The daemon
configuration was not changed.

## Not run

- Real DeepSeek and Qwen calls.
- `cargo-audit`.
- The two ignored Rust performance baselines.
- `browser-g6a.mjs`, because its server source is absent.
