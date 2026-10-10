import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
if (!binary) throw new Error("Build target/release/scc before the browser acceptance run");

const profileEdge = (index) => ({
  source_feature_id: "boss",
  selector: { kind: "profile_boundary", entity_id: `boss-edge-${String(index).padStart(4, "0")}`, side: "top" },
  qualifier: { curve_kind: "line", radius_mm: null, length_mm: null, midpoint: null, tolerance_mm: 0.000001 },
});
const toolCalls = [
  { name: "create_part", arguments: { id: "g3_part", name: "G3 通用安装板" } },
  { name: "create_parametric_plate", arguments: { id: "plate", width: 120, height: 90, thickness: 20 } },
  { name: "add_parametric_rectangular_add", arguments: {
    id: "boss", plane: "x_y", support_offset: 20, center_u: 23, center_v: 13,
    width: 30, height: 22, depth: 15,
  } },
  { name: "add_parametric_hole", arguments: {
    id: "through_hole", support_mode: "principal", plane: "x_y", support_offset: 20,
    center_u: -30, center_v: -12, diameter: 16, reversed: true,
    termination: "through_all", hole_kind: "simple",
  } },
  { name: "add_parametric_rectangular_cut", arguments: {
    id: "pocket", plane: "x_y", support_offset: 20, center_u: -32, center_v: 26,
    width: 40, height: 24, reversed: true, termination: "blind", depth: 6,
  } },
  { name: "create_fillet", arguments: { id: "boss_fillet", radius: 3, edge_refs: [0, 1, 2, 3].map(profileEdge) } },
  { name: "create_chamfer", arguments: {
    id: "hole_chamfer", distance: 0.8,
    edge_refs: [{
      source_feature_id: "through_hole", selector: { kind: "hole_entrance" },
      qualifier: { curve_kind: "circular", radius_mm: 8, length_mm: null, midpoint: null, tolerance_mm: 0.000001 },
    }],
  } },
  { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
];
const requiredTools = new Set([
  "create_part", "create_parametric_plate", "add_parametric_rectangular_add",
  "add_parametric_hole", "add_parametric_rectangular_cut", "create_fillet", "create_chamfer",
  "set_drawing_property",
]);
const provider = createFakeProvider((request) => {
  if (request.mode !== "new" || !(request.tools ?? []).some((tool) => tool.name === "create_parametric_plate")) {
    throw new Error("G3 fake provider received an unexpected Engineering Intent request");
  }
  const available = new Set(request.tools.map((tool) => tool.name));
  const missing = [...requiredTools].filter((name) => !available.has(name));
  if (missing.length) throw new Error(`Engineering Skill did not allow required G3 tools: ${missing.join(", ")}`);
  return { kind: "tool_calls", toolCalls };
});
const server = createApp({ adapter: new SccCliAdapter(binary), engineeringProvider: provider });
const host = process.env.SCC_UI_HOST ?? "127.0.0.1";
const port = Number.parseInt(process.env.SCC_UI_PORT ?? "4178", 10);
server.listen(port, host, () => process.stdout.write(`CAD Skills browser acceptance server listening on http://${host}:${port}\\n`));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.close(() => process.exit(0)));
