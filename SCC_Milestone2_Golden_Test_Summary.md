# SCC Milestone 2 Golden 测试摘要

## 目的

Golden 测试用于冻结完整管道的字节级输出，确保后续任何代码改动都不会意外改变 M1 → M2 的确定性输出。

## Golden 产物清单

所有产物位于 `crates/scc-cli/tests/golden/`：

| 文件 | 阶段 | 说明 |
| --- | --- | --- |
| `bracket.proposal.json` | 输入 | 用户意图的 Proposal JSON |
| `bracket.ir.json` | M1 输出 | 编译生成的 ConstraintIR |
| `bracket.state.json` | M2 输出 | 运行时生成的 GraphState |
| `bracket.trace.json` | M2 输出 | 运行时生成的 ExecutionTrace |

## 生成方式

1. 使用 CLI 二进制运行 `scc compile examples/bracket.proposal.json`，生成 ConstraintIR。
2. 使用 CLI 二进制运行 `scc execute bracket.proposal.ir.json`，生成 GraphState 与 ExecutionTrace。
3. 将输入与三个产物复制到 `crates/scc-cli/tests/golden/` 目录。
4. 后续任何 JSON 漂移都会触发测试失败。

## 测试用例

测试文件：`crates/scc-cli/tests/golden_tests.rs`

| 测试名 | 验证内容 |
| --- | --- |
| `golden_compile_bracket_matches_snapshot` | 编译 Proposal 得到的 ConstraintIR 与 golden 文件字节一致 |
| `golden_execute_bracket_matches_snapshot` | 执行 ConstraintIR 得到的 GraphState 与 ExecutionTrace 与 golden 文件字节一致 |
| `golden_compile_is_deterministic` | 同一 Proposal 多次编译输出字节一致 |
| `golden_execute_is_deterministic` | 同一 IR 多次执行输出字节一致 |
| `golden_end_to_end_pipeline_matches_snapshot` | 完整管道（Proposal → IR → State/Trace）与 golden 一致 |

## 结果

```text
running 5 tests
test golden_compile_bracket_matches_snapshot ... ok
test golden_execute_bracket_matches_snapshot ... ok
test golden_compile_is_deterministic ... ok
test golden_execute_is_deterministic ... ok
test golden_end_to_end_pipeline_matches_snapshot ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 维护说明

- 如果后续修改了序列化格式、字段顺序或运行时行为，Golden 测试会失败。
- 在确认变更符合架构预期后，应重新生成 golden 产物并随代码一起提交。
- 重新生成命令：
  ```bash
  cargo run --bin scc -- compile examples/bracket.proposal.json
  cargo run --bin scc -- execute examples/bracket.proposal.ir.json
  cp examples/bracket.proposal.json crates/scc-cli/tests/golden/
  cp examples/bracket.proposal.ir.json crates/scc-cli/tests/golden/bracket.ir.json
  cp examples/bracket.proposal.ir.trace.json crates/scc-cli/tests/golden/bracket.trace.json
  cp examples/bracket.proposal.ir.trace.state.json crates/scc-cli/tests/golden/bracket.state.json
  ```
