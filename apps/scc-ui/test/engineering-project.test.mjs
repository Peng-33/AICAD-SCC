import test from "node:test";
import assert from "node:assert/strict";

import { sourceProject } from "../public/js/engineering-project.js";

test("project helper keeps legacy EngineeringModel in v1 and parametric history in v2", () => {
  assert.equal(sourceProject({ features: {} }, "Legacy").schema_version, 1);
  assert.equal(sourceProject({ features: { base: { type: "base_profile" } } }, "Legacy").schema_version, 1);
  assert.equal(sourceProject({ features: { sketch: { type: "parametric" } } }, "Generic").schema_version, 2);
  assert.equal(sourceProject({ feature_order: ["base"], features: {} }, "Generic").schema_version, 2);
});
