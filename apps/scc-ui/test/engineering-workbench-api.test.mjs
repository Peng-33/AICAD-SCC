import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { createApp } from "../server.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);

const FIXTURE_DSL = `AICAD 1
PART mounting_plate "Mounting Plate"
PARAM plate_width = 120
PARAM plate_height = 80
PARAM edge_offset = 15
BASE_RECT main
  WIDTH 120
  HEIGHT 80
BIND main WIDTH <- plate_width
HOLE_PATTERN mounting CORNER
  BASE main
  OFFSET_X 15
  OFFSET_Y 15
  DIA 10
BIND mounting OFFSET_X <- edge_offset
BIND mounting OFFSET_Y <- edge_offset
`;

const SPATIAL_FIXTURE_DSL = `AICAD 1
PART mounting_plate "Mounting Plate"
BASE_ROUNDED_RECT main
  CENTER 0 0
  WIDTH 120
  HEIGHT 80
  RADIUS 10
THICKNESS 10
HOLE center_hole
  CENTER 0 0
  DIA 20
EXTENT center_hole THRU
DRAWING PROJECTION_METHOD THIRD_ANGLE
`;

async function withServer(run, options = {}) {
  const adapter = new SccCliAdapter(binary);
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

test("engineering API: dsl → workbench + geometry + source map", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const response = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: FIXTURE_DSL }));
    assert.equal(response.status, 200);
    const payload = await response.json();
    assert.ok(payload.model.part);
    assert.equal(payload.revision, 1);
    assert.ok(payload.workbench.summary);
    assert.ok(payload.geometry.entities.main);
    assert.equal(payload.entity_to_feature.main, "main");
    // The UI gets real workbench data, not raw internals.
    assert.ok(payload.workbench.parameters.some((p) => p.id === "plate_width"));
    const workbench = await fetch(`${base}/api/engineering/workbench`, json("POST", { model: payload.model }));
    assert.equal(workbench.status, 200);
    const workbenchPayload = await workbench.json();
    assert.ok(workbenchPayload.geometry.entities.main, "snapshot route returns solved geometry");
    assert.equal(workbenchPayload.fingerprint, payload.fingerprint);
    assert.equal(workbenchPayload.revision, payload.revision);
  });
});

test("engineering API: tool call edits and re-solves", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const dsl = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: FIXTURE_DSL })).then((r) => r.json());
    const response = await fetch(
      `${base}/api/engineering/tool`,
      json("POST", { model: dsl.model, toolCall: { name: "set_parameter", arguments: { target: "plate_width", value: 160 } } }),
    );
    assert.equal(response.status, 200);
    const payload = await response.json();
    assert.equal(payload.model.parameters.plate_width.value.value, 160);
    assert.ok(payload.geometry.entities.main.primitive.width === 160, "geometry re-solved after edit");
  });
});

test("engineering API: invalid tool call fails atomically", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const dsl = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: FIXTURE_DSL })).then((r) => r.json());
    const before = JSON.stringify(dsl.model);
    const response = await fetch(
      `${base}/api/engineering/tool`,
      json("POST", { model: dsl.model, toolCall: { name: "set_parameter", arguments: { target: "missing_param", value: 1 } } }),
    );
    assert.ok(response.status >= 400, "invalid tool must fail");
    // Model snapshot the client holds is unchanged (backend apply was atomic).
    assert.equal(JSON.stringify(dsl.model), before);
  });
});

test("engineering API: multiline new-model intent is preserved and returns a spatially ready model", { skip: !binary }, async () => {
  const text = "创建一块 120 × 80、厚 10 的圆角安装板。\n\n中心创建一个 Φ20 贯穿孔。\n生成主视图、俯视图和左视图。";
  const provider = createFakeProvider((request) => {
    assert.equal(request.mode, "new");
    assert.equal(request.userText, text);
    return { kind: "text", text: SPATIAL_FIXTURE_DSL };
  });
  await withServer(async (base) => {
    const response = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "new", text }));
    assert.equal(response.status, 200);
    const payload = await response.json();
    assert.equal(payload.status, "new_ready");
    assert.equal(payload.model.spatial.thickness.value, 10);
    assert.equal(payload.model.spatial.hole_extents.center_hole.type, "through_all");

    const drawing = await fetch(
      `${base}/api/engineering/drawing/three-view`,
      json("POST", { model: payload.model }),
    );
    assert.equal(drawing.status, 200);
    const threeView = await drawing.json();
    assert.equal(threeView.projection_method, "third_angle");
    assert.match(threeView.svg, /id="view_front"/);
    assert.match(threeView.svg, /id="view_top"/);
    assert.match(threeView.svg, /id="view_left"/);
  }, { engineeringProvider: provider });
});

test("engineering API: runtime OpenAI-compatible configuration drives Engineering Intent", { skip: !binary }, async () => {
  let upstreamCalls = 0;
  const fetchImpl = async () => {
    upstreamCalls += 1;
    return {
      ok: true,
      status: 200,
      json: async () => ({ choices: [{ message: { role: "assistant", content: SPATIAL_FIXTURE_DSL } }] }),
    };
  };
  await withServer(async (base) => {
    const configured = await fetch(`${base}/api/intent-config`, json("PATCH", {
      provider: "openai-compatible",
      endpoint: "https://models.example.test/v1",
      model: "fixture-model",
    }));
    assert.equal(configured.status, 200);
    assert.equal((await configured.json()).configured, true);

    const response = await fetch(
      `${base}/api/engineering/intent`,
      json("POST", { mode: "new", text: "创建一块安装板" }),
    );
    assert.equal(response.status, 200);
    assert.equal((await response.json()).status, "new_ready");
    assert.equal(upstreamCalls, 1);
  }, { fetchImpl });
});

