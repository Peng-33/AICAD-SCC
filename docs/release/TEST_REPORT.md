# Test report for 0.4.0-alpha.1

Date: 2026-10-10. The tables below were measured on the source that this
alpha publishes. A publication commit may add license text, release notes,
and the release-workflow edit without changing Rust, Node, or Python
sources. Before the tag, fmt, clippy, workspace tests, the release build,
and the gated Node suite are re-run on that commit. These tables are that
evidence when the re-run matches them. A mismatch must replace this file
before the tag. This is not a GitHub Actions result, and it is not a claim
that every historical acceptance report was re-run.

Ignored tests and skipped tests are not passes. Both gated G7C cases
passed. That does not widen Sweep or Loft beyond the domains below.

## Rust

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Exit 0 |
| `cargo test --workspace --all-targets --locked` with `SCC_CAD_PYTHON` set | Exit 0. 50 result lines, 1166 passed, 0 failed, 2 ignored. The closed-profile STEP tests ran inside this command (2 passed, 4.90s) because `SCC_CAD_PYTHON` was set. They return immediately when that variable is unset. |
| `cargo build --release --locked -p scc-cli` | Exit 0. `./target/release/scc --version` prints `scc 0.4.0-alpha.1`. Python and Node called that rebuilt binary. |

The two ignored tests are `explicit_point_solve_baseline` and
`linear_array_expansion_baseline`. Both are marked as manual release-mode
performance baselines. They were not run.

Clippy required three scoped `large_enum_variant` allows on public serde
enums. Those enums were not boxed.

## Python / CadQuery

`python -m unittest discover` from the snapshot root, with
`PYTHONPATH=apps/scc-ui/cad` and `/tmp/aicad-cad/bin/python` (CadQuery 2.8.0,
host Python 3.12.3):

119 tests, 0 failures, 33.738s, process exit 0. This run is after the
sweep lateral-arc resolution, the loft projected-extrema fix, the interior
sweep-hole fixture, and the B-spline sample-budget change. Manufacturing
assertions were not loosened. Chord tolerance stayed 0.01 mm. Associative
tolerance stayed 1e-5 mm. Spatial linear deviation stayed 0.05 mm and the
angular tolerance stayed 0.1 rad.

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
| Passed | 191 |
| Failed | 0 |
| Skipped | 0 |
| Cancelled | 0 |
| Wall time | 261.4s |
| Process exit | 0 |

Passed cases that exercise the real kernel include G1B, G2, G3, G4, G5 /
G5.1, G6A, G6B, G7A, G7B, the Project v2 M1–M16 STEP matrix, and all six
`rust-step.integration.test.mjs` cases (rectangle, rounded rectangle,
circle, polygon, composite, slot). G6A is a v2 principal-plane Section A-A
rebuilt from OCP and checked against an edit.

### G7C

Both gated G7C tests ran inside the 191-test command and passed.
Tolerances were not raised. The Loft stale check writes
`selector.second.selector.end_section_id`. Putting `end_section_id` on the
generated-face object is an unknown field and becomes `SCC-CLI-002` before
topology runs.

- Loft: pass. Drawing, formal STEP re-import within 1e-5 mm of the drawing
  solid, adjacent-section chamfer, section-radius edit, and a missing
  section identity fails closed. The old front-span miss of about 0.00047 mm
  was a tessellated HLR polyline that stayed inside the 0.01 mm chord
  tolerance and outside the 1e-5 mm dimension contract. The extreme is now a
  polyline vertex. The chord tolerance was not raised.
- Sweep: pass for an interior hole. The swept section is a radius-2 circle.
  A through hole of diameter 4 is the cap boundary. An independent CadQuery
  rebuild of that original fixture left one semicircle on the source circle:
  length about 6.283 mm against a full rim of 12.566 mm, and the end-cap
  plane was gone. ADR-067 requires the same exact curve, so the fillet does
  not bind that semicircle. The positive fixture now uses diameter 2, which
  leaves the full rim. The same test then sets diameter 4 again and expects
  `TOPOLOGY_REFERENCE_STALE`. That negative assertion passed. Drawing, formal
  STEP re-import, path-edit rebind, and a missing profile identity also
  passed.
- The 0.25 mm rim fillet is a sampled B-spline of about 4.71 mm². At the
  existing 0.05 mm / 0.1 rad deviation it measured 4790 vertices and 9313
  triangles, above the old 4096 / 8192 caps, and the validator rejected it
  as `BODY_INVALID`. The caps are now 8192 vertices and 16384 triangles.
  Those numbers are resource bounds. The deviation limits did not change.

G7C is accepted only for this bounded pair. It is not a selector for every
generated face, and a rim-consuming sweep hole stays fail-closed.

## Mounting plate

This host check was not repeated after the B-spline sample-budget rebuild.
The numbers below are the earlier loopback check on this prep tree. They
are not a new pass of the rebuilt binary.

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
Workbench and the rebuilt `scc 0.4.0-alpha.1` binary. Each script used its
isolated fake provider. `REAL_PROVIDER=NOT_RUN`. A fake-provider pass is not
a DeepSeek or Qwen test. `browser-g6a.mjs` has no server source in this tree
and was not run. The two failing assertions were not changed.

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

`docker compose build ui` was not repeated. The earlier attempt exited 1:
the daemon timed out during the TLS handshake to `registry-1.docker.io` for
`rust:1.96.0-bookworm`, `python:3.12.11-slim-bookworm`, and
`node:24.18.1-bookworm-slim`. A 25-second `docker pull hello-world` in this
pass also did not finish (exit 124). The image `aicad-scc-ui:0.4.0-alpha.1`
was not built. It was not pushed. The daemon configuration was not changed.
Image redistribution stays blocked until the OCCT notices in
`THIRD_PARTY_NOTICES.md` are present beside any redistributed binaries.

## Not run

- Real DeepSeek and Qwen calls.
- `cargo-audit`. `gitleaks` is not installed. The local pattern scan is in
  `SECURITY_SCAN.md`.
- The two ignored Rust performance baselines.
- `browser-g6a.mjs`, because its server source is absent.
- The mounting-plate HTTP check, after the release binary was rebuilt.
- A completed Docker image build.
