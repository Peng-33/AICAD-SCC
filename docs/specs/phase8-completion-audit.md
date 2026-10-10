# Phase 8 Completion Audit — Engineering Drawing Engine

- Status: **READY WITH BLOCKERS**（1 个必须在 freeze 前修复的渲染缺陷；架构本身成立）
- Date: 2026-08-23（审计执行日）
- Scope: 只审计、只验证、只输出报告；未修改任何正式源码。
- Evidence basis: 当前真实代码 > 测试 > ADR-022~030 > phase8 spec > 历史 prompt。

---

## 1. Verdict

**READY WITH BLOCKERS.**

Phase 8.1–8.9 的架构、数据所有权、依赖方向、provenance、dimension truth、fail-closed
行为全部经独立验证成立；唯一 blocker 是 SVG renderer 的一个真实坐标缺陷
（`segment()` 第二端点未做 Y-flip），导致所有经它输出的 `<line>`
（center marks、centerlines、extension lines、dimension lines）第二端点坐标错误。
该缺陷是确定性的、被 golden 文件按错误字节锁定，且现有测试只断言计数/文本，
未断言逐端点几何，因此全部测试仍然 PASS。

DXF renderer 无此问题（Y-up 直通，无需 flip）。

---

## 2. Blocker 详情

**B-1（BLOCKS FREEZE）：`crates/scc-drawing/src/svg.rs` `segment()` 的 y2 未翻转。**

```rust
// svg.rs, fn segment()
out.push_str(&format!(
    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}/>\n",
    number(a.x),
    number(flip(a.y, height)),   // ✓ 翻转
    number(b.x),
    number(b.y),                 // ✗ 未翻转，应为 flip(b.y, height)
```

实测（/tmp harness，120×80 plate + corner holes，sheet H=110）：

```
<line x1="22" y1="85" x2="28" y2="25" data-drawing-id="center_mark:corner_holes_array[0]"/>
```

水平 center-mark 段的 y1=85、y2=25（应相等）；垂直段同样跨距错误；
extension line 出现 `x1≠x2 且 y1≠y2` 的斜线。影响所有经 `segment()` 输出的线：
center marks / centerlines / dimension lines / extension lines。
Arc、Circle、Rect、Polygon、Text 均正确（各自独立翻转）。

逃逸原因：

1. `golden_engineering_svg_is_byte_stable` 把含错字节锁定为 golden；
2. 各渲染测试断言 `<line` 计数、文本内容、分组结构，从不断言端点数值；
3. `svg_dxf_semantic_parity_on_one_model` 只比对计数与格式化字符串，不比对几何；
4. UI 尚未接入 drawing 输出，无人目检过 SVG。

修复要求（后续轮次）：

- `y2 = flip(b.y, height)`；
- 删除并重建 `tests/golden/engineering_plate.svg`；
- 新增端点级断言：水平段 y1==y2、mark 十字正交性在 SVG 输出层校验；
- 复跑 100 次 determinism 与 SVG/DXF parity。

---

## 3. Verified End-to-End Pipeline（真实代码链）

| 步骤                                        | crate / file                          | function / type                                                                          |
| ------------------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------- |
| EngineeringModel（唯一 engineering truth）  | scc-engineering/src/model.rs          | `EngineeringModel{part, features, parameters, feature_parameters, constraints, drawing}` |
| 唯一写入口                                  | scc-engineering/src/executor.rs       | `CommandExecutor::apply(&mut model, &EngineeringCommand)`                                |
| Lowering                                    | scc-engineering/src/lowering.rs       | `lower_engineering_model()` → `EngineeringLoweringResult{proposal, source_map}`          |
| Proposal                                    | scc-proposal                          | `Proposal` schema                                                                        |
| 分析                                        | scc-analysis/src/declaration_graph.rs | `analyze_detailed()`                                                                     |
| ConstraintIR→执行                           | scc-lowering / scc-execution          | lowerer → `execute`                                                                      |
| 求解                                        | scc-engineering/src/lowering.rs       | `lower_and_solve()` → `(EngineeringLoweringResult, GeometryModel)`                       |
| GeometryModel（唯一 solved geometry truth） | scc-geometry/src/model.rs             | `GeometryModel{entities: BTreeMap<EntityId, GeometryEntity>}`                            |
| Drawing 构建                                | scc-drawing/src/builder.rs            | `build_drawing(&DrawingInputs) -> Result<DrawingModel, DrawingError>`                    |
| DrawingModel（derived/read-only）           | scc-drawing/src/model.rs              | `DrawingModel{sheets, source_map}`                                                       |
| SVG 渲染                                    | scc-drawing/src/svg.rs                | `drawing_to_svg(&DrawingModel)`                                                          |
| DXF 渲染                                    | scc-drawing/src/dxf.rs                | `drawing_to_dxf(&DrawingModel)`                                                          |

