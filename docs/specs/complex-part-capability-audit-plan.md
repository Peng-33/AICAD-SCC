# AICAD-COMPLEX-001 复杂工件能力审计与实施计划

审计日期：2026-09-25
审计基线：`codex/aicad-pre-rollback-recovery` @ `f23a6a40bc473ace59abcb00b193afc7016aa591`
范围：只读源码审计、当前能力运行验证、分阶段计划。此轮未修改产品源代码、模型文件或历史验收资料。

## 1. 结论

AICAD 当前正式链路是一套严格校验的 2.5D EngineeringModel：一个平面基体沿 +Z 拉伸，附加 +Z 圆孔/孔阵列和贯穿长圆槽，再由该 SpatialModel 派生 Front/Top/Left、Visibility、DrawingModel、SVG/DXF 和经 OCP 重新导入验证的 STEP。该链路适合当前已列明的板件能力，但不能精确表达本报告的两个局部增材凸台、多个不同高度区间、横向 X 轴钻孔、矩形盲腔、局部沉孔及指定 3D 边处理。

基础板、中心孔、四个底板长圆槽的正式 DSL、三视图、SVG/DXF 已实际运行通过。当前 Rust CLI 经本机已有的 CadQuery 2.8.0/OCP 镜像，完成了单槽基础件的正式 STEP 输出和交付校验。额外将四槽同时送入正式 STEP 时，后端在 60 秒时限内未返回，不能记为通过。没有运行真实浏览器、真实 DeepSeek、完整复杂件 STEP 或完整复杂件生成。

六大 CAD Skills 和简体中文 Engineering Workbench 已集成，但 Skills 是需求分析、操作计划、制图、注释、导出验证和诊断层；它们不增加正式几何特征，也不能把自然语言理解成功等同于零件实现成功。

## 2. 基线、资料和安全边界

### 2.1 Git 和工作区

- 分支：`codex/aicad-pre-rollback-recovery`
- 本轮初始 HEAD：`f23a6a40bc473ace59abcb00b193afc7016aa591`，提交标题 `feat(aicad): integrate six built-in CAD skills`
- 初始工作树只有预存的未跟踪目录 `docs/testing/mmf204-real-user-acceptance/`；本轮没有读取后修改、暂存或删除该验收证据。
- 仓库与工作区规则已检查；仓库内没有更深层的 `AGENTS.md`。

### 2.2 当前架构基线

- `README.md` 将当前阶段标为 Phase 13.5 Manufacturing / STEP Domain FROZEN，Phase 14 尚未开始。正式制造域列出 Rectangle、Circle、RoundedRectangle、Polygon、Composite、Slot 和圆柱孔切除。
- `docs/specs/phase13.5-manufacturing-domain-closure-reaudit.md` 与 ADR-062/063 规定正式 STEP 唯一权威链路来自 EngineeringModel；旧 `CadDocument` / `FeatureGraph` 只保留兼容的非 STEP 编辑与预览用途，本报告没有用其能力推断正式工程支持。
- `docs/specs/cad-skills-integration-audit.md` 记录六大 Skills、单一 Workbench、Provider 管理和历史回归结果。该报告的假 Provider 浏览器验收是历史记录，不是本轮复杂件浏览器验收。
- 本轮参考 `docs/adr/adr-016-engineering-model.md`、ADR-032/033/034–038、ADR-040–043、ADR-053、ADR-061–063，以及 `docs/specs/engineering-ui-consolidation-audit.md`。

### 2.3 原始规格留存

当前完整需求来自用户附件：`<home>/.codex/attachments/0f85e1aa-0247-403d-9ec3-f6710d5d4f66/已粘贴的文本.txt`，SHA-256：

```text
bd72eb0694afabee54f4444457f659c0b837b24b426ac7fb3f05f926605565b0
```

本轮按用户限定只新增此审计文档，没有复制附件或改动 `docs/testing/`。开始实现前，先把该附件**完整原文**保存为 AICAD-COMPLEX-001 的只读测试输入，并核对上述哈希；结构化索引不能取代原文。现存 `docs/testing/mmf204-real-user-acceptance/mmf204-input.txt` 是不同工件 MMF-204，不能充当本工件输入，也不得覆盖或改写。

### 2.4 API Key、服务和模型

没有读取、打印、迁移或更改 DeepSeek API Key；没有调用真实 DeepSeek 或其他收费 Provider，也没有把完整复杂件描述发送给模型。本轮没有启动、停止或重配已存在的服务。用于 STEP 最小件验证的 Docker 容器设置为 `--network none`、只读根文件系统，只读挂载当前 Python 源码及本轮专用 `/tmp` 目录；现有服务容器未改变。

