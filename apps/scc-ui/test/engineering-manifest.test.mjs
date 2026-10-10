import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { resolveSccBinary, SccCliAdapter } from "../lib/scc-cli.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
const binary = await resolveSccBinary(repoRoot);
const adapter = binary ? new SccCliAdapter(binary) : null;

// The canonical Phase 5 tool surface, in sorted (registry) order. This list is
// verified against the live Rust `scc-tools` registry every test run, so it
// cannot silently drift (ADR-020 Decision 10).
const EXPECTED_TOOLS = [
  "add_circular_hole_pattern",
  "add_constraint",
  "add_construction_circle",
  "add_corner_hole_pattern",
  "add_feature_pattern",
  "add_grid_hole_pattern",
  "add_hole",
  "add_parametric_hole",
  "add_parametric_rectangular_add",
  "add_parametric_rectangular_cut",
  "add_slot",
  "bind_parameter",
  "create_base_circle",
  "create_base_polygon",
  "create_base_rectangle",
  "create_base_rounded_rectangle",
  "create_chamfer",
  "create_datum_axis",
  "create_datum_plane",
  "create_fillet",
  "create_loft",
  "create_parametric_plate",
  "create_part",
  "create_path",
  "create_revolve",
  "create_sketch",
  "create_sweep",
  "declare_parameter",
  "move_feature",
  "remove_constraint",
  "remove_feature",
  "set_drawing_property",
  "set_expression",
  "set_feature_parameter",
  "set_hole_extent",
  "set_parameter",
  "set_part_thickness",
];

test("tool manifest equals the Rust tool registry (no drift)", { skip: !binary }, async () => {
  const manifest = await adapter.engineeringManifest();
  const names = manifest.map((tool) => tool.name);
  assert.deepEqual(names, EXPECTED_TOOLS, "manifest tool names must match the Rust registry");
});

test("unsupported capabilities are not exposed as tools", { skip: !binary }, async () => {
  const manifest = await adapter.engineeringManifest();
  const names = manifest.map((tool) => tool.name);
  for (const forbidden of ["add_fillet", "add_concentric_constraint"]) {
    assert.ok(!names.includes(forbidden), `${forbidden} must not be a tool`);
  }
});

test("manifest order is stable", { skip: !binary }, async () => {
  const first = await adapter.engineeringManifest();
  for (let i = 0; i < 5; i += 1) {
    const again = await adapter.engineeringManifest();
    assert.deepEqual(
      again.map((t) => t.name),
      first.map((t) => t.name),
    );
    assert.deepEqual(again, first, "manifest serialization must be deterministic");
  }
});
