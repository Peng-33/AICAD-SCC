import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://g7b-browser:4173/";
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
    if (expectedInvalidStepRequest && path === "/api/engineering/export/step" && response.status() >= 400) return;
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
  const prompt = "创建一个 RULED、ALIGNED 三截面放样实体：XY 平面 Z=0 的圆截面中心 (0,0)，直径 20 mm；Z=30 mm 的圆截面中心 (5,0)，直径 40 mm；Z=70 mm 的圆截面中心 (-3,2)，直径 26 mm。生成 Front、Top、Left、A-A 剖视，并导出 SVG、DXF 和正式 STEP。";
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
  assert.equal(model.features.section_a.feature.type, "sketch");
  assert.equal(model.features.section_b.feature.support.feature, "section_plane_b");
  assert.equal(model.features.section_c.feature.support.feature, "section_plane_c");
  assert.deepEqual(model.features.loft_body.feature, {
    type: "loft", sections: ["section_a", "section_b", "section_c"], operation: "add",
    mode: "ruled", orientation_policy: "aligned",
  });

  const drawingResponse = await drawingPromise;
  assert.equal(drawingResponse.status(), 200);
  const drawing = await drawingResponse.json();
  assert.equal(drawing.drawing_report.view_count, 3);
  assert.deepEqual(drawing.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "top"]);
  assert.match(drawing.svg, /view_front/);
  assert.match(drawing.svg, /view_top/);
  assert.match(drawing.svg, /view_left/);
  assert.ok(drawing.spatial_summary.parametric.operation_features.includes("loft_body"));
  assert.ok(drawing.drawing_report.primitive_count > 0);
  assert.ok(drawing.spatial_summary.parametric.volume_mm3 > 0);

  await page.locator(".wb-tree-item").filter({ hasText: "loft_body" }).click();
  const loftProperties = await page.locator(".wb-prop").allTextContents();
  assert.ok(loftProperties.some((value) => value.includes("截面草图引用") && value.includes("section_a, section_b, section_c")), loftProperties.join("\n"));
  assert.ok(loftProperties.some((value) => value.includes("放样模式") && value.includes("直纹放样")), loftProperties.join("\n"));
  assert.ok(loftProperties.some((value) => value.includes("截面定向规则") && value.includes("对齐")), loftProperties.join("\n"));
  assert.ok(loftProperties.some((value) => value.includes("操作类型") && value.includes("增材 (ADD)")), loftProperties.join("\n"));

  await page.locator("[aria-label='剖切轴']").selectOption("y");
  await page.locator("[aria-label='剖切位置（mm）']").fill("0");
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
  invalidModel.features.loft_body.feature.sections[1] = "missing_section";
  expectedInvalidStepRequest = true;
  const failed = await page.evaluate(async (invalid) => {
    const response = await fetch("/api/engineering/export/step", {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ model: invalid }),
    });
    return { status: response.status, headers: Object.fromEntries(response.headers.entries()), body: await response.json() };
  }, invalidModel);
  expectedInvalidStepRequest = false;
  assert.equal(failed.status, 422);
  assert.equal(failed.body.error.details?.error_code, "LOFT_SECTION_REFERENCE_NOT_FOUND");
  assert.equal(failed.headers["x-aicad-step-sha256"], undefined);
  assert.equal(failed.headers["content-disposition"], undefined);
  assert.equal(Object.hasOwn(failed.body, "artifact"), false);

  assert.equal(downloadCount, 3);
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(unexpectedHttpErrors, []);
  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });
  console.log(`BROWSER_G7B=PASS; provider=fake; locale=zh-CN; sections=3; Loft=RULED/ALIGNED ADD; views=Front/Top/Left; section=PASS; SVG_BYTES=${svgBytes.length}; DXF_BYTES=${dxfBytes.length}; STEP_BYTES=${validatedBytes.length}; STEP_SHA256=${stepHash}; invalid-loft-no-artifact=PASS; REAL_PROVIDER=NOT_RUN`);
} catch (error) {
  console.error("BROWSER_G7B_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  console.error("BROWSER_HTTP_ERRORS", unexpectedHttpErrors);
  console.error("BROWSER_FEATURE_TREE", await page.locator(".wb-tree-item").allTextContents().catch(() => []));
  throw error;
} finally {
  await browser.close();
}