## 3. 六大 CAD Skills 与唯一工作台状态

当前 HEAD 包含六个内置 Skills：

1. `mechanical-part-analysis`：从原文提取显式尺寸，读取实际 EngineeringContext 中的派生值，区分歧义、冲突、缺失和已识别的不支持要求。
2. `engineering-model-planning`：绑定模型指纹与 Rust 工具清单，限制单次计划允许的工程命令。
3. `orthographic-drawing`：调用现有正式三视图接口。
4. `drawing-annotation`：只报告 DrawingModel 中有类型和来源的尺寸及中心几何。
5. `cad-export-validation`：分别检查 SVG 与 DXF 的结构、实体及模型指纹。
6. `engineering-diagnostics`：依据实际错误代码和模型事实生成中英双语恢复建议。

这些 Skills 复用 `EngineeringCommand`、`EngineeringModel`、现有空间/制图/导出服务；没有 `boss`、`pocket`、`counterbore`、横向孔或指定边处理的新内核能力。检测到不支持要求可以帮助解释失败，不代表系统已生成该要求。

产品入口仍为单一简体中文 Engineering Workbench，英文切换保留。工程自然语言路径仍须经过 Engineering Intent、正式 DSL/Tool Calling、Rust 命令验证和 EngineeringModel；M1/M2/M3 或兼容 FeatureGraph 不是本工件的正式来源。当前代码与既有测试证明的是工作台及协议集成，不证明外部模型能正确理解这份复杂规格。本轮没有真实浏览器验收。

## 4. 正式工程能力链和根因

### 4.1 当前模型边界

| 层 | 当前正式契约 | 与本工件的关系 |
|---|---|---|
| EngineeringCommand / EngineeringModel | `EngineeringFeature` 有 BaseProfile、Hole、HolePattern、Slot、Fillet、ConstructionCircle；命令可建件、增删特征、设参数/约束、设厚度、设孔深度和图纸属性。 | 没有局部增材棱柱、槽腔、沉孔、横向轴或稳定的边目标引用。Fillet 只有半径字段，没有目标边。 |
| DSL / Tool Registry | 有基础圆角矩形、孔/孔阵列、贯穿 Slot 和厚度/孔深度语法；标准工具清单 22 项。 | `create_base_circle` / `create_base_rounded_rectangle` 表示唯一基础轮廓，不表示附着在基体上的凸台；无 boss/pocket/counterbore/chamfer/x-axis bore 工具。Slot 可通过多个独立特征定位，未提供 Slot pattern。 |
| GeometryModel | 求解正式二维轮廓、圆孔轮廓与精确线/弧 Slot 边界。 | 能精确表示基础轮廓及所需平面边界，但它本身没有各局部特征的高度与布尔关系。 |
| SpatialModel | 一个 `SpatialBody`：基础 footprint 在 `[0,T]` 沿 +Z 拉伸；圆孔为 +Z 圆柱切除；Slot 为相同 `[0,T]` 的贯穿切除。 | 没有多个 additive prism/独立 Z 区间，也没有 X 轴切除、内腔或局部台阶。多个 BaseProfile 被明确以 `InvalidBaseReason::NotUnique` 拒绝，而不是当成凸台融合。 |
| Projection / Visibility | 从一个 2.5D body 与受支持孔/槽派生正交投影、可见/隐藏线并执行一致性校验。 | 可验证现有板件与 Slot；没有局部多高度实体输入，不能声称已经正确投影凸台、盲腔或横向孔。 |
| DrawingModel / SVG / DXF | Front/Top/Left 使用同一个 DrawingModel；独立正式 renderer 输出 SVG 与 DXF。 | 对现有 SpatialModel 可用；缺失的实体不能由 renderer 补出。 |
| SolidBuildSpec / STEP | 单一 base solid 加 `CylindricalCut` / `SlotCut` 两种操作，轴仅 `PositiveZ`；正式 worker 校验一个有效实体、STEP 重导入和几何 witness。 | 不含 additive body、任意方向切除、blind pocket、counterbore 或 edge blend。旧兼容后端没有资格替代此权威路径。 |

### 4.2 关键源码证据

