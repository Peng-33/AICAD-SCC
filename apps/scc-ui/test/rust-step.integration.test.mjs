import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

const binary = process.env.SCC_BIN ?? "/usr/local/bin/scc";

function run(args, options = {}) {
  const result = spawnSync(binary, args, { ...options, encoding: "buffer", maxBuffer: 32 * 1024 * 1024 });
  assert.equal(result.error, undefined, result.error?.message);
  assert.equal(result.status, 0, result.stderr.toString("utf8"));
  return result;
}

async function stopChild(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  let timeout;
  const exited = new Promise((resolve) => child.once("exit", resolve));
  child.kill("SIGTERM");
  try {
    await Promise.race([
      exited,
      new Promise((_, reject) => {
        timeout = setTimeout(() => {
          child.kill("SIGKILL");
          reject(new Error("local integration server did not exit after SIGTERM"));
        }, 5000);
      }),
    ]);
  } finally {
    clearTimeout(timeout);
  }
}

function captureOutput(child) {
  let output = "";
  for (const stream of [child.stdout, child.stderr]) {
    stream?.on("data", (chunk) => {
      output = `${output}${chunk.toString("utf8")}`.slice(-8192);
    });
  }
  return () => output;
}

test("Rust CLI delivers the exact CadQuery/reimport-validated STEP bytes", { skip: process.env.AICAD_RUST_STEP_INTEGRATION !== "1" || !existsSync(binary) }, async () => {
  const root = mkdtempSync(join(tmpdir(), "aicad-rust-step-"));
  try {
    const dsl = join(root, "fixture.dsl");
    const model = join(root, "model.json");
    writeFileSync(dsl, "AICAD 1\nPART p\nBASE_RECT main\n  WIDTH 120\n  HEIGHT 80\n", "utf8");
    const generated = run(["--json-errors", "engineering", "dsl", dsl]);
    const source = JSON.parse(generated.stdout.toString("utf8"));
    source.spatial = { thickness: { source: "literal", value: 10 }, hole_extents: {} };
    writeFileSync(model, `${JSON.stringify(source)}\n`, "utf8");
    const delivered = run(["--json-errors", "--quiet", "engineering", "export-step", model]);
    const bytes = delivered.stdout;
    const performance = /AICAD-STEP-PERF lowering_ms=([\d.]+) build_ms=([\d.]+) serialization_ms=([\d.]+) reimport_ms=([\d.]+) equivalence_ms=([\d.]+) total_ms=([\d.]+)/.exec(delivered.stderr.toString("utf8"));
    assert.ok(performance, delivered.stderr.toString("utf8"));
    console.log(`LEGACY_STEP_STAGES_MS=${JSON.stringify(Object.fromEntries(["lowering", "build", "serialization", "reimport", "equivalence", "total"].map((name, index) => [name, Number(performance[index + 1])])))}`);
    assert.ok(bytes.length > 1000);
    assert.match(bytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(delivered.stderr.toString("utf8"));
    assert.ok(metadata, delivered.stderr.toString("utf8"));
    assert.equal(metadata[1], sha256);
    assert.equal(Number(metadata[2]), bytes.length);

    const server = spawn("node", ["apps/scc-ui/server.mjs"], {
      cwd: repoRoot,
      env: { ...process.env, SCC_UI_PORT: "4175", SCC_UI_HOST: "127.0.0.1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const serverOutput = captureOutput(server);
    try {
      let ready = false;
      for (let attempt = 0; attempt < 40 && !ready; attempt += 1) {
        try { ready = (await fetch("http://127.0.0.1:4175/api/health")).ok; } catch {}
        if (!ready) await new Promise((resolve) => setTimeout(resolve, 250));
      }
      assert.equal(ready, true, `local product server did not become ready\n${serverOutput()}`);
      const response = await fetch("http://127.0.0.1:4175/api/engineering/export/step", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ model: source, projectName: "integration" }),
      });
      assert.equal(response.status, 200);
      const endpointBytes = Buffer.from(await response.arrayBuffer());
      const endpointSha256 = createHash("sha256").update(endpointBytes).digest("hex");
      assert.ok(endpointBytes.length > 1000);
      assert.equal(response.headers.get("x-aicad-step-sha256"), endpointSha256);
      assert.equal(Number(response.headers.get("x-aicad-step-size")), endpointBytes.length);
    } finally {
      await stopChild(server);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("Rust CLI delivers a parameter-derived rounded rectangle through the real CadQuery STEP chain", { skip: process.env.AICAD_RUST_STEP_INTEGRATION !== "1" || !existsSync(binary) }, () => {
  const root = mkdtempSync(join(tmpdir(), "aicad-rust-rounded-step-"));
  try {
    const dsl = join(root, "rounded-fixture.dsl");
    const model = join(root, "rounded-model.json");
    const sourceDsl = [
      "AICAD 1",
      "PART rounded_plate",
      "PARAM plate_width = 120",
      "PARAM plate_height := plate_width - 40",
      "PARAM corner_radius = 10",
      "BASE_ROUNDED_RECT main",
      "  CENTER 25 -15",
      "  WIDTH 120",
      "  HEIGHT 80",
      "  RADIUS 10",
      "BIND main WIDTH <- plate_width",
      "BIND main HEIGHT <- plate_height",
      "BIND main RADIUS <- corner_radius",
      "HOLE center_hole",
      "  CENTER 25 -15",
      "  DIA 12",
      "",
    ].join("\n");
    writeFileSync(dsl, sourceDsl, "utf8");
    const generated = run(["--json-errors", "engineering", "dsl", dsl]);
    const source = JSON.parse(generated.stdout.toString("utf8"));
    source.spatial = {
      thickness: { source: "literal", value: 8 },
      hole_extents: { center_hole: { type: "through_all" } },
    };
    writeFileSync(model, `${JSON.stringify(source)}\n`, "utf8");
    const delivered = run(["--json-errors", "--quiet", "engineering", "export-step", model]);
    const bytes = delivered.stdout;
    assert.ok(bytes.length > 1000);
    assert.match(bytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(delivered.stderr.toString("utf8"));
    assert.ok(metadata, delivered.stderr.toString("utf8"));
    assert.equal(metadata[1], sha256);
    assert.equal(Number(metadata[2]), bytes.length);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("Rust CLI delivers a circular flagship flange through the real CadQuery STEP chain", { skip: process.env.AICAD_RUST_STEP_INTEGRATION !== "1" || !existsSync(binary) }, () => {
  const root = mkdtempSync(join(tmpdir(), "aicad-rust-circle-step-"));
  try {
    const dsl = join(root, "circle-fixture.dsl");
    const model = join(root, "circle-model.json");
    const sourceDsl = [
      "AICAD 1",
      "PART circular_flange",
      "PARAM pcd = 120",
      "PARAM outer_diameter := pcd + 40",
      "BASE_CIRCLE main",
      "  CENTER 30 -20",
      "  DIA 160",
      "BIND main DIAMETER <- outer_diameter",
      "HOLE center_hole",
      "  CENTER 30 -20",
      "  DIA 60",
      "HOLE_PATTERN bolts CIRCULAR",
      "  CENTER 30 -20",
      "  PCD 120",
      "  COUNT 8",
      "  DIA 14",
      "  START_ANGLE 0",
      "",
    ].join("\n");
    writeFileSync(dsl, sourceDsl, "utf8");
    const generated = run(["--json-errors", "engineering", "dsl", dsl]);
    const source = JSON.parse(generated.stdout.toString("utf8"));
    source.spatial = {
      thickness: { source: "literal", value: 20 },
      hole_extents: {
        center_hole: { type: "through_all" },
        bolts: { type: "through_all" },
      },
    };
    writeFileSync(model, `${JSON.stringify(source)}\n`, "utf8");
    const delivered = run(["--json-errors", "--quiet", "engineering", "export-step", model]);
    const bytes = delivered.stdout;
    assert.ok(bytes.length > 1000);
    assert.match(bytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(delivered.stderr.toString("utf8"));
    assert.ok(metadata, delivered.stderr.toString("utf8"));
    assert.equal(metadata[1], sha256);
    assert.equal(Number(metadata[2]), bytes.length);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});


test("Rust CLI and UI deliver a validated STEP for a translated Polygon with single and pattern holes", { skip: process.env.AICAD_RUST_STEP_INTEGRATION !== "1" || !existsSync(binary) }, async () => {
  const root = mkdtempSync(join(tmpdir(), "aicad-rust-polygon-step-"));
  try {
    const dsl = join(root, "polygon-fixture.dsl");
    const model = join(root, "polygon-model.json");
    const sourceDsl = [
      "AICAD 1",
      "PART polygon_plate",
      "BASE_POLYGON main",
      "  POINT 30 -20",
      "  POINT 130 -20",
      "  POINT 150 20",
      "  POINT 110 70",
      "  POINT 30 40",
      "HOLE center_hole",
      "  CENTER 80 10",
      "  DIA 10",
      "HOLE_PATTERN bolts CIRCULAR",
      "  CENTER 80 10",
      "  PCD 20",
      "  COUNT 4",
      "  DIA 8",
      "  START_ANGLE 0",
      "",
    ].join("\n");
    writeFileSync(dsl, sourceDsl, "utf8");
    const generated = run(["--json-errors", "engineering", "dsl", dsl]);
    const source = JSON.parse(generated.stdout.toString("utf8"));
    source.spatial = {
      thickness: { source: "literal", value: 8 },
      hole_extents: {
        center_hole: { type: "through_all" },
        bolts: { type: "through_all" },
      },
    };
    writeFileSync(model, `${JSON.stringify(source)}\n`, "utf8");

    const delivered = run(["--json-errors", "--quiet", "engineering", "export-step", model]);
    const bytes = delivered.stdout;
    assert.ok(bytes.length > 1000);
    assert.match(bytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(delivered.stderr.toString("utf8"));
    assert.ok(metadata, delivered.stderr.toString("utf8"));
    assert.equal(metadata[1], sha256);
    assert.equal(Number(metadata[2]), bytes.length);

    const server = spawn("node", ["apps/scc-ui/server.mjs"], {
      cwd: repoRoot,
      env: { ...process.env, SCC_UI_PORT: "4175", SCC_UI_HOST: "127.0.0.1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const serverOutput = captureOutput(server);
    try {
      let ready = false;
      for (let attempt = 0; attempt < 40 && !ready; attempt += 1) {
        try { ready = (await fetch("http://127.0.0.1:4175/api/health")).ok; } catch {}
        if (!ready) await new Promise((resolve) => setTimeout(resolve, 250));
      }
      assert.equal(ready, true, `local product server did not become ready\n${serverOutput()}`);
      const response = await fetch("http://127.0.0.1:4175/api/engineering/export/step", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ model: source, projectName: "polygon-integration" }),
      });
      assert.equal(response.status, 200);
      const endpointBytes = Buffer.from(await response.arrayBuffer());
      const endpointSha256 = createHash("sha256").update(endpointBytes).digest("hex");
      assert.ok(endpointBytes.length > 1000);
      assert.match(endpointBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
      assert.equal(response.headers.get("x-aicad-step-sha256"), endpointSha256);
      assert.equal(Number(response.headers.get("x-aicad-step-size")), endpointBytes.length);
    } finally {
      await stopChild(server);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("Rust CLI and UI deliver a validated STEP for a typed translated Composite with exact arcs, Slot, and hole pattern", { skip: process.env.AICAD_RUST_STEP_INTEGRATION !== "1" || !existsSync(binary) }, async () => {
  const root = mkdtempSync(join(tmpdir(), "aicad-rust-composite-step-"));
  try {
    const dsl = join(root, "composite-fixture.dsl");
    const model = join(root, "composite-model.json");
    const sourceDsl = [
      "AICAD 1",
      "PART composite_plate",
      "BASE_RECT main",
      "  WIDTH 100",
      "  HEIGHT 60",
      "HOLE center_hole",
      "  CENTER 50 10",
      "  DIA 10",
      "HOLE_PATTERN bolts CIRCULAR",
      "  CENTER 50 10",
      "  PCD 20",
      "  COUNT 4",
      "  DIA 8",
      "  START_ANGLE 0",
      "",
    ].join("\n");
    writeFileSync(dsl, sourceDsl, "utf8");
    const generated = run(["--json-errors", "engineering", "dsl", dsl]);
    const source = JSON.parse(generated.stdout.toString("utf8"));
    const p = (x, y) => ({ x: x + 30, y: y - 20 });
    const notchCenterY = 60 + Math.sqrt(300);
    source.features.main = {
      type: "base_profile",
      id: "main",
      name: "main",
      profile_kind: "composite",
      segments: [
        { kind: "line", start: p(0, 0), end: p(80, 0) },
        { kind: "arc", start: p(80, 0), end: p(100, 20), center: p(80, 20), radius: 20, direction: "counter_clockwise" },
        { kind: "line", start: p(100, 20), end: p(100, 60) },
        { kind: "line", start: p(100, 60), end: p(60, 60) },
        { kind: "arc", start: p(60, 60), end: p(40, 60), center: p(50, notchCenterY), radius: 20, direction: "clockwise" },
        { kind: "line", start: p(40, 60), end: p(0, 60) },
        { kind: "line", start: p(0, 60), end: p(0, 0) },
      ],
    };
    source.features.slot_a = {
      type: "slot", id: "slot_a", center: { x: 115, y: 10 },
      length: 24, width: 8, orientation_degrees: 30,
    };
    source.spatial = {
      thickness: { source: "literal", value: 10 },
      hole_extents: {
        center_hole: { type: "through_all" },
        bolts: { type: "through_all" },
      },
    };
    writeFileSync(model, `${JSON.stringify(source)}\n`, "utf8");

    const delivered = run(["--json-errors", "--quiet", "engineering", "export-step", model]);
    const bytes = delivered.stdout;
    assert.ok(bytes.length > 1000);
    assert.match(bytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(delivered.stderr.toString("utf8"));
    assert.ok(metadata, delivered.stderr.toString("utf8"));
    assert.equal(metadata[1], sha256);
    assert.equal(Number(metadata[2]), bytes.length);

    const server = spawn("node", ["apps/scc-ui/server.mjs"], {
      cwd: repoRoot,
      env: { ...process.env, SCC_UI_PORT: "4175", SCC_UI_HOST: "127.0.0.1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const serverOutput = captureOutput(server);
    try {
      let ready = false;
      for (let attempt = 0; attempt < 40 && !ready; attempt += 1) {
        try { ready = (await fetch("http://127.0.0.1:4175/api/health")).ok; } catch {}
        if (!ready) await new Promise((resolve) => setTimeout(resolve, 250));
      }
      assert.equal(ready, true, `local product server did not become ready\n${serverOutput()}`);
      const response = await fetch("http://127.0.0.1:4175/api/engineering/export/step", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ model: source, projectName: "composite-integration" }),
      });
      assert.equal(response.status, 200);
      const endpointBytes = Buffer.from(await response.arrayBuffer());
      const endpointSha256 = createHash("sha256").update(endpointBytes).digest("hex");
      assert.ok(endpointBytes.length > 1000);
      assert.match(endpointBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
      assert.equal(response.headers.get("x-aicad-step-sha256"), endpointSha256);
      assert.equal(Number(response.headers.get("x-aicad-step-size")), endpointBytes.length);
    } finally {
      await stopChild(server);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("Rust CLI and UI deliver translated rotated Slot cuts through the generic validated STEP route", { skip: process.env.AICAD_RUST_STEP_INTEGRATION !== "1" || !existsSync(binary) }, async () => {
  const root = mkdtempSync(join(tmpdir(), "aicad-rust-slot-step-"));
  try {
    const dsl = join(root, "slot-fixture.dsl");
    const model = join(root, "slot-model.json");
    writeFileSync(dsl, "AICAD 1\nPART slot_plate\nBASE_RECT main\n  WIDTH 120\n  HEIGHT 80\nHOLE hole_a\n  CENTER -40 -20\n  DIA 6\n", "utf8");
    const generated = run(["--json-errors", "engineering", "dsl", dsl]);
    const source = JSON.parse(generated.stdout.toString("utf8"));
    source.features.slot_a = {
      type: "slot", id: "slot_a", center: { x: 13, y: -7 },
      length: 36, width: 12, orientation_degrees: 30,
    };
    source.features.slot_b = {
      type: "slot", id: "slot_b", center: { x: -25, y: 12 },
      length: 24, width: 8, orientation_degrees: 0,
    };
    source.spatial = {
      thickness: { source: "literal", value: 10 },
      hole_extents: { hole_a: { type: "through_all" } },
    };
    writeFileSync(model, `${JSON.stringify(source)}\n`, "utf8");

    const delivered = run(["--json-errors", "--quiet", "engineering", "export-step", model]);
    const bytes = delivered.stdout;
    assert.ok(bytes.length > 1000);
    assert.match(bytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
    const sha256 = createHash("sha256").update(bytes).digest("hex");
    const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(delivered.stderr.toString("utf8"));
    assert.ok(metadata, delivered.stderr.toString("utf8"));
    assert.equal(metadata[1], sha256);
    assert.equal(Number(metadata[2]), bytes.length);

    const server = spawn("node", ["apps/scc-ui/server.mjs"], {
      cwd: repoRoot,
      env: { ...process.env, SCC_UI_PORT: "4175", SCC_UI_HOST: "127.0.0.1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const serverOutput = captureOutput(server);
    try {
      let ready = false;
      for (let attempt = 0; attempt < 40 && !ready; attempt += 1) {
        try { ready = (await fetch("http://127.0.0.1:4175/api/health")).ok; } catch {}
        if (!ready) await new Promise((resolve) => setTimeout(resolve, 250));
      }
      assert.equal(ready, true, `local product server did not become ready\n${serverOutput()}`);
      const response = await fetch("http://127.0.0.1:4175/api/engineering/export/step", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ model: source, projectName: "slot-integration" }),
      });
      assert.equal(response.status, 200);
      const endpointBytes = Buffer.from(await response.arrayBuffer());
      const endpointSha256 = createHash("sha256").update(endpointBytes).digest("hex");
      assert.ok(endpointBytes.length > 1000);
      assert.match(endpointBytes.subarray(0, 32).toString("ascii"), /ISO-10303-21/);
      assert.equal(response.headers.get("x-aicad-step-sha256"), endpointSha256);
      assert.equal(Number(response.headers.get("x-aicad-step-size")), endpointBytes.length);
    } finally {
      await stopChild(server);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
