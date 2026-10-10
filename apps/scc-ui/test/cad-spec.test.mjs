import assert from "node:assert/strict";
import test from "node:test";
import { assertBuildableCadSpec, normalizeCadSpec } from "../lib/cad-spec.mjs";

function gearboxSpec() {
  return {
    version: "1.0",
    kind: "precision_gearbox_housing_v1",
    units: "mm",
    parameters: {
      envelope: {
        base_length: 260,
        base_width: 190,
        total_height: 165,
        base_thickness: 18,
        body_length: 220,
        body_width: 150,
        body_height: 147,
      },
      shell: {
        wall_thickness: 10,
        top_thickness: 12,
        bottom_reinforcement: 16,
        corner_radius: 18,
        body_radius: 8,
        inner_length: 190,
        inner_width: 120,
        inner_height: 118,
      },
      axes: {
        main: { x: 0, z: 92, through_diameter: 72 },
        second: { x: 78, z: 92, through_diameter: 48, center_distance: 78 },
        third: { z: 48, through_diameter: 35, center_distance: 62, horizontal_side: "right" },
      },
    },
    manufacturing: {
      fits: [],
      tolerances: [],
      datums: [],
      geometric_controls: [],
      surface_finishes: [],
      threads: [],
    },
    assumptions: [],
    unresolved: [],
  };
}

test("normalizes a gearbox CadSpec and derives the approved third-axis position", () => {
  const normalized = normalizeCadSpec(gearboxSpec());

  assert.equal(normalized.parameters.axes.third.x, Math.sqrt((62 ** 2) - (44 ** 2)));
  assert.match(normalized.assumptions[0].message, /right/);
  assert.match(normalized.source_hash, /^[a-f0-9]{64}$/);
  assert.equal(normalizeCadSpec(gearboxSpec()).source_hash, normalized.source_hash);
});

test("normalizing an already normalized CadSpec is stable", () => {
  const first = normalizeCadSpec(gearboxSpec());
  const second = normalizeCadSpec(first);

  assert.equal(second.source_hash, first.source_hash);
  assert.equal(
    second.assumptions.filter((item) => item.field === "parameters.axes.third.x").length,
    1,
  );
});

test("rejects fields outside the CadSpec contract", () => {
  const spec = gearboxSpec();
  spec.python = "import os";

  assert.throws(
    () => normalizeCadSpec(spec),
    (error) => error.code === "SCC-CAD-001" && /Unknown field: python/.test(error.message),
  );
});

test("rejects an impossible third-axis center distance", () => {
  const spec = gearboxSpec();
  spec.parameters.axes.third.center_distance = 40;

  assert.throws(
    () => normalizeCadSpec(spec),
    (error) => error.code === "SCC-CAD-003" && /center distance/.test(error.message),
  );
});

test("critical unresolved intent cannot enter the CAD kernel", () => {
  const spec = gearboxSpec();
  spec.unresolved.push({ severity: "critical", field: "axes.third.horizontal_side", message: "Choose left or right" });
  const normalized = normalizeCadSpec(spec);

  assert.throws(
    () => assertBuildableCadSpec(normalized),
    (error) => error.code === "SCC-CAD-004" && error.details.unresolved.length === 1,
  );
});