test("engineering API: intent (edit) previews, then confirms one atomic model change", { skip: !binary }, async () => {
  const provider = createFakeProvider(() => ({
    kind: "tool_calls",
    toolCalls: [{ name: "set_parameter", arguments: { target: "plate_width", value: 200 } }],
  }));
  await withServer(async (base) => {
    const dsl = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: FIXTURE_DSL })).then((r) => r.json());
    const response = await fetch(
      `${base}/api/engineering/intent`,
      json("POST", { mode: "edit", text: "把板宽改成200", model: dsl.model }),
    );
    assert.equal(response.status, 200);
    const payload = await response.json();
    assert.equal(payload.status, "preview_ready");
    assert.equal(payload.revision, 1);
    assert.equal(Number(payload.proposal.parameter_changes.find((change) => change.id === "plate_width").after.resolved_value), 200);
    assert.notEqual(payload.proposal.base_model_fingerprint, payload.proposal.candidate_fingerprint);
    const before = JSON.stringify(dsl.model);
    const confirm = await fetch(
      `${base}/api/engineering/intent/confirm`,
      json("POST", { model: dsl.model, proposal: payload.proposal }),
    );
    assert.equal(confirm.status, 200);
    const committed = await confirm.json();
    assert.equal(committed.status, "committed");
    assert.equal(committed.revision, 2);
    assert.equal(committed.fingerprint, payload.proposal.candidate_fingerprint, "committed fingerprint equals preview candidate");
    assert.equal(committed.model.parameters.plate_width.value.value, 200);
    assert.equal(JSON.stringify(dsl.model), before, "preview did not mutate the caller model");
  }, { engineeringProvider: provider });
});

test("engineering API: clarification is typed, resumable, and bound to the original fingerprint", { skip: !binary }, async () => {
  const provider = createFakeProvider((_request, call) => call === 1
    ? { kind: "clarification", reason: "Select the feature and diameter", fields: [
      { key: "feature", kind: "feature_reference", prompt: "Feature", required: true },
      { key: "diameter", kind: "length", prompt: "Diameter", required: true },
    ] }
    : { kind: "tool_calls", toolCalls: [{ name: "set_parameter", arguments: { target: "plate_width", value: 210 } }] });
  await withServer(async (base) => {
    const dsl = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: FIXTURE_DSL })).then((r) => r.json());
    const first = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "edit", text: "把那个孔改大", model: dsl.model }));
    assert.equal(first.status, 200);
    const clarification = await first.json();
    assert.equal(clarification.status, "needs_clarification");
    assert.equal(clarification.fields[0].kind, "feature_reference");
    assert.equal(clarification.fields[1].kind, "length");
    const resumed = await fetch(`${base}/api/engineering/intent`, json("POST", {
      mode: "edit", text: clarification.original_intent, model: dsl.model,
      baseFingerprint: clarification.base_model_fingerprint,
      clarification: {
        request_id: clarification.request_id,
        base_model_fingerprint: clarification.base_model_fingerprint,
        answers: { feature: "mounting", diameter: "12" },
      },
    }));
    assert.equal(resumed.status, 200);
    const preview = await resumed.json();
    assert.equal(preview.status, "preview_ready");
    assert.equal(provider.calls, 2);
  }, { engineeringProvider: provider });
});

test("engineering API: stale confirmation is rejected and cannot invoke the provider", { skip: !binary }, async () => {
  const provider = createFakeProvider(() => ({
    kind: "tool_calls",
    toolCalls: [{ name: "set_parameter", arguments: { target: "plate_width", value: 220 } }],
  }));
  await withServer(async (base) => {
    const dsl = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: FIXTURE_DSL })).then((r) => r.json());
    const preview = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "edit", text: "改宽", model: dsl.model })).then((r) => r.json());
    const changed = structuredClone(dsl.model);
    changed.parameters.plate_width.value.value = 121;
    const response = await fetch(`${base}/api/engineering/intent/confirm`, json("POST", { model: changed, proposal: preview.proposal }));
    assert.equal(response.status, 409);
    const payload = await response.json();
    assert.equal(payload.error.code, "SCC-UI-INTENT-STALE-001");
    assert.equal(provider.calls, 1, "confirm must not call the provider");
  }, { engineeringProvider: provider });
});

test("engineering API: non-formal command fields are rejected at confirmation", { skip: !binary }, async () => {
  const provider = createFakeProvider(() => ({
    kind: "tool_calls",
    toolCalls: [{ name: "set_parameter", arguments: { target: "plate_width", value: 230 } }],
  }));
  await withServer(async (base) => {
    const dsl = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: FIXTURE_DSL })).then((r) => r.json());
    const preview = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "edit", text: "改宽", model: dsl.model })).then((r) => r.json());
    const tampered = structuredClone(preview.proposal);
    tampered.plan.commands[0].shell = "echo unsafe";
    const response = await fetch(`${base}/api/engineering/intent/confirm`, json("POST", { model: dsl.model, proposal: tampered }));
    assert.equal(response.status, 400);
    const payload = await response.json();
    assert.equal(payload.error.code, "SCC-UI-INTENT-PREVIEW-001");
  }, { engineeringProvider: provider });
});

test("engineering API: intent without a provider is a clear 501", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const response = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "edit", text: "x" }));
    assert.equal(response.status, 501);
    const payload = await response.json();
    assert.equal(payload.error.code, "SCC-UI-INTENT-001");
    assert.equal(payload.error.details.failure_class, "PROVIDER_NOT_CONFIGURED");
  });
});
