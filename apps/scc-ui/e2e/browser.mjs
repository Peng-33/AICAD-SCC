import assert from "node:assert/strict";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://ui:4173";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true });
const page = await context.newPage();
const pageErrors = [];
page.on("pageerror", (error) => pageErrors.push(error.message));
page.on("console", (message) => { if (message.type() === "error") console.error(`BROWSER_CONSOLE_ERROR=${message.text()}`); });
page.on("requestfailed", (request) => console.error(`BROWSER_REQUEST_FAILED=${request.method()} ${request.url()} ${request.failure()?.errorText ?? "unknown"}`));
page.on("response", (response) => { if (response.status() >= 400) console.error(`BROWSER_HTTP_ERROR=${response.status()} ${response.url()}`); });

try {
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForURL(/\/engineering\.html$/);
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  assert.equal(await page.locator("#proposal-editor").count(), 0);
  await page.goto(new URL("/engineering.html", baseUrl).toString(), { waitUntil: "networkidle" });
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForURL(/\/engineering\.html$/);

  await page.locator("#language-select").selectOption("en");
  assert.equal(await page.locator("html").getAttribute("lang"), "en");
  assert.equal(await page.getByRole("button", { name: "Provider settings" }).count(), 1);
  await page.locator("#language-select").selectOption("zh-CN");
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");

  const initialConfigs = await page.evaluate(() => fetch("/api/model-configs").then((response) => response.json()));
  await page.locator("#model-manager-open").click();
  await page.locator("#model-manager-dialog[open]").waitFor();
  const initialCardCount = await page.locator(".model-card").count();
  const runId = Date.now().toString(36);
  const profileNames = [`G3 Browser A ${runId}`, `G3 Browser B ${runId}`];
  for (const [name, model] of [[profileNames[0], "model-a"], [profileNames[1], "model-b"]]) {
    await page.getByRole("button", { name: "添加配置" }).click();
    await page.locator('.model-form select[name="type"]').selectOption("local");
    await page.locator('.model-form input[name="name"]').fill(name);
    await page.locator('.model-form input[name="model"]').fill(model);
    await page.locator('.model-form button[type="submit"]').click();
    await page.locator(".model-card").filter({ hasText: name }).last().waitFor();
  }
  assert.equal(await page.locator(".model-card").count(), initialCardCount + 2);
  await page.locator(".model-card").filter({ hasText: profileNames[0] }).last().getByRole("button", { name: "一键启用" }).click();
  await page.locator(".model-card.is-active").filter({ hasText: profileNames[0] }).waitFor();
  await page.locator(".model-card").filter({ hasText: profileNames[1] }).last().getByRole("button", { name: "一键启用" }).click();
  await page.locator(".model-card.is-active").filter({ hasText: profileNames[1] }).waitFor();
  const configs = await page.evaluate(() => fetch("/api/model-configs").then((response) => response.json()));
  assert.equal(configs.profiles.length, initialConfigs.profiles.length + 2);
  assert.equal(configs.profiles.find((item) => item.active).name, profileNames[1]);
  assert.ok(profileNames.every((name) => configs.profiles.some((item) => item.name === name)), "both newly named profiles must be saved");
  assert.ok(configs.profiles.every((item) => !Object.hasOwn(item, "apiKey")));
  for (const original of initialConfigs.profiles) {
    const retained = configs.profiles.find((item) => item.id === original.id);
    assert.ok(retained, `existing ${original.type} profile must remain saved`);
    assert.deepEqual({ type: retained.type, endpoint: retained.endpoint, model: retained.model }, { type: original.type, endpoint: original.endpoint, model: original.model });
  }
  await page.locator(".model-close").click();

  await page.locator("#project-new").click();
  const multiline = [
    "创建一块 120×90×20 mm 底板，在偏左位置创建 30×22 mm 矩形凸台，高 15 mm，",
    "凸台顶面四条外轮廓边创建 R3 圆角；底板创建 Φ16 贯穿孔，孔口创建 C0.8 倒角，",
    "在底板创建 40×24 mm、深 6 mm 的矩形口袋，生成第三角投影 Front、Top、Left 三视图。",
  ].join("\n");
  await page.locator(".wb-nl").fill(multiline);
  assert.equal(await page.locator(".wb-nl").inputValue(), multiline, "Chinese multiline input must be preserved exactly");
  assert.equal(await page.locator("#export-svg").isEnabled(), false, "new unsolved model must not export stale drawing");
  const intentResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/intent") && response.request().method() === "POST");
  const drawingResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/drawing/three-view") && response.request().method() === "POST").catch((error) => ({ waitError: error.message }));
  await page.locator(".wb-topbar button").filter({ hasText: "提交请求" }).click();
  const intentResponse = await intentResponsePromise;
  const intent = await intentResponse.json();
  assert.equal(intent.status, "new_ready", JSON.stringify({ status: intent.status, code: intent.error?.code ?? intent.code, message: intent.error?.message ?? intent.message, diagnostic: intent.diagnostic, skillError: intent.skill_run?.error, skillDiagnostic: intent.skill_run?.diagnostic, trace: intent.trace }));
  assert.equal(intent.trace.provider, "fake");
  assert.equal(intent.trace.outputKind, "tool_calls");
  assert.ok(intent.model.feature_order.includes("boss"));
  assert.ok(intent.model.feature_order.includes("through_hole"));
  assert.ok(intent.model.feature_order.includes("pocket"));
  assert.ok(intent.model.feature_order.includes("boss_fillet"));
  assert.ok(intent.model.feature_order.includes("hole_chamfer"));
  assert.equal(intent.model.part.name, "G3 通用安装板");
  assert.equal(intent.skill_run.tool_calls.filter((call) => call.name === "create_part").length, 1, "exactly one part must be created");
  assert.equal(intent.model.features.boss.feature.type, "extrude");
  assert.equal(intent.model.features.boss.feature.operation, "add");
  assert.equal(intent.model.features.through_hole.feature.type, "hole");
  assert.equal(intent.model.features.pocket.feature.type, "extrude");
  assert.equal(intent.model.features.pocket.feature.operation, "cut");
  assert.equal(intent.model.features.boss_fillet.feature.type, "fillet");
  assert.equal(intent.model.features.boss_fillet.feature.radius, 3);
  assert.equal(intent.model.features.boss_fillet.feature.edge_refs.length, 4);
  assert.ok(intent.model.features.boss_fillet.feature.edge_refs.every((ref) => !Object.hasOwn(ref, "edge_index")));
  assert.equal(intent.model.features.hole_chamfer.feature.type, "chamfer");
  assert.equal(intent.model.features.hole_chamfer.feature.distance, 0.8);
  const allowed = new Set(intent.skill_run.allowed_tool_names);
  for (const tool of ["create_part", "create_parametric_plate", "add_parametric_rectangular_add", "add_parametric_hole", "add_parametric_rectangular_cut", "create_fillet", "create_chamfer"]) assert.ok(allowed.has(tool), `Skill plan allows ${tool}`);

  await page.locator(".wb-drawing-img").waitFor();
  await page.getByText("三视图已完成，可导出 SVG/DXF").waitFor();
  const drawingResponse = await drawingResponsePromise;
  assert.equal(drawingResponse.waitError, undefined, drawingResponse.waitError ?? "drawing response is unavailable");
  const drawing = await drawingResponse.json();
  assert.equal(drawing.drawing_mode, "three_view");
  assert.equal(drawing.drawing_report.view_count, 3);
  assert.deepEqual(drawing.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "top"]);
  assert.match(drawing.svg, /id="view_front"/);
  assert.match(drawing.svg, /id="view_top"/);
  assert.match(drawing.svg, /id="view_left"/);
  assert.match(drawing.dxf, /SECTION/);
  assert.equal(drawing.skills_report.SVG_EXPORT.status, "PASS");
  assert.equal(drawing.skills_report.DXF_EXPORT.status, "PASS");
  assert.equal(await page.locator("#export-svg").isEnabled(), true);
  assert.equal(await page.locator("#export-dxf").isEnabled(), true);
  const skillPanel = page.locator("#cad-skill-status");
  await skillPanel.getByText("机械三视图生成与检查").waitFor();
  await skillPanel.getByText("SVG 导出检查").waitFor();
  const skillText = await skillPanel.innerText();
  for (const skill of ["机械零件需求解析", "工程建模规划", "机械三视图生成与检查", "工程图标注", "CAD 导出与质量检查"]) assert.ok(skillText.includes(skill), `Workbench reports ${skill}`);
  assert.ok(skillText.includes("已完成"));
  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });
  const acceptanceSvg = page.waitForEvent("download");
  await page.locator("#export-svg").click();
  assert.match((await acceptanceSvg).suggestedFilename(), /\.svg$/);
  const acceptanceDxf = page.waitForEvent("download");
  await page.locator("#export-dxf").click();
  assert.match((await acceptanceDxf).suggestedFilename(), /\.dxf$/);
  assert.deepEqual(pageErrors, []);
  console.log("BROWSER_ACCEPTANCE=PASS; / and /engineering.html use the Engineering Workbench; G3 fake Engineering Intent, Skills allowlist, EngineeringModel v2 Extrude/Hole/Pocket/Fillet/Chamfer, semantic edge references, Front/Top/Left, validated SVG/DXF downloads, Chinese/English");
} catch (error) {
  console.error("BROWSER_ACCEPTANCE_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  throw error;
} finally {
  await browser.close();
}
