import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import {
  buildFeatureTree,
  buildProperties,
  buildRelations,
  buildConstraints,
  buildReview,
  createModelHistory,
} from "../public/js/engineering-workbench.js";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
const adapter = binary ? new SccCliAdapter(binary) : null;

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

const UNRESOLVED_DSL = `AICAD 1
PART p
PARAM edge_offset = UNRESOLVED
BASE_RECT main
  WIDTH 120
  HEIGHT 80
HOLE_PATTERN mounting CORNER
  BASE main
  OFFSET_X 15
  OFFSET_Y 15
  DIA 10
BIND mounting OFFSET_X <- edge_offset
BIND mounting OFFSET_Y <- edge_offset
`;

test("workbench maps features, properties, relations, review", { skip: !binary }, async () => {
  const snapshot = await adapter.engineeringDslSolve(FIXTURE_DSL);
  const view = snapshot.workbench;

  // Feature tree: one base profile + one pattern (pattern is a single feature).
  const tree = buildFeatureTree(view);
  assert.equal(tree.children.length, 2);
  assert.deepEqual(
    tree.children.map((f) => [f.id, f.type]),
    [["main", "base_profile"], ["mounting", "hole_pattern:corner"]],
  );

  // Properties: main.Width is a bound, editable parameter.
  const main = view.features.find((feature) => feature.id === "main");
  const props = buildProperties(main);
  const width = props.find((property) => property.slot === "width");
  assert.equal(width.parameter, "plate_width");
  assert.equal(width.source, "parameter:plate_width");
  assert.equal(width.value, "120");
  assert.ok(width.editable);

  // Relations: hole_diameter is derived with its formula and dependencies.
  const relations = buildRelations(view);
  const diameter = relations.find((relation) => relation.id === "hole_diameter");
  assert.equal(diameter.expression, "(plate_height / 8)");
  assert.deepEqual(diameter.dependencies, ["plate_height"]);
  assert.equal(diameter.editable, false, "derived parameters are read-only");

  // Review counts: 3 user (plate_width, plate_height, edge_offset) + 1 derived.
  const review = buildReview(view);
  assert.equal(review.summary.confirmed, 3);
  assert.equal(review.summary.derived, 1);
  assert.equal(review.summary.unresolved, 0);
  assert.deepEqual(buildConstraints(view), []);

  // SourceMap inverse is Rust-derived, not parsed from id strings.
  assert.equal(snapshot.entity_to_feature.main, "main");
  assert.ok(Object.keys(snapshot.entity_to_feature).some((id) => id.startsWith("mounting_array[")));
});

test("v2 Hole properties preserve typed parameter references and units", () => {
  const properties = buildProperties({
    id: "center_hole",
    feature_type: "hole",
    slots: [{
      slot: "blind_depth",
      label: "blind_depth",
      display_value: "6",
      source: "literal",
      parameter: null,
      editable: true,
      unit: "mm",
      parameter_reference: {
        part_id: "mounting_plate",
        feature_id: "center_hole",
        slot: "blind_depth",
        parameter_id: "fp_0123456789abcdef",
        expected_type: "length_mm",
      },
    }],
  });
  assert.equal(properties[0].unit, "mm");
  assert.deepEqual(properties[0].parameterReference, {
    part_id: "mounting_plate",
    feature_id: "center_hole",
    slot: "blind_depth",
    parameter_id: "fp_0123456789abcdef",
    expected_type: "length_mm",
  });
  assert.equal(properties[0].editable, true);
});

test("UNRESOLVED is not shown as zero and is reported", { skip: !binary }, async () => {
  const model = await adapter.engineeringDslApply(UNRESOLVED_DSL);
  const view = await adapter.engineeringWorkbench(JSON.stringify(model));

  const offset = view.parameters.find((parameter) => parameter.id === "edge_offset");
  assert.equal(offset.value, "UNRESOLVED");
  assert.notEqual(offset.value, "0");

  const mounting = view.features.find((feature) => feature.id === "mounting");
  const offsetX = mounting.slots.find((slot) => slot.slot === "offset_x");
  assert.equal(offsetX.display_value, "UNRESOLVED");

  const review = buildReview(view);
  assert.equal(review.summary.unresolved, 1);
  assert.ok(review.unresolved.some((item) => item.id === "edge_offset"));
  assert.ok(
    review.unresolved.find((item) => item.id === "edge_offset").usedBy.some((usage) => usage.includes("mounting.offset_x")),
  );
});

test("dependency propagation is visible without front-end math", { skip: !binary }, async () => {
  const snapshot = await adapter.engineeringDslSolve(FIXTURE_DSL);
  const diameter = snapshot.workbench.parameters.find((parameter) => parameter.id === "hole_diameter");
  assert.equal(diameter.value, "10");
  assert.deepEqual(diameter.dependencies, ["plate_height"]);
  // The front end shows the formula; it never computes 80/8 itself.
  assert.equal(diameter.expression, "(plate_height / 8)");
});

