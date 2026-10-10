import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import { runIntent } from "../lib/engineering-intent.mjs";
import { createFakeProvider, createScriptedProvider } from "../lib/provider-fake.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
const adapter = binary ? new SccCliAdapter(binary) : null;
const parametricIntegrationEnabled = process.env.AICAD_RUST_PARAMETRIC_INTEGRATION === "1";

const PLATE_DSL = `AICAD 1
PART mounting_plate "Mounting Plate"
PARAM plate_width = 120
PARAM plate_height = 80
PARAM edge_offset = 15
BASE_RECT main
  WIDTH 120
  HEIGHT 80
BIND main WIDTH <- plate_width
BIND main HEIGHT <- plate_height
HOLE_PATTERN mounting CORNER
  BASE main
  OFFSET_X 15
  OFFSET_Y 15
  DIA 10
BIND mounting OFFSET_X <- edge_offset
BIND mounting OFFSET_Y <- edge_offset
`;

function circleCenters(geometry) {
  const centers = [];
  for (const entity of Object.values(geometry.entities ?? {})) {
    if (entity.primitive.kind === "circle") centers.push(entity.primitive.center);
  }
  centers.sort((a, b) => a.x - b.x || a.y - b.y);
  return centers;
}

test("new-design vertical slice: rectangle + four corner holes", { skip: !binary }, async () => {
  const provider = createScriptedProvider([{ kind: "text", text: PLATE_DSL }]);
  const response = await runIntent({
    mode: "new",
    text: "画一个120×80安装板，四角4个Ø10孔，孔边距15",
    provider,
    cli: adapter,
  });
  assert.equal(response.status, "ok");
  assert.equal(response.trace.outputKind, "text");
  assert.equal(response.trace.repairCount, 0);
  const geometry = response.result.geometry;
  const plate = geometry.entities.main.primitive;
  assert.equal(plate.kind, "rectangle");
  assert.equal(plate.width, 120);
  assert.equal(plate.height, 80);
  const expected = [
    { x: -45, y: -25 }, { x: -45, y: 25 }, { x: 45, y: -25 }, { x: 45, y: 25 },
  ];
  assert.deepEqual(circleCenters(geometry), expected);
});

test("new-design with an unsupported feature is rejected", { skip: !binary }, async () => {
  // FILLET is unsupported in v1: the provider is allowed one repair, but if it
  // keeps emitting FILLET the intent is rejected.
  const provider = createFakeProvider(() => ({ kind: "text", text: "AICAD 1\nFILLET f1\n" }));
  const response = await runIntent({ mode: "new", text: "加一个圆角", provider, cli: adapter });
  assert.equal(response.status, "rejected");
  assert.equal(response.trace.repairCount, 1);
  assert.match(response.error.code, /^SCC-DSL-/);
});

test("edit vertical slice: change plate width 120 -> 160", { skip: !binary }, async () => {
  const initial = await adapter.engineeringDslSolve(PLATE_DSL);
  const before = circleCenters(initial.geometry);

  const provider = createScriptedProvider([
    (request) => {
      assert.equal(request.mode, "edit");
      assert.ok(request.context, "edit request must carry the derived context view");
      assert.equal(request.tools.length, 37, "edit request must carry the real tool manifest");
      return { kind: "tool_calls", toolCalls: [{ name: "set_parameter", arguments: { target: "plate_width", value: 160 } }] };
    },
  ]);
  const response = await runIntent({
    mode: "edit",
    text: "把板宽改成160",
    modelJson: JSON.stringify(initial.model),
    provider,
    cli: adapter,
  });
  assert.equal(response.status, "ok");
  const model = response.result.model;
  assert.equal(model.parameters.plate_width.value.value, 160);
  const after = circleCenters(response.result.geometry);
  assert.deepEqual(
    after.map((c) => Math.round(c.x)),
    [-65, -65, 65, 65],
  );
  assert.notDeepEqual(before, after);
});

test("edit vertical slice: derived parameter stays derived; only the source is edited", { skip: !binary }, async () => {
  const derivedDsl = `AICAD 1
PART p
PARAM plate_width = 120
PARAM hole_diameter := plate_width / 8
BASE_RECT main
  WIDTH 120
  HEIGHT 80
BIND main WIDTH <- plate_width
HOLE h
  DIA 10
BIND h DIAMETER <- hole_diameter
`;
  const initial = await adapter.engineeringDslSolve(derivedDsl);
  assert.equal(initial.model.parameters.hole_diameter.value.type, "expression");

  const provider = createScriptedProvider([
    { kind: "tool_calls", toolCalls: [{ name: "set_parameter", arguments: { target: "plate_width", value: 96 } }] },
  ]);
  const response = await runIntent({
    mode: "edit",
    text: "把板宽改成96",
    modelJson: JSON.stringify(initial.model),
    provider,
    cli: adapter,
  });
  assert.equal(response.status, "ok");
  const model = response.result.model;
  assert.equal(model.parameters.plate_width.value.value, 96);
  // hole_diameter stays an expression; the program recomputes it.
  assert.equal(model.parameters.hole_diameter.value.type, "expression");
});

