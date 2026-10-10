# SCC Milestone 2 测试覆盖摘要

## 测试总数

本次冲刺完成后，工作区共运行 **119 个非 doc-test 测试**，全部通过。

## 按 Crate 分布

| Crate | 单元测试 | 集成测试 | 小计 |
| --- | --- | --- | --- |
| scc-core | 23 | 0 | 23 |
| scc-proposal | 15 | 0 | 15 |
| scc-analysis | 19 | 0 | 19 |
| scc-lowering | 18 | 0 | 18 |
| scc-execution | 24 | 0 | 24 |
| scc-cli | 2 | 13 | 15 |
| **合计** | **101** | **13** | **119** |

> scc-cli 的 13 个集成测试分布在 `compile_integration_tests`（5）、`execute_integration_tests`（2）、`end_to_end_tests`（6）。

## scc-core 测试覆盖详情

| 测试类别 | 覆盖点 | 测试数量 |
| --- | --- | --- |
| ID 类型 | EntityId、ConstraintId、SnapshotId、ArrayId 的相等性、排序、显示 | 4 |
| 实体与约束 | Entity、Constraint 的序列化、EntityKind Copy 语义 | 3 |
| 权限与上下文 | AuthorityLevel 排序、ConstraintContext 序列化 | 2 |
| 图结构 | DeclarationGraph 稳定顺序、EdgeRelation 语义 | 2 |
| 运行时状态 | GraphState 初始化/快照/推进、RuntimeEntity、RuntimeConstraint | 4 |
| 执行跟踪 | ExecutionTrace 记录与序列化 | 2 |
| 参数值 | ParameterValue 数值往返 | 1 |
| 确定性 | 序列化顺序稳定、BTreeMap 插入顺序无关 | 2 |
| 错误处理 | Conflict、SccError 序列化与显示 | 2 |
| 小计 | | 23 |

## scc-proposal 测试覆盖详情

| 测试类别 | 覆盖点 | 测试数量 |
| --- | --- | --- |
| 解析 | 合法 Proposal、非法 JSON、空 Proposal、必填字段缺失 | 4 |
| 未知 kind | 未知 entity kind / relation kind 透传 | 2 |
| schema 错误 | 无效数组 schema、无效锚点 schema | 2 |
| 向后兼容 | 可选字段默认值 | 1 |
| 验证 | 合法通过、distance 必须带值、distance 有值通过、非 distance 无值通过、多 distance 错误、clone 返回 | 6 |
| 小计 | | 15 |

## 既有测试保留

前序冲刺已有的测试未被破坏：

- `scc-analysis`：声明图、MCS、诊断收集器测试（19）
- `scc-lowering`： lowering 规则、参数保留测试（18）
- `scc-execution`：运行时、plan_generator、mutation guard、overlay applier 测试（24）
- `scc-cli`：编译冲突、合法编译等测试（2）
- `compile_integration_tests`：编译集成测试（5）
- `execute_integration_tests`：mutation 解析与应用测试（2）

## 测试执行结果

```text
cargo test --workspace
结果：所有测试通过，无失败、无忽略、无过滤。
```