注：该 pipeline 目前以 library 形式存在并被集成测试完整驱动；scc-cli 尚无
`engineering drawing` 子命令（记录为 debt，不阻断 freeze）。

## 4. Data Ownership Matrix

| Data                   | 分类          | 依据                                                                     |
| ---------------------- | ------------- | ------------------------------------------------------------------------ |
| EngineeringModel       | AUTHORITATIVE | 仅 CommandExecutor 写入；drawing 层仅持有 `&EngineeringModel` 不可变借用 |
| GeometryModel          | DERIVED       | 仅由 solve pipeline 产生                                                 |
| EngineeringSourceMap   | DERIVED       | lowering 记录；drawing 只读                                              |
| DrawingModel           | DERIVED       | `build_drawing` 纯函数输出；无命令面、无持久化                           |
| DrawingPrimitive       | DERIVED       | 来自 GeometryPrimitive 一一镜像 + role                                   |
| DrawingDimension       | DERIVED       | source 为 typed Parameter/FeatureSlot；resolved 为缓存                   |
| DrawingDimensionLayout | DERIVED       | sheet-space 表现几何，仅引用 dimension id                                |
| DrawingCenterGeometry  | DERIVED       | drawing-only，不入 primitive/source map                                  |
| DrawingAnnotation      | DERIVED       | PartTitle ← PartDefinition.name                                          |
| SVG / DXF              | OUTPUT ONLY   | 纯函数字符串输出；无回写路径                                             |

未发现任何 Drawing→Engineering 写路径（grep `&mut` 无命中于工程模型）。

## 5. Dependency Direction

`cargo tree -p scc-drawing`：scc-drawing → {scc-core, scc-engineering, scc-geometry}。
`scc-engineering` 不依赖 scc-drawing（tree grep 为空）；scc-geometry/core 不感知
drawing。方向为单向 leaf，无循环。

## 6. Command Boundary

工程状态修改唯一入口 `EngineeringCommand → CommandExecutor`。scc-drawing/svg/dxf/UI
均无 `model.features/parameters/constraints/feature_parameters` 写入。UI 通过
tool calls（ADR-012/021）。PASS。

## 7. DrawingModel Purity

公开 API 中存在 `principal_sheet_mut()/principal_view_mut()`（标注 validator/test
plumbing 用）与 `DrawingSourceMap::record()`——可被调用方绕过 builder 拼装模型，
但 renderers 与 validators 会 fail-closed 拒绝残缺模型（有 tamper 测试覆盖）。
记录为 non-blocking API 风险，本轮不改。

## 8. Geometry Truth

builder 不求解约束、不重排 pattern（pattern 已展开实体）、不从 dimension 反推几何、
不重算表达式、不算拓扑。primitives 全部来自 GeometryModel；dimension value 来自
EngineeringModel；layout witness 来自 presented primitives 并与 engineering value
做 epsilon 一致性检查（mismatch → `SCC-DRW-014`，有专测
`engineering_geometry_mismatch_fails_closed`）。两条独立链验证成立。

## 9. Coordinate Space

Model(primitives) → View(scale) → Sheet(translation) 单链一次应用；
layout/center/annotation 在 sheet space；SVG `flip(y,H)=H−y` 单一公式（text 正确、
arc sweep-flag 反转正确、circle/radius 正确）；DXF Y-up 直通、$INSUNITS=4、
extents 取自 sheet.bounds。**例外即 B-1：segment() 第二端点破坏了该单链。**
无第二个 fit/margin/scale/Y-flip（sheet margin 仅在 bounds() 应用一次；
translation 由 builder 一次算出）。

## 10. Scale Audit（实测）

| Scale | model rect | sheet content                      | dim width |
| ----- | ---------- | ---------------------------------- | --------- |
| 1.0   | 120×80     | 120×80（bounds 150×110 含 margin） | "120"     |
| 0.5   | 120×80     | 60×40（bounds 90×70）              | "120"     |
| 2.0   | 120×80     | 240×160（bounds 270×190）          | "120"     |

annotation 高度、dimension offset/pitch、center mark/overhang 均 paper-mm 不随
scale 缩放（常量定义于 ADR-024/025/026/027/028，且有 scale fixture 测试）。

## 11. Provenance / Missing Provenance

DrawingPrimitiveId → EntityId → FeatureId 全程强类型 map，无 prefix/split/regex
（grep 验证）。corner/grid/circular pattern 的 N 实体 → N primitives → 同一
FeatureId 有专测。缺 provenance → `SCC-DRW-003` fail-closed
（`missing_feature_mapping_fails_closed_not_synthetic`）；Synthetic 仅作为未来
drawing-engine 几何的类型占位，builder 从不产出。

## 12. Construction / Center / Toggle

