import assert from "node:assert/strict";
import test from "node:test";
import { buildChangePreview } from "../lib/change-preview.mjs";

const before = {
  schema_version: 1,
  parameters: { width: { value: 120 }, hole_diameter: { value: { type: "expression", expression: "width / 12" } } },
  features: { main: { type: "base_profile", width: 120 }, hole: { type: "hole", diameter: 10 } },
};
const after = {
  schema_version: 1,
  parameters: { width: { value: 160 }, hole_diameter: { value: { type: "expression", expression: "width / 12" } } },
  features: { main: { type: "base_profile", width: 160 }, hole: { type: "hole", diameter: 10 }, boss: { type: "base_profile", width: 20 } },
};

const beforeSnapshot = { workbench: { parameters: [
  { id: "width", expression: null, value: 120, origin: "direct" },
  { id: "hole_diameter", expression: "width / 12", value: 10, origin: "derived" },
] } };
const afterSnapshot = { workbench: { parameters: [
  { id: "width", expression: null, value: 160, origin: "direct" },
  { id: "hole_diameter", expression: "width / 12", value: 13.333333, origin: "derived" },
] } };

test("change preview is structured, source-aware, and does not mutate the base model", () => {
  const baseText = JSON.stringify(before);
  const preview = buildChangePreview({
    proposalId: "proposal-1",
    baseModel: before,
    candidateModel: after,
    baseSnapshot: beforeSnapshot,
    candidateSnapshot: afterSnapshot,
    baseFingerprint: "base",
    candidateFingerprint: "candidate",
    commands: [{ name: "set_parameter", arguments: { target: "width", value: 160 } }],
    intentSummary: "Change the plate width",
  });

  assert.equal(JSON.stringify(before), baseText);
  assert.equal(preview.schema_version, 1);
  assert.equal(preview.proposal_id, "proposal-1");
  assert.equal(preview.parameter_changes.length, 2);
  assert.equal(preview.parameter_changes.find((change) => change.id === "width").change_kind, "direct");
  assert.equal(preview.parameter_changes.find((change) => change.id === "hole_diameter").change_kind, "derived");
  assert.equal(preview.parameter_changes.find((change) => change.id === "hole_diameter").after.source_expression, "width / 12");
  assert.deepEqual(preview.feature_changes.map((change) => [change.id, change.kind]), [["boss", "added"], ["hole", "unchanged"], ["main", "modified"]]);
  assert.equal(preview.downstream_readiness.principal, "ready");
  assert.equal(preview.downstream_readiness.three_view, "not_evaluated");
  assert.equal(preview.plan.commands[0].name, "set_parameter");
});

test("change preview reports solve diagnostics without inventing downstream readiness", () => {
  const preview = buildChangePreview({
    proposalId: "proposal-2", baseModel: {}, candidateModel: {},
    baseFingerprint: "a", candidateFingerprint: "b", commands: [],
    candidateSnapshot: { solve_error: { code: "SCC-ENG-001", message: "unresolved" } },
  });
  assert.deepEqual(preview.diagnostics, [{ code: "SCC-ENG-001", message: "unresolved" }]);
  assert.equal(preview.downstream_readiness.principal, "not_ready");
  assert.equal(preview.downstream_readiness.step, "not_ready");
});
