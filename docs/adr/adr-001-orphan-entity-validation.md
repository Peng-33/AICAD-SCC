# ADR-001: SCC 孤立实体校验策略

## 背景

SCC M2 运行时新增了 `GraphState::validate_final()` 方法，要求"每个实体必须至少被一个约束引用"（不允许孤立实体）。该校验在 `ExecutionRuntime::run()` 中于 overlay 应用后被强制调用，导致 4 个已有测试失败。

## 证据分析

### 1. 架构规范中 M2 的职责

SCC 架构规范明确：

> **M2 运行时**：纯状态机；无求解器、无几何计算
> **M2 职责**：确定性状态转换，仅处理符号状态（GraphState）
> **M3（未来）**：只读 GraphState 并输出几何 / CAD 输出

M2 的核心契约是"给定输入 ConstraintIR + MutationPatch，输出确定的 GraphState + ExecutionTrace"。

### 2. 测试表达的合法行为

4 个失败测试的共同模式：

```
test_ir(): entities = [plate, hole], constraints = []
overlay = UpdateParameter(hole.diameter)
```

或：

```
execute_with_overlay: entities = [circle_1], constraints = []
overlay = UpdateParameter(circle_1.diameter)
```

这些测试表达了一个**完全合法的使用场景**：实体先被定义，约束后添加（通过 overlay）。M2 不应阻止这种工作流。

### 3. 现有校验的层级混淆

`GraphState` 提供两个校验方法：

| 方法 | 检查内容 | 是否阻断 | 适用层级 |
|------|---------|---------|---------|
| `validate()` | 引用完整性：约束引用的实体必须存在 | 是 | M2 运行时 |
| `validate_final()` | 连通性：每个实体必须有约束引用 | 是（当前） | M3 前端检查 |

`validate_final()` 的连通性检查实际上是**语义不变量**而非**结构不变量**。M2 作为纯状态机，只应强制结构不变量。连通性规则更适合由调用方（CLI、M3 前置检查、事务提交层）在需要时调用。

### 4. overlay 的事务语义

当前 overlay 工作流：

```
base_ir + overlay → apply_overlay → validate_final → commit
```

如果 overlay 在一个操作中添加实体，在后续操作中添加约束，中间状态必然存在孤立实体。在校验点时刻，可能是"实体刚添加、约束尚未添加"的阶段。因此在校验点检查连通性会误阻断合法的工作流。

## 决策

### 最终方案

**将连通性检查从 M2 运行时中分离，M2 只保留引用完整性检查。**

具体变更：

1. **保留 `GraphState::validate_final()` 作为公共 API**，不删除、不修改其语义。
2. **`ExecutionRuntime::run()` 不再强制调用 `validate_final()`**，改为调用 `validate()` 只检查引用完整性。
3. **CLI 层在需要时可显式调用 `validate_final()`**，例如在最终输出前做额外检查。
4. **新增 `validate_integrity()` 方法**（可选）明确语义为"引用完整性检查"。

### 被否决方案

| 方案 | 否决原因 |
|------|---------|
| 在测试夹具中加 dummy constraint | 掩耳盗铃——生产代码同样会出现孤立实体 |
| `validate_final` 改为 log warning | 改变 API 可观察行为，warning 可能无人读取 |
| 无条件删除 `validate_final` | 丢失有用的公共 API |
| 按实体类型区分 | 增加不必要的复杂度，架构未定义实体分类 |

### 影响范围

| 组件 | 影响 |
|------|------|
| `scc-core/src/runtime.rs` | 无修改。保留 `validate_final()` 完整语义 |
| `scc-execution/src/runtime/execution_runtime.rs` | `run()` 中 `validate_final()` → `validate()` |
| 4 个回归测试 | 自动恢复通过 |
| CLI (`scc-cli`) | 无影响，execute 子命令仍获得正常输出 |
| 公开 API | `validate_final()` 保留，签名不变 |

## 兼容性

- 所有已有 Proposal JSON 和 ConstraintIR 不受影响
- 公开 API 完全向后兼容
- ExecutionTrace 格式不变
- GraphState JSON 格式不变

## 新增测试

新增 6 个测试覆盖：

1. **正常引用**：多个实体被约束引用 → 验证通过
2. **初始孤立实体**：初始 IR 中存在孤立实体 → 不阻断执行
3. **Overlay 新增孤立实体**：Overlay 添加新实体无约束 → 不阻断执行
4. **悬空引用**：约束引用不存在实体 → 阻断（validate 捕获）
5. **显式严格验证**：在外部调用 `validate_final()` 能检测孤立实体
6. **确定性**：相同输入（含孤立实体）重复执行得到相同 Trace 和 root

## 结论

允许 M2 状态机中存在孤立实体，连通性检查留给调用方和 M3 阶段。