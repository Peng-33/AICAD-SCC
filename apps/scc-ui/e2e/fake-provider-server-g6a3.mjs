import { createApp } from "../server.mjs";
import { createFakeProvider } from "../lib/provider-fake.mjs";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
if (!binary) throw new Error("Build target/release/scc before the G6A.3 browser acceptance run");

const fixtureCalls = [
  { name: "create_part", arguments: { id: "g6a3_browser_part", name: "G6A.3 参数编辑夹具" } },
  { name: "create_parametric_plate", arguments: { id: "plate", width: 120, height: 90, thickness: 20 } },
  { name: "add_parametric_hole", arguments: {
    id: "center_hole", support_mode: "principal", plane: "x_y", support_offset: 20,
    center_u: 28, center_v: 18, diameter: 10, reversed: true,
    termination: "blind", depth: 6, hole_kind: "simple",
  } },
  { name: "set_drawing_property", arguments: { property: "projection_method", value: "third_angle" } },
];
const required = new Set(fixtureCalls.map((call) => call.name));
const provider = createFakeProvider((request) => {
  if (request.mode !== "new") throw new Error("G6A.3 browser fixture only accepts a new EngineeringModel request");
  const available = new Set((request.tools ?? []).map((tool) => tool.name));
  const missing = [...required].filter((name) => !available.has(name));
  if (missing.length) throw new Error(`Engineering Skill did not allow required G6A.3 tools: ${missing.join(", ")}`);
  return { kind: "tool_calls", toolCalls: fixtureCalls };
});

const server = createApp({ adapter: new SccCliAdapter(binary), engineeringProvider: provider });
const host = process.env.SCC_UI_HOST ?? "0.0.0.0";
const port = Number.parseInt(process.env.SCC_UI_PORT ?? "4173", 10);
server.listen(port, host, () => process.stdout.write(`G6A.3 Engineering Workbench browser acceptance server listening on http://${host}:${port}\n`));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.close(() => process.exit(0)));
