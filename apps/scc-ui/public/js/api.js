export class ApiError extends Error {
  constructor(status, payload) {
    super(payload?.message ?? `Request failed with HTTP ${status}`);
    this.name = "ApiError";
    this.status = status;
    this.code = payload?.code ?? "SCC-UI-CLIENT-001";
    this.details = payload?.details;
  }
}

async function request(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: options.body ? { "content-type": "application/json", ...options.headers } : options.headers,
  });
  let payload;
  try {
    payload = await response.json();
  } catch {
    throw new ApiError(response.status, { message: "The local service returned an unreadable response" });
  }
  if (!response.ok) throw new ApiError(response.status, payload.error);
  return payload;
}

const post = (path, body) => request(path, { method: "POST", body: JSON.stringify(body) });
const patch = (path, body) => request(path, { method: "PATCH", body: JSON.stringify(body) });
const remove = (path) => request(path, { method: "DELETE" });

export const api = {
  health: () => request("/api/health"),
  intentConfig: () => request("/api/intent-config"),
  updateIntentConfig: (config) => patch("/api/intent-config", config),
  modelPresets: () => request("/api/model-presets"),
  modelConfigs: () => request("/api/model-configs"),
  addModelConfig: (config) => post("/api/model-configs", config),
  editModelConfig: (id, config) => patch(`/api/model-configs/${encodeURIComponent(id)}`, config),
  deleteModelConfig: (id) => remove(`/api/model-configs/${encodeURIComponent(id)}`),
  activateModelConfig: (id) => post(`/api/model-configs/${encodeURIComponent(id)}/activate`, {}),
  testModelConnection: (id) => post(`/api/model-configs/${encodeURIComponent(id)}/test-connection`, {}),
  testModelEngineering: (id) => post(`/api/model-configs/${encodeURIComponent(id)}/test-engineering`, {}),
  examples: () => request("/api/examples"),
  example: (id) => request(`/api/examples/${encodeURIComponent(id)}`),
  intent: (intent, documents, currentProposal) => post("/api/intent", { intent, documents, currentProposal }),
  cadIntent: (intent, documents, currentCadSpec) => post("/api/cad/intent", { intent, documents, currentCadSpec }),
  cadExample: () => request("/api/cad/examples/generic-flange"),
  cadBuild: (cadSpec) => post("/api/cad/build", { cadSpec }),
  cadArtifactUrl: (jobId, name) => `/api/cad/artifacts/${encodeURIComponent(jobId)}/${encodeURIComponent(name)}`,
  deleteCadJob: (jobId) => request(`/api/cad/artifacts/${encodeURIComponent(jobId)}`, { method: "DELETE" }),
  compile: (proposal) => post("/api/compile", { proposal }),
  execute: (ir, mutation) => post("/api/execute", { ir, mutation }),
  inspect: (ir) => post("/api/inspect", { ir }),
  solve: (state) => post("/api/solve", { state }),
  export: (geometry, format) => post("/api/export", { geometry, format }),
  extractDocument: (document) => post("/api/documents/extract", document),
  engineeringDsl: (dsl) => post("/api/engineering/dsl", { dsl }),
  engineeringWorkbench: (model) => post("/api/engineering/workbench", { model }),
  engineeringDrawing: (model) => post("/api/engineering/drawing", { model }),
  engineeringThreeViewDrawing: (model, userIntent = null) => post("/api/engineering/drawing/three-view", { model, userIntent }),
  engineeringSectionDrawing: (model, sectionPlane) => post("/api/engineering/drawing/section", { model, sectionPlane }),
  engineeringTool: (model, toolCall) => post("/api/engineering/tool", { model, toolCall }),
  engineeringSkillPlan: (mode, text, model, baseFingerprint, clarification = undefined) => post("/api/engineering/skills/plan", { mode, text, model, baseFingerprint, clarification }),
  engineeringIntentPreview: (mode, text, model, baseFingerprint, clarification = undefined, skillPlanId = undefined) => post("/api/engineering/intent", { mode, text, model, baseFingerprint, clarification, skillPlanId }),
  cadSkills: () => request("/api/cad-skills"),
  engineeringIntentConfirm: (model, proposal) => post("/api/engineering/intent/confirm", { model, proposal }),
  engineeringProjectSave: (model, name, description) => post("/api/engineering/project/save", { model, name, description }),
  engineeringProjectOpen: (project) => post("/api/engineering/project/open", { project }),
  engineeringExportStep: async (model, projectName) => {
    const response = await fetch("/api/engineering/export/step", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model, projectName }),
    });
    if (!response.ok) {
      let payload = {};
      try { payload = await response.json(); } catch {}
      throw new ApiError(response.status, payload.error ?? { message: "STEP export failed" });
    }
    const bytes = await response.arrayBuffer();
    return {
      blob: new Blob([bytes], { type: response.headers.get("content-type") ?? "application/step" }),
      sha256: response.headers.get("x-aicad-step-sha256"),
      sizeBytes: Number(response.headers.get("x-aicad-step-size") ?? bytes.byteLength),
      status: {
        manufacturing: response.headers.get("x-aicad-step-manufacturing"),
        build: response.headers.get("x-aicad-step-build"),
        reimport: response.headers.get("x-aicad-step-reimport"),
        equivalence: response.headers.get("x-aicad-step-equivalence"),
      },
      modelFingerprint: response.headers.get("x-aicad-model-fingerprint"),
      filename: response.headers.get("content-disposition")?.match(/filename="([^"]+)"/)?.[1] ?? "aicad-project.step",
    };
  },
};