ConstructionCircle → Engineering feature → Circle primitive(role=Construction)；
非 Centerline。`ShowConstructionGeometry=false` 在 builder 过滤（renderer 不读
toggle、不做可见性判断）。CenterMark/Centerline 为 drawing-only，typed source
FeatureId+Option<EntityId>，无 fake id。两 toggle 四组合独立性有专测
（3194 行）且 renderer 无 toggle 逻辑。PASS。

## 13. Dimension Truth / Layout / Formatter

- Dimension source 强类型（Parameter 优先于 FeatureSlot，绑定不可被 literal 覆盖，
  有 binding-priority 测试）；resolved 为派生缓存并有 integrity 重解析校验。
- Pattern：每 pattern 恰 1 个 Pattern dimension（count/diameter/PCD 均来自
  engineering slots，非反推）；circular count 与呈现圆数一致性 fail-closed。
- Layout ≠ semantic：仅引用 id；linear/diameter/pattern 三种布局确定性 stacking。
- UNRESOLVED 绑定参数 → `SCC-DRW-010`（上游 SCC-ENG-027 先行拦截，双层防御）。
- formatter：唯一 `format_dimension()`，SVG/DXF 共用；canonical 形态
  （120 / Ø64 / R30 / 8×Ø10 PCD Ø126 / Grid / Corner）有专测；结果不回写模型。

## 14. Renderer Purity & Parity

`drawing_to_svg` / `drawing_to_dxf` 仅消费 DrawingModel（签名与 import 验证）；
无 fit/center/layout/solve/measurement/visibility policy。DXF 无 DIMENSION/
DIMSTYLE/anonymous block（保持 LINE/TEXT/SOLID 分解，有专测）。语义 parity 测试
覆盖计数与文本；几何 parity 因 B-1 在 SVG 侧不成立（见 blocker）。

## 15. Determinism / Serialization / Numeric Safety

- /tmp harness：同一完整输入 100 次 lower→solve→draw→render，SVG/DXF 字节全同；
  测试套件另有 build/layout/dimension/centers/DXF 各自 100-run 确定性测试。
- serde roundtrip 覆盖 model/source_map/dimension/layout/center/annotation；
  集合全 BTreeMap/BTreeSet，无 HashMap。
- NaN/Inf/负 radius/零 scale/退化段/non-finite 均在各层 fail-closed
  （DRW-002/004/007/012/015/017/018）；renderer 拒绝 non-finite。
- production panic：scc-drawing src 中 unwrap/expect 仅存在于 #[cfg(test)]
  模块（error.rs / source_map.rs 测试区）；无用户输入引发的 panic 路径。

## 16. Error Architecture

SCC-DRW-001…018 已注册于 docs/error-codes.md（198–215 行），与 error.rs 一一对应、
无重复；全部通过 StructuredError contract（专测 `every_error_code_passes_contract`
与 `error_codes_are_distinct`）。无 silent fallback。

## 17. Golden / Legacy

golden SVG/DXF 存在且锁定 sheet size/styles/dimensions/centers/title 结构 +
字节稳定。**注意：SVG golden 锁定的是 B-1 的错误字节，修 bug 时必须重建。**
Legacy GeometryModel→SVG/DXF 导出独立保留并有回归测试
（`legacy_geometry_svg_export_still_works` 等），两条导出 API 未混淆。

## 18. E2E Fixture（/tmp harness 实测）

BaseProfile + Circular HolePattern(8×Ø10 PCD100) + CenterHole Ø20 +
ConstructionCircle PCD100 + 参数传播 + PartTitle：

features=4, entities=11, primitives=11(construction 可见), dimensions=4,
layouts=4, marks=9, centerlines=18, annotations=1, svg=7862 bytes,
dxf=5933 bytes, pattern_dimensions=1, 100 runs byte-identical=true。

参数传播 120→160：geom 120→160、dim "120"→"160"、source id 不变、SVG 变化。✓
Derived plate_height/8：80→96 ⇒ Ø10→Ø12，circle r 5→6，SVG/DXF 同步。✓

## 19. Feature Coverage

| Feature              | Drawing Geometry                                         | Dimension           | Center          | SVG | DXF |
| -------------------- | -------------------------------------------------------- | ------------------- | --------------- | --- | --- |
| BaseProfile          | ✓ rect                                                   | width/height linear | n/a             | ✓   | ✓   |
| Hole                 | ✓ circle                                                 | diameter            | mark+2 lines    | ✓   | ✓   |
| HolePattern Circular | ✓ N circles                                              | 1 pattern callout   | per-element     | ✓   | ✓   |
| HolePattern Grid     | ✓ N circles                                              | 1 pattern           | per-element     | ✓   | ✓   |
| HolePattern Corner   | ✓ N circles                                              | 1 pattern           | per-element     | ✓   | ✓   |
| ConstructionCircle   | ✓ dashed role                                            | none(by design)     | none(by design) | ✓   | ✓   |
| Slot / Fillet        | NOT SUPPORTED UPSTREAM（lowering fail-closed SCC-ENG-*） | –                   | –               | –   | –   |