- `crates/scc-engineering/src/feature.rs:23–53`：工程特征枚举；`Fillet { id, radius }` 没有边引用。
- `crates/scc-engineering/src/command.rs:28–85`：正式命令类型中没有局部增材/腔槽/倒角命令。
- `crates/scc-engineering/src/spatial.rs:81–94`：`PartSpatialDefinition` 只有厚度和孔 extent。
- `crates/scc-engineering/src/lowering.rs:273–285, 1640–1654`：Slot 正式降低；Fillet 正式降低失败（含 fail-closed 测试）。
- `crates/scc-spatial/src/lowering.rs:96–110`：SpatialLowering 要求恰好一个 BaseProfile，多项时返回 `NotUnique`；`455–505` 将 Slot 设为 body 全厚度 ThroughAll。
- `crates/scc-spatial/src/model.rs:358–425`：一个 SpatialBody、圆柱切除和 SlotCut；没有 additive solid。
- `crates/scc-manufacturing/src/model.rs:68–135, 165–172`：一个 SolidBodySpec，SolidOperation 仅圆柱/Slot 切除，SolidAxis 仅 PositiveZ。
- `apps/scc-ui/cad/aicad_cad/step_delivery.py:231–267`：正式 STEP 路径对来源实体要求有效且恰为一个 solid；重导入与几何/材料 witness 在同一正式 worker 内校验。
- `crates/scc-cli/src/engineering.rs:46–72`：CLI 从 EngineeringModel → snapshot → SpatialModel → SolidBuildSpec → 固定 `aicad_cad.step_delivery`，返回的是经过验证的 artifact。
- `apps/scc-ui/server.mjs:238–244`：正式 `/api/engineering/export/step` 接收 source-only EngineeringModel；三视图 route 在 `264–294` 走正式 CLI 并验证同一 DrawingModel 的 SVG/DXF。

## 5. 固定工件逐项正式能力矩阵

状态名遵照任务定义：`SUPPORTED_VERIFIED`、`IMPLEMENTED_NOT_VERIFIED`、`PARTIAL`、`UNSUPPORTED`、`BLOCKED`。`P/V` 指 ProjectionModel 与 VisibilityModel。`Browser` 列均为 `BLOCKED`，其含义是本轮没有实际浏览器验证，不能被 API/CLI 测试替代。

| 固定特征 | Engineering | DSL / Tools | Geometry | Spatial | P/V | DrawingModel | SVG / DXF | STEP | Browser |
|---|---|---|---|---|---|---|---|---|---|
| 180×120×24 底板，平面四角 R6 | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED（单槽最小件正式导出） | BLOCKED |
| 底板其余指定外缘 C1 | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | BLOCKED |
| 左侧局部圆柱凸台 Ø66、Z24–48、融合 | PARTIAL（圆形只能作唯一 BaseProfile） | PARTIAL（有 base-circle 工具，无 raised-boss） | PARTIAL（仅平面轮廓） | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | BLOCKED |
| 左凸台中心 Ø32，Z0–48 through | PARTIAL（孔存在，所需凸台和高度不存在） | PARTIAL | PARTIAL | PARTIAL（Hole extent 只能依 body `[0,T]`） | PARTIAL | PARTIAL | PARTIAL | PARTIAL | BLOCKED |
| 左凸台 PCD Ø52、四个 Ø5.5 孔，Z0–48 through | PARTIAL（圆孔阵列存在） | PARTIAL（圆周阵列工具存在） | PARTIAL | PARTIAL（没有 48 mm 局部实体高度） | PARTIAL | PARTIAL | PARTIAL | PARTIAL | BLOCKED |
| 左凸台顶外缘 R1 | UNSUPPORTED（Fillet 无边引用） | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | BLOCKED |
| Ø32 孔顶口 C0.5 | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | BLOCKED |
| 右侧 68×80、R3 局部矩形凸台，Z24–36、融合 | PARTIAL（RoundedRectangle 是唯一 base） | PARTIAL（有圆角矩形 base 工具，无局部 pad） | PARTIAL（仅平面轮廓） | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | BLOCKED |
| 右凸台 32×32、R2 顶面盲腔，Z28–36 | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | BLOCKED |
| 右盲腔中心 Ø10、Z0–28 与 Ø16 counterbore、Z25–28 | PARTIAL（一般 Hole 有限 extent 存在，无 counterbore/盲腔语义） | PARTIAL | PARTIAL | PARTIAL（+Z 孔只对单一 body 高度求 extent） | PARTIAL | PARTIAL | PARTIAL | PARTIAL | BLOCKED |
| 右凸台四个 Ø6.6 孔，Z0–36 through；Ø11 沉孔 Z32–36 | PARTIAL（孔/网格阵列存在，无台阶沉孔） | PARTIAL | PARTIAL | PARTIAL（只有一个 body 厚度） | PARTIAL | PARTIAL | PARTIAL | PARTIAL | BLOCKED |
| 四角各一个 X 向、14×7、R3.5 贯穿底板 Slot | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED（四个独立 Slot DSL/tool） | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED（每个 `[0,24]` ThroughAll） | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED | SUPPORTED_VERIFIED | PARTIAL（单槽 STEP 通过；四槽 STEP 实测超时） | BLOCKED |
| X=90 入口、轴向 X、Ø8 深18盲孔及 Ø12×2 counterbore | UNSUPPORTED（无轴字段） | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED（仅 +Z） | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED（SolidAxis 仅 PositiveZ） | BLOCKED |
| 底面中心 24×40、R3 盲槽，Z0–5 | UNSUPPORTED（无 pocket 特征） | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | BLOCKED |
| 两凸台与底板融合成唯一连续实体 | UNSUPPORTED（没有 additive union） | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED（只允许一个 BaseProfile） | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED | UNSUPPORTED（当前 STEP 单一现有 body，不具备 additive union） | BLOCKED |

