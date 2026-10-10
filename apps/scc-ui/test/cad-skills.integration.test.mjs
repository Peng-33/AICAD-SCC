import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
const PLATE_DSL = `AICAD 1
PART mounting_plate "Mounting Plate"
BASE_RECT main
  WIDTH 120
  HEIGHT 80
THICKNESS 10
DRAWING PROJECTION_METHOD THIRD_ANGLE
`;
const ACCEPTANCE_TEXT = "创建一块长120 mm、宽120 mm、厚10 mm、四角R15的安装板，在中心创建Φ38贯穿孔，生成第三角投影主视图、俯视图和左视图。";
const ACCEPTANCE_DSL = `AICAD 1
PART mounting_plate "Mounting Plate"
BASE_ROUNDED_RECT main
  CENTER 0 0
  WIDTH 120
  HEIGHT 120
  RADIUS 15
THICKNESS 10
HOLE center_hole
  CENTER 0 0
  DIA 38
EXTENT center_hole THRU
DRAWING PROJECTION_METHOD THIRD_ANGLE
`;
const json = (method, body) => ({ method, headers: { "content-type": "application/json" }, body: JSON.stringify(body) });

async function withServer(run, options = {}) {
  const server = createApp({ adapter: new SccCliAdapter(binary), ...options });
  await new Promise((resolveListen) => server.listen(0, "127.0.0.1", resolveListen));
  const base = `http://127.0.0.1:${server.address().port}`;
  try { await run(base); } finally { await new Promise((resolveClose) => server.close(resolveClose)); }
}

test("A: real Engineering Intent with fake provider validates commands, produces three views, annotations, SVG and DXF", { skip: !binary }, async () => {
  const provider = createFakeProvider((request) => {
    assert.equal(request.mode, "new");
    assert.equal(request.userText, ACCEPTANCE_TEXT);
    return { kind: "text", text: ACCEPTANCE_DSL };
  });
  await withServer(async (base) => {
    const skills = await fetch(`${base}/api/cad-skills`).then((response) => response.json());
    assert.equal(skills.skills.length, 6);
    const planned = await fetch(`${base}/api/engineering/skills/plan`, json("POST", { mode: "new", text: ACCEPTANCE_TEXT })).then((response) => response.json());
    assert.deepEqual(planned.plan.selected_skill_ids.slice(0, 2), ["mechanical-part-analysis", "engineering-model-planning"]);
    assert.ok(planned.plan.request_analysis.explicit.some((item) => item.kind === "thickness" && item.value_mm === 10));
    assert.ok(planned.plan.request_analysis.explicit.some((item) => item.kind === "diameter" && item.value_mm === 38));
    assert.equal(planned.plan.tool_manifest_count, 37);

    const generated = await fetch(`${base}/api/engineering/intent`, json("POST", {
      mode: "new", text: ACCEPTANCE_TEXT, skillPlanId: planned.plan_id,
    })).then((response) => response.json());
    assert.equal(generated.status, "new_ready");
    assert.ok(generated.skill_run.engineering_commands.some((command) => command.op === "create_part"));
    assert.ok(generated.skill_run.engineering_commands.length > 2);
    assert.equal(provider.calls, 1);

    const drawingResponse = await fetch(`${base}/api/engineering/drawing/three-view`, json("POST", { model: generated.model, userIntent: ACCEPTANCE_TEXT }));
    assert.equal(drawingResponse.status, 200);
    const drawing = await drawingResponse.json();
    for (const view of ["front", "top", "left"]) assert.match(drawing.svg, new RegExp(`id="view_${view}"`));
    assert.equal(drawing.drawing_report.source, "same_formal_drawing_model");
    assert.equal(drawing.drawing_report.view_count, 3);
    assert.ok(drawing.drawing_report.views.some((view) => view.dimensions.some((dimension) => dimension.value_mm.includes(38))));
    assert.ok(drawing.drawing_report.views.some((view) => view.center_geometry.some((element) => element.kind === "centerline")));
    assert.equal(drawing.skills_report.status, "PASS");
    assert.equal(drawing.skills_report.SVG_EXPORT.status, "PASS");
    assert.equal(drawing.skills_report.DXF_EXPORT.status, "PASS");
    assert.equal(drawing.skills_report.DXF_EXPORT.units, "millimeter");
    assert.ok(drawing.skills_report.DXF_EXPORT.entity_count > 0);
    assert.equal(drawing.skill_run.skill_states.find((item) => item.id === "orthographic-drawing").state, "COMPLETE");
  }, { engineeringProvider: provider });
});

test("B: an existing part edit cannot call create_part and leaves its EngineeringModel unchanged", { skip: !binary }, async () => {
  let sawCreatePart = false;
  const provider = createFakeProvider((request) => {
    assert.ok(request.context.part);
    sawCreatePart = request.tools.some((tool) => tool.name === "create_part");
    return { kind: "tool_calls", toolCalls: [{ name: "create_part", arguments: { id: "mounting_plate", name: "Overwrite" } }] };
  });
  await withServer(async (base) => {
    const created = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: PLATE_DSL })).then((response) => response.json());
    const before = JSON.stringify(created.model);
    const planned = await fetch(`${base}/api/engineering/skills/plan`, json("POST", { mode: "edit", text: "修改现有安装板", model: created.model })).then((response) => response.json());
    assert.equal(planned.plan.model_state.part_exists, true);
    assert.equal(planned.plan.allowed_tool_names.includes("create_part"), false);
    const response = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "edit", text: "修改现有安装板", model: created.model, skillPlanId: planned.plan_id }));
    assert.equal(response.status, 200);
    const result = await response.json();
    assert.equal(result.status, "rejected");
    assert.equal(result.error.code, "SCC-UI-INTENT-017");
    assert.equal(sawCreatePart, false);
    assert.equal(JSON.stringify(created.model), before);
    assert.equal(result.skill_run.diagnostic.mutated, false);
    assert.equal(result.skill_run.skill_states.find((item) => item.id === "engineering-diagnostics").state, "COMPLETE");
  }, { engineeringProvider: provider });
});

