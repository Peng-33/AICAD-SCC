import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
if (!binary) throw new Error("Build target/release/scc before G7A browser acceptance");

const fixtureCalls = [
  { name: "create_part", arguments: { id: "g7a_part", name: "G7A 通用弯曲扫掠夹具" } },
  { name: "create_parametric_plate", arguments: { id: "base", width: 100, height: 60, thickness: 10 } },
  { name: "create_sketch", arguments: {
    id: "sweep_profile", plane: "yz",
    profile: {
      outer: { entities: [{ id: "sweep_circle", geometry: { kind: "circle", center: { x: 0, y: 0 }, radius: 2 } }] },
      inner_loops: [],
    },
  } },
  { name: "create_path", arguments: {
    id: "curve_path", support: { kind: "principal", plane: "x_y" },
    segments: [{
      id: "curve_path_arc",
      geometry: {
        kind: "arc", start: { x: 0, y: 0 }, end: { x: 40, y: 40 },
        center: { x: 0, y: 40 }, radius: 40, direction: "counter_clockwise",
      },
    }],
  } },
  { name: "create_sweep", arguments: {
    id: "curved_sweep", profile_id: "sweep_profile", path_id: "curve_path",
    operation: "add", direction: "forward", profile_origin: { x: 0, y: 0 },
    orientation: "fixed_binormal",
  } },
  { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
];
const required = new Set(fixtureCalls.map((call) => call.name));
const provider = createFakeProvider((request) => {
  if (request.mode !== "new") throw new Error("G7A fake Provider accepts only the isolated new-part fixture request");
  const available = new Set((request.tools ?? []).map((tool) => tool.name));
  const missing = [...required].filter((name) => !available.has(name));
  if (missing.length) throw new Error(`Engineering Skill allowlist omitted G7A tools: ${missing.join(", ")}`);
  return { kind: "tool_calls", toolCalls: fixtureCalls };
});

const server = createApp({ adapter: new SccCliAdapter(binary), engineeringProvider: provider });
const host = process.env.SCC_UI_HOST ?? "0.0.0.0";
const port = Number.parseInt(process.env.SCC_UI_PORT ?? "4173", 10);
server.listen(port, host, () => process.stdout.write(`G7A isolated fake-Provider Workbench listening on http://${host}:${port}\n`));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.close(() => process.exit(0)));
