import { chromium } from "playwright";
import { mkdir, readFile, writeFile } from "node:fs/promises";

const baseUrl = process.env.AICAD_BASE_URL ?? "http://127.0.0.1:4173";
const evidenceRoot = "/tests/evidence";
const input = await readFile(`${evidenceRoot}/mmf204-input.txt`, "utf8");
const simpleInput = `创建一块 120 mm × 120 mm、厚度 10 mm、\n四角 R15 的矩形安装板。\n\n在板中心创建一个 Φ38 贯穿孔。\n\n生成主视图、俯视图和左视图。`;
const events = [];
const browserErrors = [];

await mkdir(evidenceRoot, { recursive: true });

function attach(page, name) {
  page.on("console", (message) => events.push({ page: name, type: "console", level: message.type(), text: message.text() }));
  page.on("pageerror", (error) => browserErrors.push({ page: name, type: "pageerror", text: error.message }));
  page.on("requestfailed", (request) => events.push({ page: name, type: "requestfailed", url: request.url(), error: request.failure()?.errorText ?? "unknown" }));
  page.on("response", (response) => {
    if (response.url().includes("/api/")) events.push({ page: name, type: "response", status: response.status(), url: response.url() });
  });
}

async function capture(page, name) {
  await page.screenshot({ path: `${evidenceRoot}/${name}.png`, fullPage: true });
  await writeFile(`${evidenceRoot}/${name}.txt`, await page.locator("body").innerText());
}

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1024 }, acceptDownloads: true });
  attach(page, "main");
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await capture(page, "00-main-initial");
  if (await page.locator("#welcome-dialog[open]").count()) await page.locator("#start-blank").click();
  if (await page.locator('[data-locale="zh-CN"]').count()) await page.locator('[data-locale="zh-CN"]').click();
  await page.locator("#service-status").waitFor();

  const fullInput = page.locator("#intent-input");
  await fullInput.fill(input);
  const fullAccepted = (await fullInput.inputValue()) === input;
  await capture(page, "01-main-full-input-filled");
  await page.locator("#intent-form .send-button").click();
  await page.waitForTimeout(2500);
  await capture(page, "02-main-full-input-result");
  const fullStage = await page.locator('[data-stage="intent"]').innerText();
  const fullBody = await page.locator("body").innerText();

  const simple = page.locator("#intent-input");
  await simple.fill(simpleInput);
  const simpleAccepted = (await simple.inputValue()) === simpleInput;
  await page.locator("#intent-form .send-button").click();
  await page.waitForTimeout(2500);
  await capture(page, "04-main-simple-control-result");
  const simpleStage = await page.locator('[data-stage="intent"]').innerText();
  const simpleBody = await page.locator("body").innerText();

  const engineering = await browser.newPage({ viewport: { width: 1440, height: 1024 }, acceptDownloads: true });
  attach(engineering, "engineering");
  await engineering.goto(`${baseUrl}/engineering.html`, { waitUntil: "networkidle" });
  await capture(engineering, "05-engineering-workbench-initial");
  await engineering.locator("select.wb-drawing-mode").selectOption("three-view");
  await engineering.waitForTimeout(1500);
  await capture(engineering, "06-engineering-fixture-three-view");
  const fixtureDrawing = {
    imageCount: await engineering.locator(".wb-drawing-img").count(),
    drawingMeta: await engineering.locator(".wb-drawing-meta").innerText().catch(() => ""),
    status: await engineering.locator("#status").innerText(),
    bodyExcerpt: (await engineering.locator("body").innerText()).slice(0, 4000),
  };

  const engineeringInput = engineering.locator("input.wb-nl");
  await engineeringInput.fill(input);
  const engineeringAccepted = (await engineeringInput.inputValue()) === input;
  await engineering.locator(".wb-topbar button", { hasText: "Apply" }).click();
  await engineering.waitForTimeout(2500);
  await capture(engineering, "07-engineering-full-input-result");
  const workflow = await engineering.locator("#intent-workflow").innerText().catch(() => "");
  const engineeringBody = await engineering.locator("body").innerText();

  const result = {
    baseUrl,
    pages: {
      main: {
        fullInputAccepted: fullAccepted,
        fullStage,
        fullBody,
        simpleInputAccepted: simpleAccepted,
        simpleStage,
        simpleBody,
      },
      engineeringWorkbench: {
        fullInputAccepted: engineeringAccepted,
        workflow,
        fixtureDrawing,
        body: engineeringBody,
      },
    },
    browserErrors,
    events,
    evidenceFiles: [
      "00-main-initial.png", "00-main-initial.txt",
      "01-main-full-input-filled.png", "01-main-full-input-filled.txt",
      "02-main-full-input-result.png", "02-main-full-input-result.txt",
      "04-main-simple-control-result.png", "04-main-simple-control-result.txt",
      "05-engineering-workbench-initial.png", "05-engineering-workbench-initial.txt",
      "06-engineering-fixture-three-view.png", "06-engineering-fixture-three-view.txt",
      "07-engineering-full-input-result.png", "07-engineering-full-input-result.txt",
    ],
  };
  await writeFile(`${evidenceRoot}/browser-result.json`, JSON.stringify(result, null, 2));
} finally {
  await browser.close();
}