## 20. Product Capability

单一主视图二维工程图 SVG/DXF：**YES**。参数驱动/派生参数/孔径/pattern 标注/
construction/centerline/确定性布局/SVG/DXF：YES。Three Views / Hidden lines /
PDF / 标准纸幅+title block / GD&T：NO（Phase 9 或后续）。

## 21. Debt Classification

- BLOCKS FREEZE：B-1 SVG segment Y-flip。
- BLOCKS PHASE 9：polygon solver limitation（异形轮廓 benchmark 前置）；
  constraint SourceMap counter prediction（constraint-backed dimension 前）；
  circular 非 0 start-angle 展开（pattern consistency 前）。
- NON-BLOCKING：长标题溢出/text metrics/collision avoidance；ISO/GB/ASME 格式；
  angular/constraint/user-intent dimensions；自动 PCD construction circle；
  纸幅/title block；native DXF DIMENSION（有意推迟）；two-pass planner；
  CLI `engineering drawing` 子命令缺失；`*_mut()` 公开 API 面。
- POLISH：PDF、UI integration。

Polygon solver：不阻断 Phase 8；对 Phase 9 projection 本身不阻断（投影可先支持
rect/圆类轮廓），但复杂安装板正式产品能力被阻断。UI integration 建议：**Phase 9
之前先做最小接入**——本缺陷正是"无人目检输出"的直接后果。PDF：非 freeze blocker。

## 22. Phase 9 Readiness

从当前 feature.rs 再验证：无 thickness/extrusion/hole depth/through-blind/axis/
visible-hidden 字段。Python CadQuery FeatureGraph 仍是独立世界，不得作隐藏 truth。
**NOT READY** —— 需先做 Phase 9.1 Spatial Semantics。

推荐拆分：9.1 EngineeringSpatialSemantics → 9.2 2.5D lowering → 9.3 Orthographic
Projection Model → 9.4 Front/Top/Left → 9.5 Hidden Line → 9.6 Consistency
Validator → 9.7 Three-view Drawing Integration → 9.8 UI Three-view。
（另建议前置插入 8.9.1：SVG flip 修复 + golden 重建 + 端点级断言。）

## 23. Tests Actually Executed（本次实跑）

| Command                                                                  | Exit | 结果                                         |
| ------------------------------------------------------------------------ | ---- | -------------------------------------------- |
| `cargo fmt --all -- --check`                                             | 0    | clean                                        |
| `cargo clippy --workspace --all-targets -- -D warnings`                  | 0    | clean                                        |
| `cargo test --workspace --all-targets`                                   | 0    | **683 passed / 0 failed**（31 个测试二进制） |
| `SCC_BIN=target/release/scc npm test --prefix apps/scc-ui`               | 0    | **79 pass / 0 fail**                         |
| `cargo tree -p scc-drawing / -p scc-engineering`                         | 0    | 方向单向，无反向依赖                         |
| /tmp/p8audit harness（scale/传播/derived/100-run determinism/E2E stats） | 0    | 见 §18                                       |

注：首次 workspace test 运行曾因残留的 `target/e2e-tests` 覆盖冲突出现一次
exit 101（e2e refuse-overwrite 用例的环境残留），清理后连续两次全绿；非代码缺陷。

## 24. Files Added / Modified

- Added: `docs/specs/phase8-completion-audit.md`（本文档）
- Modified: none（正式源码零修改）
- Temp: `/tmp/p8audit/`（harness 与渲染样本）

## 25. Freeze Decision

**DO NOT FREEZE** —— 待唯一 remediation 完成：

> Remediation（唯一最小修复轮）：修复 svg.rs `segment()` y2 flip → 重建
> engineering_plate.svg golden → 新增水平/垂直段端点级断言与 SVG/DXF 几何 parity
> 抽查 → 复跑全套件。

完成并通过后 Phase 8 即可 FREEZE，随后进入 Phase 9.1。

---

## Remediation 8.9.1（追加于修复轮；不改写上方原始审计结论）

原始结论 **READY WITH BLOCKERS** 保留作为历史审计记录。以下为本轮 remediation 结果。

### Blocker

B-1：`crates/scc-drawing/src/svg.rs` `segment()` 第二端点未执行 Sheet→SVG Y-flip。

### Root Cause

```rust
// 旧实现（bug）：
y1 = flip(a.y, height)   // start 翻转 ✓
y2 = b.y                 // end 原样输出 ✗
```

`flip()` 只作用于第一端点；所有经 `segment()` 输出的 `<line>`
（CenterMark 横/竖臂、Centerline、Linear Dimension Line、Extension Line、
Diameter axis line、Pattern leader）第二端点均为 sheet-space 原始 y。
golden SVG 将错误字节锁定，且既有测试只断言计数/文本/分组，未断言端点数值。

