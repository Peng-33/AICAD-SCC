import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://g6b-browser:4173/";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1540, height: 1120 }, acceptDownloads: true });
const page = await context.newPage();
const pageErrors = [];
const unexpectedHttpErrors = [];
let expectedToolFailures = 0;
let downloadCount = 0;

page.on("pageerror", (error) => pageErrors.push(error.message));
page.on("download", () => { downloadCount += 1; });
page.on("response", (response) => {
  if (response.status() >= 400) {
    const path = new URL(response.url()).pathname;
    if (path === "/api/engineering/tool") expectedToolFailures += 1;
    else unexpectedHttpErrors.push(`${response.status()} ${response.url()}`);
  }
});

function postPath(response, path) {
  return new URL(response.url()).pathname === path && response.request().method() === "POST";
}

async function editProperty(index, value) {
  const input = page.locator(".wb-prop-input").nth(index);
  const responsePromise = page.waitForResponse((response) => postPath(response, "/api/engineering/tool"));
  await input.fill(String(value));
  await input.press("Tab");
  return responsePromise;
}

try {
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForURL((url) => new URL(url).pathname === "/engineering.html");
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  assert.equal(await page.locator("#proposal-editor").count(), 0, "the retired SCC proposal workbench must not be exposed");

  await page.locator("#project-new").click();
  const request = "创建一块 100×60×10 mm 通用加工板。\n在顶面创建 Φ4 贯穿沉头孔，入口直径 10 mm、包含角 90°。\n另建 Φ4 贯穿孔种子，并在 (24,20)、(30,-15)、(5,-20) mm 显式布置。生成 Front、Top、Left 三视图。";
  await page.locator(".wb-nl").fill(request);
  assert.equal(await page.locator(".wb-nl").inputValue(), request, "multiline Chinese requirements must be preserved exactly");

  const intentPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/intent"));
  const drawingPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/drawing/three-view"));
  await page.getByRole("button", { name: "提交请求" }).click();
  const intentResponse = await intentPromise;
  assert.equal(intentResponse.status(), 200);
  const intent = await intentResponse.json();
  assert.equal(intent.status, "new_ready", JSON.stringify({ status: intent.status, error: intent.error }));
  assert.equal(intent.trace.provider, "fake");

  const model = intent.model;
  const sinkFeature = model.features.cs_hole?.feature;
  assert.equal(sinkFeature?.type, "hole");
  assert.deepEqual(sinkFeature?.kind, { type: "countersink", diameter: 10, included_angle_degrees: 90 });
  assert.equal(sinkFeature?.diameter, 4);
  const patternFeature = model.features.irregular_hole_pattern?.feature;
  assert.equal(patternFeature?.type, "pattern");
  assert.equal(patternFeature?.seed_feature_id, "irregular_seed");
  assert.deepEqual(patternFeature?.definition, {
    kind: "explicit_placement",
    reference_plane: { kind: "principal", plane: "x_y" },
    placements: [
      { local_u_mm: 24, local_v_mm: 20 },
      { local_u_mm: 30, local_v_mm: -15 },
      { local_u_mm: 5, local_v_mm: -20 },
    ],
  });

  const drawingResponse = await drawingPromise;
  assert.equal(drawingResponse.status(), 200);
  let drawing = await drawingResponse.json();
  assert.equal(drawing.drawing_report.view_count, 3);
  assert.deepEqual(drawing.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "top"]);
  assert.match(drawing.svg, /view_front/);
  assert.match(drawing.svg, /view_top/);
  assert.match(drawing.svg, /view_left/);
  const originalVolume = drawing.spatial_summary.parametric.volume_mm3;

  await page.locator(".wb-tree-item").filter({ hasText: /^cs_hole 孔/ }).click();
  assert.match(await page.locator(".wb-selected-feature").innerText(), /cs_hole.*孔/);
  const propertyLabels = await page.locator(".wb-prop-label").allTextContents();
  const mouthIndex = propertyLabels.indexOf("沉头入口直径");
  const angleIndex = propertyLabels.indexOf("沉头包含角");
  assert.ok(mouthIndex >= 0 && angleIndex >= 0, `Chinese Countersink property labels missing: ${JSON.stringify(propertyLabels)}`);
  assert.equal(await page.locator(".wb-prop-input").nth(mouthIndex).inputValue(), "10");
  assert.equal(await page.locator(".wb-prop-input").nth(angleIndex).inputValue(), "90");

  const previousDrawingUrl = await page.locator(".wb-drawing-img").getAttribute("src");
  const invalidEdit = await editProperty(mouthIndex, 4);
  assert.notEqual(invalidEdit.status(), 200, "mouth diameter equal to the main bore must be rejected");
  const invalidBody = await invalidEdit.json();
  assert.ok(invalidBody.error?.code, JSON.stringify(invalidBody));
  assert.equal(await page.locator(".wb-prop-input").nth(mouthIndex).inputValue(), "10");
  assert.equal(await page.locator(".wb-drawing-img").getAttribute("src"), previousDrawingUrl);
  assert.equal(Object.hasOwn(invalidBody, "model"), false, "a rejected edit must not return a candidate source model");

  const validDrawingPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/drawing/three-view"));
  const validEdit = await editProperty(mouthIndex, 12);
  assert.equal(validEdit.status(), 200, await validEdit.text());
  const updated = await validEdit.json();
  assert.equal(updated.model.features.cs_hole.feature.kind.diameter, 10,
    "typed edits keep the original feature literal and store the driving value as an explicit parameter");
  const mouthParameterId = updated.model.feature_parameters.cs_hole.countersink_mouth_diameter;
  assert.ok(mouthParameterId);
  assert.deepEqual(updated.model.parameters[mouthParameterId].value, { type: "scalar", value: 12 });
  const updatedDrawingResponse = await validDrawingPromise;
  assert.equal(updatedDrawingResponse.status(), 200);
  drawing = await updatedDrawingResponse.json();
  assert.ok(drawing.spatial_summary.parametric.volume_mm3 < originalVolume,
    "increasing the Countersink mouth must change the real rebuilt solid volume");

  await page.locator("[aria-label='剖切轴']").selectOption("y");
  await page.locator("[aria-label='剖切位置（mm）']").fill("0");
  const sectionPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/drawing/section"));
  await page.getByRole("button", { name: "生成 A-A 剖视图" }).click();
  const sectionResponse = await sectionPromise;
  assert.equal(sectionResponse.status(), 200);
  const section = await sectionResponse.json();
  assert.equal(section.drawing_report.view_count, 4);
  assert.ok(section.section_view.section_edge_count > 0);
  assert.match(section.svg, /section_aa_hatch/);

  const threeViewPromise = page.waitForResponse((response) => postPath(response, "/api/engineering/drawing/three-view"));
  await page.locator(".wb-drawing-mode").selectOption("three-view");
  const threeViewResponse = await threeViewPromise;
  assert.equal(threeViewResponse.status(), 200);
  drawing = await threeViewResponse.json();
  assert.equal(drawing.drawing_report.view_count, 3);
  assert.equal(await page.locator("#drawing-status").innerText(), "三视图已完成，可导出 SVG/DXF");

  const svgDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-svg").click();
  const svgDownload = await svgDownloadPromise;
  const svgBytes = await readFile(await svgDownload.path());
  assert.match(svgBytes.toString("utf8"), /view_front/);

  const dxfDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-dxf").click();
  const dxfDownload = await dxfDownloadPromise;
  const dxfBytes = await readFile(await dxfDownload.path());
  assert.match(dxfBytes.toString("utf8"), /SECTION/);

  const stepResponsePromise = page.waitForResponse((response) => postPath(response, "/api/engineering/export/step"));
  const stepDownloadPromise = page.waitForEvent("download");
  await page.locator("#project-export-step").click();
  const [stepResponse, stepDownload] = await Promise.all([stepResponsePromise, stepDownloadPromise]);
  assert.equal(stepResponse.status(), 200);
  const validatedBytes = Buffer.from(await stepResponse.body());
  const downloadedBytes = await readFile(await stepDownload.path());
  const stepHash = createHash("sha256").update(validatedBytes).digest("hex");
  assert.deepEqual(downloadedBytes, validatedBytes, "browser STEP bytes must equal the server validated bytes");
  assert.equal(stepResponse.headers()["x-aicad-step-sha256"], stepHash);
  assert.equal(Number(stepResponse.headers()["x-aicad-step-size"]), validatedBytes.length);
  for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
    assert.equal(stepResponse.headers()[`x-aicad-step-${stage}`], "passed");
  }
  assert.match(validatedBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);

  assert.equal(downloadCount, 3);
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(unexpectedHttpErrors, []);

  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });
  console.log(`BROWSER_G6B=PASS; provider=fake; locale=zh-CN; Countersink=4/10/90deg; ExplicitPlacementPattern=seed+3_nonuniform_holes; invalid_edit_rollback=PASS; volume_mm3=${originalVolume}->${drawing.spatial_summary.parametric.volume_mm3}; section_edges=${section.section_view.section_edge_count}; SVG_BYTES=${svgBytes.length}; DXF_BYTES=${dxfBytes.length}; STEP_BYTES=${validatedBytes.length}; STEP_SHA256=${stepHash}; REAL_PROVIDER=NOT_RUN`);
} catch (error) {
  console.error("BROWSER_G6B_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  console.error("BROWSER_HTTP_ERRORS", unexpectedHttpErrors);
  console.error("BROWSER_FEATURE_TREE", await page.locator(".wb-tree-item").allTextContents().catch(() => []));
  throw error;
} finally {
  await browser.close();
}