补充区分：BaseProfile 的圆角矩形 R6 或未来右凸台的平面 R3 轮廓，与 3D 顶边/孔口的 `R1`、`C0.5` 或 `C1` 并非同一语义。通过平面 RoundedRectangle 不能宣称已有通用 Fillet/Chamfer。

## 6. 当前能力运行验证

### 6.1 Rust、Node、API/CLI

| 验证 | 本轮结果 | 证据/范围 |
|---|---|---|
| `cargo test --workspace -q` | PASS | 所有已执行用例通过，0 failed；现有 ignored 项保持 ignored（汇总输出含一个 2-ignored 测试组）。 |
| `cargo fmt --all -- --check` | PASS | 退出码 0。 |
| `cargo build --release -p scc-cli --quiet` | PASS | 用当前 HEAD 构建正式 `target/release/scc` 后执行 DSL、Drawing、STEP。 |
| `npm test --prefix apps/scc-ui` | PASS（带跳过项） | 157 tests：151 pass、0 fail、6 skipped。被跳过的是需要 `AICAD_RUST_STEP_INTEGRATION=1` 的真实 STEP Node 集成用例；不能把 skipped 算作 pass。Node/API 集成覆盖 fake Provider、Engineering Intent、三视图、SVG/DXF 等路径。 |
| 正式 STEP 单槽最小件 | PASS | 当前 release CLI + 本地已有 `aicad-scc-ui:workbench-candidate` 内的 CadQuery 2.8.0/OCP；当前仓库 `apps/scc-ui/cad` 只读挂载。`engineering export-step` 返回 `ISO-10303-21;`，50,628 bytes；CLI metadata `sha256=22d4229310b909d55cd56a572ae72b6de343489020d3d9144365657c36351eb1` 与交付字节重新计算一致。成功路径经过 worker 的单 solid、STEP reimport、几何 witness 与 artifact hash 校验。此测试只涵盖当前支持的基础轮廓、Ø32 + 一条 Slot。 |
| 同一基础板四槽 STEP 扩展验证 | PARTIAL / 60 秒超时 | 四槽模型已通过正式 DSL、Spatial、Drawing、SVG/DXF；将四槽同时导出 STEP 时返回 `SCC-CLI-013`，具体原因为 `CAD backend timed out after 60 seconds`，无 STEP artifact。记录为未通过的性能/运行验证，不能推断为已生成。 |

Node 的六个 STEP 测试由于环境门控没有运行，故正式 STEP 证据采用本轮真实 EngineeringModel → 当前 Rust CLI → 当前仓库 Python worker → OCP 的一槽最小件命令，而不是用 skipped Node 用例代替。

### 6.2 A–E 正式最小模型

临时 DSL 与输出只存放在 `/tmp`，未添加预制 SVG、CadQuery 测试脚本或仓库测试代码。正式 Engineering DSL 描述了：

- BaseProfile：中心 `(0,0)`，RoundedRectangle `180×120, R6`；厚度 `24`。
- `center_hole`：中心 `(0,0)`，Ø32，ThroughAll。
- 四个独立 Slot：中心 `(-75,45)`、`(75,45)`、`(-75,-45)`、`(75,-45)`，每个总体长度14、宽7、角度0°、ThroughAll。
- `DRAWING PROJECTION_METHOD THIRD_ANGLE`。

首次忘记设 projection method 时，正式 Drawing 按 `SCC-DRW-019` 拒绝；补齐合法 `THIRD_ANGLE` 后通过。该失败没有绕过或关闭正式校验。

