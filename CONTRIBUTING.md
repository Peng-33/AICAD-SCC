# Contributing

[English](CONTRIBUTING.md) · 简体中文见文末。

AICAD accepts changes that keep the engineering chain deterministic and
honest about what was tested. Read [docs/limitations/current.md](docs/limitations/current.md)
before proposing a feature that the kernel currently rejects.

The intended license is Apache-2.0, but the copyright holder is not yet
confirmed. Do not contribute if you need a completed legal grant today.
See [NOTICE](NOTICE).

## What to work on

Useful contributions:

- a failing test that matches a real error code
- a fix that keeps unsupported operations fail-closed
- documentation that matches the current code
- bounded geometry work with a stated domain and a test

A pull request that deletes a validator, weakens a STEP equivalence check,
or rewrites a golden to match a bad result will be rejected.

## Issues

Use the bug or feature template when the repository is public.

Include:

- AICAD version (`scc --version`)
- the command or HTTP route
- the input DSL, tool call, or model, reduced as far as it still fails
- the error code (`SCC-DSL-*`, `SCC-GEOM-*`, `SCC-MFG-*`, `SCC-CAD-010`, and so on)
- whether CadQuery/OCP was on the path

Do not attach API keys, `.env` files, or private customer models.

## Tests

From a machine with Rust 1.96.0 and Node.js 20 or newer:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo build --locked --release -p scc-cli
SCC_BIN="$PWD/target/release/scc" npm test --prefix apps/scc-ui
```

Python and STEP checks need CadQuery 2.8.0. The default `npm test` run does
not execute the Rust parametric integration tests. Those stay skipped unless
you set `AICAD_RUST_PARAMETRIC_INTEGRATION=1` and `SCC_CAD_PYTHON` points at
an environment that imports CadQuery. A skip is not a pass. G7C in particular
must be reported from that gated run, not from an ancestor acceptance note.

Docker, when you choose it:

```bash
docker compose up --build ui
docker compose run --rm cad-test
docker compose run --rm cad-rust-test
```

`cad-test` and `cad-rust-test` build the UI image, which downloads Rust,
Python, Node, and CadQuery. They bind no host port. `ui` publishes only
`127.0.0.1:4173`.

## Pull requests

- One behavior change per request when you can.
- Describe the user-visible bound: what now works, what still fails closed.
- Update the README feature row and [docs/limitations/current.md](docs/limitations/current.md) in the same change if the bound moved.
- Add or adjust a test. Do not only change a document.
- Do not commit `target/`, `node_modules/`, `.env`, or generated STEP files that are not fixtures already in the tree.
- Rust is formatted with rustfmt. Node tests use `node --test`.
- Comments explain a constraint that the code cannot. They do not restate the line.

## Documentation

English `README.md` and `README.zh-CN.md` carry the same commands and the
same feature bounds. Change both. Architecture decisions go to a new ADR and
to [docs/adr/README.md](docs/adr/README.md). An old ADR stays as history;
do not silently rewrite it into a current claim.

## Security

Follow [SECURITY.md](SECURITY.md).

---

## 参与贡献（简体中文）

AICAD 接收的改动必须保持工程链确定，并如实说明测试范围。内核目前拒绝的操作，在提案前先阅读 [docs/limitations/current.md](docs/limitations/current.md)。

拟议许可证是 Apache-2.0，版权持有人尚未确认。如果你需要一份已经完成的法律授权，请先不要提交代码。见 [NOTICE](NOTICE)。

提交 Issue 时写上 `scc --version`、命令或路由、可复现的最小输入和错误代码。不要附带 API Key 或私人模型。

测试命令与上文英文部分相同。默认 `npm test` 不会运行需要 `AICAD_RUST_PARAMETRIC_INTEGRATION=1` 的参数化集成测试。跳过不是通过。G7C 必须来自这次门控运行，不能用祖先验收记录代替。

一个 PR 只做一件行为变化。如果支持边界变了，同时修改两份 README 和限制说明。不要为了变绿而放宽 STEP 等价校验或改写黄金结果。
