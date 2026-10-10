# ADR-002: SCC 结构化错误模型

## 背景

当前 SCC 使用 `scc_core::error::SccError` 作为跨 crate 的通用错误类型。该枚举以字符串消息为主：

```rust
pub enum SccError {
    Execution { message: String },
    Conflict { message: String },
    Serialization(serde_json::Error),
    Io(std::io::Error),
}
```

同时，`scc-execution/src/driver.rs` 将下层的 `RuntimeError`、`MutationError` 等压平成 `SccError::Execution { message }`：

```rust
Err(e) => Err(SccError::Execution {
    message: format!("execution failed: {e}"),
}),
```

这导致：

1. 调用方无法通过类型判断错误阶段（Proposal 解析、Analysis、Lowering、Execution、CLI）。
2. 自动化流水线无法根据错误码做分支处理。
3. 错误上下文（涉及哪个实体、哪个约束、哪个 JSON Path）丢失。
4. CLI 无法实现 `--json-errors` 等机器可读输出。

本 ADR 定义 v0.2.0 将引入的结构化错误契约，v0.1.0 仅完成设计，不修改现有公开 API。

## 决策

### 1. 错误信息模型

每个结构化错误应包含以下字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `code` | 稳定字符串 | 形如 `SCC-PROP-001`、`SCC-EXEC-003` |
| `stage` | 枚举 | `proposal`、`analysis`、`lowering`、`execution`、`cli` |
| `severity` | 枚举 | `error`、`warning` |
| `message` | 字符串 | 人类可读一句话说明 |
| `path` | 可选字符串 | JSON Path 或字段名，指向输入中的问题位置 |
| `entity_ids` | 可选 ID 列表 | 相关实体 ID |
| `constraint_ids` | 可选 ID 列表 | 相关约束 ID |
| `source` | 可选错误链 | 下层错误的 Cause chain |
| `recoverable` | 布尔 | 是否可能通过重试或修改输入恢复 |
| `suggestion` | 可选字符串 | 建议的修复动作 |

### 2. 错误码命名规范

错误码格式：`SCC-{STAGE}-{NNN}`

| Stage 缩写 | 含义 |
|-----------|------|
| `PROP` | Proposal 解析与校验 |
| `ANAL` | 语义分析 |
| `LOW` | 约束降级 |
| `EXEC` | 执行与运行时 |
| `CLI` | 命令行接口 |

示例：

- `SCC-PROP-001`：JSON 语法错误
- `SCC-PROP-002`：必填字段缺失
- `SCC-PROP-003`：输入超过大小限制（当前为保留码，未实现）
- `SCC-ANAL-001`：引用实体不存在
- `SCC-ANAL-002`：类型不匹配
- `SCC-ANAL-003`：循环依赖
- `SCC-LOW-001`：无法降级的实体类型
- `SCC-EXEC-001`：悬空实体引用
- `SCC-EXEC-002`：L0 约束修改被拒绝
- `SCC-CLI-001`：输出文件已存在

### 3. 分阶段错误类型

各 crate 定义自己的结构化错误类型，仅在边界处转换为 `SccError`：

```rust
// scc-proposal
pub enum ProposalValidationError {
    InvalidJson { code: String, path: Option<String>, source: serde_json::Error },
    MissingField { code: String, path: String },
    OversizedInput { code: String, limit: usize, actual: usize },
    // ...
}

// scc-lowering
pub enum LoweringError {
    UnsupportedEntityKind { code: String, entity_id: EntityId, kind: String },
    UnsupportedRelation { code: String, relation_kind: String },
    // ...
}

// scc-core
pub enum ValidationError {
    MissingEntity { code: String, constraint_id: ConstraintId, entity_id: EntityId },
    OrphanEntity { code: String, entity_id: EntityId },
    // ...
}

// scc-execution
pub enum RuntimeError {
    MutationRejected { code: String, entity_id: EntityId, reason: String },
    DanglingReference { code: String, entity_id: EntityId },
    // ...
}
```

### 4. `SccError` 兼容策略

`SccError` 作为历史公开类型在 v0.2.0 继续保留，并新增结构化变体：

```rust
pub enum SccError {
    // 保留现有变体，保持向后兼容
    Execution { message: String },
    Conflict { message: String },
    Serialization(serde_json::Error),
    Io(std::io::Error),

    // 新增结构化变体
    Structured(Box<StructuredError>),
    StructuredBatch(Vec<StructuredError>),
}
```

**兼容性说明：** 新增 `SccError::Structured` 与 `SccError::StructuredBatch` 变体属于公开枚举的扩展。单条诊断使用 `Structured`，同一确定性检查阶段产生多条诊断时使用 `StructuredBatch`，CLI 必须完整输出而不能只保留第一条。对于仅通过 `Display`/`Error` 使用 `SccError` 的调用方，此变更保持源码兼容；但对 `SccError` 进行穷尽 `match` 的调用方，需要新增对这些变体的处理（或通配分支）。因此 v0.2.0 不再声称“完全源码兼容”，仅保证：
- 不删除任何现有变体或字段；
- 现有公开函数签名优先保留（新增 `detailed` / `try` / `validate_contract` 等承载新能力）；
- legacy 成功路径 JSON 输出格式不变；
- ConflictReport 默认仍为正常业务产物。

