# SCC Milestone 2 剩余技术债务

## 总体评估

Milestone 2 的工程收尾任务已按验收标准完成。所有 P0/P1/P2 问题均已修复，工作区测试、Golden 测试、端到端测试、Clippy 检查与格式化全部通过。当前不存在阻碍进入 Milestone 3 的技术债务。

以下条目为建议性、非阻塞的后续改进，可在进入 Milestone 3 前或期间逐步处理。

## 1. 测试组织（P2）

**现状**：本次冲刺为 `scc-core` 新增的 23 个单元测试全部集中在 `crates/scc-core/src/lib.rs` 的 `#[cfg(test)]` 模块中。

**影响**：随着核心类型增多，单个测试文件会变得较长，可读性下降。

**建议**：将测试按子模块拆分到 `crates/scc-core/src/ids_tests.rs`、`graph_tests.rs`、`runtime_tests.rs` 等文件中，并在 `lib.rs` 中统一引入。该重构不修改测试逻辑与公共 API，可在后续迭代中完成。

## 2. 端到端测试临时目录（P2）

**现状**：`crates/scc-cli/tests/end_to_end_tests.rs` 使用 `target/e2e-tests/` 作为临时输出目录，并通过 `clean_temp_dir()` 在测试开始时清理。

**影响**：虽然当前测试通过，但多个测试并发访问同一目录理论上存在竞态风险（当前测试串行依赖较小，但仍建议隔离）。

**建议**：为每个测试生成唯一子目录（例如使用 UUID 或测试名），测试结束后自动清理。该改进不引入架构变化。

## 3. 代码覆盖率工具（P2）

**现状**：本次冲刺显著增加了测试数量，但尚未集成代码覆盖率收集工具。

**影响**：无法量化每个 crate 的覆盖比例，长期难以识别未覆盖路径。

**建议**：在 CI 中引入 `cargo tarpaulin` 或 `cargo llvm-cov`，设定最低覆盖率阈值（例如 scc-core 80%，scc-proposal 70%）。

## 4. 提案验证范围（P2）

**现状**：`scc-proposal` 的 `validate` 函数当前仅检查 `distance` 关系是否带值。未知 entity kind / relation kind 在 proposal 层透传，由 M1 的 `scc-analysis` 检测并报告冲突。

**影响**：这符合架构边界（LLM 输出为不可信输入，M1 负责验证），但 proposal 层可更早拒绝明显非法的 schema 值。

**建议**：若未来需要更严格的输入校验，可在 `validate` 中增加已知 kind 白名单。但此变更涉及 schema 解释，需在架构规范中明确归属（M1 vs M2），本次冲刺未触及。

## 5. 文档字符串（P3）

**现状**：部分核心类型的公开字段缺少 doc comment。

**影响**：对首次阅读代码的开发者不够友好，但现有测试已充分说明用法。

**建议**：在 Milestone 3 开始前为核心类型（`GraphState`、`ExecutionTrace`、`RuntimeEntity` 等）补充 `///` 文档，提升可维护性。

## 6. 性能与压力测试（P3）

**现状**：当前测试聚焦正确性与确定性，缺少大规模 Proposal 或复杂依赖图的性能基准。

**建议**：Milestone 3 进入几何求解阶段后，再引入性能测试更有意义，届时可测试 M1→M2 管道在大模型上的耗时。

## 结论

以上债务均为优化性质，不影响 Milestone 2 的验收与进入 Milestone 3。当前代码库已具备生产质量基础，可以继续进行几何求解器开发。
