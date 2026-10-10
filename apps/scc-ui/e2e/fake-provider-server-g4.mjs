import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
if (!binary) throw new Error("Build target/release/scc before the G4 browser acceptance run");

const line = (id, start, end) => ({
  id,
  geometry: { kind: "line", start: { x: start[0], y: start[1] }, end: { x: end[0], y: end[1] } },
});
const profile = {
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
const toolCalls = [
  { name: "create_part", arguments: { id: "g4_part", name: "G4 通用阶梯轴" } },
  { name: "create_sketch", arguments: { id: "shaft_profile", plane: "xy", profile } },
  { name: "create_datum_axis", arguments: {
    id: "shaft_axis", definition: {
      kind: "through_two_points", first: { x: 0, y: 0, z: 0 }, second: { x: 80, y: 0, z: 0 },
    },
  } },
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
const requiredTools = new Set([
  "create_part", "create_sketch", "create_datum_axis", "create_revolve",
  "add_parametric_hole", "add_feature_pattern", "set_drawing_property",
]);
const provider = createFakeProvider((request) => {
  if (request.mode !== "new" || !(request.tools ?? []).some((tool) => tool.name === "create_revolve")) {
    throw new Error("G4 fake provider received an unexpected Engineering Intent request");
  }
  const available = new Set(request.tools.map((tool) => tool.name));
  const missing = [...requiredTools].filter((name) => !available.has(name));
  if (missing.length) throw new Error(`Engineering Skill did not allow required G4 tools: ${missing.join(", ")}`);
  return { kind: "tool_calls", toolCalls };
});
const server = createApp({ adapter: new SccCliAdapter(binary), engineeringProvider: provider });
const host = process.env.SCC_UI_HOST ?? "127.0.0.1";
const port = Number.parseInt(process.env.SCC_UI_PORT ?? "4173", 10);
server.listen(port, host, () => process.stdout.write(`G4 Engineering Workbench browser acceptance server listening on http://${host}:${port}\\n`));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.close(() => process.exit(0)));
