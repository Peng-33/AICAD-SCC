import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { loadExample } from "../lib/examples.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import { preflightIntentResult } from "../server.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);

test("polygon example completes the real SCC pipeline", { skip: !binary }, async () => {
  const adapter = new SccCliAdapter(binary);
  const example = await loadExample(repoRoot, "polygon");
  const compiled = await adapter.compile(example.proposal);
  assert.equal(compiled.outcome, "compiled");
  const executed = await adapter.execute(compiled.ir, example.mutation);
  const geometry = await adapter.solve(executed.state);
  assert.equal(geometry.solve_report.status, "fully_constrained");
  assert.equal(geometry.entities.profile.primitive.kind, "polygon");
  assert.match(await adapter.export(geometry, "svg"), /^<svg/);
  assert.match(await adapter.export(geometry, "dxf"), /SECTION/);
});

test("intent preflight rejects an On constraint unsupported by M3", { skip: !binary }, async () => {
  const adapter = new SccCliAdapter(binary);
  const proposal = {
    version: "1.0",
    entities: [{ id: "plate", kind: "rectangle" }, { id: "hole", kind: "circle" }],
    relations: [{ kind: "inside", subject: "hole", reference: "plate" }],
    dimensions: [
      { target: "plate", kind: "width", value: 100 },
      { target: "plate", kind: "height", value: 60 },
      { target: "hole", kind: "diameter", value: 20 },
    ],
    arrays: [],
    anchors: [
      { id: "plate_center", kind: "fixed_center", entity: "plate" },
      { id: "hole_center", kind: "fixed_center", entity: "hole" },
    ],
    metadata: {},
  };

  await assert.rejects(
    () => preflightIntentResult(adapter, proposal, []),
    // Phase 6: the CLI now surfaces the real solver stage code instead of the
    // generic SCC-UI-CLI-002 wrapper.
    (error) => error.code === "SCC-GEOM-005"
      && JSON.stringify(error.details).includes("constraint kind On is not supported"),
  );
});

test("six-hole flange template solves the supported 2D top view", { skip: !binary }, async () => {
  const adapter = new SccCliAdapter(binary);
  const example = await loadExample(repoRoot, "six-hole-flange");
  const compiled = await adapter.compile(example.proposal);
  assert.equal(compiled.outcome, "compiled");
  const executed = await adapter.execute(compiled.ir, example.mutation);
  const geometry = await adapter.solve(executed.state);
  assert.equal(geometry.solve_report.status, "fully_constrained");
  assert.equal(Object.keys(geometry.entities).length, 9);
  assert.deepEqual(
    Object.keys(geometry.entities).filter((id) => id.startsWith("bolt_pattern[")).sort(),
    ["bolt_pattern[0]", "bolt_pattern[1]", "bolt_pattern[2]", "bolt_pattern[3]", "bolt_pattern[4]", "bolt_pattern[5]"],
  );
  assert.equal(example.proposal.metadata.representation, "2D top view");
  assert.ok(example.proposal.metadata.unmodeled_requirements.length > 0);
});
