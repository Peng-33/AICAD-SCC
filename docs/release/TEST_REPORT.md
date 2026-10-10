# Test report for 0.4.0-alpha.1

Date: 2026-10-10. Tree: the public-prep snapshot, after the `SCC-DRW-024` /
`SCC-DRW-025` section error-contract fix. This is not a public release, and
it is not a claim that every historical acceptance report was re-run.

Ignored tests and skipped tests are not passes. The two G7C failures are
not passes.

## Rust

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Exit 0 before the section-code fix. `cargo clippy -p scc-cli --all-targets --locked -- -D warnings` exit 0 after that fix. |
| `cargo test --workspace --all-targets --locked` | Exit 0. 1166 passed, 0 failed, 2 ignored. |
| `cargo test -p scc-cad-backend --test closed_profile_step --locked` with `SCC_CAD_PYTHON` | Exit 0. 2 passed in 3.65s. These two tests return immediately when `SCC_CAD_PYTHON` is unset, so the workspace command above did not execute them. |
| `cargo build --release -p scc-cli` | Exit 0. `./target/release/scc --version` prints `scc 0.4.0-alpha.1`. |

The two ignored tests are `explicit_point_solve_baseline` and
`linear_array_expansion_baseline`. Both are marked as manual release-mode
performance baselines. They were not run.

Clippy required three scoped `large_enum_variant` allows on public serde
enums. Those enums were not boxed.

## Python / CadQuery

`python -m unittest discover` from the snapshot root, with
`PYTHONPATH=apps/scc-ui/cad` and `/tmp/aicad-cad/bin/python` (CadQuery 2.8.0,
host Python 3.12.3):

119 tests, 0 failures, 23.010s. This run is after the circle-edge `Closed()`
fix, the parametric rebuild schema fixture update from 5 to 6, and the
sweep/loft cap-role fix. Manufacturing assertions were not loosened.

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
| Passed | 189 |
| Failed | 2 |
| Skipped | 0 |
| Wall time | 228.1s |
| Process exit | 1 |

Passed cases that exercise the real kernel include G1B, G2, G3, G4, G5 /
G5.1, G6A, G6B, G7A, G7B, the Project v2 M1–M16 STEP matrix, and all six
`rust-step.integration.test.mjs` cases (rectangle, rounded rectangle,
circle, polygon, composite, slot). G6A is a v2 principal-plane Section A-A
rebuilt from OCP and checked against an edit.

### G7C failures

Both gated G7C tests ran and failed. The assertions were not changed.

- Sweep: `SCC-SPAT-017`, `parametric_code` `TOPOLOGY_REFERENCE_NOT_FOUND`,
  message `Sweep lateral face boundary no longer exists`.
- Loft: `SCC-DRW-009`, message `invalid drawing dimension`, reason
  `associative.body.length projection span 32.959726056 on view front bounds [-15.959726056, -15, 17, 15.000000774] disagrees with spatial span 32.960198442`.

G7C is not accepted for this snapshot.

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

## Not run

- Browser. No browser tool was available. The HTTP checks above are not a
  click-through of the Workbench. Playwright e2e was not run.
- Docker. `docker pull rust:1.96.0-bookworm` failed: the daemon timed out
  opening a TCP connection to `registry-1.docker.io`. The image was not
  built. `docker compose up` was not run.
- Real DeepSeek and Qwen calls.
- `cargo-audit`.
- The two ignored Rust performance baselines.
