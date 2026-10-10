import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";

const baseUrl = process.env.SCC_UI_URL ?? "http://g5-browser:4173/";
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, acceptDownloads: true });
const page = await context.newPage();
const pageErrors = [];
const httpErrors = [];
page.on("pageerror", (error) => pageErrors.push(error.message));
page.on("response", (response) => { if (response.status() >= 400) httpErrors.push(`${response.status()} ${response.url()}`); });

async function submitIntent(text) {
  await page.locator("#project-new").click();
  await page.locator(".wb-nl").fill(text);
  assert.equal(await page.locator(".wb-nl").inputValue(), text, "Chinese multiline engineering request must be preserved exactly");
  const intentResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/intent") && response.request().method() === "POST");
  const drawingResponsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/drawing/three-view") && response.request().method() === "POST");
  await page.locator(".wb-topbar button").filter({ hasText: "提交请求" }).click();
  const [intentResponse, drawingResponse] = await Promise.all([intentResponsePromise, drawingResponsePromise]);
  const intent = await intentResponse.json();
  assert.equal(intent.status, "new_ready", JSON.stringify({ status: intent.status, error: intent.error, diagnostic: intent.diagnostic, skill: intent.skill_run }));
  assert.equal(intent.trace.provider, "fake");
  assert.equal(intent.trace.outputKind, "tool_calls");
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
  await page.locator(".wb-drawing-img").waitFor();
  await page.getByText("三视图已完成，可导出 SVG/DXF").waitFor();
  return { intent, drawing };
}

async function verifyDownload(download, extension, expectedPrefix) {
  assert.match(download.suggestedFilename(), new RegExp(`\\.${extension}$`));
  const bytes = await readFile(await download.path());
  assert.ok(bytes.byteLength > 0, `${extension.toUpperCase()} download must contain bytes`);
  assert.ok(bytes.toString("utf8").includes(expectedPrefix), `${extension.toUpperCase()} download must contain actual drawing content`);
  return bytes;
}

async function exportAndVerifyStep() {
  const responsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/export/step") && response.request().method() === "POST", { timeout: 180_000 });
  const downloadPromise = page.waitForEvent("download", { timeout: 180_000 });
  await page.locator("#project-export-step").click();
  const [response, download] = await Promise.all([responsePromise, downloadPromise]);
  assert.equal(response.status(), 200, `canonical STEP endpoint failed: ${response.status()}`);
  const headers = response.headers();
  for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
    assert.equal(headers[`x-aicad-step-${stage}`], "passed", `${stage} must pass before STEP delivery`);
  }
  const browserBytes = await readFile(await download.path());
  const responseBytes = Buffer.from(await response.body());
  assert.ok(browserBytes.length > 1000, "downloaded STEP must be a real artifact");
  assert.match(browserBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
  assert.deepEqual(browserBytes, responseBytes, "browser-saved STEP must equal the canonical API response bytes");
  const downloadedSha256 = createHash("sha256").update(browserBytes).digest("hex");
  assert.equal(downloadedSha256, headers["x-aicad-step-sha256"], "browser file hash must match the server validated-artifact hash");
  assert.equal(Number(headers["x-aicad-step-size"]), browserBytes.length);
  assert.ok(headers["x-aicad-model-fingerprint"]);
  return {
    bytes: browserBytes.length,
    sha256: downloadedSha256,
    stages: Object.fromEntries(["manufacturing", "build", "reimport", "equivalence"].map((stage) => [stage, headers[`x-aicad-step-${stage}`]])),
  };
}

async function exportFailureAndVerifyNoDownload() {
  await page.route("**/api/engineering/export/step", (route) => route.fulfill({
    status: 422,
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ error: {
      code: "SCC-MFG-006",
      stage: "manufacturing",
      message: "manufacturing_build/CUT_NO_INTERSECTION: cut does not intersect the solid",
      details: { failure_class: "GEOMETRY_BUILD_FAILURE", failure_stage: "manufacturing_build", error_code: "CUT_NO_INTERSECTION" },
    } }),
  }));
  const responsePromise = page.waitForResponse((response) => response.url().endsWith("/api/engineering/export/step") && response.request().method() === "POST");
  const downloadPromise = page.waitForEvent("download", { timeout: 1200 }).then(() => true).catch((error) => {
    if (error.name !== "TimeoutError") throw error;
    return false;
  });
  await page.locator("#project-export-step").click();
  const response = await responsePromise;
  assert.equal(response.status(), 422);
  assert.equal(response.headers()["content-disposition"], undefined);
  for (const header of ["x-aicad-step-sha256", "x-aicad-step-size", "x-aicad-step-manufacturing", "x-aicad-step-build", "x-aicad-step-reimport", "x-aicad-step-equivalence"]) {
    assert.equal(response.headers()[header], undefined, `failed UI request must not expose ${header}`);
  }
  const body = Buffer.from(await response.body());
  assert.equal(body.subarray(0, 32).toString("ascii").includes("ISO-10303-21"), false);
  assert.equal((await downloadPromise), false, "failed export must not produce a browser download");
  await page.unroute("**/api/engineering/export/step");
  return `422 ${response.url()}`;
}

