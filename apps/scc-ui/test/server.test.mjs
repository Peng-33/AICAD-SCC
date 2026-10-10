import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import test from "node:test";
import { createApp } from "../server.mjs";
import { ServiceError } from "../lib/scc-cli.mjs";

const json = (method, body) => ({
  method,
  headers: { "content-type": "application/json" },
  body: JSON.stringify(body),
});

function fakeAdapter() {
  return {
    version: async () => "scc 0.4.0-alpha.1",
    compile: async (proposal) => ({ outcome: "compiled", ir: { entities: proposal.entities, constraints: [] } }),
    execute: async () => ({ state: { root: 1, step: 1, entities: {}, constraints: {} }, trace: { root: 1, events: [] } }),
    inspect: async () => ({ entity_count: 0 }),
    solve: async () => ({ source_root: 1, source_step: 1, entities: {} }),
    export: async (_geometry, format) => format === "svg" ? "<svg></svg>" : "0\nEOF\n",
  };
}

async function withServer(run, options = {}) {
  const server = createApp({ adapter: fakeAdapter(), ...options });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  try {
    await run(`http://127.0.0.1:${port}`);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

test("serves health and static application shell", async () => {
  await withServer(async (base) => {
    const health = await fetch(`${base}/api/health`).then((response) => response.json());
    assert.deepEqual(health, { status: "ready", version: "scc 0.4.0-alpha.1", intentConfigured: false });
    const root = await fetch(base, { redirect: "manual" });
    assert.equal(root.status, 302);
    assert.equal(root.headers.get("location"), "/engineering.html");
    const page = await fetch(`${base}/engineering.html`).then((response) => response.text());
    assert.match(page, /AICAD Engineering Workbench/);
    assert.match(page, /简体中文/);
    assert.doesNotMatch(page, /proposal-editor|Semantic Constraint Compiler/);
    const app = await fetch(`${base}/js/engineering-page.js`).then((response) => response.text());
    assert.match(app, /engineeringIntentPreview/);
    const style = await fetch(`${base}/css/engineering.css`).then((response) => response.text());
    assert.match(style, /model-dialog/);
  });
});

test("configured intent endpoint returns a structured Proposal", async () => {
  let validatedProposal;
  let validatedMutation;
  let solvedState;
  const adapter = fakeAdapter();
  adapter.compile = async (proposal) => {
    validatedProposal = proposal;
    return { outcome: "compiled", ir: { entities: proposal.entities, constraints: [] } };
  };
  adapter.execute = async (_ir, mutation) => {
    validatedMutation = mutation;
    return { state: { root: 1, step: 1, entities: {}, constraints: {} }, trace: { root: 1, events: [] } };
  };
  adapter.solve = async (state) => {
    solvedState = state;
    return { source_root: 1, source_step: 1, entities: {} };
  };
  const intentServer = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({
      proposal: { version: "1.0", entities: [{ id: "intent-point", kind: "point" }] },
      summary: body.intent,
      mutation: [{ UpdateParameter: { target_id: "intent-point", parameter: "x", value: 0 } }],
    }));
  });
  await new Promise((resolve) => intentServer.listen(0, "127.0.0.1", resolve));
  const endpoint = `http://127.0.0.1:${intentServer.address().port}`;
  try {
    await withServer(async (base) => {
      const response = await fetch(`${base}/api/intent`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ intent: "Create a point", documents: [] }),
      });
      assert.equal(response.status, 200);
      const result = await response.json();
      assert.equal(result.proposal.entities[0].id, "intent-point");
      assert.equal(result.summary, "Create a point");
      assert.equal(validatedProposal.entities[0].kind, "point");
      assert.equal(validatedMutation[0].UpdateParameter.parameter, "x");
      assert.equal(solvedState.root, 1);
    }, { adapter, intentEndpoint: endpoint });
  } finally {
    await new Promise((resolve) => intentServer.close(resolve));
  }
});

