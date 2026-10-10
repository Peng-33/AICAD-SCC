import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import { createApp } from "../server.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);

// Same fixture shape as engineering-workbench tests: derived parameter
// hole_diameter := plate_height / 8 (80 → Ø10) exercises parameter
// propagation through the drawing pipeline.
const FIXTURE_DSL = `AICAD 1
PART mounting_plate "Mounting Plate"
PARAM plate_width = 120
PARAM plate_height = 80
PARAM edge_offset = 15
PARAM hole_diameter := plate_height / 8
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
BIND mounting DIAMETER <- hole_diameter
`;

const THREE_VIEW_FIXTURE = JSON.parse(readFileSync(
  new URL("./fixtures/three-view-plate.json", import.meta.url),
  "utf8",
));

async function withServer(run) {
  const adapter = new SccCliAdapter(binary);
  const server = createApp({ adapter });
  await new Promise((resolveListen) => server.listen(0, "127.0.0.1", resolveListen));
  const { port } = server.address();
  try {
    await run(`http://127.0.0.1:${port}`);
  } finally {
    await new Promise((resolveListen) => server.close(resolveListen));
  }
}

const json = (method, body) => ({
  method,
  headers: { "content-type": "application/json" },
  body: JSON.stringify(body),
});

const loadModel = async (base) =>
  fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: FIXTURE_DSL })).then((r) => r.json());

const drawing = async (base, model) =>
  fetch(`${base}/api/engineering/drawing`, json("POST", { model })).then(async (r) => ({
    status: r.status,
    payload: await r.json(),
  }));

const threeViewDrawing = async (base, model) =>
  fetch(base + "/api/engineering/drawing/three-view", json("POST", { model })).then(async (r) => ({
    status: r.status,
    payload: await r.json(),
  }));

test("three-view API: formal fixture returns SVG, metadata, and no sidecar JSON", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const { status, payload } = await threeViewDrawing(base, THREE_VIEW_FIXTURE);
    assert.equal(status, 200);
    assert.match(payload.svg, /^<svg/);
    assert.match(payload.svg, /id="view_front"/);
    assert.match(payload.svg, /id="view_top"/);
    assert.match(payload.svg, /id="view_left"/);
    assert.match(payload.dxf, /SECTION/);
    assert.match(payload.dxf, /ENTITIES[\s\S]*AICAD_OBJECT/);
    assert.match(payload.svg, /stroke-dasharray="4 2"/);
    assert.equal(payload.drawing_mode, "three_view");
    assert.equal(payload.projection_method, "third_angle");
    assert.equal(payload.part_title, "Three View Plate");
    assert.ok(payload.sheet_width_mm > 0);
    assert.ok(payload.sheet_height_mm > 0);
    assert.equal(payload.scale, 1);
    assert.equal("spatial" in payload, false);
    assert.equal("projection" in payload, false);
    assert.equal("visibility" in payload, false);
  });
});

test("derived drawing cache is fingerprinted, bounded by artifact kind, and repeatable", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const first = await drawing(base, THREE_VIEW_FIXTURE);
    const second = await drawing(base, THREE_VIEW_FIXTURE);
    assert.equal(first.payload.cache_status, "miss");
    assert.equal(second.payload.cache_status, "hit");
    assert.equal(first.payload.revision, 0);
    assert.equal(second.payload.revision, 0);
    assert.equal(second.payload.model_fingerprint, first.payload.model_fingerprint);
    assert.equal(second.payload.svg, first.payload.svg);

    const threeFirst = await threeViewDrawing(base, THREE_VIEW_FIXTURE);
    const threeSecond = await threeViewDrawing(base, THREE_VIEW_FIXTURE);
    assert.equal(threeFirst.payload.cache_status, "miss");
    assert.equal(threeSecond.payload.cache_status, "hit");
    assert.equal(threeSecond.payload.svg, threeFirst.payload.svg);
    assert.notEqual(first.payload.artifact_kind, threeFirst.payload.artifact_kind);

    const edited = structuredClone(THREE_VIEW_FIXTURE);
    edited.features.plate.width = 160;
    const changed = await drawing(base, edited);
    assert.equal(changed.payload.cache_status, "miss");
    assert.notEqual(changed.payload.model_fingerprint, first.payload.model_fingerprint);
  });
});

test("three-view API: formal pipeline changes with a 120 to 160 width edit", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const before = await threeViewDrawing(base, THREE_VIEW_FIXTURE);
    const editedModel = structuredClone(THREE_VIEW_FIXTURE);
    editedModel.features.plate.width = 160;
    const after = await threeViewDrawing(base, editedModel);
    assert.equal(before.status, 200);
    assert.equal(after.status, 200);
    assert.notEqual(after.payload.svg, before.payload.svg);
    assert.ok(after.payload.sheet_width_mm > before.payload.sheet_width_mm);
  });
});

test("three-view API: First-angle metadata uses the formal model property", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const model = structuredClone(THREE_VIEW_FIXTURE);
    model.drawing.properties.projection_method.method = "first_angle";
    const result = await threeViewDrawing(base, model);
    assert.equal(result.status, 200);
    assert.equal(result.payload.projection_method, "first_angle");
    assert.match(result.payload.svg, /id="view_top"/);
  });
});

