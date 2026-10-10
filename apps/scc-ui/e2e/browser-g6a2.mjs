import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://g6a2-browser:4173/";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 1050 }, acceptDownloads: true });
const page = await context.newPage();
const pageErrors = [];
const httpErrors = [];
page.on("pageerror", (error) => pageErrors.push(error.message));
page.on("response", (response) => { if (response.status() >= 400) httpErrors.push(`${response.status()} ${response.url()}`); });

function dimensionFor(drawing, id) {
  return drawing.drawing_report.views.flatMap((view) => view.dimensions).find((dimension) => dimension.id === id);
}

try {
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForURL((url) => new URL(url).pathname === "/engineering.html");
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  assert.equal(await page.locator("#proposal-editor").count(), 0);
  await page.goto(new URL("/engineering.html", baseUrl).toString(), { waitUntil: "networkidle" });
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);

  await page.locator("#project-new").click();
  const request = "创建一块 120×90×20 mm 安装板。\n在顶面增加 Φ10、深 6 mm 的盲孔，并生成 Front、Top、Left 三视图。";
  await page.locator(".wb-nl").fill(request);
  assert.equal(await page.locator(".wb-nl").inputValue(), request, "multiline Chinese engineering request must be preserved");
  const intentResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/intent") && response.request().method() === "POST");
  const drawingResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/drawing/three-view") && response.request().method() === "POST");
  await page.locator(".wb-topbar button").filter({ hasText: "提交请求" }).click();
  const intentResponse = await intentResponsePromise;
  assert.equal(intentResponse.status(), 200);
  const intent = await intentResponse.json();
  assert.equal(intent.status, "new_ready", JSON.stringify({ status: intent.status, error: intent.error, skill: intent.skill_run }));
  const drawingResponse = await drawingResponsePromise;
  assert.equal(intent.trace.provider, "fake");
  assert.ok(intent.model.feature_order.includes("center_hole"));

  assert.equal(drawingResponse.status(), 200);
  const initialDrawing = await drawingResponse.json();
  assert.equal(initialDrawing.drawing_report.view_count, 3);
  assert.deepEqual(initialDrawing.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "top"]);
  const initialAssociation = initialDrawing.drawing_report.associative_dimensions.find((item) => item.annotation_id === "associative.hole.center_hole.callout");
  assert.equal(initialAssociation.status, "resolved");
  assert.equal(initialAssociation.nominal.mm, 10);
  assert.match(dimensionFor(initialDrawing, initialAssociation.drawing_dimension_id).formatted, /Ø10/);
  assert.match(dimensionFor(initialDrawing, initialAssociation.drawing_dimension_id).formatted, /↧6/);

  await page.locator(".wb-tree-item").filter({ hasText: /^center_hole hole$/ }).click();
  const diameterInput = page.locator(".wb-prop-input");
  assert.equal(await diameterInput.count(), 0, "this unbound Project v2 Hole exposes no editable parameter slot in the current Workbench");
  await page.locator(".wb-drawing-img").waitFor();
  assert.equal(await page.locator(".wb-drawing-img").evaluate((image) => image.complete), true);
  const visibleSvg = initialDrawing.svg;
  assert.match(visibleSvg, /Ø10/);
  assert.match(visibleSvg, /↧6/);

  const svgDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-svg").click();
  const svgDownload = await svgDownloadPromise;
  const svgBytes = await readFile(await svgDownload.path());
  assert.match(svgBytes.toString("utf8"), /Ø10/);
  assert.match(svgBytes.toString("utf8"), /↧6/);
  const dxfDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-dxf").click();
  const dxfDownload = await dxfDownloadPromise;
  const dxfBytes = await readFile(await dxfDownload.path());
  assert.ok(dxfBytes.byteLength > 0);
  assert.match(dxfBytes.toString("utf8"), /Ø10/);
  assert.match(dxfBytes.toString("utf8"), /↧6/);
  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(httpErrors, []);
  console.log(`BROWSER_G6A2=PARTIAL; root+compatibility_entry=true; provider=fake; associative_hole_callout=Ø10/depth6; drawing=Front/Top/Left; parameter_edit=BLOCKED because Project v2 Parametric Hole has no bindable diameter slot; SVG_BYTES=${svgBytes.length}; DXF_BYTES=${dxfBytes.length}; REAL_PROVIDER=NOT_RUN`);
} catch (error) {
  console.error("BROWSER_G6A2_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  console.error("BROWSER_HTTP_ERRORS", httpErrors);
  throw error;
} finally {
  await browser.close();
}
