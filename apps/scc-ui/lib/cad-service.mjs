import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
import { assertBuildableCadSpec, normalizeCadSpec } from "./cad-spec.mjs";
import { ServiceError } from "./scc-cli.mjs";

const execFileAsync = promisify(execFile);
const moduleRoot = dirname(fileURLToPath(import.meta.url));
const DEFAULT_CAD_ROOT = resolve(moduleRoot, "..", "cad");
const MAX_ARTIFACT_BYTES = 64 * 1024 * 1024;

export function createCadService({
  pythonPath = process.env.SCC_CAD_PYTHON ?? "python3",
  cadRoot = DEFAULT_CAD_ROOT,
  runWorker,
} = {}) {
  const jobs = new Map();
  let building = false;
  const invokeWorker = runWorker ?? ((spec, directory) => executeWorker(pythonPath, cadRoot, spec, directory));

  return {
    async build(input) {
      if (building) throw new ServiceError(429, "SCC-CAD-005", "A CAD build is already running");
      const cadSpec = assertBuildableCadSpec(normalizeCadSpec(input));
      const directory = await mkdtemp(join(tmpdir(), "aicad-cad-"));
      building = true;
      try {
        const manifest = await invokeWorker(cadSpec, directory);
        validateManifest(manifest, cadSpec);
        const totalBytes = Object.values(manifest.artifacts).reduce((sum, artifact) => sum + artifact.bytes, 0);
        if (totalBytes > MAX_ARTIFACT_BYTES) {
          throw new ServiceError(413, "SCC-CAD-007", "Generated CAD artifacts exceed the 64 MiB job limit");
        }
        const previewArtifact = manifest.artifacts.svg;
        const previewSvg = previewArtifact
          ? await readFile(safeArtifactPath(directory, previewArtifact.filename), "utf8")
          : null;
        const jobId = randomUUID();
        jobs.set(jobId, { directory, manifest });
        return { job_id: jobId, cad_spec: cadSpec, manifest, preview_svg: previewSvg };
      } catch (error) {
        await rm(directory, { recursive: true, force: true });
        throw normalizeWorkerError(error);
      } finally {
        building = false;
      }
    },

    async artifact(jobId, name) {
      const job = jobs.get(jobId);
      if (!job) throw new ServiceError(404, "SCC-CAD-008", "CAD artifact job was not found");
      if (!/^[a-z0-9_-]+$/.test(name)) throw new ServiceError(404, "SCC-CAD-008", "CAD artifact was not found");
      if (name === "step") {
        throw new ServiceError(400, "SCC-CAD-010", "STEP export is not supported for CadDocument compatibility builds; use EngineeringModel export");
      }
      const artifact = job.manifest.artifacts[name];
      if (!artifact) throw new ServiceError(404, "SCC-CAD-008", "CAD artifact was not found");
      return {
        filename: artifact.filename,
        contentType: artifact.content_type,
        content: await readFile(safeArtifactPath(job.directory, artifact.filename)),
      };
    },

    async delete(jobId) {
      const job = jobs.get(jobId);
      if (!job) return false;
      jobs.delete(jobId);
      await rm(job.directory, { recursive: true, force: true });
      return true;
    },

    async close() {
      const current = [...jobs.entries()];
      jobs.clear();
      await Promise.all(current.map(([, job]) => rm(job.directory, { recursive: true, force: true })));
    },
  };
}

async function executeWorker(pythonPath, cadRoot, spec, directory) {
  const input = join(directory, "input.cadspec.json");
  await writeFile(input, `${JSON.stringify(spec, null, 2)}\n`, "utf8");
  try {
    const { stdout } = await execFileAsync(pythonPath, [
      "-m", "aicad_cad.worker", "--input", input, "--output", directory,
    ], {
      cwd: cadRoot,
      env: { ...process.env, PYTHONPATH: cadRoot, XDG_CACHE_HOME: join(directory, ".cache") },
      timeout: 60_000,
      maxBuffer: 2 * 1024 * 1024,
      windowsHide: true,
      shell: false,
    });
    return JSON.parse(stdout);
  } catch (error) {
    if (error.killed || error.signal) {
      throw new ServiceError(504, "SCC-CAD-006", "CAD build exceeded the 60-second timeout");
    }
    throw new ServiceError(422, "SCC-CAD-006", "CAD kernel failed to build the requested solid", {
      diagnostics: String(error.stderr ?? error.message).slice(0, 8_000),
    });
  }
}

function validateManifest(manifest, cadSpec) {
  if (manifest?.status !== "built" || manifest.solid_valid !== true || !manifest.artifacts || typeof manifest.artifacts !== "object") {
    throw new ServiceError(500, "SCC-CAD-009", "CAD worker returned an invalid artifact manifest");
  }
  if (manifest.source_hash !== cadSpec.source_hash) {
    throw new ServiceError(500, "SCC-CAD-009", "CAD worker manifest does not match the normalized source");
  }
  if (Object.hasOwn(manifest.artifacts, "step")) {
    throw new ServiceError(500, "SCC-CAD-010", "Compatibility CAD worker returned a forbidden STEP artifact; use EngineeringModel export");
  }
  if (cadSpec.kind === "feature_graph_v1"
    && (manifest.document_kind !== "feature_graph_v1"
      || !Array.isArray(manifest.feature_records)
      || manifest.feature_records.length !== cadSpec.features.length
      || !manifest.coverage
      || !new Set(["complete", "partial"]).has(manifest.coverage.status)
      || manifest.coverage.total !== cadSpec.requirements.length
      || !manifest.parameter_evidence
      || manifest.parameter_evidence.total !== Object.keys(cadSpec.parameters).length
      || manifest.parameter_evidence.derived !== Object.values(cadSpec.parameters).filter((parameter) => parameter.source === "derived").length
      || !manifest.manufacturing_evidence
      || manifest.manufacturing_evidence.total !== Object.values(cadSpec.manufacturing).flat().length
      || !sameIds(
        manifest.manufacturing_evidence.ids,
        Object.values(cadSpec.manufacturing).flat().map((semantic) => semantic.id),
      ))) {
    throw new ServiceError(500, "SCC-CAD-009", "CAD worker omitted FeatureGraph execution or requirement coverage evidence");
  }
  for (const [name, artifact] of Object.entries(manifest.artifacts)) {
    if (!/^[a-z0-9_-]+$/.test(name)
      || typeof artifact.filename !== "string"
      || basename(artifact.filename) !== artifact.filename
      || typeof artifact.content_type !== "string"
      || !Number.isSafeInteger(artifact.bytes)
      || artifact.bytes < 0) {
      throw new ServiceError(500, "SCC-CAD-009", "CAD worker returned an unsafe artifact manifest");
    }
  }
}

function sameIds(actual, expected) {
  return Array.isArray(actual)
    && actual.length === expected.length
    && actual.every((id, index) => id === expected[index]);
}

function safeArtifactPath(directory, filename) {
  if (basename(filename) !== filename) throw new ServiceError(500, "SCC-CAD-009", "Unsafe CAD artifact filename");
  return join(directory, filename);
}

function normalizeWorkerError(error) {
  if (error instanceof ServiceError) return error;
  return new ServiceError(500, "SCC-CAD-009", "Unexpected CAD worker failure", { cause: error.message });
}