test("one bounded repair fixes an invalid DSL", { skip: !binary }, async () => {
  const invalid = "AICAD 1\nHOLE h\n  CENTER 0\n"; // CENTER needs two numbers
  const provider = createFakeProvider((request, call) =>
    call === 1 ? { kind: "text", text: invalid } : { kind: "text", text: PLATE_DSL });
  const response = await runIntent({ mode: "new", text: "画一块安装板", provider, cli: adapter });
  assert.equal(response.status, "ok");
  assert.equal(response.trace.repairCount, 1);
  assert.equal(provider.calls, 2, "exactly one repair, never a third call");
});

test("repair still invalid after one attempt is rejected", { skip: !binary }, async () => {
  const invalid = "AICAD 1\nHOLE h\n  CENTER 0\n";
  const provider = createFakeProvider(() => ({ kind: "text", text: invalid }));
  const response = await runIntent({ mode: "new", text: "画一块板", provider, cli: adapter });
  assert.equal(response.status, "rejected");
  assert.equal(response.trace.repairCount, 1);
  assert.equal(provider.calls, 2);
});

test("edit requires tool calls instead of free text", { skip: !binary }, async () => {
  const initial = await adapter.engineeringDslSolve(PLATE_DSL);
  const provider = createScriptedProvider([
    { kind: "text", text: "把板宽改成160" },
  ]);
  const response = await runIntent({ mode: "edit", text: "把板宽改成160", modelJson: JSON.stringify(initial.model), provider, cli: adapter });
  assert.equal(response.status, "rejected");
  assert.equal(response.error.code, "SCC-UI-INTENT-010");
});