通过 `From<ProposalValidationError> for SccError`、`From<LoweringError> for SccError` 等实现无损转换。

### 4.1 错误码前缀与契约校验

`ErrorStage::code_prefix()` 固定映射：

| ErrorStage | 前缀 |
|------------|------|
| Proposal | `PROP` |
| Analysis | `ANAL` |
| Lowering | `LOW` |
| Execution | `EXEC` |
| Cli | `CLI` |

`StructuredError::validate_contract()` 校验（无 regex 依赖）：

- `code` 非空；
- `message` trim 后非空；
- `code` 格式为 `SCC-{STAGE}-{NNN}`，`NNN` 为严格三位 ASCII 数字；
- `{STAGE}` 与 `ErrorStage::code_prefix()` 一致；
- 存在 `source` 时递归校验，失败映射为 `ErrorContractViolation::InvalidSource`。

```rust
pub enum ErrorContractViolation {
    EmptyCode,
    EmptyMessage,
    InvalidCodeFormat,
    StageMismatch,
    InvalidSource,
}
```

正式构造的 `StructuredError` 在对外输出前必须通过 contract validation。

Driver 不再将下层错误压平为字符串，而是保留完整 Cause：

```rust
Err(e) => Err(SccError::Structured(Box::new(
    StructuredError::new("SCC-EXEC-001", ErrorStage::Execution, "execution failed")
        .with_source(*e),
))),
```

### 5. CLI 退出码映射

| 退出码 | 含义 | 触发条件 |
|--------|------|---------|
| `0` | 成功 | 正常完成，包括生成 ConflictReport |
| `1` | 输入/系统错误 | JSON 错误、IO 错误、未知命令 |
| `2` | 业务冲突 | 使用 `--fail-on-conflict` 时检测到冲突 |
| `3` | 运行时完整性错误 | 执行阶段验证失败 |

默认情况下，冲突报告作为正常产物输出，保持向后兼容。

### 6. JSON 错误协议

`--json-errors` 启用时，CLI 将错误输出为：

```json
{
  "status": "error",
  "errors": [
    {
      "code": "SCC-CLI-001",
      "stage": "cli",
      "severity": "error",
      "message": "output file already exists",
      "path": "bracket.ir.json",
      "recoverable": true,
      "suggestion": "use --force to overwrite or choose a different output path"
    }
  ]
}
```

即使只有一个错误，也使用 `errors` 数组。`status` 固定为 `"error"`。`stage` 与 `severity` 使用小写序列化值。所有字段均需符合 [`StructuredError`] 契约，并在输出前通过 `validate_contract()` 校验。`--json-errors` 同样覆盖 CLI 参数解析失败；与 `--fail-on-conflict` 组合时，ConflictReport 继续落盘，同时 stderr 输出 `SCC-CLI-009`。

## 被否决方案

| 方案 | 否决原因 |
|------|---------|
| v0.1.0 直接替换 `SccError` | 破坏公开 API，扩大首次发布回归面 |
| 直接删除字符串错误变体 | 导致所有现有调用点编译失败 |
| 统一使用 `anyhow` | 失去错误阶段和错误码的机器可读性 |
| 将错误码写死为数字 | 数字不如层级化的字符串易读和稳定 |

## 影响范围

| 组件 | v0.1.0 | v0.2.0 |
|------|--------|--------|
| `scc-core/src/error.rs` | 不修改 | 新增 `StructuredError` 和 `ValidationError` |
| `scc-proposal/src/validate.rs` | 不修改 | 返回 `ProposalValidationError` |
| `scc-lowering/src/lowering.rs` | 不修改 | 返回 `LoweringError` |
| `scc-execution/src/driver.rs` | 不修改 | 保留错误链，不压平为字符串 |
| `scc-cli` | 不修改 | 增加 `--json-errors`、结构化输出、退出码 2/3 |

## 迁移计划

1. **v0.1.0**：ADR 定稿，公开 API 不变。
2. **v0.2.0-alpha**：在各 crate 内部新增结构化错误类型。
3. **v0.2.0-beta**：Driver 保留错误链；CLI 实现 `--json-errors`。
4. **v0.2.0**：完成跨 crate 迁移，更新测试与文档。
5. **未来主版本**：考虑移除 `SccError::Execution { message }` 等旧字符串变体。

## 结论

v0.2.0 将引入分阶段、带错误码、可携带上下文的结构化错误模型；v0.1.0 仅完成设计并冻结当前字符串错误接口，不引入破坏性变更。

## v0.3.0 extension

ADR-004 extends the stage registry with `geometry` / `GEOM` for M3. Geometry codes use `SCC-GEOM-NNN`, and the CLI maps geometry-stage failures to exit code `4`. This is an additive enum variant and therefore requires an extra arm for callers that exhaustively match `ErrorStage`.