// ---------- createModelHistory (pure; no binary needed) ----------
//
// Stack semantics: undoStack = "all states except the current one" + a null
// baseline sentinel (loadSnapshot pushes null before the first mutation). The
// current state is only ever passed explicitly to undo()/redo() as the state
// being left.

test("history: full walk push/push → undo → undo → redo → redo → redo(empty) → undo", () => {
  const history = createModelHistory();
  history.push(null);                 // loadSnapshot baseline
  history.push({ tag: "s0" });        // saved before editing s0 → s1
  history.push({ tag: "s1" });        // saved before editing s1 → s2
  let current = { tag: "s2" };

  // undo from s2: leaves s2, returns s1.
  assert.equal(history.undo(current).tag, "s1");
  assert.equal(history.canRedo(), true);
  // undo again: leaves s1, returns s0.
  assert.equal(history.undo({ tag: "s1" }).tag, "s0");
  // undo at the baseline: pops the null sentinel, restores it, stays a no-op.
  assert.equal(history.undo({ tag: "s0" }), null);
  assert.equal(history.canUndo(), true, "null baseline stays on the stack");
  assert.equal(history.undo({ tag: "s0" }), null);

  // redo from the baseline: target s1, leaving-state s0 goes back on undoStack.
  assert.equal(history.redo({ tag: "s0" }).tag, "s1");
  assert.equal(history.redo({ tag: "s1" }).tag, "s2");
  assert.equal(history.canRedo(), false);
  // redo with an empty redoStack is a no-op.
  assert.equal(history.redo({ tag: "s2" }), null);
  // and undo still works afterwards.
  assert.equal(history.undo({ tag: "s2" }).tag, "s1");
});

test("history: undo at the empty stack is a no-op without corrupting state", () => {
  const history = createModelHistory();
  assert.equal(history.undo({ tag: "x" }), null);
  assert.equal(history.canUndo(), false);
  assert.equal(history.canRedo(), false);
});

test("history: pushing a new state clears the redo branch (loadSnapshot semantics)", () => {
  const history = createModelHistory();
  history.push(null);
  history.push({ tag: "s0" });
  assert.equal(history.undo({ tag: "s0" }).tag, "s0");
  assert.equal(history.canRedo(), true);
  history.push({ tag: "s2" }); // new edit after an undo
  assert.equal(history.canRedo(), false, "redo branch must be dropped");
  assert.equal(history.redo({ tag: "s2" }), null);
});

test("history: MAX_HISTORY truncation keeps the newest states and does not crash", () => {
  const history = createModelHistory();
  history.push(null);
  for (let i = 0; i < 60; i++) history.push({ i });
  // Walk all the way down: exactly 50 real states above the baseline.
  let steps = 0;
  let current = { i: 59 };
  while (steps < 60) {
    const previous = history.undo(current);
    if (!previous) break;
    current = previous;
    steps++;
  }
  assert.ok(steps <= 50, "stack must be truncated to MAX_HISTORY entries");
  assert.equal(current?.i, 10, "oldest surviving state after truncation of baseline + 60 pushes");
  // The structure remains usable afterwards.
  assert.equal(history.redo(current).i, current.i + 1);
});


test("Workbench displays ordered Loft Sketch references and mode as read-only properties", () => {
  const properties = buildProperties({
    id: "loft_body", feature_type: "loft", slots: [
      { slot: "section_sketch_refs", label: "section_sketch_refs", display_value: "section_a, section_b, section_c", source: "literal", editable: false },
      { slot: "operation", label: "operation", display_value: "add", source: "literal", editable: false },
      { slot: "loft_mode", label: "loft_mode", display_value: "ruled", source: "literal", editable: false },
      { slot: "orientation_policy", label: "orientation_policy", display_value: "aligned", source: "literal", editable: false },
    ],
  });
  assert.deepEqual(properties.map(({ label, value, editable }) => ({ label, value, editable })), [
    { label: "Section sketch refs", value: "section_a, section_b, section_c", editable: false },
    { label: "Operation", value: "add", editable: false },
    { label: "Loft mode", value: "ruled", editable: false },
    { label: "Orientation policy", value: "aligned", editable: false },
  ]);
});

test("Workbench displays Sweep dependencies and orientation as read-only properties", () => {
  const properties = buildProperties({
    id: "pipe_sweep", feature_type: "sweep", slots: [
      { slot: "profile", label: "profile", display_value: "pipe_profile", source: "literal", editable: false },
      { slot: "path", label: "path", display_value: "bend_path", source: "literal", editable: false },
      { slot: "operation", label: "operation", display_value: "add", source: "literal", editable: false },
      { slot: "orientation", label: "orientation", display_value: "fixedbinormal", source: "literal", editable: false },
    ],
  });
  assert.deepEqual(properties.map(({ slot, value, editable }) => ({ slot, value, editable })), [
    { slot: "profile", value: "pipe_profile", editable: false },
    { slot: "path", value: "bend_path", editable: false },
    { slot: "operation", value: "add", editable: false },
    { slot: "orientation", value: "fixedbinormal", editable: false },
  ]);
});
