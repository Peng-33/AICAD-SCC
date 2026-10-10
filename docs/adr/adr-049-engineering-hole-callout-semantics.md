# ADR-049 — 工程孔标注语义：通孔、盲孔深度与起始面

- 状态：Accepted for Phase 11.4
- 日期：2026-09-12
- 相关：ADR-032、ADR-033、ADR-036、ADR-038、ADR-047、ADR-048

## 背景

Phase 11.3 已经把 Front 视图中的孔位、阵列中心、间距和阵列语义接入 `DrawingModel`。剩余的工程标注缺口是孔的轴向范围：通孔需要显示 `THRU`，盲孔需要显示解析后的深度，并且需要区分从 Base 面还是 Opposite 面开始。投影中的圆、隐藏线、实体 ID 拼写和工程层表达式都不足以单独承担这个语义。

本阶段只扩展三视图派生标注，不改变 `EngineeringModel`、`SpatialModel`、`SolidBuildSpec`、CadQuery、STEP 或 AicadProject 的既有所有权。

## 决策

### 1. 权威边界

- `EngineeringModel.spatial.hole_extents[feature]` 是 `ThroughAll` / `Blind { start_face, depth }` 意图的唯一工程权威。
- `SpatialModel.cuts[*]` 的 `SpatialCutKind` 和 `ZInterval` 是已经解析并验证的空间 witness。
- `DrawingModel` 只消费上述已解析数据；它不解析 ID、不从投影推断范围、不从 Z 反推侧面、不读取或计算盲孔表达式，也不使用隐藏线状态决定 `THRU`/盲孔。
- 侧面是显式的 `HoleStartFace::Base` 或 `HoleStartFace::Opposite`；如果工程语义与空间 witness 不一致，构建或验证必须失败。

### 2. 三种轴向 witness

设板厚为 `T`、盲孔深度为 `D`：

| 语义 | SpatialCutKind | Z witness | 标注尾缀 |
| --- | --- | --- | --- |
| ThroughAll | `ThroughAll` | `[0,T]` | `THRU` |
| Blind/Base | `Blind { start_face: Base }` | `[0,D]` | `↧D BASE` |
| Blind/Opposite | `Blind { start_face: Opposite }` | `[T-D,T]` | `↧D OPP` |

`D` 在标注中使用代表 cut 的 `z.max - z.min`，而不是重新求值工程表达式。阵列内所有 cut 必须具有一致的半径、范围和起始面；否则 fail closed。

### 3. 类型契约

三视图孔标注使用派生的 `HoleCalloutSemantic`：

- `diameter: DimensionComponent`：`ResolvedGeometry` 指向确定性的代表 cut entity，值为该 cut 半径的两倍。
- `quantity: Option<DimensionComponent>`：Circular pattern 使用 `ResolvedPatternSemantic { field: Count }`；Grid/Corner 和单孔不重复存储数量。
- `extent`：`Through` 或 `Blind { depth, side }`；depth 的 source 是 `ResolvedHoleDepth`，side 是显式 `HoleStartFace`。
- `provenance`：feature、代表 entity 以及每个 resolved cut 的 entity/radius/Z/kind witness。

旧的 `Measured` / `Pattern` 值仍服务于既有 Principal drawing；`HoleCallout` 只由三视图生产标注使用。工程语义先保持 typed value，只有 `format_dimension` / SVG / DXF 的 presentation 边界生成文本。

### 4. 视图、布局和 renderer

Front 继续拥有 Z 轴孔标注；Top 与 Left 不复制同一孔标注。代表圆可以是 `Object` 或经过 typed projection provenance 绑定的 `Hidden` 圆，但隐藏线只能作为几何 witness，不能作为孔范围推断依据。直孔使用既有 diameter layout，阵列使用既有 `PatternCallout` layout 和 Phase 11.2 collision/reflow authority，不建立第二个布局引擎。SVG 与 DXF 都消费同一个 `DrawingModel` 和同一个 formatter；不要求原生 DXF semantic dimension entity。

### 5. 验证和持久化

构建后验证以下关系：

- target feature、代表 entity、全量 witness 与 `SpatialModel` 一致且无重复/遗漏；
- 直径等于代表 cut 半径的两倍；盲孔深度等于代表 Z interval 长度；
- ThroughAll、Blind/Base、Blind/Opposite 的 kind、范围和显式 side 逐项一致；
- Circular quantity 与 typed pattern count 一致，pattern 的半径/深度/侧面一致；
- leader 或 diameter geometry 仍附着于代表投影圆。

`AicadProject v1` 仍只保存 `EngineeringModel`。`HoleCalloutSemantic`、DrawingModel、SpatialModel、SolidBuildSpec 和 STEP 都是派生或交换产物，不进入项目源文件。

## 被拒绝的方案

1. **从前视图圆或隐藏线推断 Through/Blind**：投影丢失轴向意图，且视角/隐藏分类变化会改变工程含义。
2. **从 Z interval 推断 Base/Opposite**：Z 只证明空间范围；侧面必须保留工程层显式语义，避免把几何巧合当成设计意图。
3. **在 Drawing 层重新求值 `SourcedLength`/参数表达式**：会绕过单一解析路径并可能产生与 SpatialModel 不同的值。
4. **在每个阵列实例上生成独立孔标注或另建布局引擎**：破坏 Phase 11.2/11.3 的单一阵列语义和稳定碰撞布局。
5. **把派生 callout 写回 AicadProject 或 STEP 作为 truth**：会混合 source 与 artifact，破坏 Phase 10.4/10.3 的持久化和交付边界。

## 影响与兼容性

这是 DrawingModel 的 additive schema 扩展：新增 typed source/semantic variant；既有 Principal drawing、制造 lowering、CadQuery、STEP 和 UI project persistence 不变。旧的 `DimensionValue::Measured` / `Pattern` 消费者需要为新的 variant 增加 exhaustiveness 分支，但既有序列化数据不需要迁移。

金样仅在 Phase 11.4 的三视图标注文本与代表 leader anchor 改变；Principal 金样保持不变。

## 迁移与冻结

Phase 11.4 的冻结条件是：直孔与 Circular/Grid/Corner pattern 的 ThroughAll、Blind/Base、Blind/Opposite 均由 typed witness 生成；side/depth/radius/cardinality/representative/layout 篡改均 fail closed；SVG/DXF 文本一致；Save/Open 不序列化派生 callout；Rust workspace 回归通过。达到条件后不在本阶段增加新孔型、沉头/螺纹/PMI 或高级 leader routing；后续需求进入 Phase 11.5 的独立决策。