受影响消费者确认（真实调用点）：
CenterMark、Centerline、DimensionLine（含 Diameter axis line）、
ExtensionLine、PatternCallout leader —— 全部经 `segment()`。
**DrawingPrimitive::Line 不经 `segment()`**：它走 `emit_primitive` 内的
`to_svg` 闭包（两端均正确 flip），不受影响。Point/Circle/Arc/Rectangle/Polygon
同理不回归。

### Patch

- 新增私有 helper `sheet_to_svg_point(p, height)`（唯一 ADR-029 规则
  `svg = (x, H − y)`），`segment()` 的 start 与 end 均经同一 helper，
  结构上杜绝"第二端点被遗忘"。未引入第二套 transform，无 `<g transform>`，
  未改 text orientation。
- 除 svg.rs 外零正式源码改动；DXF renderer 未动。

### Tests（新增，永久生效）

svg.rs 单元测试（合成 segment，H=100）：
horizontal (20,25)→(30,25) ⇒ (20,75)-(30,75)，断言 y1==y2；
vertical (25,20)→(25,30) ⇒ (25,80)-(25,70)，断言 x1==x2；
diagonal (10,20)→(30,40) ⇒ (10,80)-(30,60)；三例长度保持（1e-9）。
四个新测试已在修复前代码上实测 FAIL、修复后 PASS。

drawing_integration.rs 集成测试：
端点级精确坐标回归（CenterMark 十字正交 + 精确值、Centerline 长度保持、
Dimension/Extension line 轴对齐与精确值、Pattern leader 双端点）、以及
SVG/DXF 几何 parity：同一 DrawingModel 双渲染，DXF(Y-up) 线段经
`sheet_y = H − svg_y` 反翻后与 SVG 线段按无序端点集双射匹配（两端点均比较，
长度保持校验），覆盖 golden fixture 与 Circular Pattern fixture。

### Golden Diff Audit

`engineering_plate.svg` 删除后由正式 golden 测试路径重建
（sha256 `80baa7df…` → `ca05d58a…`）。diff 仅含预期的 22 条 `<line>` y2 修正
（16 center 线、1 pattern leader、3 height 维度线/ext、2 width ext、
1 width dim line）；viewBox（150×110）、Circle、Rect、arrow polygons、
全部 text、样式、id、分组顺序零变化。

### Regression Confirmation

- DXF golden byte-for-byte unchanged（sha256 `fdbe2937…` 前后一致）。
- DrawingModel 序列化语义不变（serde roundtrip 测试通过；本阶段零模型改动）。
- SVG viewBox 不变；Source/provenance 不变；无新 ErrorCode；无新 ADR。

### 新发现（报告，不在本轮 patch 范围）

`emit_arrow()`（svg.rs）发射箭头 polygon 时完全未做 Sheet→SVG 转换
（tip 与 direction 均按 sheet-space 原样输出；DXF 侧为 Y-up 直通故正确）。
即 B-1 同族的第二处转换不对称：修复 segment 后，SVG 箭头将脱离其所属
dimension line（golden 中可见，如 width dim line 在 y=12 而箭头在 y≈98）。
按任务纪律仅记录不顺手扩大 patch；建议在 UI integration（目检输出）之前
作为下一个最小 remediation 处理。

### Tests Actually Executed（修复轮实跑）

| Command                                                    | Exit | 结果                                                                 |
| ---------------------------------------------------------- | ---- | -------------------------------------------------------------------- |
| `cargo fmt --all -- --check`                               | 0    | clean                                                                |
| `cargo clippy --workspace --all-targets -- -D warnings`    | 0    | clean                                                                |
| `cargo test --workspace --all-targets`                     | 0    | **691 passed / 0 failed**（31 个测试二进制，较审计轮 +8 为本轮新增） |
| `SCC_BIN=target/release/scc npm test --prefix apps/scc-ui` | 0    | **79 pass / 0 fail**                                                 |
| SVG golden 测试内建 100-run determinism                    | pass | byte-for-byte identical                                              |
| DXF golden 测试内建 100-run determinism                    | pass | byte-for-byte identical                                              |

### Remediation 后 Freeze Gate

A–E（truth 单一性、单坐标链、无写路径）：不变，PASS（零架构改动）。
F：segment 两端点统一转换 — PASS（单元+集成+parity 三层证明）。
G：DXF Y-up unchanged — PASS（golden 字节不变）。
H：DrawingScale once — 不变 PASS。I/J/K：dimension truth / provenance /
renderer purity — 不变 PASS。
L：SVG/DXF geometry parity restored — PASS（新增双端点双射 parity 测试）。
M/N：SVG/DXF deterministic — PASS。O：full tests green — PASS（691/0 + UI 79/0）。

### Final Status & Freeze Decision

**Phase 8: PASS WITH NON-BLOCKING DEBT → FREEZE。**

遗留 debt 分类更新：

- BLOCKS PHASE 9：polygon solver limitation；constraint SourceMap counter
  prediction；circular 非 0 start-angle 展开。（不变）