test("intent configuration can be changed at runtime without exposing secrets", async () => {
  await withServer(async (base) => {
    const initial = await fetch(`${base}/api/intent-config`).then((response) => response.json());
    assert.equal(initial.provider, "disabled");
    assert.equal(initial.configured, false);

    const response = await fetch(`${base}/api/intent-config`, {
      method: "PATCH",
      headers: { "content-type": "application/json", origin: base },
      body: JSON.stringify({
        provider: "openai-compatible",
        endpoint: "https://models.example.test/v1",
        model: "design-model",
        apiKey: "never-return-this",
      }),
    });
    assert.equal(response.status, 200);
    const updated = await response.json();
    assert.equal(updated.configured, true);
    assert.equal(updated.hasApiKey, true);
    assert.equal(JSON.stringify(updated).includes("never-return-this"), false);

    const health = await fetch(`${base}/api/health`).then((item) => item.json());
    assert.equal(health.intentConfigured, true);
    const current = await fetch(`${base}/api/intent-config`).then((item) => item.json());
    assert.equal(JSON.stringify(current).includes("never-return-this"), false);
  }, { intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
});

test("intent configuration rejects cross-origin browser writes", async () => {
  await withServer(async (base) => {
    const response = await fetch(`${base}/api/intent-config`, {
      method: "PATCH",
      headers: { "content-type": "application/json", origin: "https://malicious.example" },
      body: JSON.stringify({ provider: "disabled" }),
    });
    assert.equal(response.status, 403);
    assert.equal((await response.json()).error.code, "SCC-UI-INTENT-CONFIG-009");
  }, { intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
});

test("compile route passes structured data to the adapter", async () => {
  await withServer(async (base) => {
    const response = await fetch(`${base}/api/compile`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ proposal: { entities: [{ id: "p", kind: "point" }] } }),
    });
    assert.equal(response.status, 200);
    const body = await response.json();
    assert.equal(body.outcome, "compiled");
    assert.equal(body.ir.entities[0].id, "p");
  });
});

test("intent route has a meaningful unconfigured state", async () => {
  await withServer(async (base) => {
    const response = await fetch(`${base}/api/intent`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ text: "Create a plate" }),
    });
    assert.equal(response.status, 501);
    const body = await response.json();
    assert.equal(body.error.code, "SCC-UI-INTENT-001");
  });
});

test("CAD routes build non-STEP artifacts and reject compatibility STEP delivery", async () => {
  let deletedJob;
  const cadService = {
    build: async (cadSpec) => ({
      job_id: "00000000-0000-4000-8000-000000000001",
      cad_spec: cadSpec,
      manifest: { status: "built", solid_valid: true, artifacts: { brep: { filename: "housing.brep" } } },
      preview_svg: "<svg></svg>",
    }),
    artifact: async (_jobId, name) => {
      if (name === "step") throw new ServiceError(400, "SCC-CAD-010", "STEP export is not supported for CadDocument compatibility builds; use EngineeringModel export");
      return { filename: `housing.${name}`, contentType: "application/octet-stream", content: Buffer.from("BRep") };
    },
    delete: async (jobId) => { deletedJob = jobId; return true; },
  };
  await withServer(async (base) => {
    const build = await fetch(`${base}/api/cad/build`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ cadSpec: { kind: "precision_gearbox_housing_v1" } }),
    });
    assert.equal(build.status, 200);
    const buildPayload = await build.json();
    assert.match(buildPayload.preview_svg, /^<svg/);
    assert.equal("step" in buildPayload.manifest.artifacts, false);

    const rejectedStep = await fetch(`${base}/api/cad/artifacts/00000000-0000-4000-8000-000000000001/step`);
    assert.equal(rejectedStep.status, 400);
    assert.equal((await rejectedStep.json()).error.code, "SCC-CAD-010");

    const retained = await fetch(`${base}/api/cad/artifacts/00000000-0000-4000-8000-000000000001/brep`);
    assert.equal(retained.status, 200);
    assert.equal(retained.headers.get("content-type"), "application/octet-stream");
    assert.equal(await retained.text(), "BRep");

    const removed = await fetch(`${base}/api/cad/artifacts/00000000-0000-4000-8000-000000000001`, { method: "DELETE" });
    assert.equal(removed.status, 200);
    assert.equal(deletedJob, "00000000-0000-4000-8000-000000000001");
  }, { cadService });
});

