import { access, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export class ServiceError extends Error {
  constructor(status, code, message, details = undefined) {
    super(message);
    this.status = status;
    this.code = code;
    this.details = details;
  }

  toJSON() {
    return { code: this.code, message: this.message, details: this.details };
  }
}

async function executable(path) {
  if (!path) return false;
  try {
    await access(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

export async function resolveSccBinary(repoRoot, configured = process.env.SCC_BIN) {
  const suffix = process.platform === "win32" ? ".exe" : "";
  const candidates = [
    configured,
    join(repoRoot, "target", "release", `scc${suffix}`),
    join(repoRoot, "target", "debug", `scc${suffix}`),
  ];
  for (const candidate of candidates) {
    if (await executable(candidate)) return resolve(candidate);
  }
  return null;
}

function apiFailureDetails(diagnostics) {
  const diagnostic = diagnostics[0] ?? {};
  const details = diagnostic.details ?? {};
  const nested = Array.isArray(details.diagnostics) ? details.diagnostics[0]?.details ?? {} : {};
  return {
    failure_class: details.failure_class ?? nested.failure_class ?? "UNKNOWN_FAILURE_CLASS",
    failure_stage: details.failure_stage ?? details.backend_stage ?? diagnostic.stage ?? "unknown",
    error_code: details.error_code ?? details.backend_code ?? details.parametric_code ?? diagnostic.code ?? "UNKNOWN_ERROR",
  };
}

function parseCliError(stderr, fallback) {
  const trimmed = stderr.trim();
  try {
    const parsed = JSON.parse(trimmed);
    // The CLI emits {"errors": [...]}; some paths emit a bare array/object.
    const list = Array.isArray(parsed)
      ? parsed
      : Array.isArray(parsed?.errors) && parsed.errors.length > 0
        ? parsed.errors
        : [parsed];
    return list.length > 0 ? list : [{ code: "SCC-UI-CLI-001", stage: "cli", message: trimmed || fallback }];
  } catch {
    return [{ code: "SCC-UI-CLI-001", stage: "cli", message: trimmed || fallback }];
  }
}

export class SccCliAdapter {
  constructor(binary) {
    this.binary = binary;
  }

  async run(args) {
    if (!this.binary) {
      throw new ServiceError(503, "SCC-UI-CLI-001", "SCC binary is not available", {
        suggestion: "Build it with cargo build --release -p scc-cli or set SCC_BIN",
      });
    }
    try {
      return await execFileAsync(this.binary, args, {
        windowsHide: true,
        timeout: 30_000,
        maxBuffer: 12 * 1024 * 1024,
        shell: false,
      });
    } catch (error) {
      const diagnostics = parseCliError(error.stderr ?? "", error.message);
      throw new ServiceError(422, diagnostics[0]?.code ?? "SCC-UI-CLI-002", diagnostics[0]?.message ?? "SCC command failed", {
        diagnostics,
        exitCode: error.code,
        ...apiFailureDetails(diagnostics),
      });
    }
  }

  async version() {
    const { stdout } = await this.run(["--version"]);
    return stdout.trim();
  }

  async compile(proposal, envelope = false) {
    return this.#withWorkspace(async (directory) => {
      const input = join(directory, "proposal.json");
      const output = join(directory, "compiled.json");
      await writeJson(input, proposal);
      await this.run([...globalArgs(envelope), "compile", input, "--output", output]);
      const artifact = await readJson(output);
      return artifact.status === "conflict"
        ? { outcome: "conflict", conflicts: artifact }
        : { outcome: "compiled", ir: artifact };
    });
  }

  async execute(ir, mutation = [], envelope = false) {
    return this.#withWorkspace(async (directory) => {
      const input = join(directory, "constraint-ir.json");
      const patch = join(directory, "mutation.json");
      const trace = join(directory, "execution.trace.json");
      await writeJson(input, ir);
      await writeJson(patch, mutation);
      const args = [...globalArgs(envelope), "execute", input, "--trace", trace];
      if (mutation.length > 0) args.push("--mutation", patch);
      await this.run(args);
      return {
        trace: await readJson(trace),
        state: await readJson(join(directory, "execution.trace.state.json")),
      };
    });
  }

  async inspect(ir) {
    return this.#withWorkspace(async (directory) => {
      const input = join(directory, "constraint-ir.json");
      await writeJson(input, ir);
      const { stdout } = await this.run(["--json-errors", "inspect", input]);
      return JSON.parse(stdout);
    });
  }

  async solve(state, envelope = false) {
    return this.#withWorkspace(async (directory) => {
      const input = join(directory, "graph-state.json");
      const output = join(directory, "geometry.json");
      await writeJson(input, state);
      await this.run([...globalArgs(envelope), "solve", input, "--output", output]);
      return await readJson(output);
    });
  }

  async export(geometry, format) {
    if (!new Set(["svg", "dxf"]).has(format)) {
      throw new ServiceError(400, "SCC-UI-REQ-004", "Export format must be svg or dxf");
    }
    return this.#withWorkspace(async (directory) => {
      const input = join(directory, "geometry.json");
      const output = join(directory, `model.${format}`);
      await writeJson(input, geometry);
      await this.run(["--json-errors", "--quiet", "--force", "export", input, "--format", format, "--output", output]);
      return await readFile(output, "utf8");
    });
  }

  // ---- AICAD engineering intent bridge (Phase 6) ----

  async engineeringManifest() {
    const { stdout } = await this.run(["--json-errors", "engineering", "manifest"]);
    return JSON.parse(stdout);
  }

  async engineeringDslApply(dsl) {
    return this.#withWorkspace(async (directory) => {
      const file = join(directory, "design.dsl");
      await writeFile(file, dsl, "utf8");
      const { stdout } = await this.run(["--json-errors", "engineering", "dsl", file]);
      return JSON.parse(stdout);
    });
  }

  async engineeringDslSolve(dsl) {
    return this.#withWorkspace(async (directory) => {
      const file = join(directory, "design.dsl");
      await writeFile(file, dsl, "utf8");
      const { stdout } = await this.run(["--json-errors", "engineering", "dsl", file, "--solve"]);
      return JSON.parse(stdout);
    });
  }

  async engineeringToolsApply(toolCalls, modelJson = undefined, solve = false) {
    return this.#withWorkspace(async (directory) => {
      const calls = join(directory, "tool-calls.json");
      await writeJson(calls, toolCalls);
      const args = ["--json-errors", "engineering", "tools", calls];
      if (modelJson !== undefined && modelJson !== null) {
        const model = join(directory, "model.json");
        await writeFile(model, modelJson, "utf8");
        args.push("--model", model);
      }
      if (solve) args.push("--solve");
      const { stdout } = await this.run(args);
      return JSON.parse(stdout);
    });
  }

  async engineeringContext(modelJson) {
    return this.#withWorkspace(async (directory) => {
      const model = join(directory, "model.json");
      await writeFile(model, modelJson, "utf8");
      const { stdout } = await this.run(["--json-errors", "engineering", "context", model]);
      return JSON.parse(stdout);
    });
  }

  async engineeringSnapshot(modelJson) {
    return this.#withWorkspace(async (directory) => {
      const model = join(directory, "model.json");
      await writeFile(model, modelJson, "utf8");
      const { stdout } = await this.run(["--json-errors", "engineering", "snapshot", model]);
      return JSON.parse(stdout);
    });
  }

  async engineeringWorkbench(modelJson) {
    return this.#withWorkspace(async (directory) => {
      const model = join(directory, "model.json");
      await writeFile(model, modelJson, "utf8");
      const { stdout } = await this.run(["--json-errors", "engineering", "workbench", model]);
      return JSON.parse(stdout);
    });
  }

  async engineeringDrawing(modelJson) {
    return this.#withWorkspace(async (directory) => {
      const model = join(directory, "model.json");
      await writeFile(model, modelJson, "utf8");
      const { stdout } = await this.run(["--json-errors", "engineering", "drawing", model]);
      return JSON.parse(stdout);
    });
  }

  async engineeringThreeViewDrawing(modelJson) {
    return this.#withWorkspace(async (directory) => {
      const model = join(directory, "model.json");
      await writeFile(model, modelJson, "utf8");
      const { stdout } = await this.run(["--json-errors", "engineering", "drawing-three-view", model]);
      return JSON.parse(stdout);
    });
  }

  async engineeringSectionDrawing(modelJson, sectionPlane) {
    return this.#withWorkspace(async (directory) => {
      const model = join(directory, "model.json");
      await writeFile(model, modelJson, "utf8");
      const args = ["--json-errors", "engineering", "drawing-section", model,
        "--axis", String(sectionPlane.axis).toLowerCase(),
        "--offset-mm", String(sectionPlane.offset_mm),
        "--view-side", String(sectionPlane.view_side ?? "positive").toLowerCase()];
      const { stdout } = await this.run(args);
      return JSON.parse(stdout);
    });
  }

  async engineeringProjectSave(modelJson, name, description = undefined) {
    return this.#withWorkspace(async (directory) => {
      const model = join(directory, "model.json");
      await writeFile(model, modelJson, "utf8");
      const args = ["--json-errors", "engineering", "project-save", model, "--name", name];
      if (description !== undefined && description !== null) args.push("--description", description);
      const { stdout } = await this.run(args);
      return JSON.parse(stdout);
    });
  }

  async engineeringProjectOpen(projectJson) {
    return this.#withWorkspace(async (directory) => {
      const project = join(directory, "project.aicad.json");
      await writeFile(project, projectJson, "utf8");
      const { stdout } = await this.run(["--json-errors", "engineering", "project-open", project]);
      return JSON.parse(stdout);
    });
  }

  async engineeringStep(modelJson) {
    return this.#withWorkspace(async (directory) => {
      const model = join(directory, "model.json");
      await writeFile(model, modelJson, "utf8");
      const result = await this.runBinary(["--json-errors", "--quiet", "engineering", "export-step", model]);
      const bytes = Buffer.isBuffer(result.stdout) ? result.stdout : Buffer.from(result.stdout);
      const sha256 = createHash("sha256").update(bytes).digest("hex");
      const diagnostics = result.stderr.toString("utf8");
      const metadata = /AICAD-STEP-META\s+sha256=([0-9a-f]{64})\s+size_bytes=(\d+)/.exec(diagnostics);
      const status = /AICAD-STEP-STATUS\s+manufacturing=(passed)\s+build=(passed)\s+reimport=(passed)\s+equivalence=(passed)/.exec(diagnostics);
      if (!metadata || metadata[1] !== sha256 || Number(metadata[2]) !== bytes.length || !status) {
        throw new ServiceError(502, "SCC-UI-STEP-002", "Validated STEP metadata or validation stages do not match delivered bytes");
      }
      return { bytes, sha256, sizeBytes: bytes.length, status: { manufacturing: status[1], build: status[2], reimport: status[3], equivalence: status[4] } };
    });
  }

  async runBinary(args) {
    if (!this.binary) {
      throw new ServiceError(503, "SCC-UI-CLI-001", "SCC binary is not available", {
        suggestion: "Build it with cargo build --release -p scc-cli or set SCC_BIN",
      });
    }
    try {
      return await execFileAsync(this.binary, args, {
        windowsHide: true,
        timeout: 120_000,
        maxBuffer: 32 * 1024 * 1024,
        shell: false,
        encoding: "buffer",
      });
    } catch (error) {
      const diagnostics = parseCliError(error.stderr?.toString?.("utf8") ?? "", error.message);
      throw new ServiceError(422, diagnostics[0]?.code ?? "SCC-UI-CLI-002", diagnostics[0]?.message ?? "SCC command failed", {
        diagnostics,
        exitCode: error.code,
        ...apiFailureDetails(diagnostics),
      });
    }
  }

  async #withWorkspace(operation) {
    const directory = await mkdtemp(join(tmpdir(), "scc-ui-"));
    try {
      return await operation(directory);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  }
}

function globalArgs(envelope) {
  return ["--json-errors", "--quiet", "--force", ...(envelope ? ["--schema-version", "1.0"] : [])];
}

async function writeJson(path, value) {
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

async function readJson(path) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    throw new ServiceError(500, "SCC-UI-ARTIFACT-001", `Unable to read ${basename(path)}`, { cause: error.message });
  }
}
