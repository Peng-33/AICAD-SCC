# ADR-003: SCC Schema 版本化策略

## 背景

SCC 当前输出多种 JSON 文档：`Proposal`、`ConstraintIR`、`GraphState`、`ExecutionTrace`。这些结构目前都没有显式的 schema 版本字段。随着 M3 引入几何求解和更多约束类型，JSON 结构可能发生变化，导致：

1. 旧文件无法被新版本的 CLI 正确识别。
2. 新文件被旧版本的 M3 消费者错误解析。
3. Golden Fixture 的维护成本增加。
4. 无法优雅地拒绝不兼容的旧文件。

`Proposal` 已有业务字段 `version`（如 `"1.0"`），但它表示的是用户提案的业务版本，不应与内部 IR/State/Trace 的 schema 版本混用。

本 ADR 定义 v0.2.0 将引入的 Schema 版本化方案；v0.1.0 仅完成设计，不改变现有 JSON 输出。

## 决策

### 1. 需要版本化的文档

| 文档类型 | 当前是否有版本字段 | 说明 |
|---------|-------------------|------|
| `Proposal` | 有（`version`） | 业务版本，非 schema 版本 |
| `ConstraintIR` | 无 | 必须新增 schema 版本 |
| `GraphState` | 无 | 必须新增 schema 版本 |
| `ExecutionTrace` | 无 | 必须新增 schema 版本 |

### 2. 外层 Envelope 方案

推荐采用外层 Envelope，而非在每个结构内部添加字段：

```json
{
  "schema_version": "1.0",
  "document_type": "constraint_ir",
  "payload": { ... }
}
```

`document_type` 取值：

- `proposal`
- `constraint_ir`
- `graph_state`
- `execution_trace`

### 3. 采用 Envelope 而非内嵌字段的原因

| 方案 | 优点 | 缺点 |
|------|------|------|
| 外层 Envelope | 版本与负载解耦；便于 future proof；CLI 可先读版本再决定解析策略 | 需要一次额外的反序列化步骤 |
| 内嵌 `schema_version` 字段 | 与现有结构简单 | 污染领域模型；每个结构都要改；与 `Proposal.version` 容易混淆 |
| HTTP Content-Type 头 | 不修改 JSON | 仅适用于传输，不适用于文件存储 |

Envelope 方案允许 CLI 在解析 `payload` 之前先检查版本，避免将不兼容数据传给反序列化器。

### 4. 版本号规则

Schema 版本采用**两段式** `MAJOR.MINOR`，不等同于软件的 SemVer：

- `MAJOR.MINOR`（例如 `1.0`、`1.1`、`2.0`）
- 主版本升级：结构不兼容，旧消费者必须拒绝。
- 次版本升级：仅新增可选字段，旧消费者可安全忽略。

初始 schema 版本定为 `1.0`，与 `Proposal` 的现有业务 `version` 字段解耦。

### 5. 兼容性策略

#### 5.1 缺少版本字段的旧文件

v0.2.0 的 CLI 读取没有 Envelope 的文件时：

- 尝试将文件识别为 `legacy-unversioned` 格式（即当前无版本字段的格式）。
- 如果成功解析，按 `legacy-unversioned` 处理。
- 如果解析失败，返回 `SCC-CLI-008`："无法识别文件 schema 版本或 payload 格式"。

`legacy-unversioned` 不是 schema 版本，而是对 v0.1.0 输出格式的兼容性标签。

#### 5.2 主版本不兼容

当文件 schema 主版本高于 CLI 支持的最大主版本时：

```json
{
  "error": {
    "code": "SCC-CLI-006",
    "message": "unsupported schema version 2.0, maximum supported is 1.x"
  }
}
```

#### 5.3 次版本新增字段

次版本新增的可选字段必须具有 serde default，旧消费者反序列化后**默认忽略未知字段**（serde 默认行为）：

```rust
#[derive(Deserialize)]
struct ConstraintIr { ... }
```

