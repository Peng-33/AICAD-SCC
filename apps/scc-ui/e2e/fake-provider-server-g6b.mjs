import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
if (!binary) throw new Error("Build target/release/scc before G6B browser acceptance");

const fixtureCalls = [
  { name: "create_part", arguments: { id: "g6b_browser_part", name: "G6B 通用沉头与显式阵列夹具" } },
  { name: "create_parametric_plate", arguments: { id: "plate", width: 100, height: 60, thickness: 10 } },
  { name: "add_parametric_hole", arguments: {
    id: "cs_hole", support_mode: "principal", plane: "x_y", support_offset: 10,
    center_u: -30, center_v: 0, diameter: 4, reversed: true,
    termination: "through_all", hole_kind: "countersink",
    countersink_diameter: 10, countersink_angle_degrees: 90,
  } },
  { name: "add_parametric_hole", arguments: {
    id: "irregular_seed", support_mode: "principal", plane: "x_y",
    center_u: 10, center_v: 15, diameter: 4, reversed: false,
    termination: "through_all", hole_kind: "simple",
  } },
  { name: "add_feature_pattern", arguments: {
    id: "irregular_hole_pattern", seed_feature_id: "irregular_seed", pattern_kind: "explicit_placement",
    reference_plane: { kind: "principal", plane: "x_y" },
    placements: [
      { local_u_mm: 24, local_v_mm: 20 },
      { local_u_mm: 30, local_v_mm: -15 },
      { local_u_mm: 5, local_v_mm: -20 },
    ],
  } },
  { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
];
const required = new Set(fixtureCalls.map((call) => call.name));
const provider = createFakeProvider((request) => {
  if (request.mode !== "new") throw new Error("G6B fake Provider accepts only the isolated fixture creation request");
  const available = new Set((request.tools ?? []).map((tool) => tool.name));
  const missing = [...required].filter((name) => !available.has(name));
  if (missing.length) throw new Error(`Engineering Skill allowlist omitted required G6B tools: ${missing.join(", ")}`);
  return { kind: "tool_calls", toolCalls: fixtureCalls };
});

const server = createApp({ adapter: new SccCliAdapter(binary), engineeringProvider: provider });
const host = process.env.SCC_UI_HOST ?? "0.0.0.0";
const port = Number.parseInt(process.env.SCC_UI_PORT ?? "4173", 10);
server.listen(port, host, () => process.stdout.write(`G6B isolated fake-Provider Workbench listening on http://${host}:${port}\n`));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.close(() => process.exit(0)));
