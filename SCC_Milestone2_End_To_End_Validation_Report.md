# SCC Milestone 2 端到端验证报告

## 测试目标

验证 CLI 可执行文件 `scc` 在真实命令行场景下能够正确完成：

1. `scc compile <proposal.json>` → ConstraintIR / 冲突报告
2. `scc execute <constraint_ir.json>` → GraphState + ExecutionTrace
3. 带 mutation 的执行
4. 非法输入的合理失败

## 测试文件

`crates/scc-cli/tests/end_to_end_tests.rs`

## 测试覆盖

| 测试名 | 输入 | 预期输出 | 验证结果 |
| --- | --- | --- | --- |
| `e2e_compile_success` | `examples/bracket.proposal.json` | `bracket.ir.json`（ConstraintIR） | ✅ 通过 |
| `e2e_compile_conflict` | `examples/bracket.conflict.proposal.json` | `bracket.conflict.json`（冲突报告） | ✅ 通过 |
| `e2e_execute_success` | `examples/bracket.proposal.ir.json` | `bracket.trace.json` + `bracket.trace.state.json` | ✅ 通过 |
| `e2e_execute_with_mutation` | `bracket.proposal.ir.json` + `bracket.mutation.json` | 更新后的 State/Trace，其中 `hole.diameter` = 15.0 | ✅ 通过 |
| `e2e_full_pipeline` | `bracket.proposal.json` | 依次生成 IR、Trace、State | ✅ 通过 |
| `e2e_invalid_input_fails` | 非法 JSON | 进程非 0 退出 | ✅ 通过 |

## 实现方式

- 使用 `std::process::Command` 调用 `env!("CARGO_BIN_EXE_scc")` 指向的真实二进制。
- 临时输出目录位于 `target/e2e-tests/`，避免权限问题并便于清理。
- 每个测试在独立子目录中运行，防止并行冲突。
- 同时测试成功路径与失败路径，确保 CLI 对错误输入有明确反馈。

## 执行结果

```text
running 6 tests
test e2e_compile_success ... ok
test e2e_compile_conflict ... ok
test e2e_execute_success ... ok
test e2e_execute_with_mutation ... ok
test e2e_full_pipeline ... ok
test e2e_invalid_input_fails ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 结论

CLI 端到端行为符合架构设计：

- M1 编译阶段只生成 ConstraintIR 或冲突报告，不调用 M2。
- M2 执行阶段只生成 GraphState 与 ExecutionTrace，不调用 solver 或 LLM。
- mutation 机制通过命令行参数注入，由 M2 运行时以符号方式应用。
- 非法输入不会导致 panic 或进入错误阶段，而是返回非零退出码。