- NEW（建议 UI integration 前处理）：SVG arrowhead 未做 Sheet→SVG 转换
  （见上"新发现"；DXF 正确，属 renderer-only 缺陷）。
- NON-BLOCKING / POLISH：同原审计 §21。

---

## Remediation 8.9.2（追加于第二轮修复；不改写原始结论与 8.9.1 记录）

### Blocker

B-2：`svg.rs emit_arrow()` 发射箭头 `<polygon>` 时完全未做 Sheet→SVG 转换，
箭头脱离其 dimension line（B-1 同族第二处转换不对称）。

### Root Cause

- `ArrowAnchor { point, direction }` 均为 sheet space（+Y up，dimension_layout.rs
  模块级约定）；`emit_arrow()` 在 sheet space 直接计算 p1/p2/p3 并原样写入
  `points="..."`——tip 未 flip、wings 未 flip、direction 未镜像，
  即"triangle vertices 完全在错误 coordinate system 输出"（C 类）。
- Dimension line 经 8.9.1 修复已正确翻转（如 width dim line SVG y=12），
  箭头仍在 sheet-space y≈98，脱距 86mm。
- DrawingModel/DimensionLayout 数据经 harness 核实为正确 sheet-space truth；
  DXF SOLID（Y-up 直通）与之逐顶点一致 —— 缺陷仅在 SVG renderer emission。

最小复现（修复前）：断言"SVG arrow tip == SVG dim-line endpoint"的测试实测 FAIL：
`SVG arrow tip (10.0, 98.0) does not sit on dimension-line endpoints [(10.0,12.0),(130.0,12.0)]`。

### Point vs Vector Transformation

Point：(x,y) → (x, H−y)；Vector：(dx,dy) → (dx,−dy)，禁止对 vector 用 H−dy。
最终采用 architect 方案 A：**在 Sheet Space 构造三角形三顶点，
每个顶点作为 POINT 统一经 `sheet_to_svg_point(vertex, H)`**；
direction 仅用于 sheet-space 内 base/wing 偏移，从不作为 point 转换。
无需 `sheet_to_svg_vector()`（未新增）。Y reflection 反转 winding，
SVG fill 不受影响，无补偿代码。

### Patch

`emit_arrow()` 增加 `height: f64` 参数；p1/p2/p3 计算后各经一次
`sheet_to_svg_point(_, height)`；Linear/Diameter 两处调用点传入 height。
DXF renderer、ArrowAnchor、DimensionLayout、DrawingScale、formatter、
样式、error model：零改动。renderer purity 保持（仅 &DrawingModel）。

### Tests（新增永久语义断言）

svg.rs 单元（H=100）：horizontal tip(20,25)→[(20,75),(17.5,74),(17.5,76)]；
vertical tip(25,20)→[(25,80),(24,82.5),(26,82.5)]；45°（SQRT_2 表达式构建期望值）；
tip 恒等保持；三边长保持（√7.25×2 + 2.0，winding 翻转但长度不变）。
集成测试：width/height 箭头 tip==dim-line endpoint（解析 polygon 顶点）；
inward 方向（base_mid − tip == (−dir.x·L, +dir.y·L)）；Ø64 diameter 45° 双箭头
tip 精确落在翻转后 axis endpoints（viewBox 解析 H，含 70±32/√2 交叉核对）；
scale 1/0.5/2 下 axis 长 64·scale 变化而三角边长恒 √7.25/2.0（paper-mm 不变，
无二次 scale）；SVG/DXF 全三角形 parity（SOLID 三角逐顶点 flip 后与 SVG
polygon 无序顶点集双射匹配，非仅 tip），覆盖 golden fixture 与 center_hole fixture。

### Golden Diff Audit

engineering_plate.svg 重建（sha256 `ca05d58a…` → `2f428276…`）。diff 恰好 4 行，
全部为 a_eplate.* 箭头 polygon：
height: `138,10 137,7.5 139,7.5`→`138,100 137,102.5 139,102.5`、
`138,90 139,92.5 137,92.5`→`138,20 139,17.5 137,17.5`；
width: `10,98 7.5,99 7.5,97`→`10,12 7.5,11 7.5,13`、
`130,98 132.5,97 132.5,99`→`130,12 132.5,13 132.5,11`。
reviewer 独立手工复算全部一致；viewBox/object/construction/center/dim lines/
ext lines/leader/texts/styles/ids/group order 零变化。

### Regression Confirmation

- engineering_plate.dxf byte-for-byte unchanged（sha256 `fdbe2937…`）。
- Phase 8.9.1 segment 测试全数保持 PASS；primitive/text 回归 PASS。
- viewBox 150×110 不变；dimension strings（120/80/4×Ø10…）与 PartTitle 不变。

### 新发现（记录，不阻断）

