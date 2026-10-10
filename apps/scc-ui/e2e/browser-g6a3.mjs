import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://g6a3-browser:4173/";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1500, height: 1100 }, acceptDownloads: true });
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

function pathIs(response, path) {
  return new URL(response.url()).pathname === path && response.request().method() === "POST";
}

function associationFor(drawing, annotationId) {
  const association = drawing.drawing_report.associative_dimensions.find((item) => item.annotation_id === annotationId);
  assert.equal(association?.status, "resolved", `${annotationId} must resolve`);
  const dimension = drawing.drawing_report.views.flatMap((view) => view.dimensions)
    .find((item) => item.id === association.drawing_dimension_id);
  assert.ok(dimension, `${annotationId} must be a DrawingModel dimension`);
  return { association, dimension };
}

async function editHoleParameter(slotIndex, value) {
  const inputs = page.locator(".wb-prop-input");
  const input = inputs.nth(slotIndex);
  const responsePromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/tool"));
  await input.fill(String(value));
  await input.press("Tab");
  const response = await responsePromise;
  return { response, inputCall: response.request().postDataJSON() };
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
  assert.equal(await page.locator(".wb-nl").inputValue(), request);
  const intentResponsePromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/intent"));
  const initialDrawingPromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/drawing/three-view"));
  await page.getByRole("button", { name: "提交请求" }).click();
  const intentResponse = await intentResponsePromise;
  assert.equal(intentResponse.status(), 200);
  const intent = await intentResponse.json();
  assert.equal(intent.status, "new_ready", JSON.stringify({ status: intent.status, error: intent.error }));
  assert.equal(intent.trace.provider, "fake");
  assert.ok(intent.model.feature_order.includes("center_hole"));
  const initialResponse = await initialDrawingPromise;
  assert.equal(initialResponse.status(), 200);
  const initialDrawing = await initialResponse.json();
  assert.equal(initialDrawing.drawing_report.view_count, 3);
  assert.deepEqual(initialDrawing.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "top"]);
  const initial = associationFor(initialDrawing, "associative.hole.center_hole.callout");
  assert.equal(initial.association.nominal.mm, 10);
  assert.match(initial.dimension.formatted, /Ø10/);
  assert.match(initial.dimension.formatted, /↧6/);
  const initialVolume = initialDrawing.spatial_summary.parametric.volume_mm3;

  await page.locator(".wb-tree-item").filter({ hasText: /^center_hole 孔$/ }).click();
  assert.match(await page.locator(".wb-selected-feature").innerText(), /center_hole.*孔/);
  assert.equal(await page.locator(".wb-prop-input").count(), 2);
  assert.deepEqual(await page.locator(".wb-prop-label").allTextContents(), ["直径", "盲孔深度"]);
  assert.deepEqual(await page.locator(".wb-prop-input").evaluateAll((inputs) => inputs.map((input) => input.value)), ["10", "6"]);

  let drawingPromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/drawing/three-view"));
  let edit = await editHoleParameter(0, 12);
  assert.equal(edit.response.status(), 200, await edit.response.text());
  assert.equal(edit.inputCall.toolCall.name, "set_feature_parameter");
  assert.equal(edit.inputCall.toolCall.arguments.part_id, "g6a3_browser_part");
  assert.equal(edit.inputCall.toolCall.arguments.feature, "center_hole");
  assert.equal(edit.inputCall.toolCall.arguments.slot, "diameter");
  assert.equal(edit.inputCall.toolCall.arguments.expected_type, "length_mm");
  const diameterEdit = await edit.response.json();
  const diameterParameterId = diameterEdit.model.feature_parameters.center_hole.diameter;
  assert.equal(diameterEdit.model.parameters[diameterParameterId].value.value, 12);
  const diameterDrawingResponse = await drawingPromise;
  assert.equal(diameterDrawingResponse.status(), 200);
  const diameterDrawing = await diameterDrawingResponse.json();
  const diameterAssociation = associationFor(diameterDrawing, "associative.hole.center_hole.callout");
  assert.equal(diameterAssociation.association.nominal.mm, 12);
  assert.match(diameterAssociation.dimension.formatted, /Ø12/);
  assert.match(diameterAssociation.dimension.formatted, /↧6/);
  const diameterVolume = diameterDrawing.spatial_summary.parametric.volume_mm3;
  assert.ok(diameterVolume < initialVolume, "real OCP B-Rep volume must decrease when the Hole diameter increases");

  await page.locator("[aria-label='剖切位置（mm）']").fill("13");
  const initialSectionPromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/drawing/section"));
  await page.getByRole("button", { name: "生成 A-A 剖视图" }).click();
  const initialSectionResponse = await initialSectionPromise;
  assert.equal(initialSectionResponse.status(), 200);
  const initialSection = await initialSectionResponse.json();
  assert.equal(initialSection.drawing_report.view_count, 4);
  assert.ok(initialSection.section_view.boundary_loop_count >= 1);

  drawingPromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/drawing/section"));
  edit = await editHoleParameter(1, 8);
  assert.equal(edit.response.status(), 200, await edit.response.text());
  assert.equal(edit.inputCall.toolCall.arguments.slot, "blind_depth");
  const depthEdit = await edit.response.json();
  const depthParameterId = depthEdit.model.feature_parameters.center_hole.blind_depth;
  assert.equal(depthEdit.model.parameters[depthParameterId].value.value, 8);
  const updatedSectionResponse = await drawingPromise;
  assert.equal(updatedSectionResponse.status(), 200);
  const updatedSection = await updatedSectionResponse.json();
  assert.equal(updatedSection.drawing_report.view_count, 4);
  assert.ok(updatedSection.section_view.boundary_loop_count > initialSection.section_view.boundary_loop_count,
    "the z=13 section must gain the blind-hole void after depth changes from 6 to 8 mm");
  const sectionAssociation = associationFor(updatedSection, "associative.hole.center_hole.callout");
  assert.equal(sectionAssociation.association.nominal.mm, 12);
  assert.match(sectionAssociation.dimension.formatted, /↧8/);

  const modeSelect = page.locator(".wb-drawing-mode");
  const finalDrawingPromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/drawing/three-view"));
  await modeSelect.selectOption("three-view");
  const finalDrawingResponse = await finalDrawingPromise;
  assert.equal(finalDrawingResponse.status(), 200);
  const finalDrawing = await finalDrawingResponse.json();
  const finalAssociation = associationFor(finalDrawing, "associative.hole.center_hole.callout");
  assert.equal(finalAssociation.association.nominal.mm, 12);
  assert.match(finalAssociation.dimension.formatted, /Ø12/);
  assert.match(finalAssociation.dimension.formatted, /↧8/);
  assert.match(finalDrawing.svg, /Ø12/);
  assert.match(finalDrawing.svg, /↧8/);
  assert.match(finalDrawing.dxf, /Ø12/);
  assert.match(finalDrawing.dxf, /↧8/);
  assert.ok(finalDrawing.spatial_summary.parametric.volume_mm3 < diameterVolume,
    "real OCP B-Rep volume must decrease when blind depth increases");
  assert.equal(await page.locator("#drawing-status").innerText(), "三视图已完成，可导出 SVG/DXF");

  const svgPromise = page.waitForEvent("download");
  await page.locator("#export-svg").click();
  const svgDownload = await svgPromise;
  const svgBytes = await readFile(await svgDownload.path());
  assert.match(svgBytes.toString("utf8"), /Ø12/);
  assert.match(svgBytes.toString("utf8"), /↧8/);
  const dxfPromise = page.waitForEvent("download");
  await page.locator("#export-dxf").click();
  const dxfDownload = await dxfPromise;
  const dxfBytes = await readFile(await dxfDownload.path());
  assert.match(dxfBytes.toString("utf8"), /Ø12/);
  assert.match(dxfBytes.toString("utf8"), /↧8/);

  const stepResponsePromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/export/step"));
  const stepDownloadPromise = page.waitForEvent("download");
  await page.locator("#project-export-step").click();
  const [stepResponse, stepDownload] = await Promise.all([stepResponsePromise, stepDownloadPromise]);
  assert.equal(stepResponse.status(), 200);
  const validatedBytes = Buffer.from(await stepResponse.body());
  const downloadedBytes = await readFile(await stepDownload.path());
  const stepHash = createHash("sha256").update(validatedBytes).digest("hex");
  assert.deepEqual(downloadedBytes, validatedBytes, "browser download must match the canonical server-validated STEP bytes");
  assert.equal(stepResponse.headers()["x-aicad-step-sha256"], stepHash);
  assert.equal(Number(stepResponse.headers()["x-aicad-step-size"]), validatedBytes.length);
  for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
    assert.equal(stepResponse.headers()[`x-aicad-step-${stage}`], "passed", `${stage} must pass before bytes are delivered`);
  }
  assert.match(validatedBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);

  const unchangedDrawingUrl = await page.locator(".wb-drawing-img").getAttribute("src");
  edit = await editHoleParameter(0, -1);
  assert.notEqual(edit.response.status(), 200);
  const invalidValueError = await edit.response.json();
  assert.equal(invalidValueError.error.code, "SCC-TOOL-005", JSON.stringify(invalidValueError));
  assert.equal(await page.locator(".wb-prop-input").nth(0).inputValue(), "12", "illegal value must not replace the source model");
  assert.equal(await page.locator(".wb-drawing-img").getAttribute("src"), unchangedDrawingUrl, "failed edit must retain the prior drawing");
  assert.match(await page.locator("#status").innerText(), /SCC-TOOL-005.*正数的毫米值/);

  drawingPromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/drawing/three-view"));
  const wideHoleEdit = await editHoleParameter(0, 300);
  assert.equal(wideHoleEdit.response.status(), 200, await wideHoleEdit.response.text());
  await drawingPromise;
  const wideHoleDrawingUrl = await page.locator(".wb-drawing-img").getAttribute("src");
  edit = await editHoleParameter(1, 20);
  assert.notEqual(edit.response.status(), 200, "a blind cut that removes the entire body must fail during OCP rebuild");
  const rebuildError = await edit.response.json();
  const rebuildDiagnostic = rebuildError.error.details?.diagnostics?.find((item) => item.details?.parametric_code);
  const parametricFailureCode = rebuildDiagnostic?.details?.parametric_code;
  assert.ok(["CUT_REMOVES_BODY", "NO_BODY"].includes(parametricFailureCode), JSON.stringify(rebuildError));
  assert.equal(Object.hasOwn(rebuildError, "model"), false, "failed OCP rebuild returns no candidate EngineeringModel");
  assert.equal(await page.locator(".wb-prop-input").nth(0).inputValue(), "300", "failed geometry rebuild must retain the last committed parameter");
  assert.equal(await page.locator(".wb-prop-input").nth(1).inputValue(), "8");
  assert.equal(await page.locator(".wb-drawing-img").getAttribute("src"), wideHoleDrawingUrl);

  drawingPromise = page.waitForResponse((response) => pathIs(response, "/api/engineering/drawing/three-view"));
  const restoredEdit = await editHoleParameter(0, 12);
  assert.equal(restoredEdit.response.status(), 200, await restoredEdit.response.text());
  await drawingPromise;
  assert.equal(await page.locator(".wb-prop-input").nth(0).inputValue(), "12");
  assert.equal(await page.locator(".wb-prop-input").nth(1).inputValue(), "8");
  await page.getByText("三视图已完成，可导出 SVG/DXF").waitFor();
  assert.equal(downloadCount, 3, "only SVG, DXF, and the validated STEP should have downloaded");
  assert.equal(expectedToolFailures, 2);
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(unexpectedHttpErrors, []);

  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });
  console.log(`BROWSER_G6A3=PASS; provider=fake; hole_diameter=10->12; blind_depth=6->8; volume_mm3=${initialVolume}->${diameterVolume}->${finalDrawing.spatial_summary.parametric.volume_mm3}; section_z13_loops=${initialSection.section_view.boundary_loop_count}->${updatedSection.section_view.boundary_loop_count}; SVG_BYTES=${svgBytes.length}; DXF_BYTES=${dxfBytes.length}; STEP_BYTES=${validatedBytes.length}; STEP_SHA256=${stepHash}; invalid_rollback=PASS; OCP_rebuild_rollback=PASS; geometry_failure_code=${parametricFailureCode}; geometry_failure_envelope=${rebuildError.error.code}; REAL_PROVIDER=NOT_RUN`);
} catch (error) {
  console.error("BROWSER_G6A3_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  console.error("BROWSER_HTTP_ERRORS", unexpectedHttpErrors);
  throw error;
} finally {
  await browser.close();
}
