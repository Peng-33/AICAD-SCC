import assert from "node:assert/strict";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://g4-browser:4173/";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, acceptDownloads: true });
const page = await context.newPage();
const pageErrors = [];
const httpErrors = [];
page.on("pageerror", (error) => pageErrors.push(error.message));
page.on("response", (response) => { if (response.status() >= 400) httpErrors.push(`${response.status()} ${response.url()}`); });

try {
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForURL(/\/engineering\.html$/);
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  assert.equal(await page.locator("#proposal-editor").count(), 0);
  await page.goto(new URL("/engineering.html", baseUrl).toString(), { waitUntil: "networkidle" });
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  await page.goto(baseUrl, { waitUntil: "networkidle" });

  await page.locator("#language-select").selectOption("en");
  assert.equal(await page.locator("html").getAttribute("lang"), "en");
  await page.locator("#language-select").selectOption("zh-CN");
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");

  await page.locator("#project-new").click();
  const requestText = [
    "新建一个通用阶梯轴，轴向总长 80 mm，左段直径 40 mm 长 20 mm，中段直径 30 mm 长 40 mm，右段直径 20 mm 长 20 mm。",
    "轴线沿 X 方向，左端法兰布置 4 个等分直径 3 mm 的轴向孔，孔中心圆半径 12 mm。",
    "使用 EngineeringModel v2 的 Sketch、DatumAxis、Revolve、Hole 和 CircularPattern，生成第三角投影 Front、Top、Left 三视图。",
  ].join("\n");
  await page.locator(".wb-nl").fill(requestText);
  assert.equal(await page.locator(".wb-nl").inputValue(), requestText, "Chinese multiline engineering request must be preserved exactly");
  assert.equal(await page.locator("#export-svg").isEnabled(), false, "empty model must not export stale drawing");

  const intentPromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/intent") && response.request().method() === "POST");
  const drawingPromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/drawing/three-view") && response.request().method() === "POST");
  await page.locator(".wb-topbar button").filter({ hasText: "提交请求" }).click();
  const intentResponse = await intentPromise;
  const intent = await intentResponse.json();
  assert.equal(intent.status, "new_ready", JSON.stringify({ status: intent.status, code: intent.error?.code ?? intent.code, message: intent.error?.message ?? intent.message, diagnostic: intent.diagnostic, skillError: intent.skill_run?.error, skillDiagnostic: intent.skill_run?.diagnostic, trace: intent.trace }));
  assert.equal(intent.trace.provider, "fake");
  assert.equal(intent.trace.outputKind, "tool_calls");
  assert.equal(intent.model.part.name, "G4 通用阶梯轴");
  for (const id of ["shaft_profile", "shaft_axis", "shaft_revolve", "flange_hole_seed", "flange_hole_pattern"]) assert.ok(intent.model.feature_order.includes(id), `${id} must be in the EngineeringModel v2 history`);
  assert.equal(intent.model.features.shaft_profile.feature.type, "sketch");
  assert.equal(intent.model.features.shaft_axis.feature.type, "datum_axis");
  assert.deepEqual(intent.model.features.shaft_axis.feature.definition, {
    kind: "through_two_points", first: { x: 0, y: 0, z: 0 }, second: { x: 80, y: 0, z: 0 },
  });
  assert.equal(intent.model.features.shaft_revolve.feature.type, "revolve");
  assert.equal(intent.model.features.shaft_revolve.feature.operation, "add");
  assert.deepEqual(intent.model.features.shaft_revolve.feature.angle, { kind: "full_360" });
  assert.equal(intent.model.features.flange_hole_seed.feature.type, "hole");
  assert.equal(intent.model.features.flange_hole_pattern.feature.type, "pattern");
  const allowedTools = new Set(intent.skill_run.allowed_tool_names);
  for (const tool of ["create_part", "create_sketch", "create_datum_axis", "create_revolve", "add_parametric_hole", "add_feature_pattern"]) assert.ok(allowedTools.has(tool), `Engineering Skill must allow ${tool}`);

  await page.locator(".wb-drawing-img").waitFor();
  await page.getByText("三视图已完成，可导出 SVG/DXF").waitFor();
  const drawingResponse = await drawingPromise;
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
  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });
  const svgDownload = page.waitForEvent("download");
  await page.locator("#export-svg").click();
  assert.match((await svgDownload).suggestedFilename(), /\.svg$/);
  const dxfDownload = page.waitForEvent("download");
  await page.locator("#export-dxf").click();
  assert.match((await dxfDownload).suggestedFilename(), /\.dxf$/);
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(httpErrors, []);
  console.log("BROWSER_ACCEPTANCE=PASS; home and compatibility URL use one zh-CN Engineering Workbench; fake Engineering Intent emits Sketch + two-point DatumAxis + full Revolve ADD + Hole + CircularPattern; formal Front/Top/Left DrawingModel and SVG/DXF exports validated");
} catch (error) {
  console.error("BROWSER_ACCEPTANCE_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  console.error("BROWSER_HTTP_ERRORS", httpErrors);
  throw error;
} finally {
  await browser.close();
}