当前 CLI `engineering drawing-three-view` 的实际结果：6 个模型特征（1 BaseProfile + 1 Hole + 4 Slots）；Front/Left/Top 三个视图共 69 个投影图元，四个 Slot callout；SVG 15,302 个字符，DXF 7,952 个字符。SVG 含且仅含各一个 `view_front` / `view_top` / `view_left` group；DXF 实际含 AC1027 header、ENTITIES section 和 EOF。两种导出来自同一个正式 DrawingModel。Slot 的 Z 区间是从底板厚度派生出的 `[0,24]`，不是任意 per-feature Z 参数。

该测试通过正式命令验证当前平面槽特征和制图链，但没有凸台、局部 pocket、指定边处理或完整复杂工件；不能将它描述为 AICAD-COMPLEX-001 完整验收。

### 6.3 未运行的验证

- `REAL_DEEPSEEK_TEST = NOT_RUN`：无真实 Provider 请求，无 Key 操作。
- `FULL_COMPLEX_PART_BROWSER_TEST = NOT_RUN`：本轮没有打开浏览器或启动新服务做完整工件操作。
- `FULL_COMPLEX_PART_STEP = NOT_RUN`：固定工件的必要特征尚不能形成合法完整 EngineeringModel。基础件 STEP 成功不等于完整件成功。
- 没有把任何 fake Provider、旧 FeatureGraph、预制图纸或 HTTP 200 结果标记为复杂件生成通过。

## 7. 静态几何关系及工程问题

以下结果来自规格坐标、基本平面距离公式与包络关系；除注明外不代表布尔实体已运行、空间干涉已验证或可制造性已批准。

| 关系 | 静态计算 | 验证级别/解释 |
|---|---|---|
| 左圆凸台平面范围 | 圆心 `(-35,0)`、R33，范围 X `[-68,-2]`、Y `[-33,33]`。 | 平面解析范围；实体高度 `[24,48]` 未建模。 |
| 右矩形凸台范围 | X `[14,82]`、Y `[-40,40]`，Z `[24,36]`。左凸台最右 X=-2 与其最左 X=14 之间为 16 mm 平面间隔。 | 两凸台不直接平面重叠；各自底面位于 Z=24，与底板顶面共面接触，最终必须做 union 并验证唯一 solid，不能用未融合零件替代。 |
| 左 PCD 孔位置 | PCD 半径26，坐标约 `(-53.385,±18.385)` 与 `(-16.615,±18.385)`。中心孔半径16、安装孔半径2.75，名义环带余量 `26−16−2.75=7.25 mm`；孔外缘到 Ø66 凸台边缘余量 `33−26−2.75=4.25 mm`。 | 同轴平面圆周距离；尚未验证 48 mm 高的真实实体。 |
| 右凸台孔壁 | 每个孔中心距矩形外包络最近侧边10 mm；Ø11 沉孔半径5.5，名义边缘余量4.5 mm。 | 孔位置、直径为静态值；沉孔实体没有实现。 |
| 右孔系至中央 R2 盲腔 | 盲腔包围矩形 X `[32,64]`、Y `[-16,16]`。最近孔中心到包围矩形角 `sqrt(8²+14²)=16.125 mm`；减去 Ø11 沉孔半径5.5后，至该矩形包络至少约 `10.625 mm`。R2 圆角切除区位于矩形内，因此该值是保守平面下界。 | 非 B-rep 间距；实际圆弧边界与沉孔切削体未作内核相交测试。 |
| 右侧两个 Slot 至右凸台 | 正 X 槽的平面 Y 范围 `[41.5,48.5]`（负 X 同理）；矩形凸台上边 Y=40，最近间距1.5 mm。槽 X 范围 `[68,82]`，与直边区间相交，故在规格平面几何中该 1.5 mm 间距确实可达到。 | 局部窄 web / 制造风险；切槽止于底板 Z=24，凸台从 Z=24 起且其平面边界与槽相距1.5。最终实体与过程工艺仍须验证。 |
| 槽至底板外缘 | X 向最小名义余量 `90−82=8 mm`；Y 向最小余量 `60−48.5=11.5 mm`。 | 平面边界计算；当前完整四槽底板 DSL/Drawing 已运行。 |
| 底面 R3 盲槽与左 PCD 孔 | 底槽包围范围 X `[-12,12]`、Y `[-20,20]`。最靠右的左阵列孔中心 X约=-16.615，中心至包围矩形至少4.615 mm；减 Ø5.5 半径后，切除包络间的保守平面间隙至少约1.865 mm。两者 Z 切除范围都覆盖 `[0,5]`，需检查。 | 这是保守包围矩形下界，不是最终圆弧/圆柱 B-rep 干涉结论；约1.9 mm 是应进入 Phase B 真实切削体验证的窄余量。 |
| 横向盲孔与相对端面 | X 从90到72，长度18 mm；盲孔末端距相对外端 X=-90 为162 mm，名义上不会贯穿相对端面。Ø12入口深度2 mm按 X 方向量取时为 X `[88,90]`。 | 规格区间的静态推导；轴向 X 语义未实现。 |
| 横向孔与右凸台/其他孔槽 | 主钻 Ø8 在 Z `[12,20]`，入口 Ø12 在 Z `[10,22]`；右凸台底面为 Z=24，因此重叠 XY 下仍有至少2 mm 的 Z 包络距离。X 终点72距右盲腔包围区最右端64为8 mm；同 X 处的右竖孔在 `|Y|=30`，与侧孔中心 `Y=0` 相离。 | 静态包络关系；端部轴肩、孔径、面接触及真实 B-rep 未运行。 |
| 右中心 Ø10/Ø16 与方腔 | 按“盲槽底部中心孔”的上下文解释，三者同轴于右凸台中心 `(48,0)`：方腔地板Z=28，Ø10到Z=28，Ø16区间Z25–28。Ø16半径8 小于腔体半宽16，Z=28 与方腔相接。 | 这是清晰的预期嵌套切削；同轴位置建议实施前让用户确认。 |

