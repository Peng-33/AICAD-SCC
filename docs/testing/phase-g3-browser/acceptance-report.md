# Phase G3 验收报告

## 结果

`PHASE_G3=PASS`。EngineeringModel v2 保存稳定语义边引用与 Fillet/Chamfer 参数；CadQuery/OCP 内核对象只在单次重建中派生。缺失、歧义、过期引用失败关闭，编辑/抑制后由完整特征历史重建。

## 验收矩阵

| 项目 | 状态 | 证据/范围 |
|---|---|---|
| Semantic Face / Edge Reference | PASS | PlanarFaceReference 保留；Profile 顶/底边、Hole 入口/出口、Pocket 入口、语义面交边及凸/凹边 selector 有 OCP 测试。EngineeringModel 与 Rust wire 不接受内核索引。 |
| 拓扑稳定与歧义失败关闭 | PASS | 无关下游 CUT 与上游尺寸编辑后按 SketchEntityId 重解析；不存在、歧义、过期引用不做最近边重绑定。 |
| General / multi-edge Fillet | PASS | 真正的 `Shape.fillet` 常半径特征；泛型工件多边 R3、修改 R2→R3、抑制/恢复、F10 下游 CUT 与确定性重建通过。 |
| General / hole-mouth Chamfer | PASS | 真正的 `Shape.chamfer` 等距特征；孔口 C0.8/C0.5、指定外边 C1、单边/多边、修改/抑制与过大参数失败通过。 |
| Feature ordering / dependencies | PASS | Fillet/Chamfer 有序 lower、替换、抑制、修改和非法重排依赖检查通过。 |
| Spatial → Projection → Visibility → Drawing | PASS | 处理后的真实 B-Rep 进入正式管线；Front / Top / Left、SVG 与 DXF 均通过。 |
| Generic G3 fixture | PASS | 120×90×20 底板、偏置矩形凸台 ADD15、Ø16 贯穿孔、40×24 深6口袋、凸台 R3 与孔口 C0.8。 |
| AICAD-COMPLEX-001 子集 | PASS_WITH_NOTES | 仅验收圆柱凸台 R1、中心孔 C0.5 和明确指定底板边 C1；不代表整个复杂零件已支持。 |
| G1B / G2 / Project v1 / 正式 STEP | PASS | Rust 全 workspace 与隔离集成回归通过；Parametric STEP 仍不进入正式 STEP 路径。 |
| Browser acceptance | PASS | Playwright + 隔离 fake Provider；首页与 `/engineering.html` 共用 Engineering Workbench，Intent、Semantic References、三视图及 SVG/DXF 下载通过。 |
| Real DeepSeek | NOT_RUN | 未发起收费或真实 Provider 请求。 |

## 回归结果

- `cargo fmt --all -- --check`：PASS。
- `cargo check --workspace --all-targets`：PASS。
- `cargo test --workspace --all-targets`：PASS。
- `cargo clippy --workspace --all-targets -- -D warnings`：PASS。
- `npm test --prefix apps/scc-ui`：155 PASS、0 FAIL、11 SKIP；运行时门控的 Rust/CadQuery 与 STEP 集成另在隔离容器执行。
- 隔离 CadQuery/OCP：85/85 PASS。
- 隔离 Rust→CadQuery/OCP 与 STEP 集成：24/24 PASS。
- Playwright 浏览器：PASS，未使用真实 Provider。
- `docker compose config --quiet`、`git diff --check`：PASS。

## 实测性能

Docker 隔离环境单次记录，单位 ms。耗时会随主机负载变化。`combined_fixture_ms` 是 Rust 到 worker 的完整调用时间，包含 Python 启动、重建和空间/HLR 证据；`base_rebuild_ms` 是处理前 ADD/CUT 历史耗时；Fillet/Chamfer 值包含语义解析、内核处理、单体校验及来源证据更新，故与 `topology_resolution_ms` 有重叠。

| 计时项 | Generic G3 fixture | Complex G3 subset |
|---|---:|---:|
| topology resolution | 15.569 | 9.926 |
| base/history rebuild | 52.477 | 54.499 |
| Fillet rebuild | 29.725 | 55.216 |
| Chamfer rebuild | 13.670 | 157.080 |
| combined fixture worker call | 2766.499 | 3369.878 |
| Front/Top/Left generation | 0.335 | 0.329 |
| SVG export | 0.175 | 0.177 |
| DXF export | 0.122 | 0.132 |

## 投影精度

OCP B-Rep 中的解析曲线和面保持真实工程几何。Front/Top/Left 的 HLR 圆边按轴向与视线关系处理：视线沿圆轴时保留解析圆/弧；侧视圆边恢复为带 `derived_approximation` 的线段，容差 1e-5 mm；斜视圆曲线输出有界折线，容差 1e-4 mm。Rust 响应校验要求派生标记与容差成对存在。未修改 EngineeringModel 尺寸或内核 Shape。

## 保留的限制

- Countersink 与普通 Chamfer 仍是不同语义；精确 Countersink：DEFERRED。
- Parametric STEP：DEFERRED；正式 STEP 继续使用既有权威路径。
- Revolve、Sweep、Loft、NURBS 与多实体：DEFERRED。
- 凹边 Chamfer 已测；凹边 Fillet、可变半径/面 Fillet、Distance-Angle Chamfer 未纳入本阶段验收。
- AICAD-COMPLEX-001 只通过上述 G3 子集；不得据此宣称任意复杂机械零件已支持。

## 浏览器证据

真实浏览器截图：[`aicad-g3-engineering-workbench.png`](aicad-g3-engineering-workbench.png)。浏览器 acceptance 使用假 Provider，不包含密钥或真实 API 调用。