如果需要显式拒绝未知字段以捕获配置错误，应仅在内部调试模式或严格校验模式下启用 `deny_unknown_fields`，而不是作为默认兼容策略。

### 6. `Proposal.version` 的处理

`Proposal.version` 继续作为业务版本保留，不参与 schema 版本判断。Envelope 中 `document_type = "proposal"` 时，`payload` 内部仍可包含 `"version": "1.0"`。

### 7. Golden Fixture 迁移

1. v0.2.0 开发期间，保留现有无 Envelope 的 Golden Fixture 作为 `legacy-unversioned` 基线。
2. 新增带 Envelope 的 Golden Fixture 作为 `schema 1.0` 基线。
3. CI 同时测试两种格式的解析。
4. 弃用 `legacy-unversioned` fixture 前需满足：
   - 至少保留一个完整软件主版本的兼容窗口。
   - 发布弃用公告。
   - 提供迁移工具或脚本（如需要）。
   - 确认无已知外部消费者依赖该格式。

### 8. M3 消费边界

M3 几何求解器在消费 `GraphState` 时必须声明接受的 schema 版本范围：

```rust
pub const SUPPORTED_GRAPH_STATE_SCHEMA: &str = "1.0";
```

CLI 在执行 `scc execute` 前检查输入 IR 的 schema 版本；M3 在读取 State 前检查 State 的 schema 版本。

### 9. CLI 行为

新增 `--schema-version` 选项用于显式指定输出格式（默认保持 `legacy-unversioned`，避免破坏现有消费者）：

```text
scc compile input.json --schema-version 1.0
scc execute input.ir.json --schema-version 1.0
```

无论输出格式如何，`scc execute` 和 `scc inspect` 都同时支持读取带 Envelope 的 `1.0` 文件和 `legacy-unversioned` 文件，实现向前和向后兼容。

v0.2.0 的 CLI Envelope 范围明确如下：

- `compile` 和 `validate` 的输入仍为 legacy Proposal JSON；`Proposal.version` 只表示业务版本。
- `execute` 和 `inspect` 读取 `constraint_ir` Envelope 或 legacy Constraint IR。
- `compile --schema-version 1.0` 输出 `constraint_ir` Envelope；`execute --schema-version 1.0` 输出 `graph_state` 与 `execution_trace` Envelope。
- ConflictReport 不属于 v0.2.0 Envelope 读取/写入矩阵，仍保持 legacy JSON，避免未经 ADR 决策扩展公开文档类型。

只要顶层出现 `schema_version`、`document_type` 或 `payload` 任一 Envelope 标记，就必须按完整 Envelope 校验，禁止回退到 legacy 解析。版本文本必须严格满足两个数字段组成的 `MAJOR.MINOR`，且主版本必须与当前支持主版本一致。

`--output-format` 未来可扩展为 `json`、`json-envelope`、`msgpack` 等。

### 10. 弃用策略

- 每个不兼容的 schema 主版本至少保留一个完整软件主版本的读取支持。
- 删除 `legacy-unversioned` 支持前，必须发布至少一个版本的弃用公告。
- 优先提供迁移工具或命令（如 `scc migrate --to-schema 1.0`）。
- 在确认无外部消费者依赖旧格式后，才允许彻底移除兼容代码。

## 11. 输入/输出 Schema 版本矩阵

下表定义 v0.2.0 中每种文档类型的输入接受格式和输出生成格式。

| 文档类型 | 输入可接受格式 | 输出生成格式 |
|---|---|---|
| Proposal | Legacy JSON（无 Envelope） | 不适用（仅作为输入消费） |
| ConflictReport | 不适用（仅作为输出生成） | Legacy JSON（无 Envelope） |
| ConstraintIR | Legacy JSON 或 Envelope（自动检测） | Legacy JSON（默认）或 Envelope（`--schema-version 1.0`） |
| GraphState | 不适用（仅作为输出生成） | Legacy JSON（默认）或 Envelope（`--schema-version 1.0`） |
| ExecutionTrace | 不适用（仅作为输出生成） | Legacy JSON（默认）或 Envelope（`--schema-version 1.0`） |