try {
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForURL((url) => new URL(url).pathname === "/engineering.html");
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  assert.equal(await page.locator("#proposal-editor").count(), 0, "legacy SCC workbench is not the user page");
  await page.goto(new URL("/engineering.html", baseUrl).toString(), { waitUntil: "networkidle" });
  assert.equal(await page.getByRole("heading", { name: "AICAD 工程工作台" }).count(), 1);
  await page.locator("#language-select").selectOption("en");
  assert.equal(await page.locator("html").getAttribute("lang"), "en");
  await page.locator("#language-select").selectOption("zh-CN");
  assert.equal(await page.locator("html").getAttribute("lang"), "zh-CN");

  const machiningText = [
    "创建一块 120×90×20 mm 安装板，在偏左位置增加 30×22 mm、高 15 mm 的矩形凸台。",
    "创建 Φ10 贯穿孔并沿圆周阵列 4 个；在凸台顶边创建 R3 圆角，在孔口创建 C0.8 倒角。",
    "再加工一个 40×24 mm、深 6 mm 的盲口袋，生成第三角投影 Front、Top、Left 三视图。",
  ].join("\n");
  const machining = await submitIntent(machiningText);
  const model = machining.intent.model;
  for (const id of ["plate_sketch", "plate_extrude", "boss", "seed_hole", "hole_pattern", "pocket", "boss_fillet", "hole_chamfer"]) {
    assert.ok(model.feature_order.includes(id), `${id} must be in the Project v2 history`);
  }
  assert.equal(model.features.plate_extrude.feature.type, "extrude");
  assert.equal(model.features.plate_extrude.feature.operation, "add");
  assert.equal(model.features.boss.feature.type, "extrude");
  assert.equal(model.features.boss.feature.operation, "add");
  assert.equal(model.features.seed_hole.feature.type, "hole");
  assert.equal(model.features.hole_pattern.feature.type, "pattern");
  assert.equal(model.features.hole_pattern.feature.definition.kind, "circular");
  assert.equal(model.features.boss_fillet.feature.type, "fillet");
  assert.equal(model.features.hole_chamfer.feature.type, "chamfer");
  assert.ok(model.features.boss_fillet.feature.edge_refs.every((ref) => !Object.hasOwn(ref, "edge_index")));

  const svgDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-svg").click();
  const svgBytes = await verifyDownload(await svgDownloadPromise, "svg", "<svg");
  const dxfDownloadPromise = page.waitForEvent("download");
  await page.locator("#export-dxf").click();
  const dxfBytes = await verifyDownload(await dxfDownloadPromise, "dxf", "SECTION");
  const machiningStep = await exportAndVerifyStep();
  if (process.env.SCC_UI_SCREENSHOT) await page.screenshot({ path: process.env.SCC_UI_SCREENSHOT, fullPage: true });

  const revolveText = [
    "新建一个通用阶梯轴，轴向总长 80 mm，左段直径 40 mm 长 20 mm，中段直径 30 mm 长 40 mm，右段直径 20 mm 长 20 mm。",
    "轴线沿 X 方向，使用 Sketch、DatumAxis 和 Revolve 创建整周回转实体，并在法兰上布置 4 个等分轴向孔。",
  ].join("\n");
  const revolved = await submitIntent(revolveText);
  for (const id of ["shaft_profile", "shaft_axis", "shaft_revolve", "flange_hole_seed", "flange_hole_pattern"]) {
    assert.ok(revolved.intent.model.feature_order.includes(id), `${id} must be in Revolve EngineeringModel v2 history`);
  }
  assert.equal(revolved.intent.model.features.shaft_revolve.feature.type, "revolve");
  assert.equal(revolved.intent.model.features.shaft_revolve.feature.operation, "add");
  const revolveStep = await exportAndVerifyStep();
  const expectedFailureResponse = await exportFailureAndVerifyNoDownload();

  assert.deepEqual(pageErrors, []);
  assert.deepEqual(httpErrors.filter((error) => error !== expectedFailureResponse), []);
  console.log(`BROWSER_ACCEPTANCE=PASS; / and /engineering.html share zh-CN Engineering Workbench; STEP bytes matched canonical API validation; failed 422 export produced no download; machining SVG=${svgBytes.length} DXF=${dxfBytes.length} STEP=${JSON.stringify(machiningStep)}; Revolve STEP=${JSON.stringify(revolveStep)}`);
} catch (error) {
  console.error("BROWSER_ACCEPTANCE_ERROR", error);
  console.error("BROWSER_PAGE_ERRORS", pageErrors);
  console.error("BROWSER_HTTP_ERRORS", httpErrors);
  throw error;
} finally {
  await browser.close();
}
