# AICAD Engineering Workbench 单入口与模型配置审计

日期：2026-09-24。基线：`codex/aicad-pre-rollback-recovery`，`53e41bf76a739181f7e2c16840c045b83bc43234`。范围仅为 SCC UI 产品入口、其浏览器资源、共享 Provider 服务及三视图 DXF 交付；保留工程内核、CLI、兼容 API 与正式 STEP 链路。

## 依赖分类

| 分类 | 已核查的组件 | 处理 |
| --- | --- | --- |
| `ENGINEERING_REQUIRED` | `public/engineering.html`，`public/js/engineering-workbench.js`，`engineering-project.js`，Engineering Intent/DSL/Tool Calling，EngineeringCommand/EngineeringModel，Geometry/Spatial/Projection/Visibility/DrawingModel，Rust `scc` CLI，`/api/engineering/*`，正式 STEP 路径 | 保留；工程页拆出 `engineering-page.js`、`engineering-i18n.js`、`engineering-model-manager.js` 和 `engineering.css`；三视图绘图响应增加同一 DrawingModel 的 DXF。 |
| `SHARED_INFRASTRUCTURE` | `server.mjs`，`lib/intent-service.mjs`，`lib/provider-openai-compatible.mjs`，`public/js/api.js`，Provider Settings `/api/intent-config`，`/api/intent`，`/api/cad/intent`，M1/M2/M3 `/api/compile`、`execute`、`solve`、`export`，CAD worker 与 API/CLI | 保留；命名配置在现有 `createIntentService` 内管理，Engineering Intent 仍从同一服务获取 Provider 快照。兼容 API 不作为独立 UI。 |
| `LEGACY_UI_ONLY` | 旧 `index.html` 的 2D SCC 页面和浏览器入口；`public/js/{app,cad-viewer,i18n,persistence,renderers,store}.js`；`public/css/{components,layout,responsive,tokens}.css`；只测试旧浏览器 Store/i18n 的两个测试 | 浏览器页面替换为仅跳转 Engineering Workbench 的兼容页；上述独占脚本、样式和测试经静态引用检查后删除。 |
| `UNKNOWN_REQUIRES_VERIFICATION` | `docs/plans/` 等历史设计/诊断文档中对旧 UI 文件的引用；`docs/testing/` 既有历史验收证据；正在运行的 4173 容器中的内存配置 | 历史文档和证据原样保留；不据这些陈旧引用推断运行依赖。4173 旧进程的密钥不可由新进程安全读取，因此不重启/替换该容器。 |

静态引用检查覆盖 `public/`、服务器、测试与全仓文本；Engineering Workbench 只引用 `/js/api.js`、`engineering-workbench.js`、`engineering-project.js` 及新的同页资源，没有引用被删除的旧浏览器文件。旧 3D 查看器虽然曾挂在 SCC 主页，现有 Engineering Workbench 的正式三视图与 STEP 导出不依赖它。`public/index.html` 保留为旧镜像/静态服务的轻量跳转兜底；新服务器在 `/`、`/index.html` 直接返回 302 到 `/engineering.html`。两个 URL 共用同一页面与进程内状态。

## 旧 Provider 与密钥生命周期

原实现只有 `createIntentService` 的单份内存 `config`：环境变量 `SCC_INTENT_*` 可提供初值，`PATCH /api/intent-config` 可运行时更新；旧 SCC 首页与 Engineering 页使用同一个 `/api/intent-config`，`/api/intent` 和 `/api/engineering/intent` 均取同一服务的配置，但分别执行 Proposal 协议与 Engineering DSL/Tool 协议。无持久化数据库或安全密钥库；Docker 重启会丢失运行时设置，只有显式环境变量初值会重新注入。旧设置接口只回传 `hasApiKey`，不回传密钥。

此改动在同一 `createIntentService` 中加入内存命名配置，启动时从原单份配置导入现有 Provider；每套配置独立保存密钥、连接状态与工程协议状态。激活设置该服务的活动配置；后续 Engineering Intent 从活动配置建立 Provider 对象，已开始的请求保留其 Provider 快照。旧 `/api/intent-config` 与 `/api/intent` 继续兼容同一活动配置，不建立第二套 Provider 存储。启用不完整云端配置会被拒绝；删除当前配置不会自动改用其他收费模型。任何 API 响应只含 `hasApiKey` 与 `ephemeralSecret`，浏览器只持久化界面语言，不存密钥。

本机 `127.0.0.1:4173` 当前由无源码挂载的旧 Docker 镜像运行，公开状态显示 DeepSeek 已配置、来源 `runtime`、`hasApiKey=true`、`ephemeralSecret=true`。密钥只在旧 Node 进程内存中，没有安全读取/迁移接口。本次保持该容器运行且不读取、打印或重置密钥。新代码在独立候选服务验证；正式将 4173 切至新容器时，用户需要在新唯一 UI 重新输入密钥。候选服务的内存配置在其重启后同样丢失；此版本不宣称跨重启持久化。

## 模型管理与调用链

服务端预设只提供 DeepSeek、Qwen、通用/本地 OpenAI 兼容 Endpoint；Model ID 由用户填写。UI 支持命名配置的添加、编辑、删除确认、启用、连接测试与固定简单安装板的工程协议测试。两项测试由用户显式触发并提示可能计费；连接成功仅说明一次 API 请求成功，工程状态必须经 Engineering Intent、Rust 工程求解与 Front/Top/Left 三视图验证。中文自然语言不会作为 SCC Proposal 或最终 SVG/DXF 直接交给模型；模型只产生受约束的 Engineering DSL/Tool 命令，正式 Rust 管线产生模型、空间/投影/可见性/图纸和导出。编辑仍走变更预览及确认。

## 验证边界

候选服务的 API、CLI、Rust 与真实浏览器回归独立执行。最终 Node/API 测试为 142 项（136 通过、0 失败、6 项在宿主机跳过）；Rust 全工作区、Clippy、fmt、release CLI、Compose 均通过；容器中的正式 STEP 集成测试 6 项通过。真实 Chrome/Playwright 证明页面交互、切换、状态、三视图及 SVG/DXF 下载；使用模拟 OpenAI 兼容响应与本机 Rust CLI 证明完整 Engineering 协议。候选服务没有用户的真实 DeepSeek 密钥，因此真实 DeepSeek 网络连接和真实收费模型的工程协议测试为 `NOT_RUN`。当前复杂零件和未受支持的复杂特征仍受现有工程内核能力限制；单入口 UI 不改变其支持范围。实际候选地址为 `http://127.0.0.1:4175/`（302 到 `/engineering.html`）；原 `http://127.0.0.1:4173/` 仍为旧运行镜像，目的是保留仅在其进程内存中的 DeepSeek 密钥。
