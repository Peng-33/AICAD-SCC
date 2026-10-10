# Phase G4 验收报告：General DatumAxis + Revolve

验收日期：2026-09-26
分支：`codex/aicad-pre-rollback-recovery`
基线 HEAD：`c78471bd1c8bb44dce03c7da708ff56a2729ed61`
验收范围：EngineeringModel v2 的通用轴、回转特征及正式派生绘图流程；正式 Manufacturing/STEP authority 未改变。

## 结果摘要

**PHASE_G4 = PASS**。DatumAxis 和通用 Revolve ADD/CUT 已进入现有有序历史，由真实 CadQuery/OCP 执行；同一历史可混合 Extrude、Hole、Pattern、Fillet、Chamfer，并继续生成正式 Spatial、Projection、Visibility、Drawing、Front/Top/Left、SVG 与 DXF。没有创建 shaft、flange、groove 等零件专用 Feature。

该结论只覆盖下列可复现的通用 fixture 和有界回归，不表示 AICAD 已支持任意复杂机械工件。

## 验收矩阵

| 项目 | 状态 | 证据/说明 |
|---|---|---|
| PHASE_G4 | PASS | 下列核心门禁全部通过 |
| DATUM_AXIS | PASS | 正式 EngineeringModel v2 类型；X/Y/Z 和两点定义；非法、零长度轴 fail closed |
| AXIS_REFERENCE | PASS | Revolve 保存稳定 DatumAxis 引用；G2 Hole/Pattern 向量序列化保持兼容 |
| GENERAL_REVOLVE | PASS | 基于现有 Sketch/Profile 的通用 ADD/CUT，不含零件专用类型 |
| REVOLVE_ADD | PASS | 首个 ADD 与并入现有单实体 Body 的 ADD 均经真实 OCP 验证；分离实体拒绝 |
| REVOLVE_CUT | PASS | 真实 OCP 回转切削，检查体积变化、单实体和有效性 |
| FULL_REVOLVE | PASS | 360° 圆柱、阶梯与空心剖面；检查体积、包围盒、有效单实体和曲面证据 |
| PARTIAL_REVOLVE | PASS | 90°、180°；正反方向由实体包围盒验证 |
| HOLLOW_REVOLVE | PASS | 外环及内环剖面产生单实体空心回转体并校验解析体积 |
| STEPPED_REVOLVE | PASS | 单一通用 Sketch + Revolve 产生阶梯轴 fixture |
| REVOLVE_EXTRUDE_COMPOSITION | PASS | Revolve ADD → Extrude CUT；另有 Extrude ADD → Revolve CUT |
| REVOLVE_HOLE_COMPOSITION | PASS | Revolve Body 上执行轴向及径向 Hole CUT |
| REVOLVE_PATTERN_COMPOSITION | PASS | Revolve Body 上执行 CircularPattern；同一 fixture 同时经过其他加工特征 |
| REVOLVE_FILLET_COMPOSITION | PASS | Revolve 生成圆边由语义 EdgeReference 重新解析后执行真实 Fillet |
| REVOLVE_CHAMFER_COMPOSITION | PASS | Revolve 生成圆边由语义 EdgeReference 重新解析后执行真实 Chamfer |
| SEMANTIC_SURFACE_PROVENANCE | PASS | OCP 回归覆盖 Cylinder、Cone、Plane 曲面证据；无内核序号作为模型身份 |
| SEMANTIC_EDGE_REFERENCE | PASS | 按 Sketch entity ID / endpoint 在当前历史前缀解析；缺失或歧义 fail closed |
| DETERMINISTIC_REBUILD | PASS | 重复 rebuild 的几何证据一致；参数修改完整重建并改变实体体积 |
| SUPPRESSION | PASS | OCP 全历史抑制/恢复 Revolve；Rust 模型阻止先抑制被依赖轴 |
| MODIFICATION | PASS | Profile 和角度修改后的确定性重建通过 |
| REORDERING | PASS | 依赖允许时重排通过；将 DatumAxis 移到 Revolve 后被拒绝 |
| SPATIAL_INTEGRATION | PASS | CadQuery 返回真实 B-Rep bounds、体积、面/边来源及曲面证据 |
| PROJECTION_INTEGRATION | PASS | 正式 Projection / Visibility 路径；不是以 kernel 截图代替工程视图 |
| VISIBILITY_INTEGRATION | PASS | Front/Top/Left HLR 证据进入现有可见性与 Drawing 流程 |
| DRAWING_INTEGRATION | PASS | 正式 DrawingModel 由工程模型派生 |
| FRONT_VIEW | PASS | 正式主视投影及 SVG/DXF 数据 |
| TOP_VIEW | PASS | 正式俯视投影及 SVG/DXF 数据 |
| LEFT_VIEW | PASS | 正式左视投影及 SVG/DXF 数据 |
| SVG_EXPORT | PASS | 正式 DrawingModel 导出并由 UI/API 校验 |
| DXF_EXPORT | PASS | 独立 DXF 校验、毫米单位和实体数量校验通过 |
| GENERIC_REVOLVED_FIXTURE | PASS | 真实圆柱、阶梯体、空心回转体和带语义边处理的通用 fixture |
| REVOLVE_CUT_FIXTURE | PASS | 现有圆柱体上用通用 Revolve CUT 形成环槽，验证真实材料去除 |
| MIXED_FEATURE_FIXTURE | PASS | Revolve ADD → Extrude CUT → Hole → Pattern → Fillet → Chamfer，同一 ordered history |
| COMPLEX_PART_REGRESSION | PASS_WITH_NOTES | 既有 AICAD-COMPLEX-001 有界 G3 子集通过；这不构成通用复杂件能力声明 |
| G1B_REGRESSION | PASS | CadQuery/OCP 与 Rust → OCP 集成回归通过 |
| G2_REGRESSION | PASS | Hole、Counterbore、语义面和 Pattern 回归通过 |
| G3_REGRESSION | PASS | Fillet、Chamfer、语义边及有界复杂子集通过 |
| PROJECT_V1_REGRESSION | PASS | Project v1 与既有 EngineeringModel 读取/派生测试通过 |
| FORMAL_STEP_REGRESSION | PASS | 25 项 Rust→OCP/STEP 集成套件通过；STEP 仍走正式 Manufacturing authority |
| COUNTERSINK | DEFERRED | 本阶段未实现精确沉头 |
| EXPLICIT_PATTERN | DEFERRED | 本阶段未实现显式放置模式 |
| SWEEP | DEFERRED | 未实现 |
| LOFT | DEFERRED | 未实现 |
| PARAMETRIC_STEP | DEFERRED | 未将 runtime OCP Shape 或 G4 History 接入 STEP authority |
| BROWSER_ACCEPTANCE | PASS | Playwright 真浏览器 + 隔离假 Provider；首页与兼容 URL 均进入同一简中工作台 |
| REAL_DEEPSEEK_TEST | NOT_RUN | 未调用真实/收费 Provider；未读取或触碰 API Key |

