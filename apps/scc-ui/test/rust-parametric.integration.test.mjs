import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

const binary = process.env.SCC_BIN ?? "/usr/local/bin/scc";
const enabled = process.env.AICAD_RUST_PARAMETRIC_INTEGRATION === "1" && existsSync(binary);

function line(start, end) {
  return { kind: "line", start: point(start), end: point(end) };
}

function arc(start, end, center, radius, direction = "counter_clockwise") {
  return { kind: "arc", start: point(start), end: point(end), center: point(center), radius, direction };
}

function circle(center, radius) {
  return { kind: "circle", center: point(center), radius };
}

function point(pair) {
  return { x: pair[0], y: pair[1] };
}

function rectangle(cx, cy, width, height) {
  const hx = width / 2;
  const hy = height / 2;
  const corners = [[cx - hx, cy - hy], [cx + hx, cy - hy], [cx + hx, cy + hy], [cx - hx, cy + hy]];
  return corners.map((start, index) => line(start, corners[(index + 1) % corners.length]));
}

function roundedRectangle(cx, cy, width, height, radius) {
  const hx = width / 2;
  const hy = height / 2;
  return [
    line([cx - hx + radius, cy - hy], [cx + hx - radius, cy - hy]),
    arc([cx + hx - radius, cy - hy], [cx + hx, cy - hy + radius], [cx + hx - radius, cy - hy + radius], radius),
    line([cx + hx, cy - hy + radius], [cx + hx, cy + hy - radius]),
    arc([cx + hx, cy + hy - radius], [cx + hx - radius, cy + hy], [cx + hx - radius, cy + hy - radius], radius),
    line([cx + hx - radius, cy + hy], [cx - hx + radius, cy + hy]),
    arc([cx - hx + radius, cy + hy], [cx - hx, cy + hy - radius], [cx - hx + radius, cy + hy - radius], radius),
    line([cx - hx, cy + hy - radius], [cx - hx, cy - hy + radius]),
    arc([cx - hx, cy - hy + radius], [cx - hx + radius, cy - hy], [cx - hx + radius, cy - hy + radius], radius),
  ];
}

function slot(cx, cy, overallLength, width) {
  const halfCentres = (overallLength - width) / 2;
  return [
    line([cx - halfCentres, cy - width / 2], [cx + halfCentres, cy - width / 2]),
    arc([cx + halfCentres, cy - width / 2], [cx + halfCentres, cy + width / 2], [cx + halfCentres, cy], width / 2),
    line([cx + halfCentres, cy + width / 2], [cx - halfCentres, cy + width / 2]),
    arc([cx - halfCentres, cy + width / 2], [cx - halfCentres, cy - width / 2], [cx - halfCentres, cy], width / 2),
  ];
}

function modelBuilder(name = "G1B fixture") {
  const model = {
    part: { id: "part", name },
    features: {},
    feature_order: [],
    parameters: {},
    constraints: {},
    feature_parameters: {},
    drawing: { properties: { projection_method: { type: "projection_method", method: "third_angle" } } },
    spatial: { hole_extents: {} },
  };
  const addFeature = (id, feature, suppressed = false) => {
    model.features[id] = { type: "parametric", id, name: id, feature, suppressed };
    model.feature_order.push(id);
  };
  const datum = (id, plane = "x_y", offset = null, base = null) => {
    const definition = offset === null
      ? { kind: "principal", plane }
      : { kind: "offset", base: base ?? { kind: "principal", plane }, distance: offset };
    addFeature(id, { type: "datum_plane", definition });
    return { kind: "datum", feature: id };
  };
  const sketch = (id, loops, support = { kind: "principal", plane: "x_y" }) => {
    const encodeLoop = (curves, loopId) => ({
      entities: curves.map((geometry, index) => ({ id: `${loopId}-${String(index).padStart(4, "0")}`, geometry })),
    });
    const profile = {
      outer: encodeLoop(loops[0], `${id}-outer`),
      inner_loops: loops.slice(1).map((loop, index) => encodeLoop(loop, `${id}-inner-${index}`)),
    };
    addFeature(id, { type: "sketch", support, profile });
    return id;
  };
  const extrude = (id, sketchId, operation, extent, { reversed = false, vector = null, suppressed = false } = {}) => {
    const direction = vector === null ? { Normal: { reversed } } : { Vector: { value: { x: vector[0], y: vector[1], z: vector[2] } } };
    addFeature(id, { type: "extrude", profile: sketchId, operation, direction, extent }, suppressed);
    return id;
  };
  const datumAxis = (id, definition = { kind: "principal", axis: "x" }) => {
    addFeature(id, { type: "datum_axis", definition });
    return id;
  };
  const revolve = (id, sketchId, axisId, operation = "add", angle = { kind: "full_360" }, orientation = "forward", suppressed = false) => {
    addFeature(id, {
      type: "revolve", profile: sketchId,
      axis: { kind: "datum_axis", feature: axisId }, operation, angle, orientation,
    }, suppressed);
    return id;
  };
  const loft = (id, sections, operation = "add", suppressed = false) => {
    addFeature(id, { type: "loft", sections, operation, mode: "ruled", orientation_policy: "aligned" }, suppressed);
    return id;
  };
  return { model, addFeature, datum, datumAxis, sketch, extrude, revolve, loft };
}

function extDistance(length) { return { type: "distance", length }; }
function extThroughAll() { return { type: "through_all" }; }
function extSymmetric(length) { return { type: "symmetric", length }; }

function plateHistory({ width = 100, height = 60, thickness = 10 } = {}) {
  const builder = modelBuilder();
  const sketchId = builder.sketch("base_sketch", [rectangle(0, 0, width, height)]);
  builder.extrude("base_add", sketchId, "add", extDistance(thickness));
  return builder;
}

function addOffsetBoss(builder, { id = "boss", profile = [circle([0, 0], 10)], z = 10, extent = 8 } = {}) {
  const datum = builder.datum(`${id}_datum`, "x_y", z);
  const sketchId = builder.sketch(`${id}_sketch`, [profile], datum);
  builder.extrude(`${id}_add`, sketchId, "add", extDistance(extent));
}

