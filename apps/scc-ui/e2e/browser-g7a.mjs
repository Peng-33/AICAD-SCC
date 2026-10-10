import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://g7a-browser:4173/";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1540, height: 1120 }, acceptDownloads: true });
const page = await context.newPage();
const pageErrors = [];
const unexpectedHttpErrors = [];
let downloadCount = 0;
let expectedInvalidStepRequest = false;
page.on("pageerror", (error) => pageErrors.push(error.message));
page.on("download", () => { downloadCount += 1; });
page.on("response", (response) => {
  if (response.status() >= 400) {
    const path = new URL(response.url()).pathname;
    if (expectedInvalidStepRequest && path === "/api/engineering/export/step" && response.status() === 422) return;
    unexpectedHttpErrors.push(`${response.status()} ${response.url()}`);
  }
});
const postPath = (response, path) => new URL(response.url()).pathname === path && response.request().method() === "POST";

try {
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForURL((url) => new URL(url).pathname === "/engineering.html");
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  assert.equal(await page.locator("#proposal-editor").count(), 0, "the retired SCC proposal workbench must not be exposed");

  await page.locator("#project-new").click();
  const prompt = "在一块 100×60×10 mm 底板上，沿 XY 平面上半径 40 mm 的四分之一圆弧路径，扫掠 YZ 平面上的直径 4 mm 圆截面，执行 ADD。生成 Front、Top、Left 和 Section A-A，并导出 SVG、DXF、STEP。";
  await page.locator(".wb-nl").fill(prompt);
  assert.equal(await page.locator(".wb-nl").inputValue(), prompt, "Chinese natural-language input must be preserved");

  const intentPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/intent"));
  const drawingPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/drawing/three-view"));
  await page.getByRole("button", { name: "提交请求" }).click();
  const intentResponse = await intentPromise;
  assert.equal(intentResponse.status(), 200);
  const intent = await intentResponse.json();
  assert.equal(intent.status, "new_ready", JSON.stringify({ status: intent.status, error: intent.error }));
  assert.equal(intent.trace.provider, "fake");
  const model = intent.model;
  assert.equal(model.features.curve_path.feature.type, "path");
  assert.equal(model.features.curve_path.feature.segments[0].geometry.radius, 40);
  assert.equal(model.features.sweep_profile.feature.type, "sketch");
  assert.deepEqual(model.features.curved_sweep.feature, {
    type: "sweep", profile: "sweep_profile", path: "curve_path", operation: "add",
    direction: "forward", profile_origin: { x: 0, y: 0 }, orientation: "fixed_binormal",
  });

  const drawingResponse = await drawingPromise;
  assert.equal(drawingResponse.status(), 200);
  const drawing = await drawingResponse.json();
  assert.equal(drawing.drawing_report.view_count, 3);
  assert.deepEqual(drawing.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "top"]);
  assert.match(drawing.svg, /view_front/);
  assert.match(drawing.svg, /view_top/);
  assert.match(drawing.svg, /view_left/);
  assert.ok(drawing.spatial_summary.parametric.operation_features.includes("curved_sweep"));
  assert.ok(drawing.spatial_summary.parametric.volume_mm3 > 100 * 60 * 10);

  await page.locator("[aria-label='剖切轴']").selectOption("y");
  await page.locator("[aria-label='剖切位置（mm）']").fill("35");
  const sectionPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/drawing/section"));
  await page.getByRole("button", { name: "生成 A-A 剖视图" }).click();
  const sectionResponse = await sectionPromise;
  const sectionText = await sectionResponse.text();
  assert.equal(sectionResponse.status(), 200, sectionText);
  const section = JSON.parse(sectionText);
  assert.equal(section.drawing_report.view_count, 4);
  assert.ok(section.section_view.section_edge_count > 0);
  assert.match(section.svg, /section_aa_hatch/);

  const threeViewPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/drawing/three-view"));
  await page.locator(".wb-drawing-mode").selectOption("three-view");
  const threeViewResponse = await threeViewPromise;
  assert.equal(threeViewResponse.status(), 200);
  assert.equal(await page.locator("#drawing-status").innerText(), "三视图已完成，可导出 SVG/DXF");

  const svgDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-svg").click();
  const svgBytes = await readFile(await (await svgDownloadPromise).path());
  assert.match(svgBytes.toString("utf8"), /view_front/);
  const dxfDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-dxf").click();
  const dxfBytes = await readFile(await (await dxfDownloadPromise).path());
  assert.match(dxfBytes.toString("utf8"), /SECTION/);

  const stepResponsePromise = page.waitForResponse((response) => postPath(response, "/api/engineering/export/step"));
  const stepDownloadPromise = page.waitForEvent("download");
  await page.locator("#project-export-step").click();
  const [stepResponse, stepDownload] = await Promise.all([stepResponsePromise, stepDownloadPromise]);
  assert.equal(stepResponse.status(), 200);
  const validatedBytes = Buffer.from(await stepResponse.body());
  const downloadedBytes = await readFile(await stepDownload.path());
  const stepHash = createHash("sha256").update(validatedBytes).digest("hex");
  assert.deepEqual(downloadedBytes, validatedBytes, "download must be the exact bytes validated by the server");
  assert.equal(stepResponse.headers()["x-aicad-step-sha256"], stepHash);
  assert.equal(Number(stepResponse.headers()["x-aicad-step-size"]), validatedBytes.length);
  for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
    assert.equal(stepResponse.headers()[`x-aicad-step-${stage}`], "passed");
  }

  const invalidModel = structuredClone(model);
  invalidModel.features.curve_path.feature.segments.push({
    id: "disconnected_segment",
    geometry: { kind: "line", start: { x: 100, y: 100 }, end: { x: 110, y: 100 } },
  });
  expectedInvalidStepRequest = true;
  const failed = await page.evaluate(async (invalid) => {
    const response = await fetch("/api/engineering/export/step", {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ model: invalid }),
    });
    return { status: response.status, headers: Object.fromEntries(response.headers.entries()), body: await response.json() };
  }, invalidModel);
  expectedInvalidStepRequest = false;
  assert.equal(failed.status, 422);
  assert.match(failed.body.error?.message ?? "", /PATH_DISCONTINUOUS/, JSON.stringify(failed.body));
  assert.equal(failed.headers["x-aicad-step-sha256"], undefined);
  assert.equal(failed.headers["content-disposition"], undefined);
  assert.equal(Object.hasOwn(failed.body, "artifact"), false);

  assert.equal(downloadCount, 3);
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(unexpectedHttpErrors, []);
  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });
  console.log(`BROWSER_G7A=PASS; provider=fake; locale=zh-CN; Path=quarter-arc-R40; Sweep=ADD; views=Front/Top/Left; section=PASS; SVG_BYTES=${svgBytes.length}; DXF_BYTES=${dxfBytes.length}; STEP_BYTES=${validatedBytes.length}; STEP_SHA256=${stepHash}; invalid-sweep-no-artifact=PASS; REAL_PROVIDER=NOT_RUN`);
} catch (error) {
  console.error("BROWSER_G7A_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  console.error("BROWSER_HTTP_ERRORS", unexpectedHttpErrors);
  console.error("BROWSER_FEATURE_TREE", await page.locator(".wb-tree-item").allTextContents().catch(() => []));
  throw error;
} finally {
  await browser.close();
}
