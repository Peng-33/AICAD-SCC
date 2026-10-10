# ADR-064：EngineeringModel 通用参数化特征基础（G1）

- Status: Accepted for Phase G1 foundation; downstream geometry enablement remains partial
- Date: 2026-09-25
- Scope: EngineeringModel persistence, ordered parametric feature history, migration boundary, and future geometry integration contract

## Context

现行正式 EngineeringModel (`crates/scc-engineering`) 把设计意图存为单一 `BTreeMap<FeatureId, EngineeringFeature>`。模型尚无有序建模历史或正式 Sketch/Datum/Extrude；SpatialModel 当前只接受唯一 BaseProfile，并把它降低为沿 +Z 的实体，再处理既有 Hole/Slot。SolidBuildSpec 与正式 STEP 流程也以该 SpatialModel 为输入。旧的 `feature_graph_v1` 属于兼容 CAD 编辑路径，不能被反向提升为正式 EngineeringModel 或 STEP 权威（ADR-063）。

用户已改变产品目标：机械零件应由通用参数化原语组合。G1 必须扩展现有唯一工程真值，而非建立第二个可编辑 CAD Document、测试件特判、renderer 特判或后端特判。

## Decision

### 1. 唯一真值及旧文档兼容

1. `EngineeringModel` 继续是唯一正式可编辑模型。新对象以 `EngineeringFeature` 的带类型变体存在于既有 feature map；map 是按稳定 ID 寻址的对象存储。
2. 新增的可选 `feature_order: Vec<FeatureId>` 只表示历史顺序，不复制特征参数或几何。进入通用历史模式时，它必须恰好列举模型内所有 feature 一次；ID 查找仍由原 map 完成。
3. 旧文档没有 `feature_order` 时继续反序列化为 legacy 模式。旧模型的历史顺序原先未定义，读取时不虚构其用户意图；旧几何仍由原 pipeline 处理。空顺序不写入 JSON，以维持 legacy fixture 的字节稳定。无参数化 history 的项目继续按严格 Project v1 保存；带参数化 feature 的项目使用 Project v2，避免旧消费者把新 tagged feature 当成 v1。v1 读取不执行自动数据转换。
4. 参数化历史模式拒绝旧 `PartSpatialDefinition` 的 thickness/hole extents 与旧几何 features 混入，避免同一实体同时存在两份空间真值。尚不自动把旧 BaseProfile/Hole/Slot 影射为新 Sketch/Extrude。转换可能改变约束、语义孔、空间延伸、参数绑定及项目指纹。后续迁移必须是显式、可预览、可回滚的纯 EngineeringModel 转换，并对不等价输入 fail-closed。新 parametric history 与旧几何 feature 混用在 G1 初期拒绝。
5. `CadDocument`/FeatureGraph 保持 ADR-063 所限定的兼容用途；不得成为 EngineeringModel 的第二正式真值或正式 STEP 来源。

### 2. G1 对象模型

```text
EngineeringModel
  ├─ PartDefinition
  └─ ordered feature IDs → feature map
       ├─ DatumPlane
       ├─ Sketch (support + outer loop + zero-or-more inner loops)
       └─ Extrude (profile ref + ADD/CUT + direction + extent)
```

- `SketchProfile` 由稳定 ID 的 2D Line/Arc/Circle 实体构成。Circle 是单实体闭环；其余 loop 必须连续闭合。Rectangle、RoundedRectangle、Slot、Polygon 是这些基本曲线的确定性构造器，不是独立实体内核 Feature。闭环自交、重合、loop 间相交/包含关系和 kernel 可接受性由几何重建边界再次验证，不能以本地 schema 检查替代。
- Profile 持有一个 outer loop 和 `inner_loops` 列表；G1B 可暂时拒绝 inner loops 的 solid lowering，但数据模型不得退化成单 `BaseProfile` 字段。
- Sketch 必须引用主平面或先前创建的 DatumPlane。主平面为 XY/XZ/YZ；DatumPlane 可由基准平面偏移得到。PlanarFaceReference 与 CustomPlane 作为保留扩展点，在实现前不得解引用。
- `Extrude` 引用先前 Sketch，仅支持 ADD/CUT、法向/显式方向及 DISTANCE/THROUGH_ALL/SYMMETRIC。UP_TO_FACE/NEXT 在 transport/type contract 中保留前，不得宣称支持或接受到正式 lowering。
- DatumAxis/DatumPoint、语义 Hole、Linear/Circular Pattern、Mirror/Transform 可沿同一对象历史和强类型引用扩展；Pattern 保留 count/spacing-or-radius/axis/start-angle 参数，在 derived rebuild 才展开。它们不是 G1B 已支持能力。
- Hole 扩展契约为 `support_plane + position + axis + diameter + termination`；termination 至少分 `THROUGH_ALL` 与带 depth 的 `BLIND`，counterbore/countersink/spotface 是可选次级尺寸。现有 `Hole` 保持旧 XY 语义，不能因此声称任意轴向孔已实现。
- Linear/Circular Pattern、Mirror、Transform 各自引用 seed Feature ID 与 typed transform 参数；实例不复制成无关联工程特征。Derived rebuild 分配确定性 instance key，并保留 pattern owner 与 seed provenance。
- DatumAxis/DatumPoint 使用稳定 Feature ID；Axis 由 datum origin/point 与方向确定，Point 由 support 与局部坐标确定。不得用渲染或 kernel entity 序号作引用。
- 历史引用必须向后、无环、可重建。Replace 保留目标 ID；suppress/unsuppress 不删除源对象；reorder 必须再次校验所有依赖。删除仍采用拒绝被引用目标策略。