### 7.1 实施前需确认的设计条件

1. 底板 `C1` 的准确边集合：是外轮廓顶、底两圈所有锐边，还是只指定一面/部分边？是否按45°等距倒角解释？凸台接合邻边是否排除？
2. `C0.5` 的倒角规格：是否为45°等距；只对 Ø32 上口（Z=48）加工吗？
3. 左凸台顶外缘 `R1` 是整圈顶面与外圆侧壁交线吗？是否排除与孔口相交边？
4. 右中心 Ø10/Ø16 是否明确同轴于 `(48,0)`，如上下文所示？
5. PCD 的首孔45°以 +X 为零方向并逆时针计角是否正确？
6. 是否接受四角槽与右凸台之间1.5 mm 的最小平面余量，以及底槽与左安装孔之间约1.9 mm 的保守切削间隙？是否有最小壁厚/最小加工余量规范？
7. 正式图纸采用第一角法还是第三角法？本轮测试使用第三角法仅为合法投影参数，并非对最终图纸标准的确认。
8. STEP 当前只能承诺名义几何；需求没有材料、密度、几何公差、默认公差或表面处理。若要这些制造属性，需要另外给出，不应从尺寸自动推定。

## 8. 依赖驱动的实施路线与 PASS/FAIL 门禁

所有阶段继续使用唯一 EngineeringModel 为可编辑真值；Geometry/Spatial/Projection/Visibility/Drawing/SolidBuildSpec 都是有来源映射的派生物。不得从 AI 生成的 SVG/DXF 或兼容 FeatureGraph 补写工程模型。每阶段同时跑合法、非法、重复 ID、几何冲突、重新打开/重复生成回归；不支持的输入须 fail closed。真实浏览器用假 Provider 验 UI 操作，不调用收费模型；需要真实 Provider 时另行授权。

### Phase A1 — 单圆柱凸台的工程语义与 2D 三视图闭环（建议首阶段）

**目标范围：**仅增加一个明确类型的 +Z 局部圆柱增材特征，先使用底板 180×120×24 R6 和圆凸台中心 `(-35,0)`、Ø66、Z24–48。只实现精确竖直棱柱与占位来源映射，不做 Hole/Counterbore/Fillet；不是通用 Sketch/Extrude/Feature History。此小阶段优先交付 AICAD 的主产品能力，即从同一个工程真值生成正确 Front/Top/Left 和 SVG/DXF。

**预计模块：**`scc-engineering` 新增强类型特征/命令/校验；DSL 与 Rust ToolRegistry 暴露同一组类型参数；Geometry lowering/source map 保持凸台轮廓来源；`scc-spatial` 新增有界 additive prism 与明确的 union-to-base 语义；Projection/Visibility/DrawingModel 增加局部高度、遮挡和同 DrawingModel 输出规则；CLI/API、诊断和唯一 Workbench 给出合法/不支持反馈。测试覆盖 Engineering/DSL/Tool、Spatial、三视图、Drawing、SVG/DXF、API 与真实浏览器假 Provider。

**PASS 门禁：**