minor（non-blocking，两 renderer 共有既有模式）：`d_len <= 0.0` 不能捕获 NaN
direction（NaN 比较为 false）；layout 仅产出硬编码单位向量故当前不可达，
后续可在两侧对称补 `ensure_finite(direction)`。

### Tests Actually Executed（8.9.2 轮实跑）

| Command                                                       | Exit    | 结果                                                                 |
| ------------------------------------------------------------- | ------- | -------------------------------------------------------------------- |
| `cargo fmt --all -- --check`                                  | 0       | clean                                                                |
| `cargo clippy --workspace --all-targets -- -D warnings`       | 0       | clean                                                                |
| `cargo test --workspace --all-targets`                        | 0       | **702 passed / 0 failed**（31 个二进制；较 8.9.1 轮 +11 为本轮新增） |
| `SCC_BIN=target/release/scc npm test --prefix apps/scc-ui`    | 0       | **79 pass / 0 fail**                                                 |
| 点名单测（segment/arrow/parity/golden/determinism 共 15+ 项） | 全部 ok | —                                                                    |

### Final Freeze Gate（A–T）

A segment endpoints ✓ / B 三角顶点正确 ✓ / C tip 贴合 dim-line endpoint ✓ /
D horizontal 方向 ✓ / E vertical 方向 ✓ / F diagonal·diameter 方向 ✓ /
G Y reflection 保尺寸 ✓ / H 无二次 scale ✓ / I SVG/DXF 三角 parity ✓ /
J DXF bytes unchanged ✓ / K DrawingModel unchanged ✓ / L viewBox unchanged ✓ /
M dimension strings unchanged ✓ / N PartTitle unchanged ✓ / O center geometry
unchanged ✓ / P SVG deterministic ✓ / Q DXF deterministic ✓ / R full tests green ✓ /
S renderer purity ✓ / T 无 Phase 9 code ✓。

### Final Status & Freeze Decision

**Phase 8: PASS WITH NON-BLOCKING DEBT → FREEZE。**

完整历史：Completion Audit（READY WITH BLOCKERS）
→ 8.9.1 segment Y-flip fix → 8.9.2 arrow coordinate fix → FREEZE。

遗留 debt：

- BLOCKS PHASE 9：polygon solver limitation；constraint SourceMap counter
  prediction；circular 非 0 start-angle 展开。
- Non-blocking：NaN-direction 防护（见上）；`*_mut()` 公开 API 面；
  text metrics/collision avoidance；ISO/GB/ASME 格式；angular dimensions；
  native DXF DIMENSION（有意推迟）；CLI drawing 子命令。
- Product polish：PDF；UI integration（下一步推荐项）。

## Remediation 8.9.2 独立复核（追加于复核轮；不改写上方任何记录）

本节记录对 8.9.2 修复成果的第六方独立验证（explorer → debugger → architect →
tester → reviewer 流水线，coder 因无代码缺口未介入）。结论：**维持 FREEZE 判定**。

### 独立证据

- **explorer**：全量清点 svg.rs 坐标发射路径，确认 segment()/emit_primitive()/
  emit_arrow()/emit_text()/viewBox 全部各经一次 flip 规则；ArrowAnchor 的 point
  与 direction 均为 sheet space（dimension_layout.rs 模块约定）；layout 构造处
  linear/diameter anchor tip 与 dim-line endpoint 共享同一变量绑定（构造即重合）。
- **debugger（mutation 验证）**：临时注释 emit_arrow 中三次 `sheet_to_svg_point`
  转换后，`--no-fail-fast` 下精确复现旧 bug 签名——10 个测试 FAIL，包括
  `plate.width: tip y on dim line — left: 98.0, right: 12.0`、golden diff 出现
  `points="10,98 …"` 对 `y1="12"` 的脱距。恢复后 sha256 与修复版逐字节一致
  （`e90152e4…`），DXF golden `fdbe2937…` 不变，全部测试复绿。证明箭头语义
  测试为真实回归防线，非空洞断言。
- **architect**：代数证明"sheet space 构图 + 三顶点按 point 转换"与"tip 按 point
  - direction 按 (dx,−dy) 镜像后 SVG 空间重构"数学等价，且后者存在垂直方向
    手性陷阱（反射反转 handedness）；维持现方案为最小且最安全实现。确认 winding
    反转无需补偿、无 renderer-specific state 泄漏。提出三项非阻断后续：
    P3 将 arrow direction 纳入 `DimensionLayoutGeometry::points()` 以闭合 NaN
    fail-open 缝隙；`emit_text` 内联 flip 归一到 `sheet_to_svg_point`；DXF SOLID
    补第 4 角（codes 13/23）。
- **tester（实跑）**：fmt/clippy clean；workspace 702 passed / 0 failed；点名
  箭头单测 5/5、segment 回归 5/5、箭头集成 6/6、golden + 100-run determinism
  5/5 全 ok；UI 套件 79 pass / 0 fail；DXF golden sha256 复核一致；本轮零改动
  工作树。附注：`scc-drawing` 当前在 workspace 内无下游依赖（scc-cli 走
  scc-engineering 渲染路径），release 二进制新鲜度经 cargo 双跑确认为正确行为。
