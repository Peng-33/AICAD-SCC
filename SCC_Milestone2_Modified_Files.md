# SCC Milestone 2 修改文件清单

## 本冲刺新增文件

| 文件路径 | 说明 |
| --- | --- |
| `crates/scc-cli/tests/end_to_end_tests.rs` | 工作区级 CLI 端到端测试 |
| `crates/scc-cli/tests/golden_tests.rs` | Golden 快照测试 |
| `crates/scc-cli/tests/golden/bracket.proposal.json` | Golden 输入：Proposal |
| `crates/scc-cli/tests/golden/bracket.ir.json` | Golden 产物：ConstraintIR |
| `crates/scc-cli/tests/golden/bracket.state.json` | Golden 产物：GraphState |
| `crates/scc-cli/tests/golden/bracket.trace.json` | Golden 产物：ExecutionTrace |

## 本冲刺修改文件

| 文件路径 | 主要修改内容 |
| --- | --- |
| `crates/scc-analysis/src/declaration_graph.rs` | `HashMap`/`HashSet` → `BTreeMap`/`BTreeSet`，确保循环检测与引用检查确定性 |
| `crates/scc-analysis/src/diagnostics.rs` | Clippy 清理：冗余字段名、Copy 类型 clone |
| `crates/scc-core/src/lib.rs` | 新增 23 个核心单元测试 |
| `crates/scc-proposal/src/parser.rs` | 新增 9 个解析测试 |
| `crates/scc-proposal/src/validate.rs` | 新增 6 个验证测试 |
| `crates/scc-execution/src/mutation/mutation_guard.rs` | Clippy 清理：Unit 结构体 default |
| `crates/scc-execution/src/plan_generator.rs` | Clippy 清理：Unit 结构体 default、Copy 类型 clone |
| `crates/scc-execution/src/runtime/command_dispatcher.rs` | Clippy 清理：Copy 类型 clone |
| `crates/scc-lowering/src/lowering.rs` | Clippy 自动修复 |
| `crates/scc-cli/src/compile.rs` | 格式化 |
| `crates/scc-cli/src/main.rs` | 格式化 |
| `crates/scc-cli/tests/compile_integration_tests.rs` | 格式化 |
| `crates/scc-core/src/error.rs` | 格式化 |
| `crates/scc-execution/src/driver.rs` | 格式化 |
| `crates/scc-execution/src/execution_plan.rs` | 格式化 |
| `crates/scc-execution/src/lib.rs` | 格式化 |
| `crates/scc-execution/src/overlay_canonicalizer.rs` | 格式化 |
| `crates/scc-execution/src/overlay_view.rs` | 格式化 |
| `crates/scc-execution/src/runtime/execution_runtime.rs` | 格式化 |
| `crates/scc-execution/src/runtime/graph_state_machine.rs` | 格式化 |
| `crates/scc-execution/src/runtime/mod.rs` | 格式化 |
| `crates/scc-execution/src/runtime/overlay_applier.rs` | 格式化 |
| `crates/scc-execution/src/runtime/runtime_error.rs` | 格式化 |
| `Cargo.lock` | 依赖锁定更新 |
| `.gitignore` | 忽略规则更新 |

## 已删除文件（由前序冲刺完成）

| 文件路径 | 说明 |
| --- | --- |
| `crates/scc-execution/src/solver_backend.rs` | 已移除 solver 后端，消除 M2 solver 泄漏 |
| `crates/scc-execution/src/export_backend.rs` | 已移除导出后端，消除 M2 跨阶段概念 |
| `crates/scc-execution/src/runtime/cad_state_store.rs` | 已移除 CAD 状态存储，消除 M2 几何概念 |

## 其他未修改说明

架构规范文档（`SCC_Architecture_Specification.docx` 等）未在本次冲刺中修改。所有修改均严格限定在工程完成、测试覆盖与代码质量范围内，未引入新架构概念。
