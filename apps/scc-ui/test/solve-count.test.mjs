import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";

// Test-only runtime observer. It is injected through the adapter double rather
// than stored in production global state, so every server session owns its own
// evidence and parallel tests cannot share or corrupt a solve counter.
function createSolveObserver() {
  const events = [];
  return {
    observe(operation) { events.push(operation); },
    count(operation) { return events.filter((entry) => entry === operation).length; },
    events,
  };
}

const BASE_MODEL = {
  part: { id: "p", name: "Plate" },
  features: {},
  parameters: { width: { value: { value: 120, unit: "mm" }, origin: "direct" } },
  constraints: {},
  feature_parameters: {},
  drawing: { properties: {} },
};

function clone(value) {
  return structuredClone(value);
}

function modelWithWidth(modelJson, width = 160) {
  const model = JSON.parse(modelJson);
  model.parameters.width.value.value = width;
  return model;
}

function payload(model) {
  const width = model.parameters.width.value.value;
  return {
    model: clone(model),
    proposal: { version: "snapshot-test" },
    geometry: { entities: { main: { primitive: { width } } } },
    workbench: { parameters: [{ id: "width", value: width, expression: null, origin: "direct" }] },
    source_map: { features: { main: "main" } },
    entity_to_feature: { main: "main" },
    diagnostics: [],
  };
}

function adapterWithObserver(observer) {
  return {
    version: async () => "scc 0.4.0-alpha.1",
    engineeringDslSolve: async () => {
      observer.observe("DSL commit");
      return payload(clone(BASE_MODEL));
    },
    engineeringSnapshot: async (modelJson) => {
      observer.observe("Workbench snapshot");
      return payload(JSON.parse(modelJson));
    },
    engineeringContext: async (modelJson) => ({ model: JSON.parse(modelJson), tools: [] }),
    engineeringManifest: async () => [{ name: "set_parameter" }],
    engineeringWorkbench: async (modelJson) => ({ parameters: [{ id: "width", value: JSON.parse(modelJson).parameters.width.value.value }] }),
    engineeringToolsApply: async (_calls, modelJson) => {
      observer.observe("Tool/NL solve");
      const model = modelWithWidth(modelJson);
      return payload(model);
    },
    engineeringDrawing: async () => {
      observer.observe("Principal cold");
      return { svg: "<svg data-kind=principal/>", sheet_width_mm: 120, sheet_height_mm: 80, scale: 1, part_title: "Plate" };
    },
    engineeringThreeViewDrawing: async () => {
      observer.observe("Three-view cold");
      return { svg: "<svg data-kind=three-view/>", sheet_width_mm: 120, sheet_height_mm: 80, scale: 1, part_title: "Plate", drawing_mode: "three_view", projection_method: "third_angle" };
    },
    engineeringStep: async () => {
      observer.observe("STEP export");
      const bytes = Buffer.from("ISO-10303-21;\nEND-ISO-10303-21;\n");
      return {
        bytes, sha256: createHash("sha256").update(bytes).digest("hex"), sizeBytes: bytes.length,
        status: { manufacturing: "passed", build: "passed", reimport: "passed", equivalence: "passed" },
      };
    },
  };
}

async function withServer(run, adapter, options = {}) {
  const server = createApp({ adapter, ...options });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  try {
    await run(`http://127.0.0.1:${port}`);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

const json = (method, body) => ({
  method,
  headers: { "content-type": "application/json" },
  body: JSON.stringify(body),
});

async function assertOk(response) {
  if (response.status !== 200) throw new Error(`${response.status}: ${await response.text()}`);
  return response;
}

test("runtime solve observer proves one solve per operation and zero for drawing cache hits", async () => {
  const observer = createSolveObserver();
  const provider = createFakeProvider(() => ({
    kind: "tool_calls",
    toolCalls: [{ name: "set_parameter", arguments: { target: "width", value: 160 } }],
  }));
  await withServer(async (base) => {
    await assertOk(await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: "AICAD 1" })));
    assert.equal(observer.count("DSL commit"), 1);

    await assertOk(await fetch(`${base}/api/engineering/tool`, json("POST", { model: BASE_MODEL, toolCall: { name: "set_parameter", arguments: { target: "width", value: 160 } } })));
    assert.equal(observer.count("Tool/NL solve"), 1);

    const preview = await assertOk(await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "edit", text: "change width", model: BASE_MODEL })));
    const previewPayload = await preview.json();
    assert.equal(previewPayload.status, "preview_ready");
    assert.equal(observer.count("Tool/NL solve"), 2, "NL preview has exactly one candidate solve");

    await assertOk(await fetch(`${base}/api/engineering/intent/confirm`, json("POST", { model: BASE_MODEL, proposal: previewPayload.proposal })));
    assert.equal(observer.count("Tool/NL solve"), 3, "NL confirm has exactly one committed solve");

    await assertOk(await fetch(`${base}/api/engineering/workbench`, json("POST", { model: BASE_MODEL })));
    assert.equal(observer.count("Workbench snapshot"), 1);

    const principal = (model) => fetch(`${base}/api/engineering/drawing`, json("POST", { model }));
    assert.equal((await (await principal(BASE_MODEL)).json()).cache_status, "miss");
    assert.equal((await (await principal(BASE_MODEL)).json()).cache_status, "hit");
    assert.equal(observer.count("Principal cold"), 1);

    const threeView = (model) => fetch(`${base}/api/engineering/drawing/three-view`, json("POST", { model }));
    assert.equal((await (await threeView(BASE_MODEL)).json()).cache_status, "miss");
    assert.equal((await (await threeView(BASE_MODEL)).json()).cache_status, "hit");
    assert.equal(observer.count("Three-view cold"), 1);

    const step = await assertOk(await fetch(`${base}/api/engineering/export/step`, json("POST", { model: BASE_MODEL, projectName: "plate" })));
    assert.equal(Number(step.headers.get("x-aicad-step-size")), Buffer.from("ISO-10303-21;\nEND-ISO-10303-21;\n").length);
    assert.equal(observer.count("STEP export"), 1);
  }, adapterWithObserver(observer), { engineeringProvider: provider });

  assert.deepEqual(observer.events, [
    "DSL commit",
    "Tool/NL solve",
    "Tool/NL solve",
    "Tool/NL solve",
    "Workbench snapshot",
    "Principal cold",
    "Three-view cold",
    "STEP export",
  ]);
});