- 法定模型中只有一个 base 和一个稳定 ID 的凸台特征；Z interval 恰为 `[24,48]`，变更可回写工程源模型，JSON 保存/重开保持来源与几何相同。
- 基于同一 SpatialModel 对 Front/Top/Left 做曲线级期望坐标、视图包络、可见/隐藏线和遮挡校验；不得只检查输出字符串存在。
- 同一个 DrawingModel 导出的 SVG 三视图分组、DXF 单位/ENTITIES/弧线数量/边界均符合解析后的模型；重复运行确定性一致。
- 非正尺寸、Z逆序、Z与底板分离、重叠/接触非法情况、重复 ID、多 BaseProfile 输入返回带代码错误；不输出不完整绘图。
- Engineering Intent fake-provider 测试能产生该正式特征对应的合法命令；不能映射的要求必须明确报告，不得提交 FeatureGraph 或预制曲线。
- 真实 Workbench 浏览器中能输入简体中文、看到同一 EngineeringModel 的三个视图、下载 SVG/DXF，切换英文后仍一致。
- **本阶段不提供凸台 STEP。**`export-step` 对含未支持 additive operation 的模型必须返回结构化错误，不能忽略凸台并输出仅含底板的 STEP。

**FAIL 条件：**模型真值被复制；只生成单张伪造二维轮廓；侧视/遮挡缺失；SVG/DXF 使用不同几何来源；或 STEP 静默丢弃凸台。失败时停在 A1，不进入多孔槽开发。

### Phase A2 — 第二类凸台与严格单实体制造输入

**目标范围：**复用 A1 的局部增材语义，实现右侧中心 `(48,0)`、68×80、R3 的圆角矩形凸台 Z24–36；增加 SolidBuildSpec 的 exact additive-prism 语义与 CadQuery union。保持原 STEP 权威路由，不建立第二条导出路径。

**预计模块：**A1 工程/空间/制图层；`scc-manufacturing` SolidBuildSpec schema、lowering 和 fail-closed validator；`apps/scc-ui/cad/aicad_cad/solid_backend.py` 的准确棱柱融合；`step_delivery.py` 的单实体及重导入 witness；现有 STEP CLI/API 集成测试。考虑 SolidBuildSpec schema 版本和老项目 JSON 兼容。

**PASS 门禁：**仅含底板与两凸台时总包络严格为180×120×48；CadQuery 初始源体和 STEP 重导入体均恰为一个有效 solid；体积与名义基准 `664954.294 mm³`（无孔、无腔、无倒角/圆角 3D 边处理）在既定容差内一致；三维包络、体积、表面积/重心 witness 与 STEP reimport 一致；两种凸台相接的共面内部面必须融合或明确失败，不能留下两个 Solid。API/CLI 返回的字节必须就是通过校验的 artifact，SVG/DXF 与 STEP 来自同一 EngineeringModel fingerprint。

**FAIL 条件：**多 Solid、boolean 未完成却返回 `ok`、体积/包络不符、重导入损失圆弧、静默退化为底板或读取到第二工程真值。

### Phase B1 — 不同高度的 +Z 孔、阵列及同轴沉孔

**范围：**左侧 Ø32 `[0,48]`、PCD Ø52 四个 Ø5.5 `[0,48]`；右侧四个 Ø6.6 `[0,36]` 加 Ø11 `[32,36]` 沉孔；右中心 Ø10 `[0,28]` 与 Ø16 `[25,28]` 同轴沉孔。建立由绝对 face/Z witness 驱动的轴向 cut interval 和 counterbore 语义，而不是把所有孔扩大为贯穿底板。

**预计模块：**EngineeringFeature/Command/Part spatial schema、DSL/Tool、Geometry source map、Spatial cut records、Projection/Visibility/Drawing typed hole callouts、SolidBuildSpec operations 与 CadQuery exact subtraction、重导入 material probes。

**PASS 门禁：**逐孔检查中心/半径/起止 Z/方向/阵列数量；孔端面与凸台顶/腔底 witness 对应；沉孔直径、阶肩 Z 与深度精确；孔之间与口袋/凸台外侧不产生非预期连通；共轴重合与合法相交按规范保留并给出体积变化；重复阵列不重复加孔；越界、负深度、无支持面、孔间非预期相交分别 fail closed。三视图中心线/隐藏线/孔标注逐项核对，STEP 重新导入后用孔内 void probes 与保留材料 probes 验证。

### Phase B2 — 正交盲腔、底面槽和横向盲孔

**范围：**右凸台顶面32×32、R2、Z28–36 盲腔；底面24×40、R3、Z0–5 开口减重槽；X=90向内的Ø8、终止X=72横向盲孔及Ø12入口counterbore，明确轴向端面与入口深度。

**预计模块：**Engineering typed closed-profile pocket 和轴向定义、DSL/Tools、Spatial 受约束区域切除、Projection/Visibility 对开口边和隐藏底边处理、Drawing、SolidBuildSpec +Z/-Z/+X cut axes 与准确 Python backend operations。

