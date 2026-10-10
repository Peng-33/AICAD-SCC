import assert from "node:assert/strict";
import test from "node:test";
import { normalizeCadDocument } from "../lib/cad-document.mjs";
import { normalizeCadSpec } from "../lib/cad-spec.mjs";

function flangeDocument() {
  return {
    version: "1.0",
    kind: "feature_graph_v1",
    name: "six-hole-flange",
    units: "mm",
    parameters: {
      outer_diameter: { value: 160, source: "user" },
      thickness: { value: 18, source: "user" },
      bore_diameter: { value: 60, source: "user" },
      mounting_hole_diameter: { value: 14, source: "user" },
      mounting_pcd: { value: 120, source: "user" },
    },
    features: [
      {
        id: "body",
        type: "cylinder",
        operation: "add",
        diameter: { parameter: "outer_diameter" },
        length: { parameter: "thickness" },
        axis: "z",
        origin: { x: 0, y: 0, z: 0 },
      },
      {
        id: "center_bore",
        type: "cylinder",
        operation: "cut",
        diameter: { parameter: "bore_diameter" },
        length: 20,
        axis: "z",
        origin: { x: 0, y: 0, z: -1 },
      },
      {
        id: "mounting_pattern",
        type: "circular_hole_pattern",
        operation: "cut",
        diameter: { parameter: "mounting_hole_diameter" },
        length: 20,
        count: 6,
        pitch_diameter: { parameter: "mounting_pcd" },
        start_angle: 0,
        axis: "z",
        origin: { x: 0, y: 0, z: -1 },
      },
    ],
    requirements: [
      { id: "req_body", text: "Outer diameter 160 mm and thickness 18 mm", disposition: "geometry", feature_ids: ["body"] },
      { id: "req_bores", text: "Center bore and six mounting holes", disposition: "geometry", feature_ids: ["center_bore", "mounting_pattern"] },
      { id: "req_fit", text: "Center bore H7", disposition: "manufacturing", manufacturing_ref: "fit_center_bore" },
    ],
    manufacturing: {
      fits: [{
        id: "fit_center_bore",
        feature_id: "center_bore",
        nominal_diameter: 60,
        designation: "H7",
      }],
      tolerances: [{
        id: "tol_outer_diameter",
        parameter_id: "outer_diameter",
        kind: "bilateral",
        lower_deviation: -0.1,
        upper_deviation: 0.1,
      }],
      datums: [{ id: "datum_a", label: "A", kind: "axis", feature_id: "center_bore" }],
      geometric_controls: [{
        id: "gdt_hole_position",
        kind: "position",
        feature_id: "mounting_pattern",
        tolerance: 0.1,
        diameter_zone: true,
        datum_ids: ["datum_a"],
      }],
      surface_finishes: [{ id: "finish_center_bore", feature_id: "center_bore", ra_um: 1.6 }],
      threads: [],
    },
    assumptions: [],
    unresolved: [],
  };
}

test("normalizes a generic FeatureGraph with stable parameter references and coverage", () => {
  const normalized = normalizeCadDocument(flangeDocument());

  assert.equal(normalized.kind, "feature_graph_v1");
  assert.equal(normalized.features.length, 3);
  assert.equal(normalized.requirements[1].feature_ids[1], "mounting_pattern");
  assert.equal(normalized.manufacturing.fits[0].id, "fit_center_bore");
  assert.match(normalized.source_hash, /^[a-f0-9]{64}$/);
  assert.equal(normalizeCadDocument(normalized).source_hash, normalized.source_hash);
});

test("evaluates derived parameter expressions and propagates driver edits deterministically", () => {
  const original = flangeDocument();
  original.parameters.through_cut_length = {
    source: "derived",
    expression: { op: "add", args: [{ parameter: "thickness" }, 2] },
  };
  original.features[1].length = { parameter: "through_cut_length" };
  original.features[2].length = { parameter: "through_cut_length" };
  original.requirements.push({
    id: "req_through_cut_length",
    text: "Through cuts extend one millimetre beyond each face",
    disposition: "derived",
    parameter_ids: ["through_cut_length"],
  });

  const first = normalizeCadDocument(original);
  assert.equal(first.parameters.through_cut_length.value, 20);
  assert.equal(normalizeCadDocument(first).parameters.through_cut_length.value, 20);

  const edited = structuredClone(first);
  edited.parameters.thickness.value = 24;
  const rebuilt = normalizeCadDocument(edited);
  assert.equal(rebuilt.parameters.through_cut_length.value, 26);
  assert.notEqual(rebuilt.source_hash, first.source_hash);
});