test("CAD intent route returns a normalized three-dimensional contract", async () => {
  const cadSpec = {
    version: "1.0",
    kind: "precision_gearbox_housing_v1",
    units: "mm",
    parameters: {
      envelope: { base_length: 260, base_width: 190, total_height: 165, base_thickness: 18, body_length: 220, body_width: 150, body_height: 147 },
      shell: { wall_thickness: 10, top_thickness: 12, bottom_reinforcement: 16, corner_radius: 18, body_radius: 8, inner_length: 190, inner_width: 120, inner_height: 118 },
      axes: {
        main: { x: 0, z: 92, through_diameter: 72 },
        second: { x: 78, z: 92, through_diameter: 48, center_distance: 78 },
        third: { z: 48, through_diameter: 35, center_distance: 62, horizontal_side: "right" },
      },
    },
    manufacturing: { fits: [], tolerances: [], datums: [], geometric_controls: [], surface_finishes: [], threads: [] },
    assumptions: [],
    unresolved: [],
  };
  const intentServer = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ cadSpec, summary: "Normalized gearbox" }));
  });
  await new Promise((resolve) => intentServer.listen(0, "127.0.0.1", resolve));
  try {
    await withServer(async (base) => {
      const response = await fetch(`${base}/api/cad/intent`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ intent: "生成减速箱壳体", documents: [] }),
      });
      assert.equal(response.status, 200);
      const result = await response.json();
      assert.equal(result.cadSpec.parameters.axes.third.x, Math.sqrt((62 ** 2) - (44 ** 2)));
      assert.match(result.cadSpec.source_hash, /^[a-f0-9]{64}$/);
    }, { intentEndpoint: `http://127.0.0.1:${intentServer.address().port}` });
  } finally {
    await new Promise((resolve) => intentServer.close(resolve));
  }
});


test("engineering project persistence and STEP delivery use source-only input and exact bytes", async () => {
  const model = { part: { id: "p", name: "Plate" }, features: {}, parameters: {}, constraints: {}, feature_parameters: {}, drawing: { properties: {} } };
  const bytes = Buffer.from("ISO-10303-21;\nEND-ISO-10303-21;\n");
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  const adapter = fakeAdapter();
  adapter.engineeringProjectSave = async (_model, name) => JSON.stringify({ schema_version: 1, metadata: { name }, engineering: model });
  adapter.engineeringProjectOpen = async (project) => project;
  adapter.engineeringStep = async () => ({
    bytes, sha256, sizeBytes: bytes.length,
    status: { manufacturing: "passed", build: "passed", reimport: "passed", equivalence: "passed" },
  });
  await withServer(async (base) => {
    const saved = await fetch(`${base}/api/engineering/project/save`, json("POST", { model, name: "Installation Plate" }));
    assert.equal(saved.status, 200);
    const savedPayload = await saved.json();
    assert.equal(savedPayload.project.schema_version, 1);
    assert.deepEqual(savedPayload.model, model);
    assert.equal(savedPayload.revision, 0, "save is not a committed transition");
    assert.equal("geometry" in savedPayload.project, false, "project remains source-only");
    assert.equal("revision" in savedPayload.project, false, "revision is session metadata, not project data");

    const opened = await fetch(`${base}/api/engineering/project/open`, json("POST", { project: savedPayload.project }));
    assert.equal(opened.status, 200);
    const openedPayload = await opened.json();
    assert.deepEqual(openedPayload.model, model);
    assert.equal(openedPayload.revision, 1, "open creates a new session revision");

    const delivered = await fetch(`${base}/api/engineering/export/step`, json("POST", { model, projectName: "Installation Plate" }));
    assert.equal(delivered.status, 200);
    assert.equal(delivered.headers.get("x-aicad-step-sha256"), sha256);
    assert.match(delivered.headers.get("x-aicad-model-fingerprint"), /^[a-f0-9]{64}$/);
    assert.equal(Buffer.from(await delivered.arrayBuffer()).toString(), bytes.toString());
    assert.match(delivered.headers.get("content-disposition"), /Installation-Plate\.step/);

    const rejected = await fetch(`${base}/api/engineering/export/step`, json("POST", { model: { ...model, solid_build_spec: {} } }));
    assert.equal(rejected.status, 400);
  }, { adapter });
});
