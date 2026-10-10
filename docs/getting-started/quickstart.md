# Quick start

Commands below are the ones this snapshot's files implement. They are not
marked as passed until [../release/TEST_REPORT.md](../release/TEST_REPORT.md)
says so.

## Requirements

| Tool | Version in this tree |
| --- | --- |
| Rust | 1.96.0, from `rust-toolchain.toml`, with rustfmt and clippy |
| Node.js | `>=20`. The Docker runtime is `node:24.18.1-bookworm-slim` |
| npm | Comes with Node. The UI lockfile is npm lockfileVersion 3 |
| Docker | Optional. Required for the packaged STEP/OCP image |
| Python | 3.12.11 in the image. `cadquery==2.8.0` from `apps/scc-ui/cad/requirements.txt` |
| GPU | Not used |

Host builds do not need Docker if you only solve and draw the mounting plate.
`scc engineering export-step` and v2 OCP rebuilds need CadQuery on
`SCC_CAD_PYTHON`.

## A. Docker

From the snapshot root:

```bash
docker compose up --build ui
```

This builds `aicad-scc-ui:0.4.0-alpha.1` from `apps/scc-ui/Dockerfile`.
The build pulls three official library images and installs CadQuery from PyPI.
The container is read-only, drops all capabilities, and Compose publishes
only `127.0.0.1:4173`.

On 2026-10-10 this host could not pull `rust:1.96.0-bookworm`. The daemon
reported a TCP timeout reaching `registry-1.docker.io`. The image was not
built here, so the Docker path is not a verified quick start on this machine.

Open <http://127.0.0.1:4173>. `/` redirects to `/engineering.html`.

Offline demo:

1. Open the DSL disclosure and choose **载入内置安装板**.
2. Apply it. The server calls the Rust CLI. No provider key is read.
3. Switch the drawing panel to the three-view mode if it is not already there.
4. Export STEP from the engineering action. The request is `POST /api/engineering/export/step`.

Stop the process with `Ctrl-C`. Replacing the container drops keys that were
typed into the page. There is no key volume.

## B. From source

```bash
cargo build --locked --release -p scc-cli
./target/release/scc --version
npm ci --prefix apps/scc-ui
SCC_BIN="$PWD/target/release/scc" npm start --prefix apps/scc-ui
```

Expect `scc 0.4.0-alpha.1`. The UI then listens on `127.0.0.1:4173`.

The same plate without the browser:

```bash
./target/release/scc engineering dsl examples/mounting-plate.dsl > /tmp/mounting-plate.model.json
./target/release/scc engineering drawing-three-view /tmp/mounting-plate.model.json
```

`drawing-three-view` prints one JSON document that includes the SVG. Formal
STEP, only when CadQuery imports:

```bash
export SCC_CAD_PYTHON=/path/to/venv/bin/python
./target/release/scc engineering export-step /tmp/mounting-plate.model.json > /tmp/mounting-plate.step
```

Stderr prints `AICAD-STEP-STATUS` with the re-import and equivalence result.
Stdout is the STEP payload. Keep stderr and stdout apart when you redirect.

Windows PowerShell uses `target\release\scc.exe` and `$env:SCC_BIN`.

## Tests that belong to a release check

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
SCC_BIN="$PWD/target/release/scc" npm test --prefix apps/scc-ui
python scripts/smoke-test.py
```

G7C and the other parametric OCP integration tests:

```bash
AICAD_RUST_PARAMETRIC_INTEGRATION=1 SCC_BIN="$PWD/target/release/scc" \
  node --test apps/scc-ui/test/rust-parametric.integration.test.mjs
```

Or, in Docker, `docker compose run --rm cad-rust-test`. That service sets
`AICAD_RUST_PARAMETRIC_INTEGRATION=1` and `AICAD_RUST_STEP_INTEGRATION=1`,
and it uses `network_mode: none` after the image already exists.

## Common errors

See the table in the README. The short version: `SCC-CAD-010` means you asked
the compatibility route for STEP; a missing CadQuery interpreter means the
formal route cannot start; a version string other than `scc 0.4.0-alpha.1`
means the binary is not this snapshot. `SCC-DRW-024` on Section A-A means the
model is v1. The mounting plate is v1: its three-view and formal STEP do not
imply that a section exists. If `curl http://127.0.0.1:4173` returns 502 while
the process is listening, a local `HTTP_PROXY` is intercepting loopback. Use
`curl --noproxy '*'`.