### 11.1 关键约束

- **Proposal** 的 `version` 字段是业务版本，不等同于 Schema 版本，Proposal 不强制使用 Envelope。
- **ConflictReport** 不属于 Schema 版本化范围，不进入 Envelope。
- **CLI 命令行为：**
  - `compile --schema-version 1.0` → 输出带 Envelope 的 ConstraintIR。
  - `execute --schema-version 1.0` → 输出带 Envelope 的 GraphState 和 ExecutionTrace。
  - `execute` 和 `inspect` 可自动读取 Legacy 或 Envelope 格式的 ConstraintIR。
- **输出版本固定：** CLI `--schema-version` 只接受 `legacy`（默认）或 `1.0` 两个值。
- **输入同主版本兼容：** Envelope 解析器接受 `1.x` 系列的所有次版本。
- **防降级保护：** 当顶层出现 `schema_version`、`document_type` 或 `payload` 任一字段时，系统必须按完整 Envelope 解析，不能回退为 legacy JSON。该策略避免不完整或恶意版本信息被静默忽略。

### 11.2 版本校验

CLI `--schema-version` 只接受规范的两段数字格式 `MAJOR.MINOR`。以下值均被拒绝（退出码 1，错误码 `SCC-CLI-006`）：

- `1`（缺少次版本）
- `1.`（次版本为空）
- `1.foo`（次版本非数字）
- `1.0.0`（三段式，非 MAJOR.MINOR）
- `0.9`（低于 `1.0` 最低支持版本）
- `2.0`（超出 `1.x` 支持范围）

## 12. 被否决方案

| 方案 | 否决原因 |
|------|---------|
| 直接用 `Proposal.version` 作为所有结构的版本 | 业务版本与 schema 版本语义不同 |
| 在 v0.1.0 直接加入 `schema_version` 字段 | 改变首次发布的 JSON 输出，破坏 Golden Fixture |
| 使用单一全局版本号 | 无法独立演进 Proposal、IR、State、Trace |
| 不版本化，依赖 Cargo 版本 | 文件可能脱离二进制长期存在，需要自描述 |

## 13. 影响范围

| 组件 | v0.1.0 | v0.2.0 |
|------|--------|--------|
| `scc-core` 序列化结构 | 不修改 | 支持 Envelope 反序列化 |
| `scc-cli compile` | 不修改 | 默认输出 `legacy-unversioned`；`--schema-version 1.0` 时输出带 Envelope 的 JSON |
| `scc-cli execute` | 不修改 | 默认输出 `legacy-unversioned`；读取时兼容带 Envelope 的 IR 与 `legacy-unversioned` |
| `scc-cli inspect` | 不修改 | 读取时兼容带 Envelope 的 IR 与 `legacy-unversioned` |
| Golden Fixture | 不修改 | 新增 v1.0 fixture |
| M3 接口 | 不修改 | 声明接受的 GraphState schema 版本 |

## 14. 迁移计划

1. **v0.1.0**：ADR 定稿，所有 JSON 输出保持当前格式。
2. **v0.2.0-alpha**：在 `scc-core` 中定义 `SchemaEnvelope` 类型。
3. **v0.2.0-beta**：CLI `compile`/`execute` 默认继续输出 `legacy-unversioned`，同时支持 `--schema-version 1.0` 输出带 Envelope 的 JSON；保留读取 `legacy-unversioned` 格式能力。
4. **v0.2.0**：更新 Golden Fixture 和文档；M3 接口声明 schema 版本支持。
5. **未来版本**：在满足弃用条件（兼容窗口、公告、迁移工具、无外部依赖）后，评估是否停止支持 `legacy-unversioned` 格式。

## 15. 结论

v0.2.0 将引入外层 Envelope 的 schema 版本化机制，初始版本为 `1.0`，与 `Proposal.version` 解耦；v0.1.0 保持现有 JSON 格式不变，避免在首次发布前引入破坏性输出变更。