test("B: a CAD Skill plan is single-use and stale model snapshots are rejected", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const planned = await fetch(`${base}/api/engineering/skills/plan`, json("POST", { mode: "new", text: "创建 120×80×10 mm 安装板" })).then((response) => response.json());
    const changedPlan = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "new", text: "创建 120×80×10 mm 安装板", skillPlanId: planned.plan_id }));
    assert.equal(changedPlan.status, 501, "plan is consumed before provider availability is checked");
    const replay = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "new", text: "创建 120×80×10 mm 安装板", skillPlanId: planned.plan_id }));
    assert.equal(replay.status, 409);
    assert.equal((await replay.json()).error.code, "SCC-UI-SKILL-001");
  });
});

test("C: missing BaseProfile reports actual SCC-SPAT-002, no drawing artifacts, no recovery mutation", { skip: !binary }, async () => {
  const fixture = JSON.parse(await readFile(resolve(repoRoot, "apps/scc-ui/test/fixtures/three-view-plate.json"), "utf8"));
  delete fixture.features.plate;
  await withServer(async (base) => {
    const response = await fetch(`${base}/api/engineering/drawing/three-view`, json("POST", { model: fixture }));
    assert.equal(response.status, 422);
    const result = await response.json();
    assert.equal(result.error.code, "SCC-SPAT-002");
    assert.equal(result.error.details.diagnostic.stage, "spatial");
    assert.equal(result.error.details.diagnostic.model_facts.base_profile_exists, false);
    assert.equal(Object.hasOwn(result, "svg"), false);
    assert.equal(result.error.details.diagnostic.mutated, false);
  });
});

test("D: MMF-204 full request is analyzed but never sent to a paid provider or marked complete", async () => {
  const text = await readFile(resolve(repoRoot, "apps/scc-ui/test/fixtures/mmf204-input.txt"), "utf8");
  const provider = createFakeProvider(() => { throw new Error("MMF-204 provider call must not run in analysis-only gate"); });
  const { analyzeMechanicalRequest } = await import("../lib/cad-skills/analysis.mjs");
  const analysis = analyzeMechanicalRequest({ text });
  assert.ok(analysis.unsupported.length >= 8);
  assert.equal(provider.calls, 0);
  assert.equal("generation_status" in analysis, false);
});

test("E: formal dimensions, hole centerlines, numeric tolerance, and unsupported roughness have distinct evidence", { skip: !binary }, async () => {
  const provider = createFakeProvider(() => ({ kind: "text", text: ACCEPTANCE_DSL }));
  await withServer(async (base) => {
    const generated = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "new", text: ACCEPTANCE_TEXT })).then((response) => response.json());
    const drawing = await fetch(`${base}/api/engineering/drawing/three-view`, json("POST", { model: generated.model, userIntent: "Φ38 孔需要H7，并要求表面粗糙度 Ra 1.6。" })).then((response) => response.json());
    const annotation = drawing.skills_report.DRAWING_ANNOTATION;
    assert.ok(annotation.dimensions.some((dimension) => /diameter|hole_callout/i.test(JSON.stringify(dimension.kind))));
    assert.ok(annotation.center_geometry.some((element) => element.kind === "centerline"));
    assert.equal(annotation.capabilities.numeric_tolerances, "NOT_RUN");
    assert.equal(annotation.capabilities.surface_roughness, "UNSUPPORTED");
    assert.equal(drawing.skills_report.requested_coverage.find((item) => item.requirement === "surface_roughness").status, "UNSUPPORTED");
    assert.equal(drawing.skills_report.requested_coverage.find((item) => item.requirement === "fit_tolerance_class").status, "UNSUPPORTED");
  }, { engineeringProvider: provider });
});

test("F: corrupt DXF fails its own check while the unchanged formal SVG still passes", { skip: !binary }, async () => {
  const provider = createFakeProvider(() => ({ kind: "text", text: ACCEPTANCE_DSL }));
  await withServer(async (base) => {
    const generated = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "new", text: ACCEPTANCE_TEXT })).then((response) => response.json());
    const drawing = await fetch(`${base}/api/engineering/drawing/three-view`, json("POST", { model: generated.model })).then((response) => response.json());
    const { validateThreeViewArtifacts } = await import("../lib/cad-skills/export-validation.mjs");
    const result = validateThreeViewArtifacts({ drawing: { ...drawing, dxf: "0\nEOF\n" }, modelFingerprint: drawing.model_fingerprint });
    assert.equal(result.SVG_EXPORT.status, "PASS");
    assert.equal(result.DXF_EXPORT.status, "FAIL");
    assert.equal(result.DRAWING_CONSISTENCY.status, "PASS");
    assert.equal(result.status, "FAIL");
  }, { engineeringProvider: provider });
});