function invoke(value) {
  const root = mkdtempSync(join(tmpdir(), "aicad-g1b-"));
  try {
    const path = join(root, "model.json");
    writeFileSync(path, `${JSON.stringify(value)}\n`, "utf8");
    const result = spawnSync(binary, ["--json-errors", "--quiet", "engineering", "drawing-three-view", path], {
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
      env: process.env,
    });
    return { ...result, path };
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function draw(model, context = "drawing") {
  const result = invoke(model);
  assert.equal(result.error, undefined, result.error?.message);
  assert.equal(result.status, 0, `${context}\n${result.stderr}`);
  return JSON.parse(result.stdout);
}

function errorCode(model) {
  const result = invoke(model);
  assert.notEqual(result.status, 0, "invalid body must fail closed");
  const combined = `${result.stderr}\n${result.stdout}`;
  const parsed = [...combined.matchAll(/(?:\"code\"\s*:\s*\"|)([A-Z_]+)(?:\"|)/g)].map((match) => match[1]);
  return parsed.find((code) => ["DETACHED_ADD", "MULTI_BODY_NOT_ALLOWED", "CUT_NO_INTERSECTION", "CUT_REMOVES_BODY", "PATTERN_INSTANCE_COLLISION", "INVALID_COUNTERBORE", "INVALID_COUNTERSINK", "COUNTERSINK_NO_INTERSECTION", "COUNTERSINK_GEOMETRY_FAILED", "COUNTERSINK_GEOMETRY_DEFERRED", "PATTERN_SEED_NOT_FOUND", "PATTERN_UNSUPPORTED_SEED", "PATTERN_INVALID_PLACEMENT", "PATTERN_DUPLICATE_PLACEMENT", "NO_BODY"].includes(code)) ?? combined;
}

function volume(drawing) {
  return drawing.spatial_summary.parametric.volume_mm3;
}

function parseDxfWithEzdxf(dxf) {
  const parser = process.env.AICAD_DXF_PARSER;
  if (!parser) return null;
  const script = String.raw`import io, json, sys, ezdxf
source = sys.stdin.buffer.read().decode("utf-8")
doc = ezdxf.read(io.StringIO(source))
audit = doc.audit()
entities = []
for entity in doc.modelspace().query("DIMENSION"):
    entities.append({
        "dimtype": int(entity.dxf.dimtype) & 15,
        "text": entity.dxf.text,
        "measurement": float(entity.get_measurement()),
        "stored_measurement": float(entity.dxf.actual_measurement),
        "style": entity.dxf.dimstyle,
        "geometry": entity.dxf.geometry,
        "block_entities": [item.dxftype() for item in doc.blocks.get(entity.dxf.geometry)],
        "defpoint": [float(entity.dxf.defpoint.x), float(entity.dxf.defpoint.y)],
    })
print(json.dumps({"dimensions": entities, "audit_errors": len(audit.errors), "audit_fixes": len(audit.fixes)}))`;
  const result = spawnSync(parser, ["-c", script], {
    input: dxf,
    encoding: "utf8",
    maxBuffer: 8 * 1024 * 1024,
    env: process.env,
  });
  assert.equal(result.error, undefined, result.error?.message);
  assert.equal(result.status, 0, `ezdxf failed to parse/audit exported DXF\n${result.stderr}`);
  return JSON.parse(result.stdout);
}

function assertDrawing(drawing) {
  assert.equal(drawing.drawing_mode, "three_view");
  assert.equal(drawing.drawing_report.view_count, 3);
  const kinds = drawing.drawing_report.views.map((view) => view.kind);
  assert.deepEqual(kinds.sort(), ["front", "left", "top"]);
  assert.ok(drawing.svg.length > 1000);
  assert.match(drawing.svg, /id="view_front"/);
  assert.match(drawing.svg, /id="view_top"/);
  assert.match(drawing.svg, /id="view_left"/);
  assert.ok(drawing.dxf.length > 1000);
  assert.match(drawing.dxf, /SECTION/);
  assert.ok(drawing.spatial_summary.parametric.edge_count > 0);
  assert.ok(drawing.spatial_summary.parametric.face_count > 0);
}

test("G1B generic feature matrix and drawing pipeline use actual CadQuery B-Rep", { skip: !enabled }, (t) => {
  const cases = [];

  // 1 Rectangle ADD.
  cases.push(plateHistory());

  // 2 Circle ADD on the plate; 3 off-center rectangle ADD.
  const addCircle = plateHistory();
  addOffsetBoss(addCircle, { id: "round", profile: [circle([-15, 0], 10)] });
  cases.push(addCircle);
  const addRectangle = plateHistory();
  addOffsetBoss(addRectangle, { id: "offset_rect", profile: rectangle(25, 10, 18, 14), z: 10, extent: 5 });
  cases.push(addRectangle);

  // 4 Blind circle CUT.
  const blindCircle = plateHistory();
  const blindTop = blindCircle.datum("blind_circle_top", "x_y", 10);
  const circleSketch = blindCircle.sketch("blind_circle_sketch", [[circle([10, 0], 5)]], blindTop);
  blindCircle.extrude("blind_circle_cut", circleSketch, "cut", extDistance(4), { reversed: true });
  cases.push(blindCircle);

  // 5 Slot through CUT.
  const throughSlot = plateHistory();
  const slotSketch = throughSlot.sketch("slot_sketch", [slot(-10, 0, 30, 8)], { kind: "principal", plane: "x_y" });
  throughSlot.extrude("slot_cut", slotSketch, "cut", extThroughAll(), { reversed: true });
  cases.push(throughSlot);

  // 6 RoundedRectangle blind CUT.
  const roundedCut = plateHistory();
  const roundedTop = roundedCut.datum("rounded_cut_top", "x_y", 10);
  const roundedSketch = roundedCut.sketch("rounded_sketch", [roundedRectangle(10, 0, 20, 14, 3)], roundedTop);
  roundedCut.extrude("rounded_cut", roundedSketch, "cut", extDistance(6), { reversed: true });
  cases.push(roundedCut);

  // 7 ADD→ADD. 8 ADD→CUT→ADD→CUT.
  const addAdd = plateHistory();
  addOffsetBoss(addAdd, { id: "add_add", profile: [circle([-15, 0], 10)] });
  cases.push(addAdd);
  const mixed = plateHistory();
  addOffsetBoss(mixed, { id: "mixed_boss", profile: [circle([-20, 0], 12)], z: 10, extent: 8 });
  const mixedPocketTop = mixed.datum("mixed_pocket_top", "x_y", 18);
  const pocketSketch = mixed.sketch("mixed_pocket_sketch", [roundedRectangle(-20, 0, 12, 12, 2)], mixedPocketTop);
  mixed.extrude("mixed_pocket", pocketSketch, "cut", extDistance(3), { reversed: true });
  const secondBossDatum = mixed.datum("mixed_rect_datum", "x_y", 10);
  const secondBossSketch = mixed.sketch("mixed_rect_sketch", [rectangle(25, 0, 20, 20)], secondBossDatum);
  mixed.extrude("mixed_rect_add", secondBossSketch, "add", extDistance(5));
  const secondSlotSketch = mixed.sketch("mixed_slot_sketch", [slot(25, 0, 18, 6)]);
  mixed.extrude("mixed_slot_cut", secondSlotSketch, "cut", extThroughAll(), { reversed: true });
  cases.push(mixed);

  const symmetric = modelBuilder("Symmetric extent");
  const symmetricSketch = symmetric.sketch("symmetric_sketch", [rectangle(0, 0, 20, 10)]);
  symmetric.extrude("symmetric_add", symmetricSketch, "add", extSymmetric(12));
  cases.push(symmetric);

  for (const [index, fixture] of cases.entries()) {
    const result = draw(fixture.model, `matrix success case ${index + 1}`);
    assertDrawing(result);
    assert.ok(volume(result) > 0, `matrix success case ${index + 1} must produce a real solid`);
    assert.equal(result.spatial_summary.parametric.operation_features.length, fixture.model.feature_order.filter((id) => fixture.model.features[id].feature.type === "extrude" && !fixture.model.features[id].suppressed).length);
    if (fixture.model.part.name === "Symmetric extent") {
      assert.equal(result.spatial_summary.bounds.min_z, -6);
      assert.equal(result.spatial_summary.bounds.max_z, 6);
    }
  }

  // Invalid histories: detached ADD, CUT removes body, and no-intersection CUT.
  const detached = plateHistory();
  addOffsetBoss(detached, { id: "detached", profile: rectangle(200, 0, 10, 10), z: 30, extent: 5 });
  assert.equal(errorCode(detached.model), "MULTI_BODY_NOT_ALLOWED");

  const fullCut = plateHistory({ width: 10, height: 10, thickness: 10 });
  const fullCutSketch = fullCut.sketch("full_cut_sketch", [[circle([0, 0], 100)]]);
  fullCut.extrude("full_cut", fullCutSketch, "cut", extThroughAll(), { reversed: true });
  assert.equal(errorCode(fullCut.model), "CUT_REMOVES_BODY");

  const noIntersection = plateHistory();
  const outsideSketch = noIntersection.sketch("outside_sketch", [[circle([100, 0], 5)]]);
  noIntersection.extrude("outside_cut", outsideSketch, "cut", extThroughAll(), { reversed: true });
  assert.equal(errorCode(noIntersection.model), "CUT_NO_INTERSECTION");

  // Upstream Sketch modification performs a complete deterministic rebuild.
  const modified = plateHistory({ width: 120 });
  const first = draw(modified.model);
  const second = draw(modified.model);
  assert.equal(volume(first), volume(second));
  assert.deepEqual(first.spatial_summary, second.spatial_summary);
  const original = draw(plateHistory().model);
  assert.notEqual(volume(original), volume(first));
  assert.equal(first.spatial_summary.bounds.max_x, 60);

  // Suppressing a CUT removes it by rebuilding active history from scratch.
  const suppressCut = plateHistory();
  const suppressTop = suppressCut.datum("suppress_top", "x_y", 10);
  const suppressSketch = suppressCut.sketch("suppress_sketch", [[circle([0, 0], 5)]], suppressTop);
  suppressCut.extrude("suppress_cut", suppressSketch, "cut", extDistance(4), { reversed: true, suppressed: true });
  assert.equal(volume(draw(suppressCut.model)), 60000);
  suppressCut.model.features.suppress_cut.suppressed = false;
  assert.ok(volume(draw(suppressCut.model)) < 60000);

  // XZ and YZ plane transforms preserve the requested world direction.
  const xz = modelBuilder("XZ test");
  const xzSketch = xz.sketch("xz_sketch", [rectangle(0, 0, 20, 30)], { kind: "principal", plane: "x_z" });
  xz.extrude("xz_add", xzSketch, "add", extDistance(6));
  const xzDrawing = draw(xz.model);
  assert.equal(xzDrawing.spatial_summary.bounds.min_y, -6);
  assert.equal(xzDrawing.spatial_summary.bounds.max_z, 15);

  const yz = modelBuilder("YZ test");
  const yzSketch = yz.sketch("yz_sketch", [rectangle(0, 0, 20, 30)], { kind: "principal", plane: "y_z" });
  yz.extrude("yz_add", yzSketch, "add", extDistance(6));
  const yzDrawing = draw(yz.model);
  assert.equal(yzDrawing.spatial_summary.bounds.max_x, 6);
  assert.equal(yzDrawing.spatial_summary.bounds.max_z, 15);

  // Generic fixture specified in G1B; all operations remain Sketch + Extrude.
  const generic = modelBuilder("Generic G1B fixture");
  const baseSketch = generic.sketch("fixture_plate_sketch", [rectangle(0, 0, 160, 100)]);
  generic.extrude("fixture_plate_add", baseSketch, "add", extDistance(20));
  addOffsetBoss(generic, { id: "fixture_circle", profile: [circle([-45, 0], 15)], z: 20, extent: 15 });
  addOffsetBoss(generic, { id: "fixture_rectangle", profile: rectangle(45, 0, 48, 60), z: 20, extent: 10 });
  // A local offset datum places the blind cut on the top of the rectangular boss.
  const genericTop = generic.datum("fixture_top_datum", "x_y", 30);
  const genericPocket = generic.sketch("fixture_rounded_cut_sketch_top", [roundedRectangle(45, 0, 30, 20, 4)], genericTop);
  generic.extrude("fixture_rounded_cut", genericPocket, "cut", extDistance(6), { reversed: true });
  const genericSlotDatum = generic.datum("fixture_slot_top_datum", "x_y", 35);
  const genericSlotSketch = generic.sketch("fixture_slot_sketch", [slot(-45, 0, 30, 8)], genericSlotDatum);
  generic.extrude("fixture_slot_cut", genericSlotSketch, "cut", extThroughAll(), { reversed: true });
  const genericDrawing = draw(generic.model);
  assertDrawing(genericDrawing);
  assert.equal(genericDrawing.spatial_summary.bounds.min_x, -80);
  assert.equal(genericDrawing.spatial_summary.bounds.max_x, 80);
  assert.equal(genericDrawing.spatial_summary.bounds.min_z, 0);
  assert.equal(genericDrawing.spatial_summary.bounds.max_z, 35);
  assert.ok(Number.isFinite(genericDrawing.parametric_timings_ms.kernel_rebuild_ms));
  assert.ok(Number.isFinite(genericDrawing.parametric_timings_ms.spatial_lowering_ms));
  assert.ok(Number.isFinite(genericDrawing.parametric_timings_ms.three_view_generation_ms));
  assert.ok(Number.isFinite(genericDrawing.parametric_timings_ms.svg_export_ms));
  assert.ok(Number.isFinite(genericDrawing.parametric_timings_ms.dxf_export_ms));
  t.diagnostic(`G1B_GENERIC_TIMINGS_MS=${JSON.stringify(genericDrawing.parametric_timings_ms)}`);

  // Complex mechanical portion uses only the same generic sketch/extrude operations.
  const complex = modelBuilder("AICAD-COMPLEX-001 generic subset");
  const complexBase = complex.sketch("complex_plate_sketch", [rectangle(0, 0, 180, 120)]);
  complex.extrude("complex_plate_add", complexBase, "add", extDistance(24));
  addOffsetBoss(complex, { id: "complex_left_round", profile: [circle([-50, 0], 33)], z: 24, extent: 24 });
  addOffsetBoss(complex, { id: "complex_right_rect", profile: rectangle(48, 0, 68, 80), z: 24, extent: 12 });
  const complexTop = complex.datum("complex_right_top", "x_y", 36);
  const complexPocket = complex.sketch("complex_top_pocket_sketch", [rectangle(48, 0, 32, 32)], complexTop);
  complex.extrude("complex_top_pocket_cut", complexPocket, "cut", extDistance(8), { reversed: true });
  const bottomPocketSketch = complex.sketch("complex_bottom_pocket_sketch", [rectangle(48, 0, 24, 40)]);
  complex.extrude("complex_bottom_pocket_cut", bottomPocketSketch, "cut", extDistance(5));
  for (const [index, center] of [[-70, -40], [70, -40], [-70, 40], [70, 40]].entries()) {
    const slotSketchId = complex.sketch(`complex_slot_${index}_sketch`, [slot(center[0], center[1], 14, 7)]);
    complex.extrude(`complex_slot_${index}_cut`, slotSketchId, "cut", extThroughAll(), { reversed: true });
  }
  const complexDrawing = draw(complex.model);
  assertDrawing(complexDrawing);
  assert.equal(complexDrawing.spatial_summary.bounds.min_x, -90);
  assert.equal(complexDrawing.spatial_summary.bounds.max_x, 90);
  assert.equal(complexDrawing.spatial_summary.bounds.min_y, -60);
  assert.equal(complexDrawing.spatial_summary.bounds.max_y, 60);
  assert.equal(complexDrawing.spatial_summary.bounds.max_z, 48);
  t.diagnostic(`G1B_COMPLEX_SUBSET_TIMINGS_MS=${JSON.stringify(complexDrawing.parametric_timings_ms)}`);
});

test("G2 Hole, Counterbore, semantic face reference, and reusable patterns rebuild through CadQuery/OCP", { skip: !enabled }, (t) => {
  const addNode = (model, id, feature) => {
    model.features[id] = { type: "parametric", id, name: id, feature, suppressed: false };
    model.feature_order.push(id);
  };
  const model = modelBuilder("G2 generic machining fixture").model;
  model.part.name = "G2 generic machining fixture";
  const base = rectangle(0, 0, 140, 100);
  const baseSketch = {
    outer: { entities: base.map((geometry, index) => ({ id: `plate-edge-${index}`, geometry })) },
    inner_loops: [],
  };
  addNode(model, "plate_sketch", { type: "sketch", support: { kind: "principal", plane: "x_y" }, profile: baseSketch });
  addNode(model, "plate_add", { type: "extrude", profile: "plate_sketch", operation: "add", direction: { Normal: { reversed: false } }, extent: extDistance(20) });
  addNode(model, "top_datum", { type: "datum_plane", definition: { kind: "offset", base: { kind: "principal", plane: "x_y" }, distance: 20 } });
  const top = { kind: "datum", feature: "top_datum" };
  addNode(model, "boss_sketch", {
    type: "sketch", support: top,
    profile: { outer: { entities: [{ id: "boss-circle", geometry: circle([52, 30], 8) }] }, inner_loops: [] },
  });
  addNode(model, "boss_add", {
    type: "extrude", profile: "boss_sketch", operation: "add",
    direction: { Normal: { reversed: false } }, extent: extDistance(8),
  });
  const addHole = (id, x, y, diameter, { kind = { type: "simple" }, termination = { kind: "through_all" }, support = top, reversed = true } = {}) => {
    addNode(model, id, { type: "hole", support, position: { x, y }, diameter, reversed, termination, kind });
  };
  addHole("center_through_hole", 0, 0, 20);
  addHole("counterbored_hole", -35, 0, 8, { kind: { type: "counterbore", diameter: 14, depth: 5 } });
  addHole("top_blind_hole", -50, 0, 8, { termination: { kind: "blind", depth: 5 } });
  addHole("blind_counterbored_hole", 45, 0, 8, { kind: { type: "counterbore", diameter: 14, depth: 5 }, termination: { kind: "blind", depth: 10 } });
  const diagonal = 30 / Math.sqrt(2);
  addHole("circular_seed_hole", diagonal, diagonal, 5);
  addNode(model, "pcd_pattern", {
    type: "pattern", seed_feature_id: "circular_seed_hole",
    definition: { kind: "circular", axis: { x: 0, y: 0, z: 1 }, center: { x: 0, y: 0, z: 20 }, count: 4, radius: 30, start_angle_degrees: 45, angular_span_degrees: 360 },
  });
  addHole("linear_seed_hole", -30, -30, 4);
  addNode(model, "linear_hole_pattern", {
    type: "pattern", seed_feature_id: "linear_seed_hole",
    definition: { kind: "linear", direction: { x: 1, y: 0, z: 0 }, count: 3, spacing: 20 },
  });
  const faceSupport = {
    kind: "planar_face", source_feature_id: "plate_add", selector: "positive_x",
    qualifier: { normal: { x: 1, y: 0, z: 0 }, plane_offset: 70, tolerance: 0.000001 },
  };
  addHole("side_blind_hole", 0, 10, 6, { support: faceSupport, termination: { kind: "blind", depth: 15 }, reversed: true });
  const slotCurves = slot(-45, -20, 14, 6);
  addNode(model, "linear_slot_sketch", { type: "sketch", support: top, profile: { outer: { entities: slotCurves.map((geometry, index) => ({ id: `slot-edge-${index}`, geometry })) }, inner_loops: [] } });
  addNode(model, "linear_slot_cut", { type: "extrude", profile: "linear_slot_sketch", operation: "cut", direction: { Normal: { reversed: true } }, extent: extDistance(2) });
  addNode(model, "linear_pattern", {
    type: "pattern", seed_feature_id: "linear_slot_cut",
    definition: { kind: "linear", direction: { x: 0, y: 1, z: 0 }, count: 3, spacing: 20 },
  });

  const drawing = draw(model, "G2 generic fixture real CadQuery rebuild");
  assertDrawing(drawing);
  const semanticTypes = Object.values(model.features).map((node) => node.feature.type);
  assert.ok(semanticTypes.includes("hole"));
  assert.equal(semanticTypes.filter((type) => type === "pattern").length, 3);
  assert.equal(model.part.name, "G2 generic machining fixture");
  assert.equal(drawing.spatial_summary.parametric.operation_features.filter((id) => id === "pcd_pattern").length, 3);
  assert.equal(drawing.spatial_summary.parametric.operation_features.filter((id) => id === "linear_hole_pattern").length, 2);
  assert.equal(drawing.spatial_summary.parametric.operation_features.filter((id) => id === "linear_pattern").length, 2);
  assert.equal(drawing.drawing_report.view_count, 3);
  const associations = drawing.drawing_report.associative_dimensions ?? [];
  const associationById = new Map(associations.map((item) => [item.annotation_id, item]));
  const drawingDimensions = new Map(drawing.drawing_report.views.flatMap((view) => view.dimensions.map((item) => [item.id, item])));
  for (const [id, expected] of [
    ["associative.body.length", 140],
    ["associative.body.width", 100],
    ["associative.base.thickness", 20],
  ]) {
    const association = associationById.get(id);
    assert.equal(association?.status, "resolved", `${id} must resolve`);
    assert.equal(association?.nominal?.mm, expected, `${id} must carry the expected nominal`);
    const dimension = drawingDimensions.get(association.drawing_dimension_id);
    assert.ok(dimension, `${id} must exist as an actual DrawingModel dimension`);
    assert.ok(dimension.value_mm.includes(expected), `${id} DrawingModel value must match`);
  }
  const dimensionFor = (annotationId) => {
    const association = associationById.get(annotationId);
    assert.equal(association?.status, "resolved", `${annotationId} must resolve`);
    const dimension = drawingDimensions.get(association.drawing_dimension_id);
    assert.ok(dimension, `${annotationId} must link to an actual DrawingModel dimension`);
    return dimension;
  };
  const throughHoleDimension = dimensionFor("associative.hole.center_through_hole.callout");
  assert.ok(throughHoleDimension.value_mm.includes(20));
  assert.match(throughHoleDimension.formatted, /Ø20 THRU/);
  const bossDimension = dimensionFor("associative.boss.boss_add.diameter");
  assert.deepEqual(bossDimension.kind, { kind: "diameter" });
  assert.ok(bossDimension.value_mm.includes(16));
  assert.match(bossDimension.formatted, /Ø16/);
  const bossAssociation = associationById.get("associative.boss.boss_add.diameter");
  assert.equal(bossAssociation.reference.reference, "edge_reference");
  assert.equal(bossAssociation.reference.edge.selector.entity_id, "boss-circle");
  assert.ok(bossAssociation.witness.evidence_ids.some((id) => id.startsWith("face:")));
  assert.ok(bossAssociation.witness.evidence_ids.some((id) => id.startsWith("edge:")));
  const blindHoleDimension = dimensionFor("associative.hole.top_blind_hole.callout");
  assert.ok(blindHoleDimension.value_mm.includes(8));
  assert.ok(blindHoleDimension.value_mm.includes(5));
  assert.match(blindHoleDimension.formatted, /Ø8 ↧5/);
  const counterboreDimension = dimensionFor("associative.hole.blind_counterbored_hole.callout");
  for (const value of [8, 10, 14, 5]) assert.ok(counterboreDimension.value_mm.includes(value));
  assert.match(counterboreDimension.formatted, /Ø8 ↧10.*Ø14.*5/);
  const associationIds = new Set(associations.map((item) => item.annotation_id));
  for (const id of [
    "associative.body.length",
    "associative.body.width",
    "associative.base.thickness",
    "associative.hole.center_through_hole.callout",
    "associative.hole.counterbored_hole.callout",
    "associative.hole.top_blind_hole.callout",
    "associative.hole.blind_counterbored_hole.callout",
    "associative.hole.pcd_pattern.callout",
    "associative.hole.linear_hole_pattern.callout",
  ]) {
    assert.ok(associationIds.has(id), `missing associative dimension ${id}`);
  }
  assert.equal(associations.find((item) => item.annotation_id === "associative.hole.top_blind_hole.callout").observability, "annotated_from_validated_feature");
  const sideHole = associations.find((item) => item.annotation_id === "associative.hole.side_blind_hole.callout");
  assert.equal(sideHole.status, "unsupported");
  assert.equal(sideHole.drawing_dimension_id, null);
  assert.ok(sideHole.diagnostic);
  assert.match(drawing.svg, /Ø8 D4|Ø8/);
  assert.match(drawing.svg, /Ø16/);
  assert.match(drawing.dxf, /AICAD_DIMENSION/);
  assert.match(drawing.dxf, /THRU/);
  assert.match(drawing.dxf, /\n0\nDIMENSION\n/);
  const parsedDxf = parseDxfWithEzdxf(drawing.dxf);
  if (parsedDxf) {
    assert.equal(parsedDxf.audit_errors, 0);
    assert.equal(parsedDxf.audit_fixes, 0);
    const nativeBoss = parsedDxf.dimensions.find((item) => item.dimtype === 3 && /Ø16/.test(item.text));
    assert.ok(nativeBoss, "ezdxf must reimport a native diameter DIMENSION for the boss");
    assert.equal(nativeBoss.style, "AICAD_MM");
    assert.ok(Math.abs(nativeBoss.measurement - nativeBoss.stored_measurement) < 1e-6);
    assert.ok(nativeBoss.block_entities.includes("LINE"));
    assert.ok(nativeBoss.block_entities.includes("SOLID"));
    assert.ok(nativeBoss.block_entities.includes("TEXT"));
    const nativeLength = parsedDxf.dimensions.find((item) => item.dimtype === 0 && item.text === "140");
    assert.ok(nativeLength, "ezdxf must reimport the native overall length DIMENSION");
    assert.ok(Math.abs(nativeBoss.measurement / 16 - nativeLength.measurement / 140) < 1e-6, "boss and overall length must use the same Front view scale");
  }
  assert.ok(drawing.svg.includes('id="view_front"'));
  assert.ok(drawing.svg.includes('id="view_top"'));
  assert.ok(drawing.svg.includes('id="view_left"'));
  assert.ok(drawing.dxf.includes("SECTION"));
  assert.ok(drawing.spatial_summary.parametric.volume_mm3 < 280000);
  t.diagnostic(`G2_GENERIC_TIMINGS_MS=${JSON.stringify(drawing.parametric_timings_ms)}`);

  // Changing the seed's diameter rebuilds all derived circular-pattern copies.
  const modified = structuredClone(model);
  modified.features.circular_seed_hole.feature.diameter = 7;
  const rebuilt = draw(modified, "G2 changed seed pattern rebuild");
  assert.notEqual(rebuilt.spatial_summary.parametric.volume_mm3, drawing.spatial_summary.parametric.volume_mm3);
  assert.equal(rebuilt.spatial_summary.parametric.operation_features.filter((id) => id === "pcd_pattern").length, 3);

  // D8: a hole parameter edit rebuilds actual geometry and its linked DrawingModel callout.
  const editedHole = structuredClone(model);
  editedHole.features.top_blind_hole.feature.diameter = 7;
  editedHole.features.top_blind_hole.feature.termination.depth = 6;
  const editedDrawing = draw(editedHole, "G6A.1 changed blind hole associative dimension");
  const editedAssoc = editedDrawing.drawing_report.associative_dimensions.find((item) => item.annotation_id === "associative.hole.top_blind_hole.callout");
  assert.equal(editedAssoc.status, "resolved");
  assert.equal(editedAssoc.nominal.mm, 7);
  const front = editedDrawing.drawing_report.views.find((view) => view.kind === "front");
  const editedDim = front.dimensions.find((item) => item.id === editedAssoc.drawing_dimension_id);
  assert.match(editedDim.formatted, /Ø7/);
  assert.match(editedDim.formatted, /↧6/);

  const editedBoss = structuredClone(model);
  editedBoss.features.boss_sketch.feature.profile.outer.entities[0].geometry.radius = 10;
  const editedBossDrawing = draw(editedBoss, "G6A.2 changed boss diameter associative dimension");
  const editedBossAssoc = editedBossDrawing.drawing_report.associative_dimensions.find((item) => item.annotation_id === "associative.boss.boss_add.diameter");
  assert.equal(editedBossAssoc.status, "resolved");
  assert.equal(editedBossAssoc.nominal.mm, 20);
  const editedBossDimension = editedBossDrawing.drawing_report.views.flatMap((view) => view.dimensions).find((item) => item.id === editedBossAssoc.drawing_dimension_id);
  assert.match(editedBossDimension.formatted, /Ø20/);
  assert.match(editedBossDrawing.svg, /Ø20/);
  const editedBossDxf = parseDxfWithEzdxf(editedBossDrawing.dxf);
  if (editedBossDxf) assert.ok(editedBossDxf.dimensions.some((item) => item.dimtype === 3 && /Ø20/.test(item.text)));

  const suppressedBoss = structuredClone(model);
  suppressedBoss.features.boss_add.suppressed = true;
  const suppressedBossDrawing = draw(suppressedBoss, "G6A.2 suppressed boss removes associative diameter");
  assert.ok(!suppressedBossDrawing.drawing_report.associative_dimensions.some((item) => item.annotation_id === "associative.boss.boss_add.diameter"));

  // D9: suppressing a source removes its derived dimension; it is not rebound.
  const suppressedHole = structuredClone(model);
  suppressedHole.features.top_blind_hole.suppressed = true;
  const suppressedDrawing = draw(suppressedHole, "G6A.1 suppressed hole removes associative callout");
  assert.ok(!suppressedDrawing.drawing_report.associative_dimensions.some((item) => item.annotation_id === "associative.hole.top_blind_hole.callout"));

  // An active pattern whose seed is suppressed fails model-history validation.
  const suppressed = structuredClone(model);
  suppressed.features.circular_seed_hole.suppressed = true;
  const invalid = invoke(suppressed);
  assert.notEqual(invalid.status, 0);
  assert.match(`${invalid.stderr}\n${invalid.stdout}`, /PATTERN_SEED_NOT_FOUND|suppressed/i);

  const missingSeed = structuredClone(model);
  missingSeed.features.pcd_pattern.feature.seed_feature_id = "missing_hole";
  assert.equal(errorCode(missingSeed), "PATTERN_SEED_NOT_FOUND");

  const invalidCounterbore = structuredClone(model);
  invalidCounterbore.features.counterbored_hole.feature.kind = { type: "counterbore", diameter: 6, depth: 5 };
  assert.equal(errorCode(invalidCounterbore), "INVALID_COUNTERBORE");

  const collision = modelBuilder("G2 invalid pattern instance");
  const collisionSketch = collision.sketch("collision_base_sketch", [rectangle(0, 0, 100, 60)]);
  collision.extrude("collision_base_add", collisionSketch, "add", extDistance(10));
  const collisionTop = collision.datum("collision_top", "x_y", 10);
  const collisionSeedSketch = collision.sketch("collision_seed_sketch", [[circle([0, 0], 3)]], collisionTop);
  collision.extrude("collision_seed_cut", collisionSeedSketch, "cut", extThroughAll(), { reversed: true });
  addNode(collision.model, "collision_pattern", {
    type: "pattern", seed_feature_id: "collision_seed_cut",
    definition: { kind: "linear", direction: { x: 1, y: 0, z: 0 }, count: 2, spacing: 200 },
  });
  assert.equal(errorCode(collision.model), "PATTERN_INSTANCE_COLLISION");

  // The user complex-part subset stays generic Hole + Pattern semantics.
  const complex = modelBuilder("AICAD-COMPLEX-001 G2 generic subset").model;
  complex.part.name = "AICAD-COMPLEX-001 G2 generic subset";
  const complexBase = rectangle(0, 0, 180, 120);
  addNode(complex, "complex_plate_sketch", { type: "sketch", support: { kind: "principal", plane: "x_y" }, profile: { outer: { entities: complexBase.map((geometry, index) => ({ id: `complex-edge-${index}`, geometry })) }, inner_loops: [] } });
  addNode(complex, "complex_plate_add", { type: "extrude", profile: "complex_plate_sketch", operation: "add", direction: { Normal: { reversed: false } }, extent: extDistance(24) });
  addNode(complex, "complex_top", { type: "datum_plane", definition: { kind: "offset", base: { kind: "principal", plane: "x_y" }, distance: 24 } });
  const complexTop = { kind: "datum", feature: "complex_top" };
  addNode(complex, "left_center_hole", { type: "hole", support: complexTop, position: { x: -50, y: 0 }, diameter: 32, reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" } });
  const patternRadius = 26;
  const patternStart = 45 * Math.PI / 180;
  addNode(complex, "left_pattern_seed", { type: "hole", support: complexTop, position: { x: -50 + patternRadius * Math.cos(patternStart), y: patternRadius * Math.sin(patternStart) }, diameter: 5.5, reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" } });
  addNode(complex, "left_pattern", { type: "pattern", seed_feature_id: "left_pattern_seed", definition: { kind: "circular", axis: { x: 0, y: 0, z: 1 }, center: { x: -50, y: 0, z: 24 }, count: 4, radius: patternRadius, start_angle_degrees: 45, angular_span_degrees: 360 } });
  addNode(complex, "right_center_hole", { type: "hole", support: complexTop, position: { x: 48, y: 0 }, diameter: 10, reversed: true, termination: { kind: "through_all" }, kind: { type: "counterbore", diameter: 16, depth: 3 } });
  addNode(complex, "right_pattern_seed", { type: "hole", support: complexTop, position: { x: 48, y: 18 }, diameter: 6.6, reversed: true, termination: { kind: "through_all" }, kind: { type: "counterbore", diameter: 11, depth: 4 } });
  addNode(complex, "right_pattern", { type: "pattern", seed_feature_id: "right_pattern_seed", definition: { kind: "circular", axis: { x: 0, y: 0, z: 1 }, center: { x: 48, y: 0, z: 24 }, count: 4, radius: 18, start_angle_degrees: 90, angular_span_degrees: 360 } });
  const complexFaceSupport = {
    kind: "planar_face", source_feature_id: "complex_plate_add", selector: "positive_x",
    qualifier: { normal: { x: 1, y: 0, z: 0 }, plane_offset: 90, tolerance: 0.000001 },
  };
  addNode(complex, "complex_side_blind", { type: "hole", support: complexFaceSupport, position: { x: 0, y: 12 }, diameter: 8, reversed: true, termination: { kind: "blind", depth: 18 }, kind: { type: "simple" } });
  const complexDrawing = draw(complex, "AICAD-COMPLEX-001 G2 subset");
  assertDrawing(complexDrawing);
  assert.ok(Object.values(complex.features).every((node) => node.feature.type !== "legacy_hole"));
  t.diagnostic(`G2_COMPLEX_SUBSET_TIMINGS_MS=${JSON.stringify(complexDrawing.parametric_timings_ms)}`);
});

test("G3 semantic edge treatments rebuild real OCP geometry and export three views", { skip: !enabled }, (t) => {
  const makeFixture = ({ radius = 3, filletSuppressed = false, chamferDistance = 0.8 } = {}) => {
    const builder = modelBuilder("G3 generic semantic topology fixture");
    const baseSketch = builder.sketch("plate_sketch", [rectangle(0, 0, 120, 90)]);
    builder.extrude("plate", baseSketch, "add", extDistance(20));

    const bossDatum = builder.datum("boss_datum", "x_y", 20);
    const bossSketch = builder.sketch("boss_sketch", [rectangle(23, 13, 30, 22)], { kind: "datum", feature: "boss_datum" });
    builder.extrude("boss", bossSketch, "add", extDistance(15));

    const holeDatum = builder.datum("hole_datum", "x_y", 20);
    builder.addFeature("through_hole", {
      type: "hole", support: { kind: "datum", feature: "hole_datum" }, position: point([-30, -12]),
      diameter: 16, reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
    });

    const pocketDatum = builder.datum("pocket_datum", "x_y", 20);
    const pocketSketch = builder.sketch("pocket_sketch", [rectangle(-32, 26, 40, 24)], { kind: "datum", feature: "pocket_datum" });
    builder.extrude("pocket", pocketSketch, "cut", extDistance(6), { reversed: true });

    const qualifier = { curve_kind: "line", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.000001 };
    const bossEdges = [0, 1, 2, 3].map((index) => ({
      source_feature_id: "boss",
      selector: { kind: "profile_boundary", entity_id: `boss_sketch-outer-${String(index).padStart(4, "0")}`, side: "top" },
      qualifier,
    }));
    builder.addFeature("boss_fillet", { type: "fillet", edge_refs: bossEdges, radius }, filletSuppressed);
    builder.addFeature("hole_chamfer", {
      type: "chamfer", distance: chamferDistance,
      edge_refs: [{
        source_feature_id: "through_hole", selector: { kind: "hole_entrance" },
        qualifier: { ...qualifier, curve_kind: "circular", radius_mm: 8 },
      }],
    });
    return builder.model;
  };

  const suppressed = draw(makeFixture({ filletSuppressed: true }), "G3 suppressed Fillet full-history rebuild");
  const radiusTwo = draw(makeFixture({ radius: 2 }), "G3 modified Fillet R2 rebuild");
  const radiusThree = draw(makeFixture({ radius: 3 }), "G3 Fillet R3 + hole Chamfer rebuild");
  const repeated = draw(makeFixture({ radius: 3 }), "G3 deterministic repeated rebuild");
  for (const drawing of [suppressed, radiusTwo, radiusThree, repeated]) assertDrawing(drawing);
  assert.ok(!suppressed.spatial_summary.parametric.operation_features.includes("boss_fillet"), "suppressed Fillet must be omitted from the rebuilt B-Rep history");
  assert.ok(suppressed.spatial_summary.parametric.operation_features.includes("hole_chamfer"));
  assert.ok(radiusThree.spatial_summary.parametric.operation_features.includes("boss_fillet"));
  assert.ok(radiusThree.spatial_summary.parametric.operation_features.includes("hole_chamfer"));
  assert.ok(radiusThree.spatial_summary.parametric.conical_face_count > 0);
  assert.notEqual(volume(suppressed), volume(radiusTwo));
  assert.notEqual(volume(radiusTwo), volume(radiusThree), "editing R2 to R3 must rebuild geometry from EngineeringModel");
  assert.deepEqual(radiusThree.spatial_summary, repeated.spatial_summary);
  assert.equal(radiusThree.svg, repeated.svg);
  assert.equal(radiusThree.dxf, repeated.dxf);
  assert.ok(Number.isFinite(radiusThree.parametric_timings_ms.kernel_rebuild_ms));
  assert.ok(radiusThree.parametric_timings_ms.combined_fixture_ms > 0);
  assert.ok(radiusThree.parametric_timings_ms.topology_resolution_ms >= 0);
  assert.ok(radiusThree.parametric_timings_ms.base_rebuild_ms > 0);
  assert.ok(radiusThree.parametric_timings_ms.fillet_rebuild_ms > 0);
  assert.ok(radiusThree.parametric_timings_ms.chamfer_rebuild_ms > 0);
  assert.ok(radiusThree.parametric_timings_ms.three_view_generation_ms > 0);
  assert.ok(radiusThree.parametric_timings_ms.svg_export_ms > 0);
  assert.ok(radiusThree.parametric_timings_ms.dxf_export_ms > 0);
  assert.deepEqual(radiusThree.parametric_timings_ms.operation_timings.map((timing) => timing.kind), ["extrude_add", "extrude_add", "extrude_cut", "extrude_cut", "fillet", "chamfer"]);
  assert.ok(Number.isFinite(radiusThree.parametric_timings_ms.svg_export_ms));
  assert.ok(Number.isFinite(radiusThree.parametric_timings_ms.dxf_export_ms));
  t.diagnostic(`G3_GENERIC_TIMINGS_MS=${JSON.stringify(radiusThree.parametric_timings_ms)}`);
});

test("G3 AICAD-COMPLEX-001 subset applies only the requested semantic R1/C0.5/C1 edges", { skip: !enabled }, (t) => {
  const builder = modelBuilder("AICAD-COMPLEX-001 G3 bounded subset");
  const baseSketch = builder.sketch("complex_plate_sketch", [rectangle(0, 0, 180, 120)]);
  builder.extrude("complex_plate", baseSketch, "add", extDistance(24));

  const bossDatum = builder.datum("left_boss_datum", "x_y", 24);
  const bossSketch = builder.sketch("left_boss_sketch", [[circle([-50, 0], 33)]], { kind: "datum", feature: "left_boss_datum" });
  builder.extrude("left_round_boss", bossSketch, "add", extDistance(24));

  const holeDatum = builder.datum("left_boss_top", "x_y", 48);
  builder.addFeature("left_center_hole", {
    type: "hole", support: { kind: "datum", feature: "left_boss_top" }, position: point([-50, 0]),
    diameter: 32, reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
  });

  const qualifier = { curve_kind: "line", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.000001 };
  builder.addFeature("left_boss_fillet", {
    type: "fillet", radius: 1,
    edge_refs: [{
      source_feature_id: "left_round_boss",
      selector: { kind: "profile_boundary", entity_id: "left_boss_sketch-outer-0000", side: "top" },
      qualifier: { ...qualifier, curve_kind: "circular", radius_mm: 33 },
    }],
  });
  builder.addFeature("left_hole_chamfer", {
    type: "chamfer", distance: 0.5,
    edge_refs: [{
      source_feature_id: "left_center_hole", selector: { kind: "hole_entrance" },
      qualifier: { ...qualifier, curve_kind: "circular", radius_mm: 16 },
    }],
  });
  builder.addFeature("plate_bottom_edge_chamfer", {
    type: "chamfer", distance: 1,
    edge_refs: [{
      source_feature_id: "complex_plate",
      selector: { kind: "profile_boundary", entity_id: "complex_plate_sketch-outer-0000", side: "bottom" },
      qualifier,
    }],
  });

  const drawing = draw(builder.model, "AICAD-COMPLEX-001 bounded G3 subset");
  assertDrawing(drawing);
  for (const id of ["left_boss_fillet", "left_hole_chamfer", "plate_bottom_edge_chamfer"]) {
    assert.ok(drawing.spatial_summary.parametric.operation_features.includes(id), `${id} must be an active formal feature`);
  }
  assert.equal(drawing.spatial_summary.parametric.operation_features.filter((id) => id === "plate_bottom_edge_chamfer").length, 1, "only the named non-functional plate edge is chamfered");
  assert.ok(volume(drawing) > 0);
  assert.ok(drawing.spatial_summary.parametric.toroidal_face_count > 0, "the cylindrical boss R1 must remain exact toroidal B-Rep evidence");
  assert.ok(drawing.spatial_summary.parametric.conical_face_count > 0, "the hole-mouth C0.5 must remain exact conical B-Rep evidence");
  assert.ok(drawing.spatial_summary.parametric.derived_projected_edge_count > 0, "HLR curve restorations must report their bounded derived provenance");
  assert.ok(Number.isFinite(drawing.parametric_timings_ms.kernel_rebuild_ms));
  t.diagnostic(`G3_COMPLEX_SUBSET_TIMINGS_MS=${JSON.stringify(drawing.parametric_timings_ms)}`);
});


test("G4 generic DatumAxis/Revolve shares one Rust→CadQuery feature history with Extrude, Hole, Pattern, Fillet, and Chamfer", { skip: !enabled }, (t) => {
  const makeShaft = (outerRadius = 20, treatments = true) => {
    const builder = modelBuilder("G4 generic stepped revolved body");
    builder.datumAxis("shaft_axis", {
      kind: "through_two_points", first: { x: 0, y: 0, z: 0 }, second: { x: 10, y: 0, z: 0 },
    });
    const profile = [
      line([0, 0], [0, outerRadius]),
      line([0, outerRadius], [20, outerRadius]),
      line([20, outerRadius], [20, 15]),
      line([20, 15], [60, 15]),
      line([60, 15], [60, 10]),
      line([60, 10], [80, 10]),
      line([80, 10], [80, 0]),
      line([80, 0], [0, 0]),
    ];
    const sketchId = builder.sketch("shaft_sketch", [profile]);
    builder.revolve("shaft_revolve", sketchId, "shaft_axis");

    // A real side pocket is cut into the revolved solid using the existing
    // generic Sketch + Extrude CUT path.
    const groove = builder.sketch("side_groove_sketch", [rectangle(40, 0, 8, 6)], { kind: "principal", plane: "x_z" });
    builder.extrude("side_groove", groove, "cut", extDistance(25));

    // Both a coaxial bore and a four-instance circular cut pattern use the
    // same Hole/Pattern types already used on extruded bodies.
    builder.addFeature("axial_bore", {
      type: "hole", support: { kind: "principal", plane: "y_z" }, position: point([0, 0]),
      diameter: 4, reversed: false, termination: { kind: "through_all" }, kind: { type: "simple" },
    });
    builder.addFeature("radial_seed", {
      type: "hole", support: { kind: "principal", plane: "y_z" }, position: point([6, 0]),
      diameter: 3, reversed: false, termination: { kind: "through_all" }, kind: { type: "simple" },
    });
    builder.addFeature("radial_pattern", {
      type: "pattern", seed_feature_id: "radial_seed",
      definition: {
        kind: "circular", axis: { x: 1, y: 0, z: 0 }, center: { x: 0, y: 0, z: 0 },
        count: 4, radius: 6, start_angle_degrees: 0, angular_span_degrees: 360,
      },
    });

    if (treatments) {
      const circular = { curve_kind: "circular", radius_mm: outerRadius, length_mm: null, midpoint: null, tolerance_mm: 0.000001 };
      builder.addFeature("step_fillet", {
        type: "fillet", radius: 0.5,
        edge_refs: [{
          source_feature_id: "shaft_revolve",
          selector: { kind: "revolve_profile_vertex", entity_id: "shaft_sketch-outer-0002", endpoint: "start" },
          qualifier: circular,
        }],
      });
      builder.addFeature("end_chamfer", {
        type: "chamfer", distance: 0.5,
        edge_refs: [{
          source_feature_id: "shaft_revolve",
          selector: { kind: "revolve_profile_vertex", entity_id: "shaft_sketch-outer-0000", endpoint: "end" },
          qualifier: circular,
        }],
      });
    }
    return builder;
  };

  const builder = makeShaft();
  const drawing = draw(builder.model, "G4 generic stepped body with mixed features");
  assertDrawing(drawing);
  const operations = drawing.spatial_summary.parametric.operation_features;
  for (const id of ["shaft_revolve", "side_groove", "axial_bore", "radial_seed", "radial_pattern", "step_fillet", "end_chamfer"]) {
    assert.ok(operations.includes(id), `${id} must reach the formal OCP history`);
  }
  assert.equal(drawing.spatial_summary.bounds.min_x, 0);
  assert.equal(drawing.spatial_summary.bounds.max_x, 80);
  assert.ok(drawing.spatial_summary.parametric.cylindrical_face_count > 0, "revolve must retain analytic cylindrical face evidence");
  const filletAssociation = drawing.drawing_report.associative_dimensions.find((item) => item.annotation_id === "associative.fillet.step_fillet.radius");
  assert.equal(filletAssociation.status, "unsupported");
  assert.equal(filletAssociation.observability, "requires_section_view");
  assert.equal(filletAssociation.nominal.mm, 0.5);
  assert.equal(filletAssociation.drawing_dimension_id, null);
  assert.ok(volume(drawing) > 0);
  assert.ok(Number.isFinite(drawing.parametric_timings_ms.kernel_rebuild_ms));
  assert.ok(drawing.parametric_timings_ms.operation_timings.some((timing) => timing.kind === "revolve_add"));
  assert.ok(drawing.parametric_timings_ms.operation_timings.some((timing) => timing.kind === "revolve_profile_cut" || timing.kind === "extrude_cut"));

  const repeated = draw(builder.model, "G4 repeated deterministic mixed history rebuild");
  assert.equal(volume(repeated), volume(drawing));
  assert.deepEqual(repeated.spatial_summary, drawing.spatial_summary);
  assert.equal(repeated.svg, drawing.svg);
  assert.equal(repeated.dxf, drawing.dxf);

  const modified = draw(makeShaft(22).model, "G4 modified Revolve profile full-history rebuild");
  assert.notEqual(volume(modified), volume(drawing), "upstream Sketch dimension change must rebuild the Revolve body and downstream features");
  assertDrawing(modified);
  t.diagnostic(`G4_STEPPED_MIXED_TIMINGS_MS=${JSON.stringify(drawing.parametric_timings_ms)}`);

  // A second composition proves Revolve CUT is an ordinary ordered feature
  // after an Extrude ADD, not a separate part mode.
  const plate = modelBuilder("G4 Extrude then Revolve CUT");
  const plateSketch = plate.sketch("plate_sketch", [rectangle(0, 0, 100, 80)]);
  plate.extrude("plate_add", plateSketch, "add", extDistance(20));
  plate.datumAxis("ring_axis", { kind: "principal", axis: "z" });
  const ringProfile = plate.sketch("ring_cut_sketch", [rectangle(22.5, 0, 5, 20)], { kind: "principal", plane: "x_z" });
  plate.revolve("ring_cut", ringProfile, "ring_axis", "cut");
  const plateDrawing = draw(plate.model, "G4 Extrude ADD followed by Revolve CUT");
  assertDrawing(plateDrawing);
  assert.ok(plateDrawing.spatial_summary.parametric.operation_features.includes("plate_add"));
  assert.ok(plateDrawing.spatial_summary.parametric.operation_features.includes("ring_cut"));
  assert.ok(volume(plateDrawing) > 0 && volume(plateDrawing) < 100 * 80 * 20, "Revolve CUT must remove real material from the extruded body");
  t.diagnostic(`G4_EXTRUDE_REVOLVE_CUT_TIMINGS_MS=${JSON.stringify(plateDrawing.parametric_timings_ms)}`);

  // Suppression is a complete rebuild: the same Extrude base remains valid
  // when the generic Revolve ADD is suppressed and returns deterministically
  // when it is enabled again.
  const makeSuppressibleHistory = (suppressed = false) => {
    const history = modelBuilder("G4 suppressible Revolve on an Extrude base");
    const disk = history.sketch("disk_sketch", [[circle([0, 0], 20)]]);
    history.extrude("disk_add", disk, "add", extDistance(10));
    history.datumAxis("disk_axis", { kind: "principal", axis: "z" });
    const ring = history.sketch("ring_profile", [rectangle(22.5, 5, 5, 10)], { kind: "principal", plane: "x_z" });
    history.revolve("ring_add", ring, "disk_axis", "add", { kind: "full_360" }, "forward", suppressed);
    return history;
  };
  const ringActive = draw(makeSuppressibleHistory(false).model, "G4 active Revolve ADD");
  const ringSuppressed = draw(makeSuppressibleHistory(true).model, "G4 suppressed Revolve full-history rebuild");
  const ringRestored = draw(makeSuppressibleHistory(false).model, "G4 unsuppressed Revolve rebuild");
  assert.ok(ringActive.spatial_summary.parametric.operation_features.includes("ring_add"));
  assert.ok(!ringSuppressed.spatial_summary.parametric.operation_features.includes("ring_add"));
  assert.ok(ringSuppressed.spatial_summary.parametric.operation_features.includes("disk_add"));
  assert.ok(volume(ringActive) > volume(ringSuppressed) && volume(ringSuppressed) > 0);
  assert.equal(volume(ringActive), volume(ringRestored));
  assert.deepEqual(ringActive.spatial_summary, ringRestored.spatial_summary);
  assertDrawing(ringActive);
  assertDrawing(ringSuppressed);
});


function makeSweepAddHistory() {
  const builder = modelBuilder("G7A generic curved Sweep ADD fixture");
  const base = builder.sketch("base_sketch", [rectangle(0, 0, 100, 60)]);
  builder.extrude("base_add", base, "add", extDistance(10));
  const profile = builder.sketch("sweep_profile", [[circle([0, 0], 2)]], { kind: "principal", plane: "y_z" });
  builder.addFeature("curve_path", {
    type: "path", support: { kind: "principal", plane: "x_y" },
    segments: [{ id: "curve_path_arc", geometry: arc([0, 0], [40, 40], [0, 40], 40) }],
  });
  builder.addFeature("curved_sweep", {
    type: "sweep", profile, path: "curve_path", operation: "add", direction: "forward",
    profile_origin: point([0, 0]), orientation: "fixed_binormal",
  });
  const top = builder.datum("top_datum", "x_y", 10);
  builder.addFeature("clearance_hole", {
    type: "hole", support: top, position: point([-30, 0]), diameter: 4, reversed: true,
    termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("base_top_edge_fillet", {
    type: "fillet", radius: 0.5,
    edge_refs: [{
      source_feature_id: "base_add",
      selector: { kind: "profile_boundary", entity_id: "base_sketch-outer-0000", side: "top" },
      qualifier: { curve_kind: "line", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.000001 },
    }],
  });
  return builder;
}

function makeLoftAddHistory() {
  const builder = modelBuilder("G7B general ruled Loft ADD fixture");
  builder.datumAxis("body_axis", { kind: "principal", axis: "z" });
  const revolveProfile = builder.sketch("revolve_profile", [rectangle(7.5, 10, 15, 20)], {
    kind: "principal", plane: "x_z",
  });
  builder.revolve("revolve_body", revolveProfile, "body_axis");

  const sectionAPlane = builder.datum("section_a_plane", "x_y", 20);
  const sectionA = builder.sketch("section_a", [[circle([0, 0], 15)]], sectionAPlane);
  const sectionBPlane = builder.datum("section_b_plane", "x_y", 45);
  const sectionB = builder.sketch("section_b", [[circle([5, 0], 20)]], sectionBPlane);
  const sectionCPlane = builder.datum("section_c_plane", "x_y", 70);
  const sectionC = builder.sketch("section_c", [[circle([-3, 2], 13)]], sectionCPlane);
  builder.loft("loft_body", [sectionA, sectionB, sectionC]);
  builder.addFeature("loft_hole", {
    type: "hole", support: sectionCPlane, position: point([0, 0]), diameter: 4,
    reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  return builder;
}

function generatedFaceReference(sourceFeatureId, selector) {
  return { source_feature_id: sourceFeatureId, selector, qualifier: { tolerance_mm: 0.00001 } };
}

function makeSweepGeneratedTopologyHistory() {
  const builder = makeSweepAddHistory();
  const hole = builder.model.features.clearance_hole.feature;
  hole.support = { kind: "generated_face", reference: generatedFaceReference("curved_sweep", { kind: "sweep_end_cap" }) };
  hole.position = point([0, 0]);
  hole.reversed = true;
  builder.addFeature("sweep_end_rim_fillet", {
    type: "fillet", radius: 0.25,
    edge_refs: [{
      source_feature_id: "curved_sweep",
      selector: { kind: "between_generated_faces",
        first: generatedFaceReference("curved_sweep", { kind: "sweep_end_cap" }),
        second: generatedFaceReference("curved_sweep", { kind: "sweep_lateral", profile_entity_id: "sweep_profile-outer-0000" }) },
      qualifier: { curve_kind: "circular", radius_mm: 2, length_mm: null, midpoint: null, tolerance_mm: 0.00001 },
    }],
  });
  return builder;
}

function makeLoftGeneratedTopologyHistory() {
  const builder = modelBuilder("G7C generic Loft generated topology fixture");
  const planeA = builder.datum("section_a_plane", "x_y", 0);
  const sectionA = builder.sketch("section_a", [[circle([0, 0], 10)]], planeA);
  const planeB = builder.datum("section_b_plane", "x_y", 25);
  const sectionB = builder.sketch("section_b", [[circle([2, 0], 15)]], planeB);
  const planeC = builder.datum("section_c_plane", "x_y", 55);
  const sectionC = builder.sketch("section_c", [[circle([-3, 2], 13)]], planeC);
  builder.loft("loft_body", [sectionA, sectionB, sectionC]);
  builder.addFeature("loft_end_hole", {
    type: "hole",
    support: { kind: "generated_face", reference: generatedFaceReference("loft_body", { kind: "loft_end_cap" }) },
    position: point([-3, 2]), diameter: 3, reversed: true,
    termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("loft_end_rim_chamfer", {
    type: "chamfer", distance: 0.4,
    edge_refs: [{
      source_feature_id: "loft_body",
      selector: { kind: "between_generated_faces",
        first: generatedFaceReference("loft_body", { kind: "loft_end_cap" }),
        second: generatedFaceReference("loft_body", {
          kind: "loft_lateral", start_section_id: "section_b", end_section_id: "section_c",
          start_entity_id: "section_b-outer-0000", end_entity_id: "section_c-outer-0000",
        }) },
      qualifier: { curve_kind: "circular", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.00001 },
    }],
  });
  return builder;
}

function makeLoftCutHistory() {
  const builder = modelBuilder("G7B general ruled Loft CUT fixture");
  const base = builder.sketch("cut_base_sketch", [rectangle(0, 0, 100, 80)]);
  builder.extrude("cut_base_add", base, "add", extDistance(40));
  const sectionAPlane = builder.datum("cut_section_a_plane", "x_y", 2);
  const sectionA = builder.sketch("cut_section_a", [[circle([0, 0], 4)]], sectionAPlane);
  const sectionBPlane = builder.datum("cut_section_b_plane", "x_y", 20);
  const sectionB = builder.sketch("cut_section_b", [[circle([1, 0], 8)]], sectionBPlane);
  const sectionCPlane = builder.datum("cut_section_c_plane", "x_y", 38);
  const sectionC = builder.sketch("cut_section_c", [[circle([2, 1], 5)]], sectionCPlane);
  builder.loft("loft_cut", [sectionA, sectionB, sectionC], "cut");
  return builder;
}

function makeSweepRevolveCutHistory() {
  const builder = modelBuilder("G7A generic Revolve and curved Sweep CUT fixture");
  builder.datumAxis("body_axis", { kind: "principal", axis: "z" });
  const revolveProfile = builder.sketch("revolve_profile", [rectangle(7.5, 10, 15, 20)], {
    kind: "principal", plane: "x_z",
  });
  builder.revolve("revolve_body", revolveProfile, "body_axis", "add", { kind: "full_360" });
  const pathSupport = builder.datum("path_support", "x_y", 10);
  builder.addFeature("cut_path", {
    type: "path", support: pathSupport,
    segments: [{ id: "cut_arc", geometry: arc([6, 0], [0, 6], [0, 0], 6) }],
  });
  builder.addFeature("cut_profile_plane", {
    type: "datum_plane",
    definition: { kind: "custom_plane", origin: [6, 0, 10], normal: [0, 1, 0], x_axis: [-1, 0, 0] },
  });
  const profile = builder.sketch("cut_profile", [[circle([0, 0], 1)]], {
    kind: "datum", feature: "cut_profile_plane",
  });
  builder.addFeature("curved_sweep_cut", {
    type: "sweep", profile, path: "cut_path", operation: "cut", direction: "forward",
    profile_origin: point([0, 0]), orientation: "fixed_binormal",
  });
  return builder;
}

test("G7A generic curved Sweep joins Drawing, mixed history and canonical STEP", { skip: !enabled }, async (t) => {
  const fixture = makeSweepAddHistory();
  const drawing = draw(fixture.model, "G7A Extrude → Sweep ADD → Hole → Fillet");
  assertDrawing(drawing);
  for (const feature of ["base_add", "curved_sweep", "clearance_hole", "base_top_edge_fillet"]) {
    assert.ok(drawing.spatial_summary.parametric.operation_features.includes(feature), `${feature} must be in the active B-Rep history`);
  }
  assert.ok(volume(drawing) > 100 * 60 * 10, "curved Sweep ADD must add material to the plate");
  t.diagnostic(`G7A_SWEEP_ADD_DRAWING_TIMINGS_MS=${JSON.stringify(drawing.parametric_timings_ms)}`);

  const pathEdited = structuredClone(fixture.model);
  pathEdited.features.curve_path.feature.segments[0].geometry = arc([0, 0], [35, 35], [0, 35], 35);
  const changedPathDrawing = draw(pathEdited, "G7A rebuilt after Path edit");
  assert.notDeepEqual(changedPathDrawing.spatial_summary.bounds, drawing.spatial_summary.bounds);
  const profileEdited = structuredClone(fixture.model);
  profileEdited.features.sweep_profile.feature.profile.outer.entities[0].geometry.radius = 2.5;
  const changedProfileDrawing = draw(profileEdited, "G7A rebuilt after profile edit");
  assert.ok(volume(changedProfileDrawing) > volume(drawing));
  const suppressed = structuredClone(fixture.model);
  suppressed.features.curved_sweep.suppressed = true;
  const suppressedDrawing = draw(suppressed, "G7A suppressed Sweep history");
  assert.ok(!suppressedDrawing.spatial_summary.parametric.operation_features.includes("curved_sweep"));
  assert.ok(volume(suppressedDrawing) < volume(drawing));
  const restored = draw(fixture.model, "G7A restored Sweep history");
  assert.deepEqual(restored.spatial_summary, drawing.spatial_summary, "unsuppress must deterministically restore B-Rep evidence");

  const delivered = exportStep(fixture.model);
  assert.equal(delivered.error, undefined, delivered.error?.message);
  assert.equal(delivered.status, 0, delivered.stderr.toString("utf8"));
  const bytes = delivered.stdout;
  const hash = createHash("sha256").update(bytes).digest("hex");
  assert.match(bytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
  const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(delivered.stderr.toString("utf8"));
  assert.ok(metadata, delivered.stderr.toString("utf8"));
  assert.equal(metadata[1], hash);
  assert.equal(Number(metadata[2]), bytes.length);
  const observed = observeStepArtifact(bytes);
  assert.equal(observed.solid_count, 1);
  assert.equal(observed.valid, true);
  assert.ok(observed.surfaces.some((surface) => surface.kind === "torus" && Math.abs(surface.major_radius - 40) <= 1e-5 && Math.abs(surface.minor_radius - 2) <= 1e-5), "STEP re-import must preserve the curved Sweep radii");
  assert.ok(Math.abs(observed.volume_mm3 - volume(drawing)) <= 1e-5,
    "the independent STEP re-import volume must match the actual Drawing source body");

  const revolveCut = makeSweepRevolveCutHistory();
  const cutDrawing = draw(revolveCut.model, "G7A Revolve ADD → Sweep CUT");
  assertDrawing(cutDrawing);
  assert.ok(cutDrawing.spatial_summary.parametric.operation_features.includes("curved_sweep_cut"));
  assert.ok(volume(cutDrawing) > 0 && volume(cutDrawing) < Math.PI * 15 ** 2 * 20, "curved Sweep CUT must remove actual material from the Revolve body");
  t.diagnostic(`G7A_SWEEP_CUT_DRAWING_TIMINGS_MS=${JSON.stringify(cutDrawing.parametric_timings_ms)}`);

  const cutSectionRoot = mkdtempSync(join(tmpdir(), "aicad-g7a-cut-section-"));
  let cutSection;
  try {
    const modelPath = join(cutSectionRoot, "model.json");
    writeFileSync(modelPath, `${JSON.stringify(revolveCut.model)}\n`, "utf8");
    const sectionResult = spawnSync(binary, ["--json-errors", "--quiet", "engineering", "drawing-section", modelPath,
      "--axis", "y", "--offset-mm", "3", "--view-side", "positive"], {
      encoding: "utf8", maxBuffer: 32 * 1024 * 1024, env: process.env,
    });
    assert.equal(sectionResult.error, undefined, sectionResult.error?.message);
    assert.equal(sectionResult.status, 0, `G7A Sweep CUT Section A-A\n${sectionResult.stderr}`);
    cutSection = JSON.parse(sectionResult.stdout);
  } finally {
    rmSync(cutSectionRoot, { recursive: true, force: true });
  }
  assert.equal(cutSection.drawing_mode, "three_view_section");
  assert.equal(cutSection.drawing_report.view_count, 4);
  assert.ok(cutSection.section_view.section_edge_count > 0);
  assert.ok(cutSection.section_view.boundary_loop_count >= 2);
  assert.match(cutSection.svg, /section_aa_hatch/);
  assert.match(cutSection.dxf, /AICAD_HATCH/);

  const cutDelivered = exportStep(revolveCut.model);
  assert.equal(cutDelivered.error, undefined, cutDelivered.error?.message);
  assert.equal(cutDelivered.status, 0, `G7A Sweep CUT formal STEP\n${cutDelivered.stderr.toString("utf8")}`);
  const cutBytes = cutDelivered.stdout;
  const cutHash = createHash("sha256").update(cutBytes).digest("hex");
  const cutMetadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(cutDelivered.stderr.toString("utf8"));
  assert.ok(cutMetadata, cutDelivered.stderr.toString("utf8"));
  assert.equal(cutMetadata[1], cutHash);
  assert.equal(Number(cutMetadata[2]), cutBytes.length);
  const cutObserved = observeStepArtifact(cutBytes);
  assert.equal(cutObserved.solid_count, 1);
  assert.equal(cutObserved.valid, true);
  assert.ok(cutObserved.surfaces.some((surface) => surface.kind === "torus"
    && Math.abs(surface.major_radius - 6) <= 1e-5 && Math.abs(surface.minor_radius - 1) <= 1e-5),
  "the STEP CUT geometry must preserve the declared curved path and profile radii");
  assert.ok(cutObserved.volume_mm3 > 0 && cutObserved.volume_mm3 < Math.PI * 15 ** 2 * 20);
  assert.ok(Math.abs(cutObserved.volume_mm3 - volume(cutDrawing)) <= 1e-5,
    "Drawing and re-imported formal Sweep CUT must agree on the actual body volume");
  t.diagnostic(`G7A_SWEEP_CUT_SECTION_EDGES=${cutSection.section_view.section_edge_count};STEP_BYTES=${cutBytes.length};SHA256=${cutHash};REIMPORTED_VOLUME_MM3=${cutObserved.volume_mm3}`);

  const port = 4177;
  const server = await startLocalServer(port);
  try {
    const response = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: fixture.model, projectName: "G7A generic curved sweep" }),
    });
    assert.equal(response.status, 200, await response.clone().text());
    const apiBytes = Buffer.from(await response.arrayBuffer());
    const apiHash = createHash("sha256").update(apiBytes).digest("hex");
    assert.ok(apiBytes.length > 1000);
    assert.match(apiBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
    assert.equal(response.headers.get("x-aicad-step-sha256"), apiHash);
    assert.equal(Number(response.headers.get("x-aicad-step-size")), apiBytes.length);

    const invalid = structuredClone(fixture.model);
    invalid.features.curve_path.feature.segments.push({ id: "disconnected_segment", geometry: line([100, 100], [110, 100]) });
    const failed = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: invalid, projectName: "invalid sweep" }),
    });
    assert.equal(failed.status, 422);
    const errorBody = await failed.json();
    assert.match(errorBody.error?.message ?? "", /PATH_DISCONTINUOUS/, JSON.stringify(errorBody));
    assert.equal(failed.headers.get("x-aicad-step-sha256"), null);
    assert.equal(failed.headers.get("content-disposition"), null);
    assert.equal(Object.hasOwn(errorBody, "artifact"), false);
    const recovery = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: fixture.model, projectName: "recovery valid sweep" }),
    });
    assert.equal(recovery.status, 200, await recovery.clone().text());
    const recoveryBytes = Buffer.from(await recovery.arrayBuffer());
    const recoveryHash = createHash("sha256").update(recoveryBytes).digest("hex");
    assert.equal(recovery.headers.get("x-aicad-step-sha256"), recoveryHash);
    const recoveryObserved = observeStepArtifact(recoveryBytes);
    assert.equal(recoveryObserved.solid_count, 1);
    assert.equal(recoveryObserved.valid, true);
    assert.ok(Math.abs(recoveryObserved.volume_mm3 - observed.volume_mm3) <= 1e-5);
  } finally {
    if (server.exitCode === null && server.signalCode === null) {
      server.kill("SIGTERM");
      await new Promise((resolve) => server.once("exit", resolve));
    }
  }
  t.diagnostic(`G7A_SWEEP_STEP_BYTES=${bytes.length};SHA256=${hash};CUT_VOLUME_MM3=${volume(cutDrawing)}`);
});

test("G7B ruled Loft uses ordered Sketches in Drawing and canonical Manufacturing", { skip: !enabled }, async (t) => {
  const fixture = makeLoftAddHistory();
  const drawing = draw(fixture.model, "G7B Revolve ADD → three-section Loft ADD → Hole");
  assert.equal(drawing.drawing_report.view_count, 3);
  assert.deepEqual(drawing.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "top"]);
  for (const kind of ["front", "top", "left"]) {
    assert.ok(Array.isArray(drawingView(drawing, kind).primitives) && drawingView(drawing, kind).primitives.length > 0,
      `${kind} must expose actual typed DrawingModel geometry`);
  }
  assert.match(drawing.svg, /view_front/);
  assert.match(drawing.svg, /view_top/);
  assert.match(drawing.svg, /view_left/);
  assert.match(drawing.dxf, /SECTION/);
  for (const feature of ["revolve_body", "loft_body", "loft_hole"]) {
    assert.ok(drawing.spatial_summary.parametric.operation_features.includes(feature), `${feature} must be in the active rebuild`);
  }
  assert.ok(drawing.parametric_timings_ms.operation_timings.some((timing) => timing.kind === "loft_add"));
  assert.ok(drawing.parametric_timings_ms.operation_timings.some((timing) => timing.kind === "extrude_cut"));
  assert.ok(volume(drawing) > Math.PI * 15 ** 2 * 20 * 0.9);
  t.diagnostic(`G7B_LOFT_ADD_DRAWING_TIMINGS_MS=${JSON.stringify(drawing.parametric_timings_ms)}`);

  const editedModel = structuredClone(fixture.model);
  editedModel.features.section_b.feature.profile.outer.entities[0].geometry.radius = 17;
  const edited = draw(editedModel, "G7B rebuild after middle Sketch edit");
  assert.notEqual(volume(edited), volume(drawing), "editing an ordered middle section must rebuild downstream Hole and body");
  const suppressedModel = structuredClone(fixture.model);
  suppressedModel.features.loft_body.suppressed = true;
  const suppressed = draw(suppressedModel, "G7B suppressed Loft rebuild");
  assert.ok(!suppressed.spatial_summary.parametric.operation_features.includes("loft_body"));
  assert.ok(suppressed.spatial_summary.parametric.operation_features.includes("revolve_body"));
  assert.ok(volume(suppressed) < volume(drawing));
  const restored = draw(fixture.model, "G7B unsuppressed Loft rebuild");
  assert.equal(volume(restored), volume(drawing));
  assert.deepEqual(restored.spatial_summary, drawing.spatial_summary);

  const expectedAddProbes = [[24, 0, 45], [26, 0, 45], [5, 19, 45], [5, 21, 45]];
  const addDelivered = exportStep(fixture.model);
  assert.equal(addDelivered.error, undefined, addDelivered.error?.message);
  assert.equal(addDelivered.status, 0, addDelivered.stderr.toString("utf8"));
  const addBytes = addDelivered.stdout;
  const addHash = createHash("sha256").update(addBytes).digest("hex");
  const addMetadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(addDelivered.stderr.toString("utf8"));
  assert.ok(addMetadata, addDelivered.stderr.toString("utf8"));
  assert.equal(addMetadata[1], addHash);
  assert.equal(Number(addMetadata[2]), addBytes.length);
  const addObserved = observeStepArtifact(addBytes, expectedAddProbes);
  assert.equal(addObserved.solid_count, 1);
  assert.equal(addObserved.valid, true);
  assert.deepEqual(addObserved.material_probes.map((probe) => probe.is_inside), [true, false, true, false]);
  assert.ok(Math.abs(addObserved.volume_mm3 - volume(drawing)) <= 1e-5);
  const addBounds = drawing.spatial_summary.bounds;
  assert.deepEqual(addObserved.bbox, [addBounds.min_x, addBounds.max_x, addBounds.min_y, addBounds.max_y, addBounds.min_z, addBounds.max_z]);
  const addDrawingFrontBounds = drawingBounds(drawing, "front");
  assert.ok(addDrawingFrontBounds.every((value, index) => closeTo(value, addObserved.bbox[index])),
    `actual front DrawingModel bounds ${addDrawingFrontBounds} must match re-imported STEP XY bounds ${addObserved.bbox.slice(0, 4)}`);

  const cutFixture = makeLoftCutHistory();
  const cutDrawing = draw(cutFixture.model, "G7B Extrude ADD → three-section Loft CUT");
  assert.equal(cutDrawing.drawing_report.view_count, 3);
  assert.ok(cutDrawing.spatial_summary.parametric.operation_features.includes("loft_cut"));
  assert.ok(volume(cutDrawing) > 0 && volume(cutDrawing) < 100 * 80 * 40);
  assert.ok(cutDrawing.parametric_timings_ms.operation_timings.some((timing) => timing.kind === "loft_cut"));
  const sectionRoot = mkdtempSync(join(tmpdir(), "aicad-g7b-section-"));
  let cutSection;
  try {
    const modelPath = join(sectionRoot, "model.json");
    writeFileSync(modelPath, `${JSON.stringify(cutFixture.model)}\n`, "utf8");
    const sectionResult = spawnSync(binary, ["--json-errors", "--quiet", "engineering", "drawing-section", modelPath,
      "--axis", "y", "--offset-mm", "0", "--view-side", "positive"], {
      encoding: "utf8", maxBuffer: 32 * 1024 * 1024, env: process.env,
    });
    assert.equal(sectionResult.error, undefined, sectionResult.error?.message);
    assert.equal(sectionResult.status, 0, `G7B Loft CUT Section A-A\n${sectionResult.stderr}`);
    cutSection = JSON.parse(sectionResult.stdout);
  } finally {
    rmSync(sectionRoot, { recursive: true, force: true });
  }
  assert.equal(cutSection.drawing_mode, "three_view_section");
  assert.equal(cutSection.drawing_report.view_count, 4);
  assert.ok(cutSection.section_view.section_edge_count > 0);
  assert.ok(cutSection.section_view.boundary_loop_count >= 2);
  assert.match(cutSection.svg, /section_aa_hatch/);
  assert.match(cutSection.dxf, /AICAD_HATCH/);

  const expectedCutProbes = [[1, 0, 20], [11, 0, 20], [1, 7.5, 20], [1, 8.5, 20]];
  const cutDelivered = exportStep(cutFixture.model);
  assert.equal(cutDelivered.error, undefined, cutDelivered.error?.message);
  assert.equal(cutDelivered.status, 0, cutDelivered.stderr.toString("utf8"));
  const cutBytes = cutDelivered.stdout;
  const cutHash = createHash("sha256").update(cutBytes).digest("hex");
  const cutMetadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(cutDelivered.stderr.toString("utf8"));
  assert.ok(cutMetadata, cutDelivered.stderr.toString("utf8"));
  assert.equal(cutMetadata[1], cutHash);
  assert.equal(Number(cutMetadata[2]), cutBytes.length);
  const cutObserved = observeStepArtifact(cutBytes, expectedCutProbes);
  assert.equal(cutObserved.solid_count, 1);
  assert.equal(cutObserved.valid, true);
  assert.deepEqual(cutObserved.material_probes.map((probe) => probe.is_inside), [false, true, false, true]);
  assert.ok(Math.abs(cutObserved.volume_mm3 - volume(cutDrawing)) <= 1e-5);
  const cutBounds = cutDrawing.spatial_summary.bounds;
  assert.deepEqual(cutObserved.bbox, [cutBounds.min_x, cutBounds.max_x, cutBounds.min_y, cutBounds.max_y, cutBounds.min_z, cutBounds.max_z]);
  const cutDrawingFrontBounds = drawingBounds(cutDrawing, "front");
  assert.ok(cutDrawingFrontBounds.every((value, index) => closeTo(value, cutObserved.bbox[index])),
    `actual CUT front DrawingModel bounds ${cutDrawingFrontBounds} must match re-imported STEP XY bounds ${cutObserved.bbox.slice(0, 4)}`);
  t.diagnostic(`G7B_DRAWING_STEP_FRONT_XY=PASS;TOLERANCE_MM=${GEOMETRY_TOLERANCE_MM};ADD=${addDrawingFrontBounds};CUT=${cutDrawingFrontBounds}`);
  t.diagnostic(`G7B_LOFT_ADD_STEP_BYTES=${addBytes.length};SHA256=${addHash};LOFT_CUT_STEP_BYTES=${cutBytes.length};SHA256=${cutHash};CUT_SECTION_EDGES=${cutSection.section_view.section_edge_count}`);
});

test("G7C Sweep cap and lateral roles survive Hole, Fillet, edit, Drawing and canonical STEP", { skip: !enabled }, (t) => {
  const fixture = makeSweepGeneratedTopologyHistory();
  const drawing = draw(fixture.model, "G7C Sweep generated-face Hole and generated-edge Fillet");
  assertDrawing(drawing);
  for (const feature of ["curved_sweep", "clearance_hole", "sweep_end_rim_fillet"]) {
    assert.ok(drawing.spatial_summary.parametric.operation_features.includes(feature), `${feature} must rebuild from the EngineeringModel v2 history`);
  }
  assert.match(drawing.svg, /view_front/);
  assert.match(drawing.dxf, /SECTION/);
  const delivered = exportStep(fixture.model);
  assert.equal(delivered.error, undefined, delivered.error?.message);
  assert.equal(delivered.status, 0, delivered.stderr.toString("utf8"));
  const observed = observeStepArtifact(delivered.stdout);
  assert.equal(observed.solid_count, 1);
  assert.equal(observed.valid, true);
  assert.ok(Math.abs(observed.volume_mm3 - volume(drawing)) <= 1e-5, "canonical STEP reimport must match the Drawing source solid");

  const edited = structuredClone(fixture.model);
  edited.features.curve_path.feature.segments[0].geometry = arc([0, 0], [35, 35], [0, 35], 35);
  const rebuilt = draw(edited, "G7C re-resolve Sweep roles after a valid upstream Path edit");
  assert.ok(rebuilt.spatial_summary.parametric.operation_features.includes("sweep_end_rim_fillet"));
  assert.notEqual(volume(rebuilt), volume(drawing));

  const stale = structuredClone(fixture.model);
  stale.features.sweep_end_rim_fillet.feature.edge_refs[0].selector.second.selector.profile_entity_id = "missing_profile_entity";
  const failed = invoke(stale);
  assert.notEqual(failed.status, 0, "missing generated source identity must fail closed");
  assert.match(`${failed.stderr}\n${failed.stdout}`, /TOPOLOGY_REFERENCE_NOT_FOUND|UNSUPPORTED_TOPOLOGY_REFERENCE/);
  t.diagnostic(`G7C_SWEEP_DRAWING_AND_STEP=PASS;SOURCE_EDGE_FILLET=PASS;PATH_EDIT_REBIND=PASS;STALE_REFERENCE_FAIL_CLOSED=PASS`);
});

test("G7C Loft adjacent section roles survive Hole, Chamfer, edit, Drawing and canonical STEP", { skip: !enabled }, (t) => {
  const fixture = makeLoftGeneratedTopologyHistory();
  const drawing = draw(fixture.model, "G7C Loft generated-face Hole and generated-edge Chamfer");
  assertDrawing(drawing);
  for (const feature of ["loft_body", "loft_end_hole", "loft_end_rim_chamfer"]) {
    assert.ok(drawing.spatial_summary.parametric.operation_features.includes(feature), `${feature} must rebuild from the EngineeringModel v2 history`);
  }
  const delivered = exportStep(fixture.model);
  assert.equal(delivered.error, undefined, delivered.error?.message);
  assert.equal(delivered.status, 0, delivered.stderr.toString("utf8"));
  const observed = observeStepArtifact(delivered.stdout);
  assert.equal(observed.solid_count, 1);
  assert.equal(observed.valid, true);
  assert.ok(Math.abs(observed.volume_mm3 - volume(drawing)) <= 1e-5, "canonical STEP reimport must match the Drawing source solid");

  const edited = structuredClone(fixture.model);
  edited.features.section_c.feature.profile.outer.entities[0].geometry.radius = 14;
  const rebuilt = draw(edited, "G7C re-resolve Loft roles after a valid endpoint section edit");
  assert.ok(rebuilt.spatial_summary.parametric.operation_features.includes("loft_end_rim_chamfer"));
  assert.notEqual(volume(rebuilt), volume(drawing));

  const stale = structuredClone(fixture.model);
  stale.features.loft_end_rim_chamfer.feature.edge_refs[0].selector.second.selector.end_section_id = "section_missing";
  const failed = invoke(stale);
  assert.notEqual(failed.status, 0, "missing Loft section provenance must fail closed");
  assert.match(`${failed.stderr}\n${failed.stdout}`, /SOURCE_TOPOLOGY_CHANGED|TOPOLOGY_REFERENCE_NOT_FOUND|UNSUPPORTED_TOPOLOGY_REFERENCE/);
  t.diagnostic(`G7C_LOFT_DRAWING_AND_STEP=PASS;ADJACENT_SECTION_CHAMFER=PASS;SECTION_EDIT_REBIND=PASS;STALE_REFERENCE_FAIL_CLOSED=PASS`);
});

function exportStep(model) {
  const root = mkdtempSync(join(tmpdir(), "aicad-parametric-step-"));
  try {
    const path = join(root, "model.json");
    writeFileSync(path, `${JSON.stringify(model)}\n`, "utf8");
    return spawnSync(binary, ["--json-errors", "--quiet", "engineering", "export-step", path], {
      encoding: "buffer",
      maxBuffer: 32 * 1024 * 1024,
      env: process.env,
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function observeStepArtifact(bytes, probePoints = []) {
  const root = mkdtempSync(join(tmpdir(), "aicad-step-observation-"));
  try {
    const stepPath = join(root, "api-delivered.step");
    writeFileSync(stepPath, bytes);
    const observer = join(repoRoot, "apps/scc-ui/cad/tests/step_feature_observer.py");
    const python = process.env.SCC_CAD_PYTHON ?? "python3";
    const result = spawnSync(python, [observer, stepPath, ...probePoints.map((point) => point.join(","))], {
      encoding: "utf8", maxBuffer: 8 * 1024 * 1024, env: process.env,
    });
    assert.equal(result.error, undefined, result.error?.message);
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function drawingView(drawing, kind) {
  return drawing.drawing_report.views.find((view) => view.kind === kind);
}

const GEOMETRY_TOLERANCE_MM = 1e-5;

function drawingCircles(drawing, kind) {
  return drawingView(drawing, kind).primitives
    .map((item) => item.geometry)
    .filter((geometry) => geometry.kind === "circle")
    .map((geometry) => ({ center: geometry.center, radius: geometry.radius, role: geometry.role }));
}

function drawingBounds(drawing, kind) {
  const values = [];
  for (const { geometry } of drawingView(drawing, kind).primitives) {
    if (geometry.kind === "line") values.push(geometry.start, geometry.end);
    else if (geometry.kind === "circle" || geometry.kind === "arc") {
      values.push(
        { x: geometry.center.x - geometry.radius, y: geometry.center.y - geometry.radius },
        { x: geometry.center.x + geometry.radius, y: geometry.center.y + geometry.radius },
      );
    }
  }
  assert.ok(values.length > 0, `${kind} drawing must expose measurable geometry`);
  return [
    Math.min(...values.map((value) => value.x)), Math.max(...values.map((value) => value.x)),
    Math.min(...values.map((value) => value.y)), Math.max(...values.map((value) => value.y)),
  ];
}

function closeTo(actual, expected, tolerance = GEOMETRY_TOLERANCE_MM) {
  return Number.isFinite(actual) && Math.abs(actual - expected) <= tolerance;
}

function hasDrawingCircle(drawing, view, center, radius) {
  return drawingCircles(drawing, view).some((circle) =>
    closeTo(circle.center.x, center[0]) && closeTo(circle.center.y, center[1]) && closeTo(circle.radius, radius));
}

function hasDrawingSegment(drawing, view, first, second) {
  const samePoint = (a, b) => closeTo(a.x, b[0]) && closeTo(a.y, b[1]);
  return drawingView(drawing, view).primitives.some(({ geometry }) => geometry.kind === "line" && (
    (samePoint(geometry.start, first) && samePoint(geometry.end, second)) ||
    (samePoint(geometry.start, second) && samePoint(geometry.end, first))
  ));
}

function boxAxisInterval(surface, axis) {
  const pair = { x: [0, 1], y: [2, 3], z: [4, 5] }[axis];
  return [surface.bbox[pair[0]], surface.bbox[pair[1]]];
}

function hasCylinder(observed, { radius, axis, center, interval }) {
  const axisIndex = { x: 0, y: 1, z: 2 }[axis];
  const transverse = [0, 1, 2].filter((index) => index !== axisIndex);
  const axisName = ["x", "y", "z"][axisIndex];
  const intervals = observed.surfaces.filter((surface) => {
    if (surface.kind !== "cylinder" || !closeTo(surface.radius, radius)) return false;
    const direction = surface.axis_direction;
    if (!closeTo(Math.abs(direction[axisIndex]), 1) || direction.some((value, index) => index !== axisIndex && !closeTo(value, 0))) return false;
    return transverse.every((index) => closeTo(surface.axis_origin[index], center[index]));
  }).map((surface) => boxAxisInterval(surface, axisName)).sort((a, b) => a[0] - b[0]);

  // STEP writers may split one analytic cylindrical wall into adjacent faces
  // at seam/Boolean boundaries. Verify complete continuous coverage rather
  // than requiring one transient TopoDS_Face to span the whole bore.
  let covered = interval[0];
  for (const [start, end] of intervals) {
    if (start > covered + GEOMETRY_TOLERANCE_MM) break;
    if (end > covered) covered = end;
    if (covered >= interval[1] - GEOMETRY_TOLERANCE_MM) return true;
  }
  return false;
}

function hasPlanePatch(observed, { z, x, y }) {
  return observed.surfaces.some((surface) => surface.kind === "plane" &&
    closeTo(surface.plane_normal[2], 1) && closeTo(surface.plane_origin[2], z) &&
    closeTo(surface.bbox[0], x[0]) && closeTo(surface.bbox[1], x[1]) &&
    closeTo(surface.bbox[2], y[0]) && closeTo(surface.bbox[3], y[1]) &&
    closeTo(surface.bbox[4], z) && closeTo(surface.bbox[5], z));
}

function requireDrawingAndStepRow(rows, row, drawingVerified, stepVerified) {
  assert.equal(drawingVerified, true, `${row.requirement}: DrawingModel evidence did not match`);
  assert.equal(stepVerified, true, `${row.requirement}: STEP reimport evidence did not match`);
  rows.push({ ...row, drawing_observed: row.drawing_observed, step_reimport_observed: row.step_reimport_observed });
}

function makeG6bMachiningFixture() {
  const builder = modelBuilder("G6B generic Countersink and ExplicitPlacement machining fixture");
  const baseSketch = builder.sketch("g6b_base_sketch", [rectangle(0, 0, 100, 60)]);
  builder.extrude("g6b_base_add", baseSketch, "add", extDistance(10));
  const top = builder.datum("g6b_top", "x_y", 10);
  const addHole = ({ id, support = top, position, diameter, reversed, termination = { kind: "through_all" }, kind }) => {
    builder.addFeature(id, {
      type: "hole", support, position: point(position), diameter, reversed, termination, kind,
    });
  };

  addHole({ id: "g6b_top_countersink", position: [-40, 0], diameter: 4, reversed: true,
    kind: { type: "countersink", diameter: 10, included_angle_degrees: 90 } });
  addHole({ id: "g6b_blind_countersink", position: [-25, 0], diameter: 4, reversed: true,
    termination: { kind: "blind", depth: 8 }, kind: { type: "countersink", diameter: 8, included_angle_degrees: 90 } });
  addHole({ id: "g6b_bottom_countersink", support: { kind: "principal", plane: "x_y" }, position: [20, 25],
    diameter: 4, reversed: false, kind: { type: "countersink", diameter: 8, included_angle_degrees: 90 } });
  addHole({ id: "g6b_side_countersink", support: {
    kind: "planar_face", source_feature_id: "g6b_base_add", selector: "positive_x",
    qualifier: { normal: { x: 1, y: 0, z: 0 }, plane_offset: 50, tolerance: 0.000001 },
  }, position: [-5, 5], diameter: 4, reversed: true,
  kind: { type: "countersink", diameter: 8, included_angle_degrees: 90 } });

  addHole({ id: "g6b_circular_seed", position: [40, 0], diameter: 3, reversed: true,
    kind: { type: "countersink", diameter: 7, included_angle_degrees: 90 } });
  builder.addFeature("g6b_circular_pattern", {
    type: "pattern", seed_feature_id: "g6b_circular_seed",
    definition: { kind: "circular", axis: { x: 0, y: 0, z: 1 }, center: { x: 25, y: 0, z: 10 },
      count: 4, radius: 15, start_angle_degrees: 0, angular_span_degrees: 360 },
  });

  addHole({ id: "g6b_explicit_hole_seed", position: [-40, 20], diameter: 3, reversed: true,
    kind: { type: "simple" } });
  builder.addFeature("g6b_explicit_hole_pattern", {
    type: "pattern", seed_feature_id: "g6b_explicit_hole_seed",
    definition: { kind: "explicit_placement", reference_plane: { kind: "datum", feature: "g6b_top" },
      placements: [{ local_u_mm: -20, local_v_mm: 20 }, { local_u_mm: -35, local_v_mm: -20 }, { local_u_mm: 0, local_v_mm: -20 }] },
  });

  const cutProfile = builder.sketch("g6b_cut_seed_sketch", [rectangle(0, 0, 6, 4)], top);
  builder.extrude("g6b_cut_seed", cutProfile, "cut", extDistance(6), { reversed: true });
  builder.addFeature("g6b_explicit_cut_pattern", {
    type: "pattern", seed_feature_id: "g6b_cut_seed",
    definition: { kind: "explicit_placement", reference_plane: { kind: "datum", feature: "g6b_top" },
      placements: [{ local_u_mm: -35, local_v_mm: 10 }, { local_u_mm: -10, local_v_mm: -18 }, { local_u_mm: 15, local_v_mm: 15 }] },
  });
  return builder.model;
}

function makeFormalMachiningFixture() {
  const builder = modelBuilder("G5 generic formal machining fixture");
  const base = builder.sketch("base_sketch", [rectangle(0, 0, 120, 90)]);
  builder.extrude("base_add", base, "add", extDistance(20));

  const bossDatum = builder.datum("boss_datum", "x_y", 20);
  const boss = builder.sketch("boss_sketch", [[circle([-42, 0], 14)]], bossDatum);
  builder.extrude("boss_add", boss, "add", extDistance(10));

  const top = builder.datum("top_datum", "x_y", 20);
  const pocket = builder.sketch("pocket_sketch", [rectangle(20, 24, 18, 12)], top);
  builder.extrude("pocket_cut", pocket, "cut", extDistance(6), { reversed: true });

  const secondaryDatum = builder.datum("secondary_datum", "x_y", 20);
  const secondaryBoss = builder.sketch("secondary_boss_sketch", [rectangle(40, 28, 12, 10)], secondaryDatum);
  builder.extrude("secondary_boss_add", secondaryBoss, "add", extDistance(4));

  const topRef = { kind: "datum", feature: "top_datum" };
  builder.addFeature("side_blind_hole", {
    type: "hole",
    support: {
      kind: "planar_face", source_feature_id: "base_add", selector: "positive_x",
      qualifier: { normal: { x: 1, y: 0, z: 0 }, plane_offset: 60, tolerance: 0.000001 },
    },
    position: point([0, 8]), diameter: 6, reversed: true,
    termination: { kind: "blind", depth: 10 }, kind: { type: "simple" },
  });
  builder.addFeature("through_hole", {
    type: "hole", support: topRef, position: point([38, 0]), diameter: 8,
    reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("counterbore_hole", {
    type: "hole", support: topRef, position: point([42, -24]), diameter: 6,
    reversed: true, termination: { kind: "through_all" },
    kind: { type: "counterbore", diameter: 10, depth: 3 },
  });
  builder.addFeature("circular_seed", {
    type: "hole", support: topRef, position: point([0, 24]), diameter: 5,
    reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("circular_pattern", {
    type: "pattern", seed_feature_id: "circular_seed",
    definition: {
      kind: "circular", axis: { x: 0, y: 0, z: 1 }, center: { x: 0, y: 0, z: 20 },
      count: 4, radius: 24, start_angle_degrees: 90, angular_span_degrees: 360,
    },
  });
  builder.addFeature("linear_seed", {
    type: "hole", support: topRef, position: point([-28, -28]), diameter: 4,
    reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("linear_pattern", {
    type: "pattern", seed_feature_id: "linear_seed",
    definition: { kind: "linear", direction: { x: 1, y: 0, z: 0 }, count: 3, spacing: 18 },
  });

  const lineQualifier = {
    curve_kind: "line", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.000001,
  };
  builder.addFeature("boss_fillet", {
    type: "fillet", radius: 0.5,
    edge_refs: [{
      source_feature_id: "boss_add",
      selector: { kind: "profile_boundary", entity_id: "boss_sketch-outer-0000", side: "top" },
      qualifier: { ...lineQualifier, curve_kind: "circular", radius_mm: 14 },
    }],
  });
  builder.addFeature("hole_chamfer", {
    type: "chamfer", distance: 0.5,
    edge_refs: [{
      source_feature_id: "through_hole", selector: { kind: "hole_entrance" },
      qualifier: { ...lineQualifier, curve_kind: "circular", radius_mm: 4 },
    }],
  });
  return builder.model;
}

async function startLocalServer(port) {
  const server = spawn("node", ["apps/scc-ui/server.mjs"], {
    cwd: repoRoot,
    env: { ...process.env, SCC_UI_PORT: String(port), SCC_UI_HOST: "127.0.0.1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  for (const stream of [server.stdout, server.stderr]) {
    stream?.on("data", (chunk) => { output = `${output}${chunk.toString("utf8")}`.slice(-8192); });
  }
  let ready = false;
  for (let attempt = 0; attempt < 60 && !ready; attempt += 1) {
    try { ready = (await fetch(`http://127.0.0.1:${port}/api/health`)).ok; } catch {}
    if (!ready) await new Promise((resolve) => setTimeout(resolve, 250));
  }
  assert.equal(ready, true, `local AICAD server did not become ready\n${output}`);
  return server;
}

test("Project v2 EngineeringModel reaches the canonical CLI and API STEP authority", { skip: !enabled }, async (t) => {
  const model = makeFormalMachiningFixture();
  const drawing = draw(model, "G5 Project v2 drawing conformance source");
  assertDrawing(drawing);

  const started = Date.now();
  const delivered = exportStep(model);
  const cliElapsedMs = Date.now() - started;
  assert.equal(delivered.error, undefined, delivered.error?.message);
  assert.equal(delivered.status, 0, delivered.stderr.toString("utf8"));
  const cliBytes = delivered.stdout;
  const cliHash = createHash("sha256").update(cliBytes).digest("hex");
  const diagnostics = delivered.stderr.toString("utf8");
  const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(diagnostics);
  assert.ok(metadata, diagnostics);
  assert.equal(metadata[1], cliHash);
  assert.equal(Number(metadata[2]), cliBytes.length);
  assert.match(cliBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
  assert.match(diagnostics, /AICAD-STEP-STATUS manufacturing=passed build=passed reimport=passed equivalence=passed/);
  const performance = /AICAD-STEP-PERF lowering_ms=([\d.]+) build_ms=([\d.]+) serialization_ms=([\d.]+) reimport_ms=([\d.]+) equivalence_ms=([\d.]+) total_ms=([\d.]+)/.exec(diagnostics);
  assert.ok(performance, diagnostics);
  const stageTimings = Object.fromEntries(
    ["lowering", "build", "serialization", "reimport", "equivalence", "total"]
      .map((name, index) => [name, Number(performance[index + 1])]),
  );
  assert.ok(Object.values(stageTimings).every((value) => Number.isFinite(value) && value >= 0));
  t.diagnostic(`G5_FORMAL_PARAMETRIC_STEP_STAGES_MS=${JSON.stringify(stageTimings)}`);

  const port = 4176;
  const server = await startLocalServer(port);
  try {
    const response = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model, projectName: "G5 generic machining fixture" }),
    });
    if (response.status !== 200) assert.fail(`STEP API returned ${response.status}: ${await response.text()}`);
    const apiBytes = Buffer.from(await response.arrayBuffer());
    const apiHash = createHash("sha256").update(apiBytes).digest("hex");
    assert.ok(apiBytes.length > 1000);
    assert.match(apiBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
    assert.equal(response.headers.get("x-aicad-step-sha256"), apiHash);
    assert.equal(Number(response.headers.get("x-aicad-step-size")), apiBytes.length);
    for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
      assert.equal(response.headers.get(`x-aicad-step-${stage}`), "passed", `${stage} must be validated before delivery`);
    }
    assert.ok(response.headers.get("x-aicad-model-fingerprint"));
    t.diagnostic(`G5_FORMAL_PARAMETRIC_STEP_CLI_MS=${cliElapsedMs}`);
    t.diagnostic(`G5_FORMAL_PARAMETRIC_STEP_API_BYTES=${apiBytes.length}`);
    t.diagnostic(`G5_FORMAL_PARAMETRIC_STEP_API_SHA256=${apiHash}`);
    t.diagnostic(`G5_FORMAL_PARAMETRIC_FEATURES=${model.feature_order.filter((id) => ["extrude", "hole", "pattern", "fillet", "chamfer"].includes(model.features[id].feature.type)).join(",")}`);
    assert.deepEqual(
      Object.keys(drawing.spatial_summary.bounds).sort(),
      ["max_x", "max_y", "max_z", "min_x", "min_y", "min_z"],
    );
  } finally {
    if (server.exitCode === null && server.signalCode === null) {
      server.kill("SIGTERM");
      await new Promise((resolve) => server.once("exit", resolve));
    }
  }
});


function makeFormalRevolveFixture() {
  const builder = modelBuilder("G5 generic formal revolved fixture");
  builder.datumAxis("shaft_axis", {
    kind: "through_two_points", first: { x: 0, y: 0, z: 0 }, second: { x: 10, y: 0, z: 0 },
  });
  const profile = [
    line([0, 0], [0, 20]),
    line([0, 20], [20, 20]),
    line([20, 20], [20, 15]),
    line([20, 15], [60, 15]),
    line([60, 15], [60, 10]),
    line([60, 10], [80, 10]),
    line([80, 10], [80, 0]),
    line([80, 0], [0, 0]),
  ];
  const shaftSketch = builder.sketch("shaft_sketch", [profile]);
  builder.revolve("shaft_revolve", shaftSketch, "shaft_axis");
  const groove = builder.sketch("side_groove_sketch", [rectangle(40, 0, 8, 6)], { kind: "principal", plane: "x_z" });
  builder.extrude("side_groove", groove, "cut", extDistance(25));
  builder.addFeature("axial_bore", {
    type: "hole", support: { kind: "principal", plane: "y_z" }, position: point([0, 0]),
    diameter: 4, reversed: false, termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("radial_seed", {
    type: "hole", support: { kind: "principal", plane: "y_z" }, position: point([6, 0]),
    diameter: 3, reversed: false, termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("radial_pattern", {
    type: "pattern", seed_feature_id: "radial_seed",
    definition: {
      kind: "circular", axis: { x: 1, y: 0, z: 0 }, center: { x: 0, y: 0, z: 0 },
      count: 4, radius: 6, start_angle_degrees: 0, angular_span_degrees: 360,
    },
  });
  const circular = {
    curve_kind: "circular", radius_mm: 20, length_mm: null, midpoint: null, tolerance_mm: 0.000001,
  };
  builder.addFeature("step_fillet", {
    type: "fillet", radius: 0.5,
    edge_refs: [{
      source_feature_id: "shaft_revolve",
      selector: { kind: "revolve_profile_vertex", entity_id: "shaft_sketch-outer-0002", endpoint: "start" },
      qualifier: circular,
    }],
  });
  builder.addFeature("end_chamfer", {
    type: "chamfer", distance: 0.5,
    edge_refs: [{
      source_feature_id: "shaft_revolve",
      selector: { kind: "revolve_profile_vertex", entity_id: "shaft_sketch-outer-0000", endpoint: "end" },
      qualifier: circular,
    }],
  });
  return builder.model;
}




function makeComplexPartSupportedSubset() {
  const builder = modelBuilder("AICAD-COMPLEX-001 supported subset; unspecified base C1 edges omitted");
  builder.model.part.name = "AICAD-COMPLEX-001 supported subset; not a complete-part delivery";
  const baseSketch = builder.sketch("complex_base_sketch", [roundedRectangle(0, 0, 180, 120, 6)]);
  builder.extrude("complex_base_add", baseSketch, "add", extDistance(24));

  const leftBossDatum = builder.datum("left_boss_datum", "x_y", 24);
  const leftBoss = builder.sketch("left_boss_sketch", [[circle([-35, 0], 33)]], leftBossDatum);
  builder.extrude("left_boss_add", leftBoss, "add", extDistance(24));
  const rightBossDatum = builder.datum("right_boss_datum", "x_y", 24);
  const rightBossProfile = builder.sketch("right_boss_sketch", [roundedRectangle(48, 0, 68, 80, 3)], rightBossDatum);
  builder.extrude("right_boss_add", rightBossProfile, "add", extDistance(12));

  const lineQualifier = {
    curve_kind: "line", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.000001,
  };
  builder.addFeature("left_boss_top_fillet", {
    type: "fillet", radius: 1,
    edge_refs: [{
      source_feature_id: "left_boss_add",
      selector: { kind: "profile_boundary", entity_id: "left_boss_sketch-outer-0000", side: "top" },
      qualifier: { ...lineQualifier, curve_kind: "circular", radius_mm: 33 },
    }],
  });
  const rightBossTopRefs = Array.from({ length: 8 }, (_, index) => ({
    source_feature_id: "right_boss_add",
    selector: { kind: "profile_boundary", entity_id: `right_boss_sketch-outer-${String(index).padStart(4, "0")}`, side: "top" },
    qualifier: index % 2 === 0
      ? lineQualifier
      : { ...lineQualifier, curve_kind: "circular", radius_mm: 3 },
  }));
  builder.addFeature("right_boss_top_fillet", { type: "fillet", radius: 1, edge_refs: rightBossTopRefs });

  const leftBossTop = builder.datum("left_boss_top", "x_y", 48);
  builder.addFeature("left_center_hole", {
    type: "hole", support: leftBossTop, position: point([-35, 0]), diameter: 32,
    reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("left_center_hole_chamfer", {
    type: "chamfer", distance: 0.5,
    edge_refs: [{
      source_feature_id: "left_center_hole", selector: { kind: "hole_entrance" },
      qualifier: { ...lineQualifier, curve_kind: "circular", radius_mm: 16 },
    }],
  });
  const diagonal = 26 / Math.sqrt(2);
  builder.addFeature("left_pcd_seed", {
    type: "hole", support: leftBossTop, position: point([-35 + diagonal, diagonal]), diameter: 5.5,
    reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
  });
  builder.addFeature("left_pcd_pattern", {
    type: "pattern", seed_feature_id: "left_pcd_seed",
    definition: {
      kind: "circular", axis: { x: 0, y: 0, z: 1 }, center: { x: -35, y: 0, z: 48 },
      count: 4, radius: 26, start_angle_degrees: 45, angular_span_degrees: 360,
    },
  });

  const rightBossTop = builder.datum("right_boss_top", "x_y", 36);
  const pocketProfile = builder.sketch("right_pocket_sketch", [roundedRectangle(48, 0, 32, 32, 2)], rightBossTop);
  builder.extrude("right_pocket_cut", pocketProfile, "cut", extDistance(8), { reversed: true });
  const pocketFloor = builder.datum("right_pocket_floor", "x_y", 28);
  builder.addFeature("pocket_center_hole", {
    type: "hole", support: pocketFloor, position: point([48, 0]), diameter: 10,
    reversed: true, termination: { kind: "through_all" },
    kind: { type: "counterbore", diameter: 16, depth: 3 },
  });

  const rightHoleTop = builder.datum("right_hole_top", "x_y", 36);
  for (const [index, [x, y]] of [[24, 30], [72, 30], [24, -30], [72, -30]].entries()) {
    builder.addFeature(`right_mount_hole_${index + 1}`, {
      type: "hole", support: rightHoleTop, position: point([x, y]), diameter: 6.6,
      reversed: true, termination: { kind: "through_all" },
      kind: { type: "counterbore", diameter: 11, depth: 4 },
    });
  }

  const baseTop = builder.datum("base_top", "x_y", 24);
  for (const [index, [x, y]] of [[-75, 45], [75, 45], [-75, -45], [75, -45]].entries()) {
    const slotSketch = builder.sketch(`base_slot_${index + 1}_sketch`, [slot(x, y, 14, 7)], baseTop);
    builder.extrude(`base_slot_${index + 1}_cut`, slotSketch, "cut", extDistance(24), { reversed: true });
  }

  builder.addFeature("side_blind_counterbore", {
    type: "hole",
    support: {
      kind: "planar_face", source_feature_id: "complex_base_add", selector: "positive_x",
      qualifier: { normal: { x: 1, y: 0, z: 0 }, plane_offset: 90, tolerance: 0.000001 },
    },
    position: point([0, 16]), diameter: 8, reversed: true,
    termination: { kind: "blind", depth: 18 },
    kind: { type: "counterbore", diameter: 12, depth: 2 },
  });

  const bottom = builder.datum("base_bottom", "x_y", 0);
  const reliefSketch = builder.sketch("bottom_relief_sketch", [roundedRectangle(0, 0, 24, 40, 3)], bottom);
  builder.extrude("bottom_relief_cut", reliefSketch, "cut", extDistance(5));
  return builder.model;
}


function timedCliStep(model, context) {
  const result = exportStep(model);
  assert.equal(result.error, undefined, `${context}: ${result.error?.message}`);
  assert.equal(result.status, 0, `${context}: ${result.stderr.toString("utf8")}`);
  const diagnostics = result.stderr.toString("utf8");
  const match = /AICAD-STEP-PERF lowering_ms=([\d.]+) build_ms=([\d.]+) serialization_ms=([\d.]+) reimport_ms=([\d.]+) equivalence_ms=([\d.]+) total_ms=([\d.]+)/.exec(diagnostics);
  assert.ok(match, `${context}: expected formal STEP stage timings in CLI diagnostics`);
  return Object.fromEntries(["lowering", "build", "serialization", "reimport", "equivalence", "total"]
    .map((key, index) => [key, Number(match[index + 1])]));
}

test("G5.1 AICAD-COMPLEX-001 supported generic subset drawing and formal STEP", { skip: !enabled }, async (t) => {
  const model = makeComplexPartSupportedSubset();
  const unambiguousRequirementRows = [
    { id: "CP-01", feature_ids: ["complex_base_add"], parameters: "180×120×24 base; four vertical outer corners R6" },
    { id: "CP-02", feature_ids: ["left_boss_add"], parameters: "center (-35,0), Ø66, Z24..48, fused to base" },
    { id: "CP-03", feature_ids: ["left_boss_top_fillet"], parameters: "left boss top outside R1" },
    { id: "CP-04", feature_ids: ["left_center_hole", "left_center_hole_chamfer"], parameters: "center (-35,0), Ø32 through Z0..48; top mouth C0.5" },
    { id: "CP-05", feature_ids: ["left_pcd_seed", "left_pcd_pattern"], parameters: "4×Ø5.5, PCD52, 45/135/225/315°, through Z0..48" },
    { id: "CP-06", feature_ids: ["right_boss_add"], parameters: "68×80 R3 profile, X14..82/Y±40, Z24..36, fused to base" },
    { id: "CP-07", feature_ids: ["right_boss_top_fillet"], parameters: "right boss top outside R1" },
    { id: "CP-08", feature_ids: ["right_pocket_cut"], parameters: "center (48,0), 32×32 R2, z36 down to z28" },
    { id: "CP-09", feature_ids: ["pocket_center_hole"], parameters: "Ø10 z0..28; CB Ø16 z25..28" },
    { id: "CP-10", feature_ids: ["right_mount_hole_1", "right_mount_hole_2", "right_mount_hole_3", "right_mount_hole_4"], parameters: "four Ø6.6 through at (24,30),(72,30),(24,-30),(72,-30); CB Ø11 z32..36" },
    { id: "CP-11", feature_ids: ["base_slot_1_cut", "base_slot_2_cut", "base_slot_3_cut", "base_slot_4_cut"], parameters: "four 14×7 slots, R3.5 ends, X-axis, centers (±75,±45), through Z0..24" },
    { id: "CP-12", feature_ids: ["side_blind_counterbore"], parameters: "X90/Y0/Z16, axis −X; Ø8 to X72; CB Ø12 depth2" },
    { id: "CP-13", feature_ids: ["bottom_relief_cut"], parameters: "bottom opening at (0,0), 24×40 R3, Z0..5" },
  ];
  for (const row of unambiguousRequirementRows) {
    for (const featureId of row.feature_ids) assert.ok(model.features[featureId], `${row.id} must be preserved in EngineeringModel`);
  }
  assert.ok(!model.feature_order.some((featureId) => /base.*chamfer/i.test(featureId)), "do not guess the unspecified C1 edge set");

  const drawing = draw(model, "AICAD-COMPLEX-001 explicitly bounded supported subset");
  assertDrawing(drawing);
  assert.deepEqual(drawing.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "top"]);
  assert.ok(drawing.svg.length > 1000 && drawing.dxf.length > 1000);
  const associative = new Map((drawing.drawing_report.associative_dimensions ?? []).map((item) => [item.annotation_id, item]));
  for (const id of [
    "associative.body.length",
    "associative.body.width",
    "associative.base.thickness",
    "associative.hole.left_center_hole.callout",
    "associative.hole.left_pcd_pattern.callout",
  ]) {
    assert.equal(associative.get(id)?.status, "resolved", `complex supported subset must resolve ${id}`);
  }
  for (const id of ["left_boss_top_fillet", "right_boss_top_fillet"]) {
    const radius = associative.get(`associative.fillet.${id}.radius`);
    assert.equal(radius?.status, "unsupported", `${id} must not be silently represented as a radius dimension`);
    assert.equal(radius?.drawing_dimension_id, null);
  }
  const sideCallout = associative.get("associative.hole.side_blind_counterbore.callout");
  assert.equal(sideCallout?.status, "unsupported", "side-axis CB needs a supported section or side callout");
  const requiredOperationIds = unambiguousRequirementRows.flatMap((row) => row.feature_ids)
    .filter((id) => model.features[id].feature.type !== "datum_plane" && model.features[id].feature.type !== "sketch");
  const rebuiltFeatures = new Set(drawing.spatial_summary.parametric.operation_features);
  for (const featureId of requiredOperationIds) assert.ok(rebuiltFeatures.has(featureId), `${featureId} must reach the rebuilt EngineeringModel`);

  const port = 4182;
  const server = await startLocalServer(port);
  try {
    const response = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ model, projectName: "AICAD-COMPLEX-001-supported-subset" }),
    });
    assert.equal(response.status, 200, `supported subset formal STEP API: ${response.status} ${await response.clone().text()}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    const hash = createHash("sha256").update(bytes).digest("hex");
    assert.equal(response.headers.get("x-aicad-step-sha256"), hash);
    assert.equal(Number(response.headers.get("x-aicad-step-size")), bytes.length);
    for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
      assert.equal(response.headers.get(`x-aicad-step-${stage}`), "passed", stage);
    }
    const observed = observeStepArtifact(bytes);
    assert.equal(observed.valid, true);
    assert.equal(observed.solid_count, 1);
    assert.ok(observed.bbox.every((value, index) => closeTo(value, [-90, 90, -60, 60, 0, 48][index])),
      "only the explicitly modeled supported subset has the frozen 180×120×48 overall envelope");
    assert.ok(hasCylinder(observed, { radius: 33, axis: "z", center: [-35, 0, 0], interval: [24, 47] }),
      "STEP reimport must retain the Ø66 boss wall below its R1 blend");
    assert.ok(observed.surfaces.some((surface) => surface.kind === "torus" && closeTo(surface.minor_radius, 1) &&
      closeTo(surface.major_radius, 32) && closeTo(surface.axis_origin[0], -35)), "STEP reimport must retain the left boss R1 blend");
    assert.ok([[-84, -54], [-84, 54], [84, -54], [84, 54]].every((center) =>
      hasCylinder(observed, { radius: 6, axis: "z", center: [...center, 0], interval: [0, 24] })),
    "STEP reimport must retain the base rounded-rectangle R6 corners");
    const rightBossCorners = [[17, -37], [79, -37], [17, 37], [79, 37]];
    assert.ok(rightBossCorners.every((center) =>
      hasCylinder(observed, { radius: 3, axis: "z", center: [...center, 0], interval: [24, 35] })),
    "STEP reimport must retain the right boss four R3 vertical corners");
    assert.ok(observed.surfaces.some((surface) => surface.kind === "torus" && closeTo(surface.minor_radius, 1) &&
      surface.axis_origin[0] > 14 && surface.axis_origin[0] < 82 && surface.axis_origin[1] > -40 && surface.axis_origin[1] < 40),
    "STEP reimport must retain an analytic right boss top R1 blend");
    assert.ok(hasCylinder(observed, { radius: 16, axis: "z", center: [-35, 0, 0], interval: [0, 47.5] }),
      "STEP reimport must retain the Ø32 left center hole below its chamfer");
    assert.ok(observed.surfaces.some((surface) => surface.kind === "cone" && closeTo(surface.reference_radius, 16) &&
      closeTo(surface.semi_angle_radians, Math.PI / 4) && closeTo(surface.axis_origin[0], -35) &&
      closeTo(surface.axis_origin[1], 0) && closeTo(surface.bbox[4], 47.5) && closeTo(surface.bbox[5], 48)),
    "STEP reimport must retain the left center hole C0.5 entrance");
    const diagonal = 26 / Math.sqrt(2);
    const pcdCenters = [
      [-35 + diagonal, diagonal], [-35 - diagonal, diagonal],
      [-35 - diagonal, -diagonal], [-35 + diagonal, -diagonal],
    ];
    assert.ok(pcdCenters.every((center) => hasCylinder(observed, { radius: 2.75, axis: "z", center: [...center, 0], interval: [0, 48] })),
      "STEP reimport must contain all four left-boss Ø5.5 PCD bores");
    assert.ok(hasPlanePatch(observed, { z: 28, x: [32, 64], y: [-16, 16] }), "STEP reimport must retain the right pocket floor at z28");
    const pocketCornerCenters = [[34, -14], [62, -14], [34, 14], [62, 14]];
    assert.ok(pocketCornerCenters.every((center) =>
      hasCylinder(observed, { radius: 2, axis: "z", center: [...center, 0], interval: [28, 36] })),
    "STEP reimport must retain the four R2 pocket inside corners");
    assert.ok(hasCylinder(observed, { radius: 5, axis: "z", center: [48, 0, 0], interval: [0, 25] }), "center blind hole is Ø10 from z0 to z25");
    assert.ok(hasCylinder(observed, { radius: 8, axis: "z", center: [48, 0, 0], interval: [25, 28] }), "center counterbore is Ø16 × 3");
    const rightHoleCenters = [[24, 30], [72, 30], [24, -30], [72, -30]];
    assert.ok(rightHoleCenters.every((center) => hasCylinder(observed, { radius: 3.3, axis: "z", center: [...center, 0], interval: [0, 32] })),
      "STEP reimport must retain all four Ø6.6 holes through z0..36 with their counterbores");
    assert.ok(rightHoleCenters.every((center) => hasCylinder(observed, { radius: 5.5, axis: "z", center: [...center, 0], interval: [32, 36] })),
      "STEP reimport must retain all four Ø11 × 4 counterbores");
    const slotEndCenters = [[-78.5, 45], [-71.5, 45], [71.5, 45], [78.5, 45], [-78.5, -45], [-71.5, -45], [71.5, -45], [78.5, -45]];
    assert.ok(slotEndCenters.every((center) => hasCylinder(observed, { radius: 3.5, axis: "z", center: [...center, 0], interval: [0, 24] })),
      "STEP reimport must retain all eight rounded slot ends, through the base only");
    assert.ok(hasCylinder(observed, { radius: 6, axis: "x", center: [0, 0, 16], interval: [88, 90] }), "side counterbore is Ø12 × 2");
    assert.ok(hasCylinder(observed, { radius: 4, axis: "x", center: [0, 0, 16], interval: [72, 88] }), "side blind bore is Ø8 × 18, inward from X=90");
    assert.ok(hasCylinder(observed, { radius: 3, axis: "z", center: [-9, -17, 0], interval: [0, 5] }) &&
      hasCylinder(observed, { radius: 3, axis: "z", center: [9, 17, 0], interval: [0, 5] }),
    "STEP reimport must retain rounded inner corners on the bottom relief");
    assert.ok(hasPlanePatch(observed, { z: 5, x: [-12, 12], y: [-20, 20] }),
    "underside relief floor must be at z5");

    const coverage = unambiguousRequirementRows.map((row) => ({
      REQUIREMENT_ID: row.id,
      ENGINEERING_FEATURE: row.feature_ids.join(","),
      PARAMETERS: row.parameters,
      ENGINEERING_SUPPORTED: "YES; active generic EngineeringModel v2 features",
      DRAWING_SUPPORTED: "PARTIAL; Front/Top/Left SVG/DXF generated; internal depths and some side details lack a section or dimension callout",
      MANUFACTURING_SUPPORTED: "YES; formal STEP one-solid/reimport/equivalence stages passed for this subset",
      VERIFICATION_EVIDENCE: "active feature history + DrawingModel/SVG/DXF + exact HTTP STEP bytes reimported by OCP",
    }));
    coverage.push({
      REQUIREMENT_ID: "CP-14",
      ENGINEERING_FEATURE: "Unresolved semantic Chamfer target",
      PARAMETERS: "C1 on the ‘remaining designated base outline sharp edges’; source does not identify the edge set",
      ENGINEERING_SUPPORTED: "GENERIC CHAMFER EXISTS; target selection BLOCKED pending clarification",
      DRAWING_SUPPORTED: "BLOCKED; edge identities are not specified",
      MANUFACTURING_SUPPORTED: "NOT MODELED; no guessed chamfer or complete-part STEP",
      VERIFICATION_EVIDENCE: "source wording leaves multiple candidate sharp edge sets; no selector added",
    });
    const cliTimings = timedCliStep(model, "AICAD-COMPLEX-001 supported subset CLI timing sample");
    t.diagnostic(`AICAD_COMPLEX_001_SUPPORTED_SUBSET=PASS_WITH_NOTES;FULL_COMPLEX_PART=BLOCKED`);
    t.diagnostic(`AICAD_COMPLEX_001_SUBSET_STEP_SHA256=${hash};size=${bytes.length};bbox=${JSON.stringify(observed.bbox)}`);
    t.diagnostic(`AICAD_COMPLEX_001_REQUIREMENT_COVERAGE=${JSON.stringify(coverage)}`);
    t.diagnostic(`AICAD_COMPLEX_001_DRAWING_TIMINGS_MS=${JSON.stringify(drawing.parametric_timings_ms)}`);
    t.diagnostic(`AICAD_COMPLEX_001_STEP_CLI_TIMINGS_MS=${JSON.stringify(cliTimings)}`);
    t.diagnostic(`AICAD_COMPLEX_001_DRAWING_EXPORT_BYTES=svg:${Buffer.byteLength(drawing.svg)};dxf:${Buffer.byteLength(drawing.dxf)}`);
  } finally {
    if (server.exitCode === null && server.signalCode === null) {
      server.kill("SIGTERM");
      await new Promise((resolve) => server.once("exit", resolve));
    }
  }
});

test("canonical Project v2 STEP API fails closed for a detached ADD without returning a partial artifact", { skip: !enabled }, async () => {
  const builder = plateHistory({ width: 100, height: 60, thickness: 10 });
  addOffsetBoss(builder, { id: "detached", profile: rectangle(200, 0, 10, 10), z: 10, extent: 5 });
  const port = 4177;
  const server = await startLocalServer(port);
  try {
    const response = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: builder.model, projectName: "G5 detached ADD failure" }),
    });
    assert.notEqual(response.status, 200);
    assert.ok((response.headers.get("content-type") ?? "").includes("application/json"));
    assert.equal(response.headers.get("x-aicad-step-sha256"), null);
    assert.equal(response.headers.get("x-aicad-step-size"), null);
    const failure = await response.json();
    assert.match(JSON.stringify(failure), /DETACHED_ADD|MULTI_BODY_NOT_ALLOWED/);
    assert.equal(Object.hasOwn(failure, "artifact"), false);
  } finally {
    if (server.exitCode === null && server.signalCode === null) {
      server.kill("SIGTERM");
      await new Promise((resolve) => server.once("exit", resolve));
    }
  }
});

function projectV2Prefix(model, terminalFeatureId) {
  const end = model.feature_order.indexOf(terminalFeatureId);
  assert.notEqual(end, -1, `missing matrix terminal feature ${terminalFeatureId}`);
  const feature_order = model.feature_order.slice(0, end + 1);
  const features = Object.fromEntries(feature_order.map((id) => [id, model.features[id]]));
  return { ...model, feature_order, features };
}

function makeFormalRevolveCutFixture() {
  const builder = modelBuilder("G5 M13 formal revolve cut fixture");
  const plate = builder.sketch("plate_sketch", [rectangle(0, 0, 100, 80)]);
  builder.extrude("plate_add", plate, "add", extDistance(20));
  builder.datumAxis("ring_axis", { kind: "principal", axis: "z" });
  const ring = builder.sketch("ring_cut_sketch", [rectangle(22.5, 0, 5, 20)], { kind: "principal", plane: "x_z" });
  builder.revolve("ring_cut", ring, "ring_axis", "cut");
  return builder.model;
}

test("Project v2 M1–M16 manufacturing matrix produces reimported, equivalent STEP artifacts", { skip: !enabled }, (t) => {
  const machining = makeFormalMachiningFixture();
  const addThenThroughCut = modelBuilder("G5 M2 ADD then THROUGH_ALL CUT");
  const m2Base = addThenThroughCut.sketch("m2_base_sketch", [rectangle(0, 0, 100, 60)]);
  addThenThroughCut.extrude("m2_base_add", m2Base, "add", extDistance(20));
  const m2Top = addThenThroughCut.datum("m2_top", "x_y", 20);
  const m2Cut = addThenThroughCut.sketch("m2_cut_sketch", [rectangle(0, 0, 16, 16)], m2Top);
  addThenThroughCut.extrude("m2_through_cut", m2Cut, "cut", extThroughAll(), { reversed: true });
  const revolve = makeFormalRevolveFixture();
  const mixedRevolve = makeFormalRevolveFixture();
  const revolveCut = makeFormalRevolveCutFixture();
  const matrix = [
    ["M1 Rectangle Extrude ADD", projectV2Prefix(machining, "base_add")],
    ["M2 ADD then THROUGH_ALL CUT", addThenThroughCut.model],
    ["M3 Circle ADD", projectV2Prefix(machining, "boss_add")],
    ["M4 Blind cut", projectV2Prefix(machining, "pocket_cut")],
    ["M5 Through cut", projectV2Prefix(machining, "through_hole")],
    ["M6 Multi-axis blind Hole", projectV2Prefix(machining, "side_blind_hole")],
    ["M7 Counterbore", projectV2Prefix(machining, "counterbore_hole")],
    ["M8 Circular Hole Pattern", projectV2Prefix(machining, "circular_pattern")],
    ["M9 Linear Hole Pattern", projectV2Prefix(machining, "linear_pattern")],
    ["M10 Fillet", projectV2Prefix(machining, "boss_fillet")],
    ["M11 Chamfer", projectV2Prefix(machining, "hole_chamfer")],
    ["M12 Revolve ADD", projectV2Prefix(revolve, "shaft_revolve")],
    ["M13 Revolve CUT", revolveCut],
    ["M14 Mixed Extrude and Revolve", projectV2Prefix(mixedRevolve, "side_groove")],
    ["M15 Revolve, Hole, Pattern", projectV2Prefix(revolve, "radial_pattern")],
    ["M16 Compound machining history", machining],
  ];

  for (const [caseName, model] of matrix) {
    const result = exportStep(model);
    assert.equal(result.error, undefined, `${caseName}: ${result.error?.message}`);
    assert.equal(result.status, 0, `${caseName}: ${result.stderr.toString("utf8")}`);
    assert.match(result.stdout.subarray(0, 32).toString("ascii"), /ISO-10303-21/, `${caseName} must emit STEP`);
    const diagnostics = result.stderr.toString("utf8");
    const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(diagnostics);
    assert.ok(metadata, `${caseName}: ${diagnostics}`);
    assert.equal(metadata[1], createHash("sha256").update(result.stdout).digest("hex"), `${caseName} delivered bytes must be the validated bytes`);
    assert.equal(Number(metadata[2]), result.stdout.length, `${caseName} delivered size must match validated bytes`);
    assert.match(diagnostics, /AICAD-STEP-STATUS manufacturing=passed build=passed reimport=passed equivalence=passed/, `${caseName} must pass all authority stages`);
    t.diagnostic(`${caseName.replaceAll(" ", "_")}_STEP_BYTES=${result.stdout.length}`);
  }
  t.diagnostic("PROJECT_V2_M1_M16_FORMAL_STEP_MATRIX=16/16");
});

test("Project v2 Revolve ADD, Hole, Pattern, Fillet and Chamfer reach formal STEP", { skip: !enabled }, (t) => {
  const model = makeFormalRevolveFixture();
  const drawing = draw(model, "G5 formal revolved drawing conformance source");
  assertDrawing(drawing);
  const result = exportStep(model);
  assert.equal(result.error, undefined, result.error?.message);
  assert.equal(result.status, 0, result.stderr.toString("utf8"));
  assert.match(result.stdout.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
  const hash = createHash("sha256").update(result.stdout).digest("hex");
  const diagnostics = result.stderr.toString("utf8");
  const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(diagnostics);
  assert.ok(metadata, diagnostics);
  assert.equal(metadata[1], hash);
  assert.equal(Number(metadata[2]), result.stdout.length);
  assert.match(diagnostics, /AICAD-STEP-STATUS manufacturing=passed build=passed reimport=passed equivalence=passed/);
  for (const id of ["shaft_revolve", "side_groove", "axial_bore", "radial_pattern", "step_fillet", "end_chamfer"]) {
    assert.ok(model.features[id], `${id} belongs to the EngineeringModel history`);
  }
  t.diagnostic(`G5_FORMAL_REVOLVE_STEP_BYTES=${result.stdout.length}`);
});

test("Project v2 Revolve CUT reaches formal STEP on a single mixed-history body", { skip: !enabled }, (t) => {
  const builder = modelBuilder("G5 generic revolve cut fixture");
  const plate = builder.sketch("plate_sketch", [rectangle(0, 0, 100, 80)]);
  builder.extrude("plate_add", plate, "add", extDistance(20));
  builder.datumAxis("ring_axis", { kind: "principal", axis: "z" });
  const ring = builder.sketch("ring_cut_sketch", [rectangle(22.5, 0, 5, 20)], { kind: "principal", plane: "x_z" });
  builder.revolve("ring_cut", ring, "ring_axis", "cut");
  const drawing = draw(builder.model, "G5 formal Revolve CUT drawing source");
  assertDrawing(drawing);
  assert.ok(volume(drawing) > 0 && volume(drawing) < 160000);
  const result = exportStep(builder.model);
  assert.equal(result.error, undefined, result.error?.message);
  assert.equal(result.status, 0, result.stderr.toString("utf8"));
  assert.match(result.stdout.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
  assert.match(result.stderr.toString("utf8"), /AICAD-STEP-STATUS manufacturing=passed build=passed reimport=passed equivalence=passed/);
  t.diagnostic(`G5_FORMAL_REVOLVE_CUT_STEP_BYTES=${result.stdout.length}`);
});

function makeFormalStepApiNegativeCases() {
  const noIntersection = plateHistory();
  const outside = noIntersection.sketch("x2_outside_cut_sketch", [[circle([100, 0], 5)]]);
  noIntersection.extrude("x2_no_intersection_cut", outside, "cut", extThroughAll(), { reversed: true });

  const removesBody = plateHistory({ width: 10, height: 10, thickness: 10 });
  const fullCut = removesBody.sketch("x3_full_cut_sketch", [[circle([0, 0], 100)]]);
  removesBody.extrude("x3_remove_body_cut", fullCut, "cut", extThroughAll(), { reversed: true });

  const invalidHole = plateHistory();
  const invalidHoleTop = invalidHole.datum("x4_hole_top", "x_y", 10);
  invalidHole.addFeature("x4_invalid_hole", {
    type: "hole", support: invalidHoleTop, position: point([0, 0]), diameter: 0,
    reversed: true, termination: { kind: "through_all" }, kind: { type: "simple" },
  });

  const invalidCounterbore = plateHistory();
  const invalidCounterboreTop = invalidCounterbore.datum("x5_counterbore_top", "x_y", 10);
  invalidCounterbore.addFeature("x5_invalid_counterbore", {
    type: "hole", support: invalidCounterboreTop, position: point([0, 0]), diameter: 8,
    reversed: true, termination: { kind: "through_all" },
    kind: { type: "counterbore", diameter: 6, depth: 3 },
  });

  const invalidPatternSeed = plateHistory();
  invalidPatternSeed.addFeature("x6_missing_seed_pattern", {
    type: "pattern", seed_feature_id: "x6_missing_seed",
    definition: { kind: "linear", direction: { x: 1, y: 0, z: 0 }, count: 2, spacing: 20 },
  });

  const ambiguousEdge = plateHistory({ width: 200, height: 60, thickness: 10 });
  const ambiguousTop = ambiguousEdge.datum("x7_pattern_top", "x_y", 10);
  const ambiguousPocketSketch = ambiguousEdge.sketch("x7_pattern_seed_sketch", [rectangle(0, 0, 12, 10)], ambiguousTop);
  ambiguousEdge.extrude("x7_pattern_seed_cut", ambiguousPocketSketch, "cut", extDistance(3), { reversed: true });
  ambiguousEdge.addFeature("x7_ambiguous_pattern", {
    type: "pattern", seed_feature_id: "x7_pattern_seed_cut",
    definition: { kind: "linear", direction: { x: 1, y: 0, z: 0 }, count: 3, spacing: 30 },
  });
  ambiguousEdge.addFeature("x7_ambiguous_fillet", {
    type: "fillet", radius: 0.25,
    edge_refs: [{
      source_feature_id: "x7_ambiguous_pattern",
      selector: { kind: "pocket_entrance", entity_id: "x7_pattern_seed_sketch-outer-0000" },
      qualifier: { curve_kind: "line", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.000001 },
    }],
  });

  const invalidFillet = structuredClone(makeFormalMachiningFixture());
  invalidFillet.features.boss_fillet.feature.radius = 0;

  const invalidAxis = modelBuilder("G5 X9 invalid revolve axis");
  invalidAxis.datumAxis("x9_zero_axis", {
    kind: "through_two_points",
    first: { x: 0, y: 0, z: 0 }, second: { x: 0, y: 0, z: 0 },
  });
  const invalidAxisProfile = invalidAxis.sketch("x9_axis_profile", [rectangle(20, 0, 4, 10)], { kind: "principal", plane: "x_z" });
  invalidAxis.revolve("x9_invalid_axis_revolve", invalidAxisProfile, "x9_zero_axis");

  const crossingProfile = modelBuilder("G5 X10 invalid revolve geometry");
  crossingProfile.datumAxis("x10_z_axis", { kind: "principal", axis: "z" });
  const crossingSketch = crossingProfile.sketch("x10_crossing_profile", [rectangle(0, 20, 4, 8)], { kind: "principal", plane: "x_z" });
  crossingProfile.revolve("x10_crossing_revolve", crossingSketch, "x10_z_axis");

  return [
    { id: "X2", input_summary: "base 100×60×10; ThroughAll circle cut at (100,0), Ø10, no intersection", input: noIntersection.model, code: "CUT_NO_INTERSECTION", failureClass: "GEOMETRY_BUILD_FAILURE", failureStage: "manufacturing_build" },
    { id: "X3", input_summary: "base 10×10×10; ThroughAll circle cut centered (0,0), Ø200 removes whole body", input: removesBody.model, code: "CUT_REMOVES_BODY", failureClass: "GEOMETRY_BUILD_FAILURE", failureStage: "manufacturing_build" },
    { id: "X4", input_summary: "Hole diameter=0 on a valid base", input: invalidHole.model, code: "INVALID_HOLE_DIAMETER", failureClass: "ENGINEERING_VALIDATION_FAILURE", failureStage: "engineering_validation" },
    { id: "X5", input_summary: "Hole Ø8 with invalid counterbore Ø6 depth3", input: invalidCounterbore.model, code: "INVALID_COUNTERBORE", failureClass: "ENGINEERING_VALIDATION_FAILURE", failureStage: "engineering_validation" },
    { id: "X6", input_summary: "linear pattern references absent seed feature x6_missing_seed", input: invalidPatternSeed.model, code: "PATTERN_SEED_NOT_FOUND", failureClass: "ENGINEERING_VALIDATION_FAILURE", failureStage: "engineering_validation" },
    { id: "X7", input_summary: "three repeated blind pockets then semantic entrance Fillet with ambiguous pattern edge reference", input: ambiguousEdge.model, code: "AMBIGUOUS_TOPOLOGY_REFERENCE", failureClass: "GEOMETRY_BUILD_FAILURE", failureStage: "manufacturing_build" },
    { id: "X8", input_summary: "otherwise valid machining fixture with Fillet radius=0", input: invalidFillet, code: "INVALID_FILLET_RADIUS", failureClass: "ENGINEERING_VALIDATION_FAILURE", failureStage: "engineering_validation" },
    { id: "X9", input_summary: "DatumAxis endpoints are identical (zero-length axis)", input: invalidAxis.model, code: "INVALID_DATUM_AXIS", failureClass: "ENGINEERING_VALIDATION_FAILURE", failureStage: "engineering_validation" },
    { id: "X10", input_summary: "Revolve rectangle profile crosses the specified Z axis", input: crossingProfile.model, code: "REVOLVE_PROFILE_CROSSES_AXIS", failureClass: "GEOMETRY_BUILD_FAILURE", failureStage: "manufacturing_build" },
  ];
}

test("canonical Project v2 STEP API fails closed for G5 X2–X10 and remains usable after each rejection", { skip: !enabled }, async (t) => {
  const port = 4180;
  const server = await startLocalServer(port);
  const endpoint = `http://127.0.0.1:${port}/api/engineering/export/step`;
  const post = (model) => fetch(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ model, projectName: "G5.1 formal negative matrix" }),
  });
  try {
    const matrix = [];
    for (const item of makeFormalStepApiNegativeCases()) {
      const response = await post(item.input);
      const bytes = Buffer.from(await response.arrayBuffer());
      const bodyText = bytes.toString("utf8");
      assert.equal(response.status, 422, `${item.id} HTTP status contract; content-type=${response.headers.get("content-type")} bytes=${bytes.length}`);
      assert.ok((response.headers.get("content-type") ?? "").includes("application/json"), `${item.id} must return JSON`);
      assert.equal(response.headers.get("content-disposition"), null, `${item.id} must not advertise a download`);
      for (const header of [
        "x-aicad-step-sha256", "x-aicad-step-size", "x-aicad-step-manufacturing",
        "x-aicad-step-build", "x-aicad-step-reimport", "x-aicad-step-equivalence",
      ]) assert.equal(response.headers.get(header), null, `${item.id} must not return ${header}`);
      assert.equal(bytes.subarray(0, 32).toString("ascii").includes("ISO-10303-21"), false, `${item.id} delivered STEP bytes`);
      const payload = JSON.parse(bodyText);
      assert.equal(payload.error.details.failure_class, item.failureClass, `${item.id} failure class`);
      assert.equal(payload.error.details.failure_stage, item.failureStage, `${item.id} actual failure stage`);
      assert.equal(payload.error.details.error_code, item.code, `${item.id} original machine-readable cause: ${JSON.stringify(payload.error)}`);
      assert.notEqual(payload.error.code, item.code, `${item.id} retains the existing outer SCC envelope and exposes the cause separately`);
      assert.equal(Object.hasOwn(payload, "artifact"), false, `${item.id} must not return a partial artifact`);
      assert.doesNotMatch(bodyText, /(?:\/home\/|\/tmp\/|api[_ -]?key|deepseek|authorization|bearer\s)/i, `${item.id} leaked a path or sensitive runtime configuration`);
      const matrixRow = {
        TEST_CASE: item.id,
        INPUT: item.input_summary,
        EXPECTED_FAILURE_STAGE: item.failureStage,
        ACTUAL_FAILURE_STAGE: payload.error.details.failure_stage,
        HTTP_STATUS: response.status,
        ERROR_CODE: payload.error.details.error_code,
        FAILURE_CLASS: payload.error.details.failure_class,
        ARTIFACT_DELIVERED: false,
        RESULT: "PASS",
      };
      matrix.push(matrixRow);

      const legal = await post(plateHistory({ width: 60, height: 40, thickness: 8 }).model);
      assert.equal(legal.status, 200, `${item.id} must not poison the next legal request`);
      const legalBytes = Buffer.from(await legal.arrayBuffer());
      const legalHash = createHash("sha256").update(legalBytes).digest("hex");
      assert.match(legalBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
      assert.equal(legal.headers.get("x-aicad-step-sha256"), legalHash);
      assert.equal(Number(legal.headers.get("x-aicad-step-size")), legalBytes.length);
      for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
        assert.equal(legal.headers.get(`x-aicad-step-${stage}`), "passed", `${item.id} legal follow-up ${stage}`);
      }
    }
    t.diagnostic(`G5_1_HTTP_NEGATIVE_MATRIX_X2_X10=${matrix.length}/9;LEGAL_FOLLOWUP=9/9`);
    t.diagnostic(`G5_1_HTTP_NEGATIVE_CASES=${JSON.stringify(matrix)}`);
  } finally {
    if (server.exitCode === null && server.signalCode === null) {
      server.kill("SIGTERM");
      await new Promise((resolve) => server.once("exit", resolve));
    }
  }
});

test("G5.1 independent DrawingModel and reimported STEP geometry observations", { skip: !enabled }, async (t) => {
  const port = 4181;
  const server = await startLocalServer(port);
  try {
    const machining = makeFormalMachiningFixture();
    const drawing = draw(machining, "generic machining DrawingModel evidence");
    assertDrawing(drawing);
    for (const kind of ["front", "top", "left"]) {
      const view = drawingView(drawing, kind);
      assert.ok(Array.isArray(view.primitives) && view.primitives.length > 0, `${kind} must expose typed DrawingModel primitives`);
    }
    assert.ok(drawing.svg.length > 1000 && drawing.dxf.length > 1000, "official SVG and DXF exports must be present");

    const response = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: machining, projectName: "G5.1 generic machining fixture" }),
    });
    assert.equal(response.status, 200, "generic machining formal API export");
    const bytes = Buffer.from(await response.arrayBuffer());
    const deliveredHash = createHash("sha256").update(bytes).digest("hex");
    assert.equal(response.headers.get("x-aicad-step-sha256"), deliveredHash, "API body is the validated hash input");
    assert.equal(Number(response.headers.get("x-aicad-step-size")), bytes.length, "API body is the validated byte count");
    for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
      assert.equal(response.headers.get(`x-aicad-step-${stage}`), "passed", `${stage} completed before delivery`);
    }
    const observed = observeStepArtifact(bytes);
    assert.equal(observed.source, "cadquery_ocp_reimported_step");
    assert.equal(observed.valid, true);
    assert.equal(observed.solid_count, 1);

    const rows = [];
    const frontBounds = drawingBounds(drawing, "front");
    requireDrawingAndStepRow(rows, {
      requirement: "Base envelope 120 × 90 mm; base thickness 20 mm",
      engineering_expected: "X/Y 120/90; base Z=0..20",
      drawing_observed: `Front X/Y bounds ${frontBounds.join(",")}; 20 mm Z thickness is NOT_OBSERVABLE_IN_CURRENT_DRAWING in the emitted typed primitives; a side dimension/detail is needed`,
      step_reimport_observed: `STEP bounds ${observed.bbox.join(",")}; planar base datums at z=0 and z=20`,
    }, frontBounds.every((value, index) => closeTo(value, [-60, 60, -45, 45][index])),
    observed.bbox.every((value, index) => closeTo(value, [-60, 60, -45, 45, 0, 30][index])) &&
      observed.surfaces.some((surface) => surface.kind === "plane" && closeTo(surface.plane_origin[2], 0)) &&
      observed.surfaces.some((surface) => surface.kind === "plane" && closeTo(surface.plane_origin[2], 20)));

    requireDrawingAndStepRow(rows, {
      requirement: "Circular ADD boss center, diameter, and height",
      engineering_expected: "center (-42,0), Ø28, Z20..30; top fillet R0.5",
      drawing_observed: "Front analytic circles at (-42,0), radii 14 and 13.5; projected height not separately dimensioned",
      step_reimport_observed: "cylinder R14, Z20..29.5; boss torus major R13.5/minor R0.5 at z29.5; max Z30",
    }, hasDrawingCircle(drawing, "front", [-42, 0], 14),
    hasCylinder(observed, { radius: 14, axis: "z", center: [-42, 0, 0], interval: [20, 29.5] }) &&
      observed.surfaces.some((surface) => surface.kind === "torus" && closeTo(surface.major_radius, 13.5) &&
        closeTo(surface.minor_radius, 0.5) && closeTo(surface.axis_origin[0], -42) && closeTo(surface.axis_origin[1], 0)));

    requireDrawingAndStepRow(rows, {
      requirement: "Through hole axis, position, diameter, and entrance chamfer",
      engineering_expected: "Z axis, center (38,0), Ø8 through, entrance C0.5",
      drawing_observed: "Front analytic Ø8 circle at (38,0); chamfer mouth is Ø9 in projection",
      step_reimport_observed: "R4 cylinder z0..19.5 plus 45° cone to z20, mouth radius 4.5",
    }, hasDrawingCircle(drawing, "front", [38, 0], 4) && hasDrawingCircle(drawing, "front", [38, 0], 4.5),
    hasCylinder(observed, { radius: 4, axis: "z", center: [38, 0, 0], interval: [0, 19.5] }) &&
      observed.surfaces.some((surface) => surface.kind === "cone" && closeTo(surface.reference_radius, 4) &&
        closeTo(surface.semi_angle_radians, Math.PI / 4) && closeTo(surface.axis_origin[0], 38) &&
        closeTo(surface.axis_origin[1], 0) && closeTo(surface.bbox[4], 19.5) && closeTo(surface.bbox[5], 20)));

    assert.ok(hasCylinder(observed, { radius: 3, axis: "x", center: [0, 0, 8], interval: [50, 60] }));
    rows.push({
      requirement: "Blind side hole axis, position, diameter, and depth",
      engineering_expected: "entry at (60,0,8), axis into −X, Ø6, depth 10",
      drawing_observed: "NOT_OBSERVABLE_IN_CURRENT_DRAWING: no analytic bore entry primitive is emitted in current named views; a section or auxiliary view is needed",
      step_reimport_observed: "R3 cylinder at y=0,z=8 with x interval 50..60",
    });

    requireDrawingAndStepRow(rows, {
      requirement: "Counterbore diameter and depth",
      engineering_expected: "center (42,-24), pilot Ø6 through; CB Ø10 × 3 mm from z20",
      drawing_observed: "Front contains pilot and hidden counterbore circles; counterbore depth is NOT_OBSERVABLE_IN_CURRENT_DRAWING (section needed)",
      step_reimport_observed: "R3 pilot z0..17; R5 counterbore z17..20",
    }, hasDrawingCircle(drawing, "front", [42, -24], 3) && hasDrawingCircle(drawing, "front", [42, -24], 5),
    hasCylinder(observed, { radius: 5, axis: "z", center: [42, -24, 0], interval: [17, 20] }) &&
      hasCylinder(observed, { radius: 3, axis: "z", center: [42, -24, 0], interval: [0, 17] }));

    const circularCenters = [[-24, 0], [24, 0], [0, -24], [0, 24]];
    requireDrawingAndStepRow(rows, {
      requirement: "Circular pattern instance count and positions",
      engineering_expected: "4 × Ø5 on radius 24 around (0,0)",
      drawing_observed: "Front shows four analytic Ø5 circles at the specified centers",
      step_reimport_observed: "four R2.5 cylinders through z0..20 at those centers",
    }, circularCenters.every((center) => hasDrawingCircle(drawing, "front", center, 2.5)),
    circularCenters.every((center) => hasCylinder(observed, { radius: 2.5, axis: "z", center: [...center, 0], interval: [0, 20] })));

    const linearCenters = [[-28, -28], [-10, -28], [8, -28]];
    requireDrawingAndStepRow(rows, {
      requirement: "Linear pattern instance count and positions",
      engineering_expected: "3 × Ø4, 18 mm spacing along +X from (-28,-28)",
      drawing_observed: "Front shows three analytic Ø4 circles at the specified centers",
      step_reimport_observed: "three R2 cylinders through z0..20 at those centers",
    }, linearCenters.every((center) => hasDrawingCircle(drawing, "front", center, 2)),
    linearCenters.every((center) => hasCylinder(observed, { radius: 2, axis: "z", center: [...center, 0], interval: [0, 20] })));

    requireDrawingAndStepRow(rows, {
      requirement: "Extrude CUT pocket extent and direction",
      engineering_expected: "18 × 12 pocket at (20,24), cut from z20 toward −Z by 6 mm",
      drawing_observed: "Front projected opening edges at x=11..29, y=18..30; cut depth is NOT_OBSERVABLE_IN_CURRENT_DRAWING (section needed)",
      step_reimport_observed: "horizontal pocket floor z14 within x11..29,y18..30",
    }, hasDrawingSegment(drawing, "front", [11, 18], [29, 18]) &&
      hasDrawingSegment(drawing, "front", [11, 18], [11, 30]),
    hasPlanePatch(observed, { z: 14, x: [11, 29], y: [18, 30] }));

    // Axis and external stepped radii are measurable from the drawing silhouette;
    // the 360-degree rotational coverage is not uniquely determined by orthographic views.
    const revolve = makeFormalRevolveFixture();
    const revolveDrawing = draw(revolve, "generic revolved DrawingModel evidence");
    assertDrawing(revolveDrawing);
    const revolveResponse = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: revolve, projectName: "G5.1 generic revolved fixture" }),
    });
    assert.equal(revolveResponse.status, 200, "generic revolved formal API export");
    const revolveBytes = Buffer.from(await revolveResponse.arrayBuffer());
    const revolveHash = createHash("sha256").update(revolveBytes).digest("hex");
    assert.equal(revolveResponse.headers.get("x-aicad-step-sha256"), revolveHash);
    assert.equal(Number(revolveResponse.headers.get("x-aicad-step-size")), revolveBytes.length);
    for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
      assert.equal(revolveResponse.headers.get(`x-aicad-step-${stage}`), "passed");
    }
    const revolveObserved = observeStepArtifact(revolveBytes);
    assert.equal(revolveObserved.valid, true);
    assert.equal(revolveObserved.solid_count, 1);
    const expectedRevolveSections = [
      { radius: 20, interval: [0.5, 19.5] },
      { radius: 15, interval: [20, 60] },
      { radius: 10, interval: [60, 80] },
    ];
    assert.ok(expectedRevolveSections.every(({ radius, interval }) =>
      hasCylinder(revolveObserved, { radius, axis: "x", center: [0, 0, 0], interval })),
    "STEP reimport must expose the three expected coaxial profile radii");
    assert.ok(revolveObserved.surfaces.some((surface) => surface.kind === "cone" &&
      closeTo(surface.reference_radius, 20) && closeTo(surface.semi_angle_radians, Math.PI / 4) &&
      closeTo(surface.bbox[0], 0) && closeTo(surface.bbox[1], 0.5)), "STEP reimport must expose the actual 0.5 chamfer");
    assert.ok(revolveObserved.surfaces.some((surface) => surface.kind === "torus" &&
      closeTo(surface.major_radius, 19.5) && closeTo(surface.minor_radius, 0.5)), "STEP reimport must expose the actual R0.5 fillet");
    const frontRevolve = drawingView(revolveDrawing, "front").primitives.map((item) => item.geometry);
    const hasProjectedAxialLevel = (y, minX, maxX) => frontRevolve.some((geometry) => geometry.kind === "line" &&
      closeTo(geometry.start.y, y) && closeTo(geometry.end.y, y) &&
      Math.min(geometry.start.x, geometry.end.x) <= minX + GEOMETRY_TOLERANCE_MM &&
      Math.max(geometry.start.x, geometry.end.x) >= maxX - GEOMETRY_TOLERANCE_MM);
    const drawingRevolveRadii = hasProjectedAxialLevel(20, 0.5, 19.5) &&
      hasProjectedAxialLevel(-15, 20, 36) && hasProjectedAxialLevel(10, 60, 80);
    assert.equal(drawingRevolveRadii, true, "Front silhouette must expose the stepped external radial levels");
    rows.push({
      requirement: "Revolve axis and key external radii",
      engineering_expected: "axis X; full 360°; radii 20/15/10 over x=[0,20],[20,60],[60,80]",
      drawing_observed: "Front silhouette measures the X-axis stepped levels R20/R15/R10; sweep angle is NOT_OBSERVABLE_IN_CURRENT_DRAWING",
      step_reimport_observed: "OCP analytic cylinders confirm axis X, radii 20/15/10 and full-turn u-span 2π",
    });
    rows.push({
      requirement: "Revolve sweep angle",
      engineering_expected: "full_360",
      drawing_observed: "NOT_OBSERVABLE_IN_CURRENT_DRAWING; orthographic silhouettes cannot prove rotational coverage",
      step_reimport_observed: "all main cylindrical surfaces have u-span 2π",
    });
    rows.push({
      requirement: "Actual Fillet/Chamfer machining dimensions",
      engineering_expected: "boss fillet R0.5; through-hole chamfer C0.5; revolve fillet R0.5 and chamfer C0.5",
      drawing_observed: "NOT_OBSERVABLE_IN_CURRENT_DRAWING as dimensioned manufacturing requirements; detail/section callouts needed",
      step_reimport_observed: "torus minor radii 0.5 and conical surfaces 45° with 0.5 mm axial extent",
    });
    assert.ok(rows.every((row) => typeof row.engineering_expected === "string" && row.drawing_observed && row.step_reimport_observed));
    t.diagnostic(`G5_1_DRAWING_STEP_TOLERANCE_MM=${GEOMETRY_TOLERANCE_MM}`);
    t.diagnostic(`G5_1_FEATURE_CONFORMANCE=${JSON.stringify(rows)}`);
    t.diagnostic(`G5_1_GENERIC_MACHINING_SAME_VALIDATED_BYTES=${deliveredHash};size=${bytes.length}`);
    t.diagnostic(`G5_1_GENERIC_REVOLVED_SAME_VALIDATED_BYTES=${revolveHash};size=${revolveBytes.length}`);
    t.diagnostic(`G5_1_GENERIC_DRAWING_TIMINGS_MS=${JSON.stringify(drawing.parametric_timings_ms)}`);
    t.diagnostic(`G5_1_REVOLVED_DRAWING_TIMINGS_MS=${JSON.stringify(revolveDrawing.parametric_timings_ms)}`);
  } finally {
    if (server.exitCode === null && server.signalCode === null) {
      server.kill("SIGTERM");
      await new Promise((resolve) => server.once("exit", resolve));
    }
  }
});

test("G6B Countersink and ExplicitPlacementPattern reach Drawing and canonical STEP reimport", { skip: !enabled }, async (t) => {
  const port = 4182;
  const server = await startLocalServer(port);
  try {
    const model = makeG6bMachiningFixture();
    const drawing = draw(model, "G6B generic machining DrawingModel");
    assertDrawing(drawing);
    assert.ok(drawing.spatial_summary.parametric.operation_features.includes("g6b_explicit_hole_pattern"));
    assert.ok(drawing.spatial_summary.parametric.operation_features.includes("g6b_explicit_cut_pattern"));
    assert.ok(drawing.spatial_summary.parametric.face_count > 0);
    assert.ok(drawing.svg.includes("view_front") && drawing.dxf.includes("SECTION"));
    const sinkAssociation = drawing.drawing_report.associative_dimensions.find((item) =>
      item.annotation_id === "associative.hole.g6b_top_countersink.callout");
    assert.equal(sinkAssociation?.status, "unsupported");
    assert.match(sinkAssociation?.diagnostic ?? "", /COUNTERSINK_ANNOTATION_UNSUPPORTED/);
    assert.ok(hasDrawingCircle(drawing, "front", [-40, 0], 5) && hasDrawingCircle(drawing, "front", [-40, 0], 2),
      "the orthographic projection must expose both the true countersink mouth and coaxial bore boundaries");
    assert.ok(hasDrawingCircle(drawing, "front", [-25, 0], 4) && hasDrawingCircle(drawing, "front", [-25, 0], 2),
      "a blind Countersink projection must retain its mouth and main-bore circles");

    const sectionRoot = mkdtempSync(join(tmpdir(), "aicad-g6b-section-"));
    let section;
    try {
      const modelPath = join(sectionRoot, "model.json");
      writeFileSync(modelPath, `${JSON.stringify(model)}\n`, "utf8");
      const result = spawnSync(binary, ["--json-errors", "--quiet", "engineering", "drawing-section", modelPath,
        "--axis", "y", "--offset-mm", "0", "--view-side", "positive"], {
        encoding: "utf8", maxBuffer: 32 * 1024 * 1024, env: process.env,
      });
      assert.equal(result.error, undefined, result.error?.message);
      assert.equal(result.status, 0, result.stderr);
      section = JSON.parse(result.stdout);
    } finally {
      rmSync(sectionRoot, { recursive: true, force: true });
    }
    assert.equal(section.drawing_mode, "three_view_section");
    assert.equal(section.drawing_report.view_count, 4);
    assert.ok(section.section_view.section_edge_count > 0);
    assert.match(section.svg, /section_aa_hatch/);
    assert.match(section.dxf, /AICAD_HATCH/);

    const response = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ model, projectName: "G6B generic fixture" }),
    });
    assert.equal(response.status, 200, await response.clone().text());
    const bytes = Buffer.from(await response.arrayBuffer());
    const hash = createHash("sha256").update(bytes).digest("hex");
    assert.equal(response.headers.get("x-aicad-step-sha256"), hash);
    assert.equal(Number(response.headers.get("x-aicad-step-size")), bytes.length);
    for (const stage of ["manufacturing", "build", "reimport", "equivalence"]) {
      assert.equal(response.headers.get(`x-aicad-step-${stage}`), "passed");
    }
    const observed = observeStepArtifact(bytes);
    assert.equal(observed.valid, true);
    assert.equal(observed.solid_count, 1);
    assert.ok(closeTo(observed.bbox[0], -50) && closeTo(observed.bbox[1], 50));
    const coneMatches = (origin, axis, bounds, mouthRadius = 5) => observed.surfaces.some((surface) =>
      surface.kind === "cone" && closeTo(Math.abs(surface.semi_angle_radians), Math.PI / 4) &&
      closeTo(surface.axis_origin[0], origin[0]) && closeTo(surface.axis_origin[1], origin[1]) &&
      axis.every((value, index) => closeTo(Math.abs(surface.axis_direction[index]), Math.abs(value))) &&
      bounds.every((value, index) => closeTo(surface.bbox[index], value)) &&
      closeTo(Math.max(Math.abs(surface.bbox[0] - origin[0]), Math.abs(surface.bbox[1] - origin[0])), mouthRadius));
    assert.ok(coneMatches([-40, 0, 10], [0, 0, 1], [-45, -35, -5, 5, 7, 10]),
      "reimported STEP must contain the exact top Countersink cone at the declared location and 90° included angle");
    const topBoreSurfaces = observed.surfaces.filter((surface) => surface.kind === "cylinder" &&
      closeTo(surface.radius, 2) && closeTo(surface.axis_origin[0], -40) && closeTo(surface.axis_origin[1], 0));
    assert.ok(hasCylinder(observed, { radius: 2, axis: "z", center: [-40, 0, 0], interval: [0, 7] }),
      JSON.stringify({ topBoreSurfaces, coneSurfaces: observed.surfaces.filter((surface) => surface.kind === "cone") }));
    assert.ok(hasCylinder(observed, { radius: 1.5, axis: "z", center: [-20, 20, 0], interval: [0, 10] }));
    assert.ok(hasCylinder(observed, { radius: 1.5, axis: "z", center: [-35, -20, 0], interval: [0, 10] }));
    assert.ok(hasCylinder(observed, { radius: 1.5, axis: "z", center: [0, -20, 0], interval: [0, 10] }));
    const circularCenters = [[40, 0], [25, 15], [10, 0], [25, -15]];
    const circularCylinderEvidence = circularCenters.map((center) => ({
      center,
      surfaces: observed.surfaces.filter((surface) => surface.kind === "cylinder" && closeTo(surface.radius, 1.5) &&
        closeTo(surface.axis_origin[0], center[0]) && closeTo(surface.axis_origin[1], center[1]))
        .map((surface) => ({ axis_origin: surface.axis_origin, axis_direction: surface.axis_direction, bbox: surface.bbox })),
    }));
    assert.ok(circularCenters.every((center) => hasCylinder(observed, {
      radius: 1.5, axis: "z", center: [...center, 0], interval: [0, 8],
    })), JSON.stringify(circularCylinderEvidence));
    assert.ok(observed.surfaces.some((surface) => surface.kind === "cone" &&
      closeTo(Math.abs(surface.semi_angle_radians), Math.PI / 4) &&
      closeTo(surface.bbox[0], 48) && closeTo(surface.bbox[1], 50) &&
      closeTo(surface.axis_origin[1], -5) && closeTo(surface.axis_origin[2], 5) &&
      closeTo(surface.bbox[2], -9) && closeTo(surface.bbox[3], -1) &&
      closeTo(surface.bbox[4], 1) && closeTo(surface.bbox[5], 9)),
      "a side-facing Countersink must preserve its non-Z axis in STEP");
    const explicitHoleCenters = [[-40, 20], [-20, 20], [-35, -20], [0, -20]];
    assert.ok(explicitHoleCenters.every((center) => hasDrawingCircle(drawing, "front", center, 1.5)),
      "Front Drawing projection must show the seed plus each nonuniform explicit Hole placement");
    assert.ok(explicitHoleCenters.every((center) => hasCylinder(observed, {
      radius: 1.5, axis: "z", center: [...center, 0], interval: [0, 10],
    })), "reimported STEP must contain all explicit Hole instance positions");
    const explicitCutCenters = [[0, 0], [-35, 10], [-10, -18], [15, 15]];
    const hasCutFloor = ([cx, cy]) => observed.surfaces.some((surface) => surface.kind === "plane" &&
      Math.abs(surface.plane_normal[2]) > 1 - GEOMETRY_TOLERANCE_MM &&
      closeTo(surface.bbox[0], cx - 3) && closeTo(surface.bbox[1], cx + 3) &&
      closeTo(surface.bbox[2], cy - 2) && closeTo(surface.bbox[3], cy + 2) &&
      closeTo(surface.bbox[4], 4) && closeTo(surface.bbox[5], 4));
    assert.ok(explicitCutCenters.every(hasCutFloor),
      "STEP reimport must show the seed plus each explicit Extrude CUT instance as a real floor face");
    assert.ok(explicitCutCenters.slice(1).every(([cx, cy]) =>
      hasDrawingSegment(drawing, "front", [cx - 3, cy - 2], [cx + 3, cy - 2])),
    "Drawing projection must show each explicit Extrude CUT profile, not a textual instance count");

    const repeatedDrawing = draw(model, "G6B deterministic rebuild");
    assert.deepEqual(repeatedDrawing.svg, drawing.svg);
    assert.deepEqual(repeatedDrawing.dxf, drawing.dxf);
    const editedSeed = structuredClone(model);
    editedSeed.features.g6b_explicit_hole_seed.feature.diameter = 5;
    const rebuiltSeed = draw(editedSeed, "G6B pattern after seed diameter edit");
    assert.ok(explicitHoleCenters.every((center) => hasDrawingCircle(rebuiltSeed, "front", center, 2.5)),
      "changing the seed diameter must deterministically regenerate every derived ExplicitPlacement instance");
    const suppressedSeed = structuredClone(model);
    suppressedSeed.features.g6b_explicit_hole_seed.suppressed = true;
    assert.equal(errorCode(suppressedSeed), "PATTERN_SEED_NOT_FOUND",
      "an active ExplicitPlacementPattern cannot silently ignore its suppressed seed");
    const revolveModel = projectV2Prefix(makeFormalRevolveFixture(), "shaft_revolve");
    revolveModel.part.name = "G6B generic Countersink on Revolve body";
    revolveModel.features.revolve_countersink = {
      type: "parametric", id: "revolve_countersink", name: "revolve_countersink", suppressed: false,
      feature: { type: "hole", support: { kind: "principal", plane: "y_z" }, position: { x: 0, y: 0 },
        diameter: 4, reversed: false, termination: { kind: "through_all" },
        kind: { type: "countersink", diameter: 8, included_angle_degrees: 90 } },
    };
    revolveModel.feature_order.push("revolve_countersink");
    const revolveDrawing = draw(revolveModel, "G6B Countersink on generic Revolve body");
    assertDrawing(revolveDrawing);
    // A Countersink mouth on a curved outside is not a planar circle. Use the
    // formal A-A cut through the hole axis and compare it with the same generic
    // Revolve body without the feature; the actual analytic dimensions are
    // independently checked on STEP re-import below.
    const revolveBaseModel = structuredClone(revolveModel);
    revolveBaseModel.part.name = revolveModel.part.name;
    delete revolveBaseModel.features.revolve_countersink;
    revolveBaseModel.feature_order = revolveBaseModel.feature_order.filter((id) => id !== "revolve_countersink");
    const buildRevolveSection = (model, label) => {
      const sectionRoot = mkdtempSync(join(tmpdir(), "aicad-g6b-revolve-section-"));
      try {
        const modelPath = join(sectionRoot, "model.json");
        writeFileSync(modelPath, `${JSON.stringify(model)}\n`, "utf8");
        const result = spawnSync(binary, ["--json-errors", "--quiet", "engineering", "drawing-section", modelPath,
          "--axis", "y", "--offset-mm", "0", "--view-side", "positive"], {
          encoding: "utf8", maxBuffer: 32 * 1024 * 1024, env: process.env,
        });
        assert.equal(result.error, undefined, result.error?.message);
        assert.equal(result.status, 0, `${label}: ${result.stderr}`);
        return JSON.parse(result.stdout);
      } finally {
        rmSync(sectionRoot, { recursive: true, force: true });
      }
    };
    const revolveBaseSection = buildRevolveSection(revolveBaseModel, "Revolve A-A baseline");
    const revolveCountersinkSection = buildRevolveSection(revolveModel, "Revolve A-A with Countersink");
    for (const section of [revolveBaseSection, revolveCountersinkSection]) {
      assert.equal(section.drawing_mode, "three_view_section");
      assert.equal(section.drawing_report.view_count, 4);
      assert.ok(section.section_view.section_edge_count > 0);
      assert.match(section.svg, /section_aa_hatch/);
      assert.match(section.dxf, /AICAD_HATCH/);
    }
    assert.notDeepEqual(
      drawingView(revolveCountersinkSection, "section_aa").primitives.map((item) => item.geometry),
      drawingView(revolveBaseSection, "section_aa").primitives.map((item) => item.geometry),
      "the A-A Drawing section must reflect the Countersink geometry added to the Revolve body",
    );
    const revolveResponse = await fetch(`http://127.0.0.1:${port}/api/engineering/export/step`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: revolveModel, projectName: "G6B Countersink on Revolve" }),
    });
    assert.equal(revolveResponse.status, 200, await revolveResponse.clone().text());
    const revolveBytes = Buffer.from(await revolveResponse.arrayBuffer());
    const revolveHash = createHash("sha256").update(revolveBytes).digest("hex");
    assert.equal(revolveResponse.headers.get("x-aicad-step-sha256"), revolveHash);
    const throughBoreProbePoints = [3, 10, 40, 79].flatMap((x) => [
      [x, 0, 0], [x, 1.9, 0], [x, 2.1, 0],
    ]);
    const revolveObserved = observeStepArtifact(revolveBytes, throughBoreProbePoints);
    assert.equal(revolveObserved.valid, true);
    assert.ok(revolveObserved.surfaces.some((surface) => surface.kind === "cylinder" &&
      closeTo(surface.radius, 2) && Math.abs(surface.axis_direction[0]) > 1 - GEOMETRY_TOLERANCE_MM &&
      closeTo(surface.axis_origin[1], 0) && closeTo(surface.axis_origin[2], 0)),
    "STEP reimport must expose the analytic Ø4 main-bore wall");
    const probeByPoint = new Map(revolveObserved.material_probes.map(({ point, is_inside }) => [point.join(","), is_inside]));
    for (const x of [3, 10, 40, 79]) {
      assert.equal(probeByPoint.get([x, 0, 0].join(",")), false, `through-all bore center must be void at x=${x}`);
      assert.equal(probeByPoint.get([x, 1.9, 0].join(",")), false, `Ø4 bore interior must be void at x=${x}`);
      assert.equal(probeByPoint.get([x, 2.1, 0].join(",")), true, `material outside Ø4 bore must remain at x=${x}`);
    }
    assert.ok(revolveObserved.surfaces.some((surface) => surface.kind === "cone" &&
      closeTo(Math.abs(surface.semi_angle_radians), Math.PI / 4) &&
      closeTo(surface.bbox[0], 0) && closeTo(surface.bbox[1], 2) &&
      closeTo(surface.bbox[2], -4) && closeTo(surface.bbox[3], 4) &&
      closeTo(surface.bbox[4], -4) && closeTo(surface.bbox[5], 4)),
      JSON.stringify(revolveObserved.surfaces.filter((surface) => surface.kind === "cone")));
    t.diagnostic(`G6B_EXPLICIT_PLACEMENTS=seed+3_holes_and_seed+3_extrude_cuts;nonuniform=true`);
    t.diagnostic(`G6B_COUNTERSINK_CIRCULAR_PATTERN=4_instances;side_axis=+X;reimported_cones=${observed.surfaces.filter((surface) => surface.kind === "cone").length}`);
    t.diagnostic(`G6B_DRAWING_SECTION_EDGES=${section.section_view.section_edge_count};SVG_BYTES=${Buffer.byteLength(drawing.svg)};DXF_BYTES=${Buffer.byteLength(drawing.dxf)}`);
    t.diagnostic(`G6B_REVOLVE_COUNTERSINK_STEP_BYTES=${revolveBytes.length};SHA256=${revolveHash}`);
    t.diagnostic(`G6B_FORMAL_STEP_BYTES=${bytes.length};SHA256=${hash};bounds=${observed.bbox.join(",")};timings=${JSON.stringify(drawing.parametric_timings_ms)}`);
  } finally {
    if (server.exitCode === null && server.signalCode === null) {
      server.kill("SIGTERM");
      await new Promise((resolve) => server.once("exit", resolve));
    }
  }
});