## 浏览器证据

浏览器验收操作通过 Engineering Intent 创建 Sketch、两点 DatumAxis、360° Revolve ADD，再添加 Hole 与 CircularPattern；验证 EngineeringModel v2、Front/Top/Left DrawingModel 及 SVG/DXF 导出。首页 `/` 与兼容入口 `/engineering.html` 访问同一工作台；简体中文默认生效并保留英文切换。

截图：[`aicad-g4-engineering-workbench.png`](aicad-g4-engineering-workbench.png)（1440×1684，SHA-256 `28b0995f4f26c26c7c9a3b57efffeff690bfde5ffd2beb8e215757a2a3bdbc9f`）。假 Provider 不代表真实 DeepSeek/API 连通性。

## 回归结果

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace --all-targets` | PASS：50 个测试套件，1141 passed，0 failed，2 ignored |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `npm test --prefix apps/scc-ui` | PASS：167 total，155 passed，0 failed，12 skipped（宿主机未加载 OCP/环境门控集成；真实 OCP 与 STEP 集成另在容器通过） |
| 隔离 CadQuery/OCP suite | PASS：99/99；容器只读、无网络、临时 `/tmp` |
| Rust → CadQuery/OCP + 正式 STEP 集成 | PASS：25/25；覆盖 G1B/G2/G3/G4 与 STEP 交付 |
| Playwright browser acceptance | PASS；内部隔离网络，无主机端口映射 |
| `docker compose config --quiet` | PASS |
| `git diff --check` | PASS（最终暂存前重检） |

## 性能采样

CadQuery/OCP 直接 rebuild 使用同一隔离镜像。下表为每个 fixture 连续 5 次的中位值；`kernel` 来自真实 OCP 操作计时，`worker wall` 包含 profile 验证、rebuild、拓扑证据和 HLR 结果构造，不含 UI 绘图导出。

| Fixture | OCP kernel 中位耗时 | worker wall 中位耗时 |
|---|---:|---:|
| Simple full Revolve | 1.565 ms | 9.876 ms |
| Stepped full Revolve | 2.740 ms | 21.032 ms |
| Hollow Revolve | 4.017 ms | 27.013 ms |
| Revolve CUT（base ADD / CUT） | 1.512 / 6.708 ms | 26.081 ms |

Rust→OCP 混合 fixture 一次观测：全历史 worker rebuild 1616.308 ms；Spatial lowering 0.001 ms；Projection + Visibility 0.541 ms；Front/Top/Left 1.028 ms；Drawing integration 0.487 ms；SVG 0.411 ms；DXF 0.272 ms。最终 Rust→OCP 的 25 项集成总耗时 68.727 s，包含多个完整回归 fixture 和正式 STEP round-trip，不应视为单零件响应时间。以上是当前测试机观测值，不是性能 SLA。

## 已知限制

- 通过这些 generic fixture 不能宣称任意复杂机械件已支持。原 AICAD-COMPLEX-001 仍只覆盖既有 Extrude/Hole/Pattern/Fillet/Chamfer 子集。
- Countersink、显式放置 Pattern、Sweep、Loft、NURBS 和 parametric STEP 仍未实现。
- 正式 STEP authority 未扩展到 EngineeringModel v2 参数化历史；G4 输出的 B-Rep/Spatial/Drawing 都是运行期派生数据。
- 真实 DeepSeek 连接与协议测试未运行；需要真实 Provider 时应由用户主动在唯一工作台中验证。
