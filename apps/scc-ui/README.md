# AICAD Engineering Workbench

`apps/scc-ui` 只提供一个用户界面：简体中文 Engineering Workbench。打开 `/` 会规范化跳转到 `/engineering.html`；旧 2D SCC 工作台已移除。英文可在页面右上角切换。`/api/compile`、`/api/execute`、`/api/solve`、`/api/export`、`/api/intent` 和兼容 CAD API 仍保留，供现有 API/CLI 客户端使用，但不再构成独立产品界面。

## 工程流程

自然语言新建走 `/api/engineering/intent` → AICAD DSL → 正式 EngineeringCommand/EngineeringModel；修改走 Tool Calling → 预览 → 确认。服务端使用 Rust CLI 验证，再由 Spatial、Projection、Visibility 和 Drawing 层生成 Front/Top/Left。图纸 SVG/DXF 均来自同一个 `DrawingModel`，STEP 仅通过 `/api/engineering/export/step` 的验证链路交付。模型回复或 HTTP 200 本身都不代表三视图成功。

工作台可新建、打开和下载 source-only `.aicad.json` 工程项目，载入内置安装板，查看工程特征、属性、图纸和原始几何调试信息。未建立模型时，“新建”进入空白输入状态；提交自然语言后由正式新建协议构建模型。内置安装板用于离线演示和验证，不调用 AI。

## AI 模型管理

“AI 模型管理”在同一页面中保存多套命名配置，可编辑、删除、启用并分别测试连接和工程协议。预设提供 DeepSeek、Qwen、通用及本地 OpenAI-compatible 的 Endpoint 建议；Model ID 始终由用户填写。Qwen 地域和计费计划可能要求不同 Endpoint，须按所属账号配置。连接测试只验证短请求到模型 API；工程协议测试发送固定的简单安装板需求，并经过正式 DSL、工程模型和三视图验证，最多两次模型调用。两种测试都由用户主动触发，可能产生模型用量。

配置和 API Key 只存放在本地服务进程内存中。浏览器、项目文件、图纸、API 响应和日志不会保存或回传完整 Key。编辑时 Key 留空会保留本配置原有 Key；各配置的 Key 不共用。重启服务后运行时配置消失，须重新输入。若通过受控进程环境提供初始 `SCC_INTENT_*` 配置，启动时会映射为一套当前配置；项目不实现跨重启密钥持久化。已开始的请求持有其启动时 Provider 快照；切换不会修改 EngineeringModel，也不会自动调用新模型。

兼容的单配置 `GET/PATCH /api/intent-config` 和旧 Intent API 仍可供已有客户端调用。工程工作台只使用 Engineering Intent 协议，不把旧 Proposal 当作 EngineeringModel。正在运行的旧版本服务会在工作台显示兼容配置表单，以便在安全切换前保留其进程内密钥。

## 运行

从仓库根目录执行：

```bash
cargo build --release -p scc-cli
SCC_BIN="$PWD/target/release/scc" npm start --prefix apps/scc-ui
```

默认只监听 `127.0.0.1:4173`。Docker 参考运行时通过 `docker compose up --build ui` 启动，包含正式 STEP 所需 CadQuery/OCP；容器为只读根文件系统，没有密钥持久卷。**重新构建或替换正在运行的容器会丢失通过页面输入的内存配置**，请先在新实例中重新输入密钥，再安排切换。

Windows PowerShell：

```powershell
cargo build --release -p scc-cli
$env:SCC_BIN = Join-Path $PWD "target\release\scc.exe"
npm start --prefix apps/scc-ui
```

## 验证

```bash
cargo test --workspace --all-targets
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
SCC_BIN="$PWD/target/release/scc" npm test --prefix apps/scc-ui
```

浏览器验收应针对新启动的服务实例运行。单元/API 测试不替代真实浏览器操作，模拟 Provider 通过也不代表当前外部模型连接或工程协议已通过。

## 功能边界

工程工作台支持有界的 EngineeringModel v2 历史：Sketch、Extrude、Hole、受限 Pattern、常半径 Fillet、等距 Chamfer、有界 Revolve、固定副法线（fixed-binormal）平面 Sweep，以及平行共向、直纹对齐（ruled / aligned）的 Loft。这些特征经 CadQuery/OCP 重建后进入 Front/Top/Left、受支持的剖视图和 SVG/DXF。正式 STEP 走 EngineeringModel 权威路径（`/api/engineering/export/step` 或 `scc engineering export-step`），须经过重导入与几何等价校验；兼容 `/api/cad/build` 不能交付 STEP。变半径或面圆角、Distance-Angle Chamfer、变截面/Frenet/封闭 Sweep、光滑或带内环或非平行 Loft、装配、多实体、通用 NURBS 和完整 GD&T/PMI 仍未支持。G7C 生成面选择器已在源码和门控集成测试中出现。本准备标签启用门控后两条测试都失败，不能视为已验收。本项目不是通用复杂零件生成器。缺少必要输入或不支持的操作必须按原始错误代码失败。详见 [ADR-067](../../docs/adr/adr-067-semantic-topology-fillet-chamfer.md)、[ADR-068](../../docs/adr/adr-068-general-datum-axis-revolve.md)、[ADR-069](../../docs/adr/adr-069-project-v2-formal-manufacturing-step.md)、[ADR-075](../../docs/adr/adr-075-general-parametric-fixed-section-sweep.md) 和 [ADR-076](../../docs/adr/adr-076-general-parametric-loft.md)。