test("G6A principal-plane Section A-A is rebuilt from verified OCP geometry and updates with EngineeringModel edits", { skip: !enabled }, (t) => {
  const builder = modelBuilder("G6A generic section fixture");
  const baseSketch = builder.sketch("section_plate_sketch", [rectangle(0, 0, 140, 100)]);
  builder.extrude("section_plate_add", baseSketch, "add", extDistance(20));
  const top = builder.datum("section_plate_top", "x_y", 20);
  const addHole = (id, x, diameter, termination) => builder.addFeature(id, {
    type: "hole", support: top, position: { x, y: 0 }, diameter, reversed: true,
    termination, kind: { type: "simple" },
  });
  addHole("section_through_hole", 0, 20, { kind: "through_all" });
  addHole("section_blind_hole", 38, 8, { kind: "blind", depth: 8 });
  builder.model.drawing.properties.projection_method = { type: "projection_method", method: "third_angle" };

  const invokeSection = (model) => {
    const root = mkdtempSync(join(tmpdir(), "aicad-g6a-section-"));
    try {
      const path = join(root, "model.json");
      writeFileSync(path, `${JSON.stringify(model)}\n`, "utf8");
      return spawnSync(binary, ["--json-errors", "--quiet", "engineering", "drawing-section", path,
        "--axis", "y", "--offset-mm", "0", "--view-side", "positive"], {
        encoding: "utf8", maxBuffer: 32 * 1024 * 1024, env: process.env,
      });
    } finally { rmSync(root, { recursive: true, force: true }); }
  };
  const run = (model) => {
    const result = invokeSection(model);
    assert.equal(result.error, undefined, result.error?.message);
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  };

  const first = run(builder.model);
  const repeated = run(builder.model);
  assert.equal(first.drawing_mode, "three_view_section");
  assert.deepEqual(first.svg, repeated.svg, "identical feature histories yield deterministic Section A-A SVG");
  assert.deepEqual(first.dxf, repeated.dxf, "identical feature histories yield deterministic Section A-A DXF");
  assert.ok(first.section_view.section_edge_count > 0);
  assert.ok(first.section_view.boundary_loop_count >= 2, "section keeps material and internal void boundaries");
  assert.equal(first.section_view.sample_chord_tolerance_mm, 0.01);
  assert.match(first.svg, /id="view_front"/);
  assert.match(first.svg, /id="view_top"/);
  assert.match(first.svg, /id="view_left"/);
  assert.match(first.svg, /id="view_section_aa"/);
  assert.match(first.svg, /id="section_aa_hatch"/);
  assert.match(first.svg, /剖视 A-A/);
  assert.match(first.dxf, /\$INSUNITS\n70\n4/);
  assert.match(first.dxf, /AICAD_HATCH/);
  assert.match(first.dxf, /剖视 A-A/);
  assert.equal(first.drawing_report.view_count, 4);
  assert.deepEqual(first.drawing_report.views.map((view) => view.kind).sort(), ["front", "left", "section_aa", "top"]);

  const edited = structuredClone(builder.model);
  edited.features.section_blind_hole.feature.termination.depth = 12;
  const afterEdit = run(edited);
  assert.notEqual(afterEdit.svg, first.svg, "a changed validated hole feature rebuilds the section geometry");
  assert.ok(afterEdit.section_view.section_edge_count > 0);
  t.diagnostic(`G6A_SECTION_REPEATABLE=true;loops=${first.section_view.boundary_loop_count};edges=${first.section_view.section_edge_count};chord_tolerance_mm=${first.section_view.sample_chord_tolerance_mm};updated_after_depth_edit=true`);
});