**PASS 门禁：**二维曲线按闭合直线/圆弧核实（R2/R3不能以矩形替代）；顶腔底面 Z=28、侧壁竖直且止于凸台顶；底槽真实由 Z=0 开口、顶壁停在Z=5；X向钻孔末端在X=72，入口counterbore从X=90向内2 mm，−X侧保留整面；逐对验证切除工具/最终实体的相交关系，包括本报告约1.9 mm的底槽—左孔余量和1.5 mm槽—凸台间隔；SVG/DXF 隐藏边不得被当作实体边；STEP 体积/边界/材料探针全部匹配。

**FAIL 条件：**孔槽意外贯通、X孔沿Z生成、底槽穿顶、圆角轮廓退化为 bbox、切除后实体分裂、或生成器修复非法几何而不报错。

### Phase C — 语义化局部 R/C 边处理

**范围：**在用户明确边集合后，实现凸台顶外圆R1、Ø32指定孔口C0.5、底板指定锐边C1。基体平面角R6/R3与 pocket 轮廓内角R2/R3 沿用精确 profile 圆弧，不应再套全局 edge blend。

**预计模块：**Engineering edge role/edge target selector、稳定源特征关联、命令/DSL/Tool 校验、Spatial/制造操作顺序、CadQuery fillet/chamfer、Projection/Drawing 相切线与孔口表示、项目 JSON 兼容迁移。

**PASS 门禁：**selector 指向可解释的命名边角色（例如“left_boss/top_outer_loop”或“center_hole/top_rim”），不依赖可变 B-rep edge ordinal；期望圆角/倒角半径、切点和被影响面经解析几何或内核测量核验；指定相邻孔/槽交汇时有明确失败/允许规则；缺失/多义引用失败闭锁；radius 超限、自交、重复执行与拓扑变化用 adversarial tests 覆盖。Drawing 和 STEP 均反映同一个修边模型。

### Phase D — AICAD-COMPLEX-001 全量冻结验收

保存并 hash-check 完整原文后，仅通过真实正式 Engineering Intent/命令链创建单一模型。验证完整 source → Geometry/Spatial → Projection/Visibility/DrawingModel → Front/Top/Left → SVG/DXF；同一源模型的正式 STEP 做单实体、重新导入、包络/体积/边界及切削材料 witness。记录模型 feature count、阵列坐标、各 Z interval、尺寸来源、artifact SHA-256。开展真实浏览器操作验收，但用授权的 mock Provider；真实 DeepSeek 仍单独显式授权，绝不默认调用。复核所有旧 DSL/项目/API/CLI、旧 STEP golden 和导出权威路径回归后，才可将固定件能力标记 COMPLETE。

## 9. 首个实施闭环及停止条件

建议用户首先只批准 **Phase A1 — 单圆柱凸台的工程语义与 2D 三视图闭环**。它以单个新增 prismatic semantic 验证多高度投影和遮挡，满足二维工程制图的优先目标；该阶段明确不交付凸台 STEP，且对含凸台 STEP 请求失败闭锁。A1 所有 PASS 门禁通过后停止并提交结果，再由用户决定是否继续 Phase A2 的 exact CadQuery union 与 STEP 闭环。不得在 A1 中顺带实现所有孔槽或边处理。

## 10. 本轮结果状态

```text
COMPLEX_PART_AUDIT = PARTIAL
FORMAL_FEATURE_MATRIX = COMPLETE
GEOMETRY_RELATION_CHECK = PARTIAL
CURRENT_CAPABILITY_TESTS = PARTIAL
IMPLEMENTATION_PLAN = READY
FIRST_IMPLEMENTATION_PHASE = Phase A1 — 单圆柱凸台的工程语义与 2D 三视图闭环
REAL_DEEPSEEK_TEST = NOT_RUN
FULL_COMPLEX_PART_BROWSER_TEST = NOT_RUN
FULL_COMPLEX_PART_STEP = NOT_RUN
```

整体审计之所以为 `PARTIAL`：源码支持范围已逐层确认，正式简单件有运行证据，但完整复杂件尚不可建模，静态几何关系未用最终 B-rep 验证，四槽 STEP 额外测试超时，真实浏览器/DeepSeek/完整件 STEP 未运行。矩阵为 `COMPLETE` 表示所有指定特征和正式阶段都给出证据状态，不表示这些特征全部受支持。

本地审计文档提交的最终 HEAD、提交后的工作树状态及 `PUSHED` 状态在本任务最终交付摘要中记录；本报告中的审计基线始终指本轮开始时的 HEAD。