test("three-view API: missing spatial/projection semantics fail closed without Principal fallback", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const missingSpatial = structuredClone(THREE_VIEW_FIXTURE);
    delete missingSpatial.spatial;
    const failedSpatial = await threeViewDrawing(base, missingSpatial);
    assert.equal(failedSpatial.status, 422);
    assert.match(failedSpatial.payload.error.code, /^SCC-SPAT-/);

    const principal = await drawing(base, missingSpatial);
    assert.equal(principal.status, 200, "legacy Principal remains compatible");

    const missingMethod = structuredClone(THREE_VIEW_FIXTURE);
    delete missingMethod.drawing.properties.projection_method;
    const failedMethod = await threeViewDrawing(base, missingMethod);
    assert.equal(failedMethod.status, 422);
    assert.equal(failedMethod.payload.error.code, "SCC-DRW-019");
  });
});

test("drawing API: valid model returns SVG with the five groups + metadata", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const dsl = await loadModel(base);
    const { status, payload } = await drawing(base, dsl.model);
    assert.equal(status, 200);
    const svg = payload.svg;
    for (const group of ["objects", "construction", "centers", "dimensions", "annotations"]) {
      assert.ok(svg.includes(`id="${group}"`), `svg must contain group id=${group}`);
    }
    assert.equal(payload.part_title, "Mounting Plate");
    assert.ok(payload.sheet_width_mm > 0);
    assert.ok(payload.sheet_height_mm > 0);
    assert.ok(payload.scale > 0);
  });
});

test("drawing API: parameter change propagates into dimension text", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const dsl = await loadModel(base);
    // BASE main HEIGHT is bound to plate_height=80 → a linear "80" dim text.
    const before = await drawing(base, dsl.model);
    assert.equal(before.status, 200);
    assert.match(before.payload.svg, />80<\/text>/, "initial drawing shows the 80 mm height");

    const edited = await fetch(
      `${base}/api/engineering/tool`,
      json("POST", { model: dsl.model, toolCall: { name: "set_parameter", arguments: { target: "plate_height", value: 160 } } }),
    ).then((r) => r.json());
    const after = await drawing(base, edited.model);
    assert.equal(after.status, 200);
    assert.match(after.payload.svg, />160<\/text>/, "edited drawing re-solves to a 160 mm height");
    assert.ok(after.payload.sheet_height_mm > before.payload.sheet_height_mm, "sheet grows with the part");
  });
});

test("drawing API: derived parameter hole_diameter follows plate_height", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const dsl = await loadModel(base);
    // hole_diameter := plate_height / 8 → 80/8 = Ø10 in the pattern callout.
    const before = await drawing(base, dsl.model);
    assert.match(before.payload.svg, /4×Ø10 OFF/, "initial pattern callout shows Ø10");

    const edited = await fetch(
      `${base}/api/engineering/tool`,
      json("POST", { model: dsl.model, toolCall: { name: "set_parameter", arguments: { target: "plate_height", value: 96 } } }),
    ).then((r) => r.json());
    const after = await drawing(base, edited.model);
    assert.equal(after.status, 200);
    assert.match(after.payload.svg, /4×Ø12 OFF/, "96/8 = Ø12 after the edit");
  });
});

test("drawing API: show_centerlines=false empties the centers group", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const dsl = await loadModel(base);
    // set_drawing_property is a real registry tool (verified via manifest).
    const edited = await fetch(
      `${base}/api/engineering/tool`,
      json("POST", { model: dsl.model, toolCall: { name: "set_drawing_property", arguments: { property: "show_centerlines", value: false } } }),
    ).then((r) => r.json());
    const { status, payload } = await drawing(base, edited.model);
    assert.equal(status, 200);
    // The renderer still emits the group header but no child elements.
    assert.doesNotMatch(payload.svg, /<g id="centers"[^>]*>\s*[^<\s]/, "centers group must be empty");
    assert.match(payload.svg, /id="centers"/, "group header remains present");
  });
});

test("drawing API: show_construction_geometry=false empties the construction group", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const dsl = await loadModel(base);
    // Construction circles are reference geometry: visible by default.
    const withCircle = await fetch(
      `${base}/api/engineering/tool`,
      json("POST", { model: dsl.model, toolCall: { name: "add_construction_circle", arguments: { id: "ref", center_x: 60, center_y: 40, diameter: 40 } } }),
    ).then((r) => r.json());
    const before = await drawing(base, withCircle.model);
    assert.doesNotMatch(before.payload.svg, /<g id="construction"[^>]*>\s*<\/g>/, "construction circle is drawn by default");
    const edited = await fetch(
      `${base}/api/engineering/tool`,
      json("POST", { model: withCircle.model, toolCall: { name: "set_drawing_property", arguments: { property: "show_construction_geometry", value: false } } }),
    ).then((r) => r.json());
    const { status, payload } = await drawing(base, edited.model);
    assert.equal(status, 200);
    assert.match(payload.svg, /<g id="construction"[^>]*>\s*<\/g>/, "construction group must be empty");
  });
});

test("drawing API: missing model is 400, malformed model is a structured CLI error", { skip: !binary }, async () => {
  await withServer(async (base) => {
    const missing = await fetch(`${base}/api/engineering/drawing`, json("POST", {}));
    assert.equal(missing.status, 400);
    const missingPayload = await missing.json();
    assert.equal(missingPayload.error.code, "SCC-UI-REQ-007");

    // A model that solves to zero geometry triggers SCC-DRW-005 (empty drawing).
    const empty = await fetch(`${base}/api/engineering/dsl`, json("POST", { dsl: "AICAD 1\nPART empty_plate\n" })).then((r) => r.json());
    const failed = await drawing(base, empty.model);
    assert.equal(failed.status, 422);
    assert.match(failed.payload.error.code, /^SCC-DRW-\d+$/, "drawing failure keeps its SCC-DRW code");
    assert.notEqual(failed.payload.error.code, "SCC-UI-INTERNAL-001");
  });
});
