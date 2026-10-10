import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
if (!binary) throw new Error("Build target/release/scc before G7B browser acceptance");

const profile = (id, center, radius) => ({
  outer: { entities: [{ id: `${id}_circle`, geometry: { kind: "circle", center: { x: center[0], y: center[1] }, radius } }] },
  inner_loops: [],
});
const fixtureCalls = [
  { name: "create_part", arguments: { id: "g7b_part", name: "G7B 三截面放样夹具" } },
  { name: "create_sketch", arguments: { id: "section_a", plane: "xy", profile: profile("section_a", [0, 0], 10) } },
  { name: "create_datum_plane", arguments: {
    id: "section_plane_b", base: { kind: "principal", plane: "x_y" }, distance: 30,
  } },
  { name: "create_sketch", arguments: {
    id: "section_b", support: { kind: "datum", feature: "section_plane_b" }, profile: profile("section_b", [5, 0], 20),
  } },
  { name: "create_datum_plane", arguments: {
    id: "section_plane_c", base: { kind: "principal", plane: "x_y" }, distance: 70,
  } },
  { name: "create_sketch", arguments: {
    id: "section_c", support: { kind: "datum", feature: "section_plane_c" }, profile: profile("section_c", [-3, 2], 13),
  } },
  { name: "create_loft", arguments: {
    id: "loft_body", section_ids: ["section_a", "section_b", "section_c"],
    operation: "add", mode: "ruled", orientation_policy: "aligned",
  } },
  { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
];
const required = new Set(fixtureCalls.map((call) => call.name));
const provider = createFakeProvider((request) => {
  if (request.mode !== "new") throw new Error("G7B fake Provider accepts only the isolated three-section fixture");
  const available = new Set((request.tools ?? []).map((tool) => tool.name));
  const missing = [...required].filter((name) => !available.has(name));
  if (missing.length) throw new Error(`Engineering Skill allowlist omitted G7B tools: ${missing.join(", ")}`);
  return { kind: "tool_calls", toolCalls: fixtureCalls };
});

const server = createApp({ adapter: new SccCliAdapter(binary), engineeringProvider: provider });
const host = process.env.SCC_UI_HOST ?? "0.0.0.0";
const port = Number.parseInt(process.env.SCC_UI_PORT ?? "4173", 10);
server.listen(port, host, () => process.stdout.write(`G7B isolated fake-Provider Workbench listening on http://${host}:${port}\n`));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.close(() => process.exit(0)));
