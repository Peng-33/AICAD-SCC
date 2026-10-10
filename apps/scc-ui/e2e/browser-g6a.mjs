import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://g6a-browser:4173/";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 1100 }, acceptDownloads: true });
const page = await context.newPage();
const pageErrors = [];
const httpErrors = [];
page.on("pageerror", (error) => pageErrors.push(error.message));
page.on("response", (response) => { if (response.status() >= 400) httpErrors.push(`${response.status()} ${response.url()}`); });

try {
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForURL((url) => new URL(url).pathname === "/engineering.html");
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  assert.equal(await page.locator("#proposal-editor").count(), 0);

  const request = [
    "创建一块 120×90×20 mm 安装板，在偏左位置增加 30×22 mm、高 15 mm 的矩形凸台。",
    "创建 Φ10 贯穿孔并沿圆周阵列 4 个；在凸台顶边创建 R3 圆角，在孔口创建 C0.8 倒角。",
    "再加工一个 40×24 mm、深 6 mm 的盲口袋，生成第三角投影 Front、Top、Left 三视图。",
  ].join("\n");
  await page.locator("#project-new").click();
  await page.locator(".wb-nl").fill(request);
  assert.equal(await page.locator(".wb-nl").inputValue(), request, "Chinese multiline request is preserved");
  const intentResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/intent") && response.request().method() === "POST");
  const viewsResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/drawing/three-view") && response.request().method() === "POST");
  await page.locator(".wb-topbar button").filter({ hasText: "提交请求" }).click();
  const [intentResponse, viewsResponse] = await Promise.all([intentResponsePromise, viewsResponsePromise]);
  assert.equal(intentResponse.status(), 200);
  const intent = await intentResponse.json();
  assert.equal(intent.status, "new_ready", JSON.stringify({ status: intent.status, error: intent.error }));
  assert.equal(intent.trace.provider, "fake");
  assert.equal(viewsResponse.status(), 200);
  const threeView = await viewsResponse.json();
  assert.equal(threeView.drawing_report.view_count, 3);

  await page.getByLabel("剖切轴").selectOption("y");
  await page.getByLabel("剖切位置（mm）").fill("26");
  const sectionResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/drawing/section") && response.request().method() === "POST", { timeout: 180_000 });
  await page.getByRole("button", { name: "生成 A-A 剖视图" }).click();
  const sectionResponse = await sectionResponsePromise;
  assert.equal(sectionResponse.status(), 200, await sectionResponse.text());
  const section = await sectionResponse.json();
  assert.equal(section.drawing_mode, "three_view_section");
  assert.equal(section.drawing_report.view_count, 4);
  assert.deepEqual(section.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "section_aa", "top"]);
  assert.ok(section.section_view.section_edge_count > 0);
  assert.ok(section.section_view.boundary_loop_count > 0);
  assert.equal(section.skills_report.status, "PASS", JSON.stringify(section.skills_report));
  assert.equal(section.skills_report.SECTION_GEOMETRY.status, "PASS");
  assert.equal(section.skills_report.SVG_EXPORT.status, "PASS");
  assert.equal(section.skills_report.DXF_EXPORT.status, "PASS");
  assert.equal(section.skills_report.DRAWING_COMPLETENESS.status, "PARTIAL");
  assert.equal(section.skills_report.DRAWING_ANNOTATION.status, "PARTIAL");
  assert.equal(section.skills_report.UNOBSERVABLE_FEATURE_REPORT[0].formal_dimension_annotation, "UNSUPPORTED");
  assert.match(section.svg, /id="view_section_aa"/);
  assert.match(section.svg, /id="section_aa_hatch"/);
  assert.match(section.dxf, /AICAD_HATCH/);
  await page.getByText("A-A 剖视图几何与 SVG/DXF 已通过验证；参数化特征关联尺寸标注尚未完成").waitFor();

  const svgDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-svg").click();
  const svgDownload = await svgDownloadPromise;
  const svgBytes = await readFile(await svgDownload.path());
  assert.match(svgBytes.toString("utf8"), /id="view_section_aa"/);
  assert.match(svgBytes.toString("utf8"), /section_aa_hatch/);
  const dxfDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-dxf").click();
  const dxfDownload = await dxfDownloadPromise;
  const dxfBytes = await readFile(await dxfDownload.path());
  assert.match(dxfBytes.toString("utf8"), /AICAD_HATCH/);
  assert.match(dxfBytes.toString("utf8"), /\$INSUNITS\n70\n4/);

  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(httpErrors, []);
  console.log(`G6A_TIMINGS_MS=${JSON.stringify(section.parametric_timings_ms)}`);
  console.log(`BROWSER_ACCEPTANCE=PASS; fake_provider=true; views=Front/Top/Left/Section A-A; loops=${section.section_view.boundary_loop_count}; SVG_BYTES=${svgBytes.length}; DXF_BYTES=${dxfBytes.length}; REAL_PROVIDER=NOT_RUN`);
} catch (error) {
  console.error("BROWSER_ACCEPTANCE_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  console.error("BROWSER_HTTP_ERRORS", httpErrors);
  throw error;
} finally {
  await browser.close();
}
