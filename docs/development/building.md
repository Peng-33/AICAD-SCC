# Building and release metadata

## Toolchain

`rust-toolchain.toml` pins channel `1.96.0` and asks for `clippy` and `rustfmt`.
The workspace resolver is 2. Every crate version is `0.4.0-alpha.1`, and each
crate `Cargo.toml` sets `license = "Apache-2.0"`. That field is the intended
SPDX identifier. It does not confirm the copyright holder. See [../../NOTICE](../../NOTICE).

`apps/scc-ui/package.json`, the lockfile root, and `apps/scc-ui/e2e/package.json`
use the same version. `compose.yaml` tags the production image
`aicad-scc-ui:0.4.0-alpha.1`. Older test service tags (`g3-test`, `g4-test`)
are historical names and were left in place.

`scc --version` is the Cargo package version of `scc-cli`. `scripts/smoke-test.py`
compares it with `scc 0.4.0-alpha.1`.

## Release workflow

`.github/workflows/release.yml` already accepts a pre-release tag. It strips
the leading `v`, checks SemVer including a hyphenated pre-release, and requires
that string to equal the `scc-cli` version from `cargo metadata`. It then
requires `scc --version` to print `scc <version>`.

The tag for this snapshot is `v0.4.0-alpha.1`. Creating the tag or the GitHub
Release is a publish step and is not done from the prep tree by itself.

`.github/workflows/scc-ci.yml` runs on pushes to `master`, `main`, and
`develop`, and on pull requests to `master` or `main`. A branch with another
name does not start CI. The public repository's default branch should be
`main` if that workflow is the one you want on every push.

## Local build

```bash
cargo build --locked --release -p scc-cli
cargo test --workspace --all-targets --locked
```

Node:

```bash
npm ci --prefix apps/scc-ui
SCC_BIN="$PWD/target/release/scc" npm test --prefix apps/scc-ui
```

The UI image build is the Dockerfile at `apps/scc-ui/Dockerfile`. It is a
multi-stage build. The runtime user is `node`. `.dockerignore` keeps the
e2e tree and `node_modules` out of the context.

## What not to "fix" while aligning versions

- Do not rewrite historical acceptance reports that recorded `scc 0.3.0`.
- Do not retag `g3-test` or `g4-test` images as part of the version alignment.
- Do not treat a green ancestor report as a result for `0.4.0-alpha.1`.