test("rejects cyclic, missing, and mathematically invalid derived parameters", () => {
  const cyclic = flangeDocument();
  cyclic.parameters.a = { source: "derived", expression: { op: "add", args: [{ parameter: "b" }, 1] } };
  cyclic.parameters.b = { source: "derived", expression: { op: "add", args: [{ parameter: "a" }, 1] } };
  assert.throws(
    () => normalizeCadDocument(cyclic),
    (error) => error.code === "SCC-CAD-014" && /cycle/i.test(error.message),
  );

  const missing = flangeDocument();
  missing.parameters.bad = { source: "derived", expression: { op: "add", args: [{ parameter: "absent" }, 1] } };
  assert.throws(
    () => normalizeCadDocument(missing),
    (error) => error.code === "SCC-CAD-011" && /absent/.test(error.message),
  );

  const divisionByZero = flangeDocument();
  divisionByZero.parameters.bad = { source: "derived", expression: { op: "divide", args: [10, 0] } };
  assert.throws(
    () => normalizeCadDocument(divisionByZero),
    (error) => error.code === "SCC-CAD-014" && /zero/i.test(error.message),
  );
});

test("validates stable manufacturing semantics against geometric evidence", () => {
  const normalized = normalizeCadDocument(flangeDocument());
  assert.equal(normalized.requirements[2].manufacturing_ref, "fit_center_bore");
  assert.deepEqual(normalized.manufacturing.geometric_controls[0].datum_ids, ["datum_a"]);

  const contradictoryFit = flangeDocument();
  contradictoryFit.manufacturing.fits[0].nominal_diameter = 61;
  assert.throws(
    () => normalizeCadDocument(contradictoryFit),
    (error) => error.code === "SCC-CAD-015" && /nominal diameter/i.test(error.message),
  );

  const missingDatum = flangeDocument();
  missingDatum.manufacturing.geometric_controls[0].datum_ids = ["datum_missing"];
  assert.throws(
    () => normalizeCadDocument(missingDatum),
    (error) => error.code === "SCC-CAD-015" && /datum_missing/.test(error.message),
  );

  const duplicateId = flangeDocument();
  duplicateId.manufacturing.surface_finishes[0].id = "fit_center_bore";
  assert.throws(
    () => normalizeCadDocument(duplicateId),
    (error) => error.code === "SCC-CAD-012" && /fit_center_bore/.test(error.message),
  );
});

test("the public CadSpec boundary dispatches generic documents to FeatureGraph validation", () => {
  const normalized = normalizeCadSpec(flangeDocument());

  assert.equal(normalized.kind, "feature_graph_v1");
  assert.equal(normalized.features[2].type, "circular_hole_pattern");
});

test("editing a driving parameter changes the source hash without rewriting features", () => {
  const original = normalizeCadDocument(flangeDocument());
  const changed = flangeDocument();
  changed.parameters.outer_diameter.value = 170;
  const normalized = normalizeCadDocument(changed);

  assert.notEqual(normalized.source_hash, original.source_hash);
  assert.deepEqual(normalized.features, original.features);
});

test("rejects unknown feature types and unresolved parameter references", () => {
  const unknown = flangeDocument();
  unknown.features[0].type = "run_python";
  assert.throws(() => normalizeCadDocument(unknown), (error) => error.code === "SCC-CAD-010" && /feature type/.test(error.message));

  const missing = flangeDocument();
  missing.features[0].diameter = { parameter: "missing" };
  assert.throws(() => normalizeCadDocument(missing), (error) => error.code === "SCC-CAD-011" && /missing/.test(error.message));
});

test("rejects duplicate IDs and requirement coverage that points at no feature", () => {
  const duplicate = flangeDocument();
  duplicate.features[1].id = "body";
  assert.throws(() => normalizeCadDocument(duplicate), (error) => error.code === "SCC-CAD-012" && /Duplicate feature ID/.test(error.message));

  const uncovered = flangeDocument();
  uncovered.requirements[0].feature_ids = ["imaginary_feature"];
  assert.throws(() => normalizeCadDocument(uncovered), (error) => error.code === "SCC-CAD-013" && /imaginary_feature/.test(error.message));
});

test("requires explicit unsupported reasons instead of silently accepting omissions", () => {
  const document = flangeDocument();
  document.requirements.push({ id: "req_drawing", text: "Generate an associative drawing", disposition: "unsupported" });

  assert.throws(() => normalizeCadDocument(document), (error) => error.code === "SCC-CAD-013" && /reason/.test(error.message));
});

test("rejects active geometry invented without a design-intent requirement", () => {
  const document = flangeDocument();
  document.features.splice(1, 0, {
    id: "unclaimed_boss",
    type: "cylinder",
    operation: "add",
    diameter: 80,
    length: 4,
    axis: "z",
    origin: { x: 0, y: 0, z: 18 },
  });

  assert.throws(
    () => normalizeCadDocument(document),
    (error) => error.code === "SCC-CAD-013" && /unclaimed_boss/.test(error.message),
  );
});