- **reviewer（独立直读代码 + 复跑测试）**：PASS WITH NOTES。确认单一 flip 规则、
  height 双调用点贯通、renderer purity、无 Phase 9 越界、golden 四个 tip 精确
  落点。四项发现均不阻断：F1 NaN-direction guard 缺口（今日不可达，建议一行
  `is_finite()` 加固）；F2 DXF SOLID 第 4 角缺省（严格读取器可能渲染伪面片，
  一行补齐）；F3 `emit_text` 内联 flip（风格统一项）；F4 箭头体位于测量 span
  外侧为既有语义（SVG/DXF 一致且有测试固定），非本轮缺陷。

### 复核后 Final Freeze Gate（A–T）

全部 20 项维持 ✓，判定不变：**Phase 8 = PASS WITH NON-BLOCKING DEBT → FREEZE**。

完整历史：Completion Audit（READY WITH BLOCKERS）→ 8.9.1 segment Y-flip fix →
8.9.2 arrow coordinate fix → 8.9.2 独立复核 → FREEZE（维持）。

## Phase 8.10（追加；不改写上方任何记录）

Phase 8.10 Minimal Drawing UI Integration 已接线：`scc-cli → scc-drawing` 唯一
新依赖边 + `scc engineering drawing` CLI JSON 端点 + `/api/engineering/drawing`
薄封装 + workbench "Engineering Drawing" 面板（Blob/objectURL、stale guard、
失败三分语义）。引擎与 golden 未改动，详见 ADR-031。视觉冒烟后补修：server CSP 的
script-src/style-src 追加 engineering.html 内联 script+style 的 sha256 哈希白名单
（内联脚本曾被静默拦截致页面不渲染、内联样式被拦截致三列 grid 失效），并由
test/csp-inline-scripts.test.mjs 防回归。另修复 Phase 7 遗留的 workbench redo
失效（history 栈哨兵语义错误，redo 恒 no-op），并补齐 createModelHistory 单元测试。

## Phase 8.10.1 — SVG Dimension Text Paint Remediation（追加；不改写上方任何记录）

**Visual discovery**：engineering drawing 面板中 dimension 文本不可读——3.5 mm
字形被叠加约 1 mm 描边渲染，视觉墨量从 0.0422 恶化至 0.1134（修复后回落
0.1134 → 0.0422，−63%）。annotations 组标题正常，仅 dimensions 组文本受影响。

**Inheritance root cause**：`svg.rs` 的 dimensions 组开标签带组级
`stroke="black"`（供 extension/dimension line 与 arrow 使用），而 `emit_text`
输出的 `<text>` 只展开 `fill`，未显式声明 stroke —— `<text>` 因此继承组级
1 mm 描边叠加在 3.5 mm 字形上。annotations 组无组级 stroke，故 part title
不受影响。

**Fix**：单点修复 —— `emit_text` 格式串在 `fill="black"` 之后、`text-anchor`
之前插入 ` stroke="none"`（与 Point/Arrow polygon 的 paint 隔离风格一致）。
不改 dxf.rs、format_dimension、坐标/字号/rotation 计算、五个 group 的组级
属性、UI/CSP；不新增错误码。

**新增测试**（scc-drawing，共 7 个）：
- 单元（src/svg.rs mod tests，直调私有 fn）：`emit_text_carries_paint_isolation`、
  `emit_text_preserves_geometry_attributes`（固定发射形状逐字节断言）、
  `emit_text_vertical_rotation_and_escape_unchanged`。
- 结构回归（tests/drawing_integration.rs，直渲 golden_plate_model 产物，
  不依赖 golden 文件）：`dimensions_group_texts_are_stroke_isolated`（逐个
  text 元素检查）、`pattern_callout_leader_vs_text_paint_split`、
  `part_title_text_has_own_fill_only`、
  `vertical_dimension_text_rotation_with_paint_isolation`。

**Golden hash**：
- SVG `tests/golden/engineering_plate.svg`：
  `2f42827606b65ee48ed41d9238c1ee768e4d50da042b0226e73bdb289c858a65` →
  `cadfcc705fe7a3c07cfaf3d6d8ef014172739d94b663927f6f06daee9b997d19`。
- DXF `tests/golden/engineering_plate.dxf` 不变：
  `fdbe293786547cbc11ede2c69f4beef844f0dbe3c47c009fdad2ce57c4be320b`。
- diff 审计：恰好 4 行变化（原 31/37/43/46 行各插入一处 ` stroke="none"`），
  其余 44 行字节相同；scc-drawing 全量 150 passed（20 lib + 130 integration），
  scc-cli 全量绿（36 + 其余目标），fmt/clippy clean。
