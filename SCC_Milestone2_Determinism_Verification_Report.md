# SCC Milestone 2 确定性验证报告

## 目标

根据架构规范，M1、M2 所有输出必须是稳定、可重放、与迭代顺序无关的。本次冲刺重点验证并修复了所有已知的非确定性来源。

## 已执行的修复

### 1. scc-analysis 数据结构替换

文件：`crates/scc-analysis/src/declaration_graph.rs`

| 原类型 | 新类型 | 位置/用途 |
| --- | --- | --- |
| `HashSet<String>` | `BTreeSet<String>` | `entity_ids`：实体引用存在性检查 |
| `HashMap<String, Vec<String>>` | `BTreeMap<String, Vec<String>>` | `adj`：循环依赖检测的邻接表 |
| `HashSet<String>` | `BTreeSet<String>` | `visited` / `on_stack`：DFS 状态 |
| `HashMap<&str, &EntityKind>` | `BTreeMap<&str, &EntityKind>` | `entity_kinds`：类型兼容性检查 |

- 修复理由：循环依赖检测输出的错误信息顺序依赖于邻接表迭代顺序。使用 `BTreeMap` 后，无论输入顺序如何，输出报告均按字典序稳定。
- 算法逻辑未改变，仅替换数据结构。

### 2. scc-core 确定性测试

新增测试验证：

- `serialization_order_is_deterministic`：同一 `GraphState` 两次序列化结果字节一致；约束 ID 与实体 ID 在 JSON 中按升序出现。
- `btreemap_insertion_order_does_not_affect_output`：以不同顺序插入实体后，序列化结果完全一致。
- `declaration_graph_stable_ordering`：图的声明顺序在序列化后稳定保持。

### 3. Golden 与端到端测试中的确定性验证

| 测试 | 验证方式 |
| --- | --- |
| `golden_compile_is_deterministic` | 同一 Proposal 多次编译，字节级比对 ConstraintIR |
| `golden_execute_is_deterministic` | 同一 IR 多次执行，字节级比对 State 与 Trace |
| `integration_determinism`（已有） | 同一 Proposal 两次编译输出一致 |
| `runtime_trace_determinism`（已有） | 运行时 trace 输出稳定 |
| `runtime_deterministic_root`（已有） | GraphState root 生成稳定 |
| `plan_generator_deterministically_orders_fan_in_dependencies`（已有） | 执行计划依赖顺序稳定 |

## 全工作区 HashMap/HashSet 审查结果

运行 `rg "HashMap|HashSet" crates/` 后确认：

- 替换前：`crates/scc-analysis/src/declaration_graph.rs` 存在 7 处 `HashMap`/`HashSet` 使用。
- 替换后：工作区源码中已无 `HashMap`/`HashSet` 用于需要确定性顺序的场景。
- 工作区剩余 `HashMap`/`HashSet` 均不存在。

## 验证命令与结果

```bash
cargo test --workspace
```

结果：所有测试通过，无失败。

```bash
cargo clippy --workspace --all-targets
```

结果：无警告、无错误。

## 结论

Milestone 2 的确定性要求已满足。所有 M1/M2 关键数据结构均采用 `BTreeMap`/`BTreeSet`，输出顺序稳定、可重放。Golden 测试与端到端测试共同锁定了完整管道的字节级确定性。
