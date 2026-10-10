import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
if (!binary) throw new Error("Build target/release/scc before the G5 browser acceptance run");

const profileEdge = (index) => ({
  source_feature_id: "boss",
  selector: { kind: "profile_boundary", entity_id: `boss-edge-${String(index).padStart(4, "0")}`, side: "top" },
  qualifier: { curve_kind: "line", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.000001 },
});
const machiningCalls = [
  { name: "create_part", arguments: { id: "g5_part", name: "G5 通用加工安装板" } },
  { name: "create_parametric_plate", arguments: { id: "plate", width: 120, height: 90, thickness: 20 } },
  { name: "add_parametric_rectangular_add", arguments: {
    id: "boss", plane: "x_y", support_offset: 20, center_u: 23, center_v: 13,
    width: 30, height: 22, depth: 15,
  } },
  { name: "add_parametric_hole", arguments: {
    id: "seed_hole", support_mode: "principal", plane: "x_y", support_offset: 20,
    center_u: -30, center_v: -12, diameter: 10, reversed: true,
    termination: "through_all", hole_kind: "simple",
  } },
  { name: "add_feature_pattern", arguments: {
    id: "hole_pattern", seed_feature_id: "seed_hole", pattern_kind: "circular", count: 4,
    axis_x: 0, axis_y: 0, axis_z: 1, center_x: -40, center_y: -12, center_z: 20,
    radius: 10, start_angle_degrees: 0, angular_span_degrees: 360,
  } },
  { name: "add_parametric_rectangular_cut", arguments: {
    id: "pocket", plane: "x_y", support_offset: 20, center_u: -32, center_v: 26,
    width: 40, height: 24, reversed: true, termination: "blind", depth: 6,
  } },
  { name: "create_fillet", arguments: { id: "boss_fillet", radius: 3, edge_refs: [0, 1, 2, 3].map(profileEdge) } },
  { name: "create_chamfer", arguments: {
    id: "hole_chamfer", distance: 0.8,
    edge_refs: [{
      source_feature_id: "seed_hole", selector: { kind: "hole_entrance" },
      qualifier: { curve_kind: "circular", radius_mm: 5, length_mm: null, midpoint: null, tolerance_mm: 0.000001 },
    }],
  } },
  { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
];

const line = (id, start, end) => ({
  id,
  geometry: { kind: "line", start: { x: start[0], y: start[1] }, end: { x: end[0], y: end[1] } },
});
const revolveProfile = {
  outer: { entities: [
    line("shaft-section-0000", [0, 0], [0, 20]),
    line("shaft-section-0001", [0, 20], [20, 20]),
    line("shaft-section-0002", [20, 20], [20, 15]),
    line("shaft-section-0003", [20, 15], [60, 15]),
    line("shaft-section-0004", [60, 15], [60, 10]),
    line("shaft-section-0005", [60, 10], [80, 10]),
    line("shaft-section-0006", [80, 10], [80, 0]),
    line("shaft-section-0007", [80, 0], [0, 0]),
  ] },
  inner_loops: [],
};
const revolveCalls = [
  { name: "create_part", arguments: { id: "g5_revolve_part", name: "G5 通用阶梯轴" } },
  { name: "create_sketch", arguments: { id: "shaft_profile", plane: "xy", profile: revolveProfile } },
  { name: "create_datum_axis", arguments: { id: "shaft_axis", definition: {
    kind: "through_two_points", first: { x: 0, y: 0, z: 0 }, second: { x: 80, y: 0, z: 0 },
  } } },
  { name: "create_revolve", arguments: {
    id: "shaft_revolve", profile_id: "shaft_profile", axis_feature_id: "shaft_axis",
    operation: "add", angle_mode: "full_360", orientation: "forward",
  } },
  { name: "add_parametric_hole", arguments: {
    id: "flange_hole_seed", support_mode: "principal", plane: "y_z",
    center_u: 12, center_v: 0, diameter: 3, reversed: false,
    termination: "through_all", hole_kind: "simple",
  } },
  { name: "add_feature_pattern", arguments: {
    id: "flange_hole_pattern", seed_feature_id: "flange_hole_seed",
    pattern_kind: "circular", count: 4,
    axis_x: 1, axis_y: 0, axis_z: 0,
    center_x: 0, center_y: 0, center_z: 0,
    radius: 12, start_angle_degrees: 0, angular_span_degrees: 360,
  } },
  { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
];

const requiredMachiningTools = new Set([
  "create_part", "create_parametric_plate", "add_parametric_rectangular_add", "add_parametric_hole",
  "add_feature_pattern", "add_parametric_rectangular_cut", "create_fillet", "create_chamfer", "set_drawing_property",
]);
const requiredRevolveTools = new Set([
  "create_part", "create_sketch", "create_datum_axis", "create_revolve", "add_parametric_hole",
  "add_feature_pattern", "set_drawing_property",
]);
const provider = createFakeProvider((request) => {
  if (request.mode !== "new") throw new Error("G5 browser fixture only accepts new EngineeringModel requests");
  const isRevolve = request.userText.includes("阶梯轴");
  const calls = isRevolve ? revolveCalls : machiningCalls;
  const required = isRevolve ? requiredRevolveTools : requiredMachiningTools;
  const available = new Set((request.tools ?? []).map((tool) => tool.name));
  const missing = [...required].filter((name) => !available.has(name));
  if (missing.length) throw new Error(`Engineering Skill did not allow required G5 tools: ${missing.join(", ")}`);
  return { kind: "tool_calls", toolCalls: calls };
});

const server = createApp({ adapter: new SccCliAdapter(binary), engineeringProvider: provider });
const host = process.env.SCC_UI_HOST ?? "0.0.0.0";
const port = Number.parseInt(process.env.SCC_UI_PORT ?? "4173", 10);
server.listen(port, host, () => process.stdout.write(`G5 Engineering Workbench browser acceptance server listening on http://${host}:${port}\n`));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.close(() => process.exit(0)));
