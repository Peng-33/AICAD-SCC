import { createServer as createHttpServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import { dirname, extname, join, normalize, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { extractDocument } from "./lib/documents.mjs";
import { createCadService } from "./lib/cad-service.mjs";
import { normalizeCadSpec } from "./lib/cad-spec.mjs";
import { EXAMPLES, loadExample } from "./lib/examples.mjs";
import { createIntentService } from "./lib/intent-service.mjs";
import { resolveSccBinary, SccCliAdapter, ServiceError } from "./lib/scc-cli.mjs";
import { runIntent } from "./lib/engineering-intent.mjs";
import { loadCadSkills, publicCadSkills } from "./lib/cad-skills/registry.mjs";
import { createCadSkillPlan } from "./lib/cad-skills/service.mjs";
import { completeCadSkillRun, completeDrawingSkillRun } from "./lib/cad-skills/router.mjs";
import { SkillPlanStore, bindSkillPlan } from "./lib/cad-skills/plan-store.mjs";
import { diagnoseEngineeringError } from "./lib/cad-skills/diagnostics.mjs";
import { analyzeMechanicalRequest } from "./lib/cad-skills/analysis.mjs";
import { validateSectionDrawingArtifacts, validateThreeViewArtifacts } from "./lib/cad-skills/export-validation.mjs";
import { buildChangePreview } from "./lib/change-preview.mjs";
import { ArtifactKind, DerivedArtifactCache } from "./lib/derived-cache.mjs";

const appRoot = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(appRoot, "..", "..");
const publicRoot = join(appRoot, "public");
const threeRoot = join(appRoot, "node_modules", "three");
const MAX_BODY_BYTES = 6 * 1024 * 1024;
const CAD_SKILLS = await loadCadSkills();

const mimeTypes = new Map([
  [".html", "text/html; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".svg", "image/svg+xml"],
]);

export function createApp({ adapter, cadService, intentEndpoint, intentProvider, intentModel, intentApiKey, fetchImpl, engineeringProvider } = {}) {
  const solidCadService = cadService ?? createCadService();
  const intentService = createIntentService({
    intentEndpoint,
    intentProvider,
    intentModel,
    intentApiKey,
    fetchImpl,
    validateProposal: async (proposal, mutation) => preflightIntentResult(adapter, proposal, mutation),
    validateCadSpec: async (cadSpec) => normalizeCadSpec(cadSpec),
  });
  const session = { revision: 0, derivedCache: new DerivedArtifactCache(8), skillPlans: new SkillPlanStore() };
  return createHttpServer(async (request, response) => {
    setSecurityHeaders(request, response);
    try {
      const url = new URL(request.url, "http://localhost");
      if (url.pathname.startsWith("/api/")) {
        await handleApi(request, response, url, adapter, intentService, solidCadService, engineeringProvider, session);
      } else {
        await serveStatic(response, url.pathname);
      }
    } catch (error) {
      sendError(response, error);
    }
  });
}

/** Request the formal one-solve snapshot transport from Rust.
 * The returned model, geometry, source map, and diagnostics share one solve
 * boundary; downstream consumers must not reconstruct it with an empty batch. */
async function engineeringSnapshot(adapter, modelJson) {
  const payload = await adapter.engineeringSnapshot(modelJson);
  return snapshotFromSolvedPayload(JSON.parse(modelJson), payload);
}

function cacheParametricDrawing(session, model, payload) {
  if (!payload?.parametric_drawing || !model) return;
  session.derivedCache.set(
    fingerprintModel(model),
    ArtifactKind.ThreeViewDrawing,
    payload.parametric_drawing,
  );
}

function snapshotFromSolvedPayload(model, payload) {
  return {
    workbench: payload.workbench,
    geometry: payload.geometry,
    source_map: payload.source_map,
    entity_to_feature: payload.entity_to_feature,
    solve_error: null,
    diagnostics: payload.diagnostics ?? [],
    fingerprint: fingerprintModel(payload.model ?? model),
  };
}

async function workbenchOnlySnapshot(adapter, modelJson, model) {
  return {
    workbench: await adapter.engineeringWorkbench(modelJson),
    geometry: null,
    source_map: null,
    entity_to_feature: null,
    solve_error: null,
    diagnostics: [],
    fingerprint: fingerprintModel(model),
  };
}

function nextRevision(session) {
  session.revision += 1;
  return session.revision;
}

export async function preflightIntentResult(adapter, proposal, mutation = []) {
  const compiled = await adapter.compile(proposal);
  if (compiled.outcome === "conflict") return compiled;
  const executed = await adapter.execute(compiled.ir, mutation);
  await adapter.solve(executed.state);
  return compiled;
}

async function handleApi(request, response, url, adapter, intentService, cadService, engineeringProvider, session) {
  if (request.method === "GET" && url.pathname === "/api/health") {
    const version = await adapter.version();
    return sendJson(response, 200, { status: "ready", version, intentConfigured: intentService.publicConfig().configured });
  }
  if (request.method === "GET" && url.pathname === "/api/cad-skills") {
    return sendJson(response, 200, { schema_version: 1, skills: publicCadSkills(CAD_SKILLS) });
  }
  if (request.method === "GET" && url.pathname === "/api/intent-config") {
    return sendJson(response, 200, intentService.publicConfig());
  }
  if (request.method === "GET" && url.pathname === "/api/examples") {
    return sendJson(response, 200, EXAMPLES.map(({ proposal, mutation, ...example }) => example));
  }
  if (request.method === "GET" && url.pathname === "/api/cad/examples/gearbox-housing") {
    return sendJson(response, 200, JSON.parse(await readFile(join(repoRoot, "examples", "gearbox-housing.cadspec.json"), "utf8")));
  }
  if (request.method === "GET" && url.pathname === "/api/cad/examples/generic-flange") {
    return sendJson(response, 200, JSON.parse(await readFile(join(repoRoot, "examples", "generic-six-hole-flange.caddoc.json"), "utf8")));
  }
  if (request.method === "GET" && url.pathname.startsWith("/api/examples/")) {
    const id = decodeURIComponent(url.pathname.slice("/api/examples/".length));
    return sendJson(response, 200, await loadExample(repoRoot, id));
  }
  if (request.method === "PATCH" && url.pathname === "/api/intent-config") {
    assertSameOrigin(request);
    return sendJson(response, 200, intentService.update(await readJsonBody(request)));
  }
  if (request.method === "GET" && url.pathname === "/api/model-presets") {
    return sendJson(response, 200, [
      { type: "deepseek", name: "DeepSeek", endpoint: "https://api.deepseek.com", source: "https://api-docs.deepseek.com/" },
      { type: "qwen", name: "Qwen", endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1", source: "https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope" },
      { type: "openai-compatible", name: "OpenAI-compatible", endpoint: "", source: null },
      { type: "local", name: "Local OpenAI-compatible", endpoint: "http://127.0.0.1:11434/v1", source: null },
    ]);
  }
  if (request.method === "GET" && url.pathname === "/api/model-configs") {
    return sendJson(response, 200, intentService.listProfiles());
  }
  if (request.method === "POST" && url.pathname === "/api/model-configs") {
    assertSameOrigin(request);
    return sendJson(response, 201, intentService.addProfile(await readJsonBody(request)));
  }
  const modelRoute = url.pathname.match(/^\/api\/model-configs\/([^/]+)(?:\/(activate|test-connection|test-engineering))?$/);
  if (modelRoute) {
    assertSameOrigin(request);
    const id = decodeURIComponent(modelRoute[1]);
    const action = modelRoute[2];
    if (request.method === "PATCH" && !action) return sendJson(response, 200, intentService.editProfile(id, await readJsonBody(request)));
    if (request.method === "DELETE" && !action) return sendJson(response, 200, intentService.deleteProfile(id));
    if (request.method === "POST" && action === "activate") return sendJson(response, 200, intentService.activateProfile(id));
    if (request.method === "POST" && action === "test-connection") return sendJson(response, 200, await intentService.testProfileConnection(id));
    if (request.method === "POST" && action === "test-engineering") {
      const check = async (provider) => {
        const result = await runIntent({
          mode: "new",
          text: "创建一块 120 × 80 mm、厚 10 mm 的矩形安装板，使用第三角法生成主视图、俯视图和左视图。",
          provider,
          cli: adapter,
        });
        if (result.status !== "ok") throw new ServiceError(422, result.error?.code ?? "SCC-UI-MODEL-006", result.error?.message ?? "Engineering protocol validation failed");
        const drawing = await adapter.engineeringThreeViewDrawing(JSON.stringify(result.result.model));
        const svg = typeof drawing === "string" ? JSON.parse(drawing).svg : drawing.svg;
        if (!svg?.includes('id="view_front"') || !svg.includes('id="view_top"') || !svg.includes('id="view_left"')) {
          throw new ServiceError(422, "SCC-UI-MODEL-006", "Engineering three-view validation failed");
        }
      };
      return sendJson(response, 200, await intentService.testProfileEngineering(id, check));
    }
  }
  const artifactMatch = url.pathname.match(/^\/api\/cad\/artifacts\/([^/]+)\/([^/]+)$/);
  if (request.method === "GET" && artifactMatch) {
    const artifact = await cadService.artifact(decodeURIComponent(artifactMatch[1]), decodeURIComponent(artifactMatch[2]));
    return sendArtifact(response, artifact);
  }
  const jobMatch = url.pathname.match(/^\/api\/cad\/artifacts\/([^/]+)$/);
  if (request.method === "DELETE" && jobMatch) {
    assertSameOrigin(request);
    const deleted = await cadService.delete(decodeURIComponent(jobMatch[1]));
    return sendJson(response, 200, { deleted });
  }
  if (request.method !== "POST") {
    throw new ServiceError(405, "SCC-UI-REQ-001", "Method not allowed");
  }
  const body = await readJsonBody(request);
  switch (url.pathname) {
    case "/api/engineering/skills/plan": {
      assertSameOrigin(request);
      const prepared = await prepareSkillPlan(body, adapter, session);
      return sendJson(response, 200, prepared);
    }
    case "/api/compile":
      return sendJson(response, 200, await adapter.compile(required(body, "proposal"), body.envelope));
    case "/api/execute":
      return sendJson(response, 200, await adapter.execute(required(body, "ir"), body.mutation ?? [], body.envelope));
    case "/api/inspect":
      return sendJson(response, 200, await adapter.inspect(required(body, "ir")));
    case "/api/solve":
      return sendJson(response, 200, { geometry: await adapter.solve(required(body, "state"), body.envelope) });
    case "/api/export": {
      const format = required(body, "format");
      const content = await adapter.export(required(body, "geometry"), format);
      return sendJson(response, 200, { format, content });
    }
    case "/api/documents/extract":
      return sendJson(response, 200, await extractDocument(body));
    case "/api/intent":
      return sendJson(response, 200, await intentService.compile(body));
    case "/api/cad/build":
      return sendJson(response, 200, await cadService.build(required(body, "cadSpec")));
    case "/api/cad/intent":
      return sendJson(response, 200, await intentService.compileCad(body));
    case "/api/engineering/project/save": {
      assertSameOrigin(request);
      const model = required(body, "model");
      assertSourceEngineeringModel(model);
      const name = required(body, "name");
      const projectJson = await adapter.engineeringProjectSave(JSON.stringify(model), name, body.description);
      const project = typeof projectJson === "string" ? JSON.parse(projectJson) : projectJson;
      return sendJson(response, 200, { project, model: project.engineering, fingerprint: fingerprintModel(project.engineering), revision: session.revision });
    }
    case "/api/engineering/project/open": {
      assertSameOrigin(request);
      const project = required(body, "project");
      const projectJson = await adapter.engineeringProjectOpen(typeof project === "string" ? project : JSON.stringify(project));
      const canonical = typeof projectJson === "string" ? JSON.parse(projectJson) : projectJson;
      return sendJson(response, 200, { project: canonical, model: canonical.engineering, fingerprint: fingerprintModel(canonical.engineering), revision: nextRevision(session) });
    }
    case "/api/engineering/export/step": {
      assertSameOrigin(request);
      const model = required(body, "model");
      assertSourceEngineeringModel(model);
      const artifact = await adapter.engineeringStep(JSON.stringify(model));
      return sendStepArtifact(response, artifact, body.projectName, fingerprintModel(model));
    }
    case "/api/engineering/dsl": {
      const dsl = required(body, "dsl");
      const solved = await adapter.engineeringDslSolve(dsl);
      const snapshot = snapshotFromSolvedPayload(solved.model, solved);
      return sendJson(response, 200, { model: solved.model, ...snapshot, revision: nextRevision(session) });
    }
    case "/api/engineering/workbench": {
      const model = required(body, "model");
      const snapshot = await engineeringSnapshot(adapter, JSON.stringify(model));
      return sendJson(response, 200, { model, ...snapshot, revision: session.revision });
    }
    case "/api/engineering/drawing": {
      const model = required(body, "model");
      const fingerprint = fingerprintModel(model);
      const cached = session.derivedCache.get(fingerprint, ArtifactKind.PrincipalDrawing);
      const drawing = cached ?? await adapter.engineeringDrawing(JSON.stringify(model));
      if (!cached) session.derivedCache.set(fingerprint, ArtifactKind.PrincipalDrawing, drawing);
      return sendJson(response, 200, { ...drawing, model_fingerprint: fingerprint, artifact_kind: ArtifactKind.PrincipalDrawing, revision: session.revision, cache_status: cached ? "hit" : "miss" });
    }
    case "/api/engineering/drawing/section": {
      const model = required(body, "model");
      const sectionPlane = required(body, "sectionPlane");
      assertSourceEngineeringModel(model);
      if (!sectionPlane || typeof sectionPlane !== "object" || Array.isArray(sectionPlane)
        || !["x", "y", "z"].includes(sectionPlane.axis)
        || !Number.isFinite(sectionPlane.offset_mm)
        || !["positive", "negative"].includes(sectionPlane.view_side)) {
        throw new ServiceError(400, "SECTION_REQUEST_INVALID", "剖切轴、位置或观察方向无效", {
          suggestion: "选择 X、Y 或 Z 轴平面，输入有限毫米位置，并选择正向或反向观察。",
        });
      }
      const modelFingerprint = fingerprintModel(model);
      let drawing;
      try {
        drawing = await adapter.engineeringSectionDrawing(JSON.stringify(model), sectionPlane);
      } catch (error) {
        const diagnostic = diagnoseEngineeringError(error, model);
        throw new ServiceError(error?.status ?? 422, error?.code ?? "SCC-UI-SECTION-001", error?.message ?? "Section A-A 生成失败", { ...(error?.details ?? {}), diagnostic });
      }
      const skillsReport = validateSectionDrawingArtifacts({ drawing, modelFingerprint });
      const plan = {
        schema_version: 1,
        selected_skill_ids: ["orthographic-drawing", "drawing-annotation", "cad-export-validation", "engineering-diagnostics"],
        skill_states: [
          { id: "orthographic-drawing", state: "PENDING" },
          { id: "drawing-annotation", state: "PENDING" },
          { id: "cad-export-validation", state: "PENDING" },
          { id: "engineering-diagnostics", state: "PENDING" },
        ],
      };
      const skillRun = completeDrawingSkillRun(plan, skillsReport);
      return sendJson(response, 200, { ...drawing, model_fingerprint: modelFingerprint, artifact_kind: "section_drawing", revision: session.revision, skills_report: skillsReport, skill_run: skillRun });
    }
    case "/api/engineering/drawing/three-view": {
      const model = required(body, "model");
      assertSourceEngineeringModel(model);
      const modelFingerprint = fingerprintModel(model);
      const cached = session.derivedCache.get(modelFingerprint, ArtifactKind.ThreeViewDrawing);
      let drawing;
      try {
        drawing = cached ?? await adapter.engineeringThreeViewDrawing(JSON.stringify(model));
      } catch (error) {
        const diagnostic = diagnoseEngineeringError(error, model);
        throw new ServiceError(error?.status ?? 422, error?.code ?? "SCC-UI-DRAWING-001", error?.message ?? "Three-view drawing failed", { ...(error?.details ?? {}), diagnostic });
      }
      if (!cached) session.derivedCache.set(modelFingerprint, ArtifactKind.ThreeViewDrawing, drawing);
      const context = await adapter.engineeringContext(JSON.stringify(model));
      const analysis = body.userIntent ? analyzeMechanicalRequest({ text: body.userIntent, model, context }) : null;
      const skillsReport = validateThreeViewArtifacts({ drawing, modelFingerprint, analysis });
      if (skillsReport.status === "FAIL") {
        skillsReport.diagnostic = diagnoseEngineeringError({ code: "SCC-UI-EXPORT-001", message: "Formal SVG, DXF, or DrawingModel consistency validation failed" }, model);
      }
      const plan = {
        schema_version: 1,
        selected_skill_ids: ["orthographic-drawing", "drawing-annotation", "cad-export-validation"],
        skill_states: [
          { id: "orthographic-drawing", state: "PENDING" },
          { id: "drawing-annotation", state: "PENDING" },
          { id: "cad-export-validation", state: "PENDING" },
        ],
      };
      const drawingRun = completeDrawingSkillRun(plan, skillsReport);
      return sendJson(response, 200, { ...drawing, model_fingerprint: modelFingerprint, artifact_kind: ArtifactKind.ThreeViewDrawing, revision: session.revision, cache_status: cached ? "hit" : "miss", skills_report: skillsReport, skill_run: drawingRun });
    }
    case "/api/engineering/tool": {
      const model = required(body, "model");
      const toolCall = required(body, "toolCall");
      const modelJson = JSON.stringify(model);
      const applied = await adapter.engineeringToolsApply([toolCall], modelJson, true);
      cacheParametricDrawing(session, applied.model, applied);
      const snapshot = snapshotFromSolvedPayload(applied.model, applied);
      return sendJson(response, 200, { model: applied.model, ...snapshot, applied_tool_call: toolCall, revision: nextRevision(session) });
    }
    case "/api/engineering/intent": {
      const mode = required(body, "mode");
      const text = required(body, "text");
      const activeEngineeringProvider = engineeringProvider ?? intentService.engineeringProvider();
      if (!activeEngineeringProvider && mode === "edit" && body.model === undefined) {
        const configuration = intentService.publicConfig();
        const unsupported = configuration.provider !== "disabled";
        const code = unsupported ? "SCC-UI-INTENT-016" : "SCC-UI-INTENT-001";
        const message = unsupported ? "The configured intent provider does not support Engineering Tool Calling" : "No intent provider is configured for engineering intent";
        const diagnostic = diagnoseEngineeringError({ code, message }, null);
        throw new ServiceError(501, code, message, {
          failure_class: unsupported ? "ENGINEERING_INTENT_UNSUPPORTED" : "PROVIDER_NOT_CONFIGURED",
          provider: configuration.provider,
          suggestion: unsupported ? "Use an OpenAI-compatible chat-completions endpoint for Engineering Intent" : "Configure an Engineering Intent model profile",
          diagnostic,
        });
      }
      let model = null;
      let baseFingerprint = null;
      if (mode === "edit") {
        model = required(body, "model");
        assertSourceEngineeringModel(model);
        baseFingerprint = fingerprintModel(model);
        if (body.baseFingerprint && body.baseFingerprint !== baseFingerprint) throw staleIntentError();
        if (body.clarification?.base_model_fingerprint && body.clarification.base_model_fingerprint !== baseFingerprint) throw staleIntentError();
      } else if (mode !== "new") {
        throw new ServiceError(422, "SCC-UI-INTENT-015", "Engineering Intent mode must be new or edit");
      }
      const resumedText = mergeClarificationAnswers(text, body.clarification);
      const binding = bindSkillPlan({ mode, text: resumedText, fingerprint: baseFingerprint });
      let stored;
      if (body.skillPlanId) {
        stored = session.skillPlans.consume(body.skillPlanId, binding);
      } else {
        const prepared = await createCadSkillPlan({ adapter, skills: CAD_SKILLS, mode, text: resumedText, model, modelFingerprint: baseFingerprint });
        stored = { plan: prepared.plan, context: prepared.context, manifest: prepared.manifest };
      }
      const skillPlan = stored.plan;
      if (!activeEngineeringProvider) {
        const configuration = intentService.publicConfig();
        const unsupported = configuration.provider !== "disabled";
        const code = unsupported ? "SCC-UI-INTENT-016" : "SCC-UI-INTENT-001";
        const message = unsupported ? "The configured intent provider does not support Engineering Tool Calling" : "No intent provider is configured for engineering intent";
        const diagnostic = diagnoseEngineeringError({ code, message }, model);
        throw new ServiceError(unsupported ? 501 : 501, code, message, {
          failure_class: unsupported ? "ENGINEERING_INTENT_UNSUPPORTED" : "PROVIDER_NOT_CONFIGURED",
          provider: configuration.provider,
          suggestion: unsupported ? "Use an OpenAI-compatible chat-completions endpoint for Engineering Intent" : "Configure an Engineering Intent model profile",
          diagnostic,
          skill_run: completeCadSkillRun(skillPlan, { status: "rejected", error: { code, message }, diagnostic }),
        });
      }
      let result;
      try {
        result = await runIntent({
          mode,
          text: resumedText,
          baseFingerprint,
          modelJson: mode === "edit" ? JSON.stringify(model) : null,
          provider: activeEngineeringProvider,
          cli: adapter,
          toolManifest: stored.manifest,
          allowedToolNames: skillPlan.allowed_tool_names,
          engineeringContext: stored.context,
        });
      } catch (error) {
        const diagnostic = diagnoseEngineeringError(error, model);
        throw new ServiceError(error?.status ?? 502, error?.code ?? "SCC-UI-INTENT-001", error?.message ?? "Engineering Intent failed", {
          ...(error?.details ?? {}),
          diagnostic,
          skill_run: completeCadSkillRun(skillPlan, { status: "rejected", error: { code: error?.code, message: error?.message }, diagnostic }),
        });
      }
      const skillRun = completeCadSkillRun(skillPlan, result);
      if (result.status === "ok") cacheParametricDrawing(session, result.result.model, result.result);
      if (result.status === "rejected") {
        result.diagnostic = diagnoseEngineeringError(result.error, model);
        skillRun.diagnostic = result.diagnostic;
      }
      if (result.status === "needs_clarification") return sendJson(response, 200, { ...result, skill_run: skillRun });
      if (result.status === "ok" && mode === "new") {
        const snapshot = snapshotFromSolvedPayload(result.result.model, result.result);
        return sendJson(response, 200, {
          status: "new_ready",
          mode,
          trace: result.trace,
          skill_run: skillRun,
          model: result.result.model,
          ...snapshot,
          revision: nextRevision(session),
        });
      }
      if (result.status === "ok" && mode === "edit") {
        const baseSnapshot = await workbenchOnlySnapshot(adapter, JSON.stringify(model), model);
        const candidateSnapshot = snapshotFromSolvedPayload(result.result.model, result.result);
        const proposal = buildChangePreview({
          proposalId: randomUUID(),
          baseModel: model,
          candidateModel: result.result.model,
          baseSnapshot,
          candidateSnapshot,
          baseFingerprint,
          candidateFingerprint: candidateSnapshot.fingerprint,
          commands: result.result.tool_calls ?? [],
          intentSummary: text,
        });
        return sendJson(response, 200, {
          status: "preview_ready",
          mode,
          trace: result.trace,
          skill_run: skillRun,
          clarification_request_id: body.clarification?.request_id ?? null,
          revision: session.revision,
          proposal,
        });
      }
      return sendJson(response, 200, { ...result, skill_run: skillRun });
    }
    case "/api/engineering/intent/confirm": {
      assertSameOrigin(request);
      const model = required(body, "model");
      const proposal = required(body, "proposal");
      assertSourceEngineeringModel(model);
      assertChangeProposal(proposal);
      const currentFingerprint = fingerprintModel(model);
      if (currentFingerprint !== proposal.base_model_fingerprint) throw staleIntentError();
      const commands = proposal.plan.commands;
      const applied = await adapter.engineeringToolsApply(commands, JSON.stringify(model), true);
      cacheParametricDrawing(session, applied.model, applied);
      const committedFingerprint = fingerprintModel(applied.model);
      if (committedFingerprint !== proposal.candidate_fingerprint) {
        throw new ServiceError(409, "SCC-UI-INTENT-PREVIEW-002", "The confirmed plan produced a different model than its preview");
      }
      const snapshot = snapshotFromSolvedPayload(applied.model, applied);
      return sendJson(response, 200, {
        status: "committed",
        proposal_id: proposal.proposal_id,
        model: applied.model,
        ...snapshot,
        revision: nextRevision(session),
      });
    }
    default:
      throw new ServiceError(404, "SCC-UI-REQ-002", "API route not found");
  }
}

async function prepareSkillPlan(body, adapter, session) {
  const mode = required(body, "mode");
  const text = required(body, "text");
  let model = null;
  let modelFingerprint = null;
  if (mode === "edit") {
    model = required(body, "model");
    assertSourceEngineeringModel(model);
    modelFingerprint = fingerprintModel(model);
    if (body.baseFingerprint && body.baseFingerprint !== modelFingerprint) throw staleIntentError();
    if (body.clarification?.base_model_fingerprint && body.clarification.base_model_fingerprint !== modelFingerprint) throw staleIntentError();
  } else if (mode !== "new") {
    throw new ServiceError(422, "SCC-UI-INTENT-015", "Engineering Intent mode must be new or edit");
  }
  const effectiveText = mergeClarificationAnswers(text, body.clarification);
  const prepared = await createCadSkillPlan({ adapter, skills: CAD_SKILLS, mode, text: effectiveText, model, modelFingerprint });
  const binding = bindSkillPlan({ mode, text: effectiveText, fingerprint: modelFingerprint });
  const planId = session.skillPlans.put(binding, { plan: prepared.plan, context: prepared.context, manifest: prepared.manifest });
  return { plan_id: planId, plan: prepared.plan };
}

async function readJsonBody(request) {
  if (!request.headers["content-type"]?.startsWith("application/json")) {
    throw new ServiceError(415, "SCC-UI-REQ-003", "Content-Type must be application/json");
  }
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) throw new ServiceError(413, "SCC-UI-REQ-005", "Request body exceeds 6 MiB");
    chunks.push(chunk);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new ServiceError(400, "SCC-UI-REQ-006", "Request body is not valid JSON");
  }
}

function assertSameOrigin(request) {
  const origin = request.headers.origin;
  if (!origin) return;
  let originHost;
  try {
    originHost = new URL(origin).host;
  } catch {
    throw new ServiceError(403, "SCC-UI-INTENT-CONFIG-009", "Intent configuration requires a same-origin request");
  }
  if (originHost !== request.headers.host) {
    throw new ServiceError(403, "SCC-UI-INTENT-CONFIG-009", "Intent configuration requires a same-origin request");
  }
}

async function serveStatic(response, pathname) {
  if (pathname === "/" || pathname === "/index.html") {
    response.writeHead(302, { location: "/engineering.html" });
    response.end();
    return;
  }
  if (pathname.startsWith("/vendor/three/")) {
    return serveFileWithin(response, threeRoot, pathname.slice("/vendor/three/".length));
  }
  const relative = decodeURIComponent(pathname.slice(1));
  return serveFileWithin(response, publicRoot, relative);
}

async function serveFileWithin(response, root, relative) {
  const normalized = normalize(decodeURIComponent(relative));
  const file = resolve(root, normalized);
  if (normalized.startsWith("..") || (!file.startsWith(`${root}${sep}`) && file !== root)) {
    throw new ServiceError(404, "SCC-UI-STATIC-001", "Asset not found");
  }
  try {
    if (!(await stat(file)).isFile()) throw new Error("not a file");
    response.writeHead(200, { "content-type": mimeTypes.get(extname(file)) ?? "application/octet-stream" });
    response.end(await readFile(file));
  } catch {
    if (root === publicRoot && !extname(normalized)) return serveStatic(response, "/");
    throw new ServiceError(404, "SCC-UI-STATIC-001", "Asset not found");
  }
}

function mergeClarificationAnswers(text, clarification) {
  if (!clarification) return text;
  const answers = clarification.answers;
  if (!answers || typeof answers !== "object" || Array.isArray(answers)) {
    throw new ServiceError(400, "SCC-UI-INTENT-CLARIFICATION-001", "Clarification answers must be an object");
  }
  const lines = Object.entries(answers).map(([key, value]) => `${key}: ${String(value)}`);
  if (lines.length === 0) throw new ServiceError(400, "SCC-UI-INTENT-CLARIFICATION-002", "At least one clarification answer is required");
  return `${text}\n\nClarification answers (use these exact values; do not guess other dimensions):\n${lines.join("\n")}`;
}

function staleIntentError() {
  return new ServiceError(409, "SCC-UI-INTENT-STALE-001", "The model changed after this intent was started. Generate a new preview.");
}

function assertChangeProposal(proposal) {
  if (!proposal || typeof proposal !== "object" || Array.isArray(proposal) || proposal.schema_version !== 1) {
    throw new ServiceError(400, "SCC-UI-INTENT-PREVIEW-001", "Invalid change proposal");
  }
  const commands = proposal.plan?.commands;
  if (!Array.isArray(commands) || commands.length === 0 || commands.length > 32) {
    throw new ServiceError(400, "SCC-UI-INTENT-PREVIEW-001", "Change proposal must contain 1 to 32 formal commands");
  }
  if (proposal.plan.base_model_fingerprint !== proposal.base_model_fingerprint) {
    throw new ServiceError(400, "SCC-UI-INTENT-PREVIEW-001", "Change proposal plan fingerprint mismatch");
  }
  for (const command of commands) {
    if (!command || typeof command !== "object" || Array.isArray(command) || typeof command.name !== "string" || !command.name || !command.arguments || typeof command.arguments !== "object" || Array.isArray(command.arguments)) {
      throw new ServiceError(400, "SCC-UI-INTENT-PREVIEW-001", "Change proposal contains a non-formal command");
    }
    if (Object.keys(command).some((key) => !["name", "arguments"].includes(key))) {
      throw new ServiceError(400, "SCC-UI-INTENT-PREVIEW-001", "Change proposal command contains unsupported fields");
    }
  }
}

function assertSourceEngineeringModel(model) {
  if (!model || typeof model !== "object" || Array.isArray(model)) {
    throw new ServiceError(400, "SCC-UI-PROJECT-001", "EngineeringModel must be a JSON object");
  }
  const forbidden = ["geometry", "source_map", "entity_to_feature", "workbench", "solid_build_spec", "step", "step_bytes", "output_path", "artifact_path"];
  const present = forbidden.filter((key) => Object.prototype.hasOwnProperty.call(model, key));
  if (present.length > 0) {
    throw new ServiceError(400, "SCC-UI-PROJECT-002", "Derived CAD data is not accepted as project source", { fields: present });
  }
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function fingerprintModel(model) {
  return createHash("sha256").update(canonicalJson(model)).digest("hex");
}

function safeArtifactStem(value) {
  const stem = String(value ?? "aicad-project")
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^[.-]+|[.-]+$/g, "")
    .slice(0, 80);
  return stem || "aicad-project";
}

function required(object, key) {
  if (object?.[key] === undefined || object?.[key] === null) {
    throw new ServiceError(400, "SCC-UI-REQ-007", `Missing required field: ${key}`);
  }
  return object[key];
}

function setSecurityHeaders(request, response) {
  response.setHeader("content-security-policy", "default-src 'self'; img-src 'self' blob: data:; style-src 'self' 'sha256-7hpnrz8z9c6+UmeJm/kO+iRkDo/025kJEEcZiJNULDI=' 'sha256-qHgu/b59JaXms7/9cg6bNCUOH+dwI1bdAb83CDDgVNU='; script-src 'self' 'sha256-TXQtlLv9hkrPD55l4tCZbV94hPBYXzUd42IGR1ZWUjY=' 'sha256-VMQgk6DTiFomKjXQkc4yyx04ok/B0nF/CMUsIuCBN44=' 'sha256-d3nCfH7osuxLc23ki8WPUrObQkCG6dEFoSTD7VwW1dw=' 'sha256-SRZawATTcCgd5NBsRD7nHXNCHIrU5B+eKI67dmlOmqs=' 'sha256-ATZMNwxl0t5kk6XbQN8m7yiOCNFFTqOp886ynitqFt4='; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'");
  response.setHeader("x-content-type-options", "nosniff");
  response.setHeader("referrer-policy", "no-referrer");
  const host = request.headers.host?.split(":")[0];
  if (request.socket.encrypted || host === "localhost" || host === "127.0.0.1" || host === "[::1]") {
    response.setHeader("cross-origin-opener-policy", "same-origin");
  }
}

function sendJson(response, status, value) {
  response.writeHead(status, { "content-type": "application/json; charset=utf-8" });
  response.end(`${JSON.stringify(value)}\n`);
}

function sendStepArtifact(response, artifact, projectName, modelFingerprint = undefined) {
  const bytes = Buffer.isBuffer(artifact?.bytes) ? artifact.bytes : Buffer.from(artifact?.bytes ?? []);
  const validation = artifact?.status;
  if (bytes.length === 0 || !/^[0-9a-f]{64}$/.test(artifact?.sha256 ?? "") || Number(artifact?.sizeBytes) !== bytes.length
    || !validation || ["manufacturing", "build", "reimport", "equivalence"].some((stage) => validation[stage] !== "passed")) {
    throw new ServiceError(502, "SCC-UI-STEP-003", "STEP artifact identity is invalid");
  }
  const filename = `${safeArtifactStem(projectName)}.step`;
  response.writeHead(200, {
    "content-type": "application/step",
    "content-length": bytes.length,
    "content-disposition": `attachment; filename="${filename}"`,
    "cache-control": "no-store",
    "x-aicad-step-sha256": artifact.sha256,
    "x-aicad-step-size": String(bytes.length),
    "x-aicad-step-manufacturing": validation.manufacturing,
    "x-aicad-step-build": validation.build,
    "x-aicad-step-reimport": validation.reimport,
    "x-aicad-step-equivalence": validation.equivalence,
    ...(modelFingerprint ? { "x-aicad-model-fingerprint": modelFingerprint } : {}),
  });
  response.end(bytes);
}

function sendArtifact(response, artifact) {
  response.writeHead(200, {
    "content-type": artifact.contentType,
    "content-length": artifact.content.length,
    "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(artifact.filename)}`,
    "cache-control": "no-store",
  });
  response.end(artifact.content);
}

function sendError(response, error) {
  const serviceError = error instanceof ServiceError
    ? error
    : new ServiceError(500, "SCC-UI-INTERNAL-001", "Unexpected service error", { cause: error.message });
  sendJson(response, serviceError.status, { error: serviceError.toJSON() });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const binary = await resolveSccBinary(repoRoot);
  const adapter = new SccCliAdapter(binary);
  const host = process.env.SCC_UI_HOST ?? "127.0.0.1";
  const port = Number.parseInt(process.env.SCC_UI_PORT ?? "4173", 10);
  const server = createApp({ adapter });
  server.listen(port, host, () => {
    console.log(`SCC UI listening on http://${host}:${port}`);
    console.log(binary ? `SCC binary: ${binary}` : "SCC binary unavailable; build scc-cli before running the pipeline");
  });
}