test("new parametric intent applies registered G2 calls to a fresh EngineeringModel v2", { skip: !binary || !parametricIntegrationEnabled }, async () => {
  const toolCalls = [
    { name: "create_part", arguments: { id: "plate", name: "安装板" } },
    { name: "create_parametric_plate", arguments: { id: "base", width: 140, height: 100, thickness: 20 } },
    { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
    { name: "add_parametric_hole", arguments: {
      id: "center_hole", support_mode: "principal", plane: "x_y", support_offset: 20,
      center_u: 0, center_v: 0, diameter: 20, reversed: true, termination: "through_all", hole_kind: "simple",
    } },
    { name: "add_parametric_hole", arguments: {
      id: "pcd_seed", support_mode: "principal", plane: "x_y", support_offset: 20,
      center_u: 30, center_v: 0, diameter: 6, reversed: true, termination: "through_all", hole_kind: "simple",
    } },
    { name: "add_feature_pattern", arguments: {
      id: "pcd_pattern", seed_feature_id: "pcd_seed", pattern_kind: "circular", count: 4,
      axis_x: 0, axis_y: 0, axis_z: 1, center_x: 0, center_y: 0, center_z: 20,
      radius: 30, start_angle_degrees: 0, angular_span_degrees: 360,
    } },
  ];
  const manifest = await adapter.engineeringManifest();
  const allowedToolNames = ["create_part", "create_parametric_plate", "set_drawing_property", "add_parametric_hole", "add_feature_pattern"];
  const provider = createFakeProvider((request) => {
    assert.equal(request.mode, "new");
    assert.deepEqual(request.tools.map(({ name }) => name).sort(), allowedToolNames.slice().sort());
    assert.match(request.userText, /中文参数化安装板/);
    return { kind: "tool_calls", toolCalls };
  });
  const response = await runIntent({
    mode: "new",
    text: "创建中文参数化安装板，含中心孔和四孔圆周阵列。",
    modelJson: null,
    provider,
    cli: adapter,
    toolManifest: manifest,
    allowedToolNames,
  });
  assert.equal(response.status, "ok", JSON.stringify(response.error));
  assert.equal(response.trace.outputKind, "tool_calls");
  assert.ok(response.result.model.feature_order.includes("base_sketch"));
  assert.ok(response.result.model.feature_order.includes("center_hole"));
  assert.equal(response.result.model.part.name, "安装板");
  assert.ok(response.result.model.features.center_hole);
  assert.equal(response.result.model.features.pcd_pattern.feature.seed_feature_id, "pcd_seed");
  const drawing = response.result.parametric_drawing;
  assert.ok(drawing, "parameterized Intent only succeeds after formal CAD and three-view validation");
  assert.equal(drawing.drawing_report.view_count, 3);
  assert.equal(drawing.spatial_summary.parametric.operation_features.filter((id) => id === "pcd_pattern").length, 3);

  const invalidPatternCalls = [
    { name: "create_part", arguments: { id: "bad_plate", name: "Bad Pattern" } },
    { name: "create_parametric_plate", arguments: { id: "base", width: 140, height: 100, thickness: 20 } },
    { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
    { name: "add_parametric_hole", arguments: {
      id: "seed", support_mode: "principal", plane: "x_y", support_offset: 20,
      center_u: 0, center_v: 0, diameter: 6, reversed: true, termination: "through_all", hole_kind: "simple",
    } },
    { name: "add_feature_pattern", arguments: {
      id: "bad_pattern", seed_feature_id: "seed", pattern_kind: "linear", count: 2,
      direction_x: 1, direction_y: 0, direction_z: 0, spacing: 200,
    } },
  ];
  const invalidProvider = createFakeProvider(() => ({ kind: "tool_calls", toolCalls: invalidPatternCalls }));
  const invalidResult = await runIntent({
    mode: "new", text: "创建一块板并在板外生成阵列孔。", provider: invalidProvider,
    cli: adapter, toolManifest: manifest, allowedToolNames,
  });
  assert.equal(invalidResult.status, "rejected");
  assert.equal(invalidResult.error.code, "PATTERN_INSTANCE_COLLISION");
  assert.equal(invalidProvider.calls, 1, "geometry failures must not trigger a paid-model repair call");
});

test("new parametric intent rejects calls outside its Skill allowlist", { skip: !binary }, async () => {
  const provider = createScriptedProvider([
    { kind: "tool_calls", toolCalls: [{ name: "remove_feature", arguments: { target: "unknown" } }] },
  ]);
  const manifest = await adapter.engineeringManifest();
  const response = await runIntent({
    mode: "new", text: "创建零件", provider, cli: adapter,
    toolManifest: manifest, allowedToolNames: ["create_part"],
  });
  assert.equal(response.status, "rejected");
  assert.equal(response.error.code, "SCC-UI-INTENT-017");
});

test("too many tool calls are rejected", { skip: !binary }, async () => {
  const initial = await adapter.engineeringDslSolve(PLATE_DSL);
  const toolCalls = Array.from({ length: 33 }, (_, i) => ({
    name: "set_parameter",
    arguments: { target: "plate_width", value: i + 1 },
  }));
  const provider = createScriptedProvider([{ kind: "tool_calls", toolCalls }]);
  const response = await runIntent({
    mode: "edit",
    text: "改一堆参数",
    modelJson: JSON.stringify(initial.model),
    provider,
    cli: adapter,
  });
  assert.equal(response.status, "rejected");
  assert.equal(response.error.code, "SCC-UI-INTENT-011");
});

test("clarification is returned without touching the model", { skip: !binary }, async () => {
  const initial = await adapter.engineeringDslSolve(PLATE_DSL);
  const provider = createScriptedProvider([
    { kind: "clarification", reason: "Which hole should change?", fields: ["hole_id"] },
  ]);
  const response = await runIntent({
    mode: "edit",
    text: "把那个孔改大",
    modelJson: JSON.stringify(initial.model),
    provider,
    cli: adapter,
  });
  assert.equal(response.status, "needs_clarification");
  assert.deepEqual(response.fields, [{ key: "hole_id", kind: "feature_reference", prompt: "hole_id", required: true }]);
});

test("provider failure throws and leaves nothing applied", { skip: !binary }, async () => {
  const provider = createFakeProvider(() => {
    throw new Error("network down");
  });
  await assert.rejects(
    () => runIntent({ mode: "new", text: "画一块板", provider, cli: adapter }),
    /network down/,
  );
});

test("SCC geometry failure is not auto-repaired", { skip: !binary }, async () => {
  // Two holes with a nonzero distance constraint: engineering applies and
  // lowers fine, but the geometry solver rejects the nonzero point distance
  // (SCC-GEOM-005). This must NOT trigger a repair round.
  const geomConflictDsl = `AICAD 1
PART p
BASE_RECT main
  WIDTH 100
  HEIGHT 100
HOLE h1
  DIA 10
HOLE h2
  DIA 10
CONSTRAINT d1 DISTANCE h1 h2 50
`;
  let calls = 0;
  const provider = createFakeProvider(() => {
    calls += 1;
    return { kind: "text", text: geomConflictDsl };
  });
  const response = await runIntent({ mode: "new", text: "画一块板", provider, cli: adapter });
  assert.equal(response.status, "rejected");
  assert.equal(calls, 1, "SCC geometry failures must not trigger a repair");
  assert.match(response.error.code, /^SCC-GEOM/);
});

test("determinism: same provider result -> same downstream output", { skip: !binary }, async () => {
  const build = () => runIntent({
    mode: "new",
    text: "画一块安装板",
    provider: createScriptedProvider([{ kind: "text", text: PLATE_DSL }]),
    cli: adapter,
  });
  const first = await build();
  for (let i = 0; i < 10; i += 1) {
    const candidate = await build();
    assert.equal(candidate.status, "ok");
    assert.deepEqual(candidate.result.model, first.result.model);
    assert.deepEqual(candidate.result.geometry, first.result.geometry);
  }
});
