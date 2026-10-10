import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import test from "node:test";
import { createCadService } from "../lib/cad-service.mjs";

function gearboxSpec() {
  return {
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
}

function featureGraphSpec() {
  return {
    version: "1.0",
    kind: "feature_graph_v1",
    name: "test-block",
    units: "mm",
    parameters: { width: { value: 40, source: "user" } },
    features: [{
      id: "body", type: "box", operation: "add",
      size: { x: { parameter: "width" }, y: 30, z: 10 },
      origin: { x: 0, y: 0, z: 0 },
    }],
    requirements: [{ id: "req_body", text: "40 by 30 by 10 block", disposition: "geometry", feature_ids: ["body"] }],
    manufacturing: { fits: [], tolerances: [], datums: [], geometric_controls: [], surface_finishes: [], threads: [] },
    assumptions: [],
    unresolved: [],
  };
}

test("builds a normalized CadSpec and serves only non-STEP allowlisted artifacts", async () => {
  const service = createCadService({
    runWorker: async (spec, directory) => {
      await writeFile(`${directory}/gearbox-housing.svg`, "<svg></svg>");
      return {
        status: "built",
        source_hash: spec.source_hash,
        solid_valid: true,
        artifacts: {
          svg: { filename: "gearbox-housing.svg", content_type: "image/svg+xml", bytes: 11 },
        },
      };
    },
  });

  try {
    const result = await service.build(gearboxSpec());
    assert.match(result.job_id, /^[a-f0-9-]{36}$/);
    assert.match(result.preview_svg, /^<svg/);
    assert.equal(result.cad_spec.parameters.axes.third.x, Math.sqrt((62 ** 2) - (44 ** 2)));

    await assert.rejects(
      () => service.artifact(result.job_id, "step"),
      (error) => error.code === "SCC-CAD-010" && /EngineeringModel export/.test(error.message),
    );
    await assert.rejects(() => service.artifact(result.job_id, "../../etc/passwd"), (error) => error.code === "SCC-CAD-008");
  } finally {
    await service.close();
  }
});

test("rejects a compatibility worker that tries to return a STEP artifact", async () => {
  const service = createCadService({
    runWorker: async (spec) => ({
      status: "built",
      source_hash: spec.source_hash,
      solid_valid: true,
      artifacts: { step: { filename: "forbidden.step", content_type: "model/step", bytes: 1 } },
    }),
  });

  await assert.rejects(
    () => service.build(gearboxSpec()),
    (error) => error.code === "SCC-CAD-010" && /forbidden STEP/.test(error.message),
  );
  await service.close();
});

test("refuses to invoke the CAD worker while critical intent is unresolved", async () => {
  let invoked = false;
  const service = createCadService({ runWorker: async () => { invoked = true; } });
  const spec = gearboxSpec();
  spec.unresolved.push({ severity: "critical", field: "axes.third.horizontal_side", message: "Choose a side" });

  await assert.rejects(() => service.build(spec), (error) => error.code === "SCC-CAD-004");
  assert.equal(invoked, false);
  await service.close();
});

test("rejects a generic worker manifest that silently omits coverage evidence", async () => {
  const service = createCadService({
    runWorker: async (spec) => ({
      status: "built",
      source_hash: spec.source_hash,
      solid_valid: true,
      artifacts: {},
    }),
  });

  await assert.rejects(
    () => service.build(featureGraphSpec()),
    (error) => error.code === "SCC-CAD-009" && /coverage evidence/.test(error.message),
  );
  await service.close();
});