### 3. Rebuild、Body 和拓扑引用

- 重建是 EngineeringModel 的纯函数：输入已验证模型与显式参数值，输出不可变 derived result；相同输入须有相同顺序、IDs 和结果。重建不得调用模型生成代码或允许其访问 CAD kernel。
- G1 优先单 Part、一个受控 single-solid Body。每个 ADD/CUT 必须对应同一 Body 的有序布尔操作；悬空 ADD、断连、无效、自交或切空结果均失败关闭。多 Body 是显式后续能力，不得把分离 B-Rep 叠显示当成一个实体。
- `FeatureReference` 指向稳定 feature ID。未来 `FaceReference`/`EdgeReference` 使用 owner Feature ID、语义 selector 和几何 qualifier（例如端面、圆柱面、半径/方向及容差）；不得把 kernel edge/face ordinal 或临时生成的裸 EntityId 持久化为工程含义。
- Fillet/Chamfer 引用 `(owner_feature_id, semantic selector, geometric qualifier, tolerance)`。选择器不能唯一解析或重建后失效时，特征失败关闭；G1 不硬编码测试件边。
- Revolve 引用 closed Sketch、DatumAxis、角度及 ADD/CUT operation；Sweep 引用 profile、guide path 与 orientation/termination contract；Loft 引用有序 section sketches 与连接/平滑规则。三者复用同一 ordered history、依赖校验、抑制、确定性 rebuild 和 support-status contract，不重建 EngineeringModel。

### 4. 下游能力分层

1. Sketch/Feature intent → 几何重建 → `SpatialModel` 必须是唯一正式空间来源；SpatialModel 仍只读。
2. Drawing 继续消费 `SpatialModel → ProjectionModel → VisibilityModel → DrawingModel`，不得解释测试件名称或工程特征名称来拼图。
3. `drawing_support` 与 `manufacturing_support` 是每个 Feature lowering 的独立 capability disposition。可投影不代表可制造。
4. 正式 STEP 保持 `EngineeringModel → SpatialModel → SolidBuildSpec → CadBackendClient → CadQuery/OCP → reimport/equivalence → exact validated-byte delivery`（ADR-063）。不得让 compatibility FeatureGraph 或 drawing 输出产生正式 STEP。
5. G1B 若尚未实现某个下游 lowering，应给出带 feature ID 的明确 unsupported error；不得跳过 feature、静默画成 BaseProfile 或输出残缺成功模型。

### 5. 命令与前端边界

命令只接受有限 JSON/schema 值，不接受任意 CAD 程序。AddFeature 提交一个带 ID 的类型化节点；ReplaceFeature、SetFeatureSuppressed 和 ReorderFeature 走与其他 EngineeringCommand 相同的原子 executor 与模型校验。DSL、Tool Calling、六大 CAD Skills 以后将把几何意图规划为 Sketch/Datum/Extrude 组合，不能要求模型输出零件专用 Feature 名称。G1B command layer 不等于自然语言、DSL 或 UI 已完成集成。

## Migration and implementation plan

1. G1A（本 ADR）：锁定单真值、legacy read contract、顺序/依赖/扩展/下游边界。
2. G1B foundation：将通用 typed feature、Sketch loops、DatumPlane、Extrude、历史顺序和原子 replace/suppress/reorder 加入 EngineeringModel；测试 schema、ID、引用、向后兼容与确定性历史顺序。
3. 后续分段：闭环/profile/Datum 几何 lowering → 通用 ADD/CUT 的 SpatialModel 与布尔重建 → Projection/Visibility/Drawing → 独立 SolidBuildSpec 与已验证 STEP 支持 → DSL/Tools/Skills/UI → Hole/Pattern/semantic topology。
4. 每个 downstream feature type 独立声明支持状态并有相应几何、drawing 和 manufacturing 验证；不得因 G1B schema 存在而将本 ADR 或 G1 标记全面 PASS。

## Rejected alternatives

- 保留单一 BaseProfile 并给凸台、盲槽等增加专用 Feature：违反通用组合目标，造成类型和下游特判持续增长。
- 直接把旧 compatibility FeatureGraph 用作 EngineeringModel/STEP 来源：缺少安全、无损映射，且违反 ADR-063 权威边界。
- 保存第二份 `ParametricDocument` 与现有 EngineeringModel 并互相同步：产生双写与漂移。
- 首次写入时自动把 legacy features 排序成用户历史，或自动猜测 feature 支持面/布尔顺序：输入不含这些语义，迁移会制造设计意图。
- 持久化 kernel 生成的 edge ordinal：拓扑重建后不稳定。

## G1 acceptance evidence

通过条件必须覆盖多个独立组合件、多个按序 ADD/CUT、依赖修改后的 deterministic rebuild、负例（悬空 ADD、切空、重复 ID）、legacy load、Spatial→Drawing，以及制造独立状态。AICAD-COMPLEX-001 只能是无专用代码的组合验证件。Schema/graph tests alone 不足以宣称 G1 完成。

G1B foundation 当前明确不声称通用几何/Boolean rebuild、Spatial/Drawing/STEP 闭环、DSL/Tool/Skill/UI 接入或复杂零件通过。它们保持 `PARTIAL`/`DEFERRED`，直至实际证据齐备。
