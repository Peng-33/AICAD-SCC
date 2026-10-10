import { ServiceError } from "./scc-cli.mjs";
import { randomUUID } from "node:crypto";
import { createOpenAiCompatibleProvider, probeOpenAiCompatibleProvider } from "./provider-openai-compatible.mjs";

const PROVIDERS = new Set(["disabled", "openai-compatible", "scc"]);
const MAX_ENDPOINT_LENGTH = 2048;
const MAX_MODEL_LENGTH = 200;
const MAX_API_KEY_LENGTH = 8192;
const MAX_DOCUMENTS = 8;
const MAX_DOCUMENT_CHARACTERS = 12_000;
const MAX_PROMPT_CHARACTERS = 120_000;
const MAX_REPAIR_CONTEXT_CHARACTERS = 30_000;

const SYSTEM_PROMPT = `You convert design intent into Semantic Constraint Compiler Proposal JSON.
Return exactly one JSON object and no Markdown:
{"proposal":{"version":"1.0","entities":[],"relations":[],"dimensions":[],"arrays":[],"anchors":[],"metadata":{}},"summary":"short explanation","mutation":[]}

SCC Proposal 1.0 is strict. Use these exact fields and lowercase enum values:
- entity: {"id":string,"kind":"point"|"line"|"circle"|"arc"|"rectangle"|"polygon"}
- relation: {"kind":"centered_in"|"inside"|"outside"|"parallel_to"|"perpendicular_to"|"tangent_to"|"symmetric_about"|"aligned_with"|"distance","subject":entity_id,"reference":entity_id,"axis"?:"x"|"y"|"both","value"?:number}
- dimension: {"target":entity_id,"kind":"diameter"|"radius"|"width"|"height"|"length"|"offset","value":number}
- linear array: {"id":string,"kind":"linear","count":positive_integer,"spacing":nonnegative_number,"element":entity_id,"direction"?:"x"|"y"}
- circular array: {"id":string,"kind":"circular","count":positive_integer,"radius":positive_number,"element":entity_id}
- grid array: {"id":string,"kind":"grid","count":rows*cols,"rows":positive_integer,"cols":positive_integer,"row_spacing":nonnegative_number,"col_spacing":nonnegative_number,"element":entity_id}
- anchor: {"id":string,"kind":"origin_point"} or {"id":string,"kind":"fixed_center"|"reference_edge"|"datum_line","entity":entity_id}
- mutation operation: {"UpdateParameter":{"target_id":string,"parameter":string,"value":number}}, {"SuppressConstraint":{"target_id":string}}, {"UnsuppressConstraint":{"target_id":string}}, or {"ChangeAuthority":{"target_id":string,"new_authority":"L0"|"L1"|"L2"|"L3"}}

All top-level Proposal fields are required. IDs must be stable and unique. Every relation endpoint, dimension target, array element, and non-origin anchor entity must reference an entity declared in entities. Use dimensions plus a fixed_center anchor when the requested model should be geometrically solvable. Polygon vertex_count and x_N/y_N coordinates belong in UpdateParameter mutations.

The current M3 solver supports centered_in, parallel_to, perpendicular_to, aligned_with, and distance relations when their entity kinds and parameters are valid. Do not emit inside, outside, tangent_to, or symmetric_about: M1 can parse them, but the current M3 solver cannot solve their lowered On, Tangent, or Symmetric constraints. For a renderable hole in a plate, use centered_in with dimensions and fixed_center anchors instead of inside.

SCC currently produces 2D top-view geometry, not 3D solids or manufacturing drawings. For a rich mechanical specification, model only the supported 2D top-view geometry. Preserve thicknesses, heights, through-cut semantics, chamfers, fillets, fits, tolerances, GD&T, datums, surface roughness, section-view instructions, hatching, and other unsupported requirements as strings or structured values under proposal.metadata.unmodeled_requirements. Set proposal.metadata.representation to "2D top view" and make the summary explicitly state that these requirements were preserved but not geometrically enforced. Never claim that metadata-only requirements were solved.

Never use type, properties, coordinates, relation id/entities arrays, or dimension id fields. A valid fixed circle example is:
{"proposal":{"version":"1.0","entities":[{"id":"circle","kind":"circle"}],"relations":[],"dimensions":[{"target":"circle","kind":"radius","value":10}],"arrays":[],"anchors":[{"id":"circle_center","kind":"fixed_center","entity":"circle"}],"metadata":{}},"summary":"Created a fixed circle","mutation":[]}

Preserve useful valid objects from currentProposal when the user asks for a modification, but repair non-SCC fields. Do not invent execution results, ConstraintIR, GraphState, or solved geometry. The SCC M1→M2→M3 preflight is authoritative for supported output.`;

const CAD_SYSTEM_PROMPT = `You convert mechanical design intent into an AICAD CadDocument 1.0 FeatureGraph for a real OCCT solid builder.
Return exactly one JSON object and no Markdown:
{"cadSpec":{"version":"1.0","kind":"feature_graph_v1","name":"stable-part-name","units":"mm","parameters":{},"features":[],"requirements":[],"manufacturing":{"fits":[],"tolerances":[],"datums":[],"geometric_controls":[],"surface_finishes":[],"threads":[]},"assumptions":[],"unresolved":[]},"summary":"short explanation"}

Never emit executable code, Python, CadQuery, FreeCAD commands, shell, JavaScript, or Markdown. CadDocument is data only. Use millimetres and exact numeric values supplied by the user.

Each parameters entry is {"value":number,"source":"user"|"derived"|"assumption","description"?:string}. A geometric scalar is either a number or {"parameter":"parameter_id"}. IDs are lowercase stable identifiers using letters, digits, underscores, or hyphens.

Supported ordered features are:
- box: {"id", "type":"box", "operation":"add"|"cut", "size":{"x":scalar,"y":scalar,"z":scalar}, "origin":{"x":scalar,"y":scalar,"z":scalar}}
- cylinder: {"id", "type":"cylinder", "operation":"add"|"cut", "diameter":scalar, "length":scalar, "axis":"x"|"y"|"z", "origin":{"x":scalar,"y":scalar,"z":scalar}}
- circular_hole_pattern: {"id", "type":"circular_hole_pattern", "operation":"cut", "diameter":scalar, "length":scalar, "count":integer, "pitch_diameter":scalar, "start_angle":number, "axis":"x"|"y"|"z", "origin":{"x":scalar,"y":scalar,"z":scalar}}
The first active feature must add material. Every later add must touch the existing solid. Every cut must remove material. Do not approximate unsupported geometry with a misleading primitive.

Every distinct user requirement must have one requirements entry with a stable id, original concise text, and exactly one disposition:
- geometry with feature_ids that exist;
- manufacturing with a manufacturing_ref such as "fits[0]";
- derived with parameter_ids;
- assumption with assumption_ref;
- unresolved with unresolved_ref;
- unsupported with a non-empty reason.
Never silently omit intent. Put fits, tolerances, datum definitions, GD&T, surface roughness, and ISO thread designations into the corresponding manufacturing arrays and reference them. Drawing views, semantic threads, fillets, chamfers, free-form profiles, shells, and other operations outside the supported feature list must be marked unsupported, not claimed as built.

Assumption entries use {"id":string,"severity":"assumption","field":string,"message":string}; unresolved entries use {"id":string,"severity":"critical"|"warning","field":string,"message":string}. If a dimension or position is ambiguous, record it explicitly. Preserve and update valid IDs from currentCadSpec when the user asks for a modification.

Return the complete CadDocument in cadSpec on every response. Never invent build results, files, volume, validity, requirement coverage, or previews; the local validator and OCCT worker are authoritative.`;

export function createIntentService({
  intentEndpoint = process.env.SCC_INTENT_ENDPOINT,
  intentProvider = process.env.SCC_INTENT_PROVIDER,
  intentModel = process.env.SCC_INTENT_MODEL,
  intentApiKey = process.env.SCC_INTENT_API_KEY,
  fetchImpl = globalThis.fetch,
  validateProposal,
  validateCadSpec,
} = {}) {
  let config = initialConfiguration({ intentEndpoint, intentProvider, intentModel, intentApiKey });
  const profiles = new Map();
  let activeProfileId = null;
  if (isConfigured(config)) {
    const profile = {
      id: randomUUID(), name: inferredProfileName(config.endpoint), type: config.provider === "scc" ? "scc" : inferProfileType(config.endpoint),
      endpoint: config.endpoint, model: config.model, apiKey: config.apiKey,
      authMode: requiresBearer(config.provider === "scc" ? "scc" : inferProfileType(config.endpoint)) || config.apiKey ? "bearer" : "none",
      connection: { status: "saved" }, engineering: { status: "saved" },
    };
    profiles.set(profile.id, profile);
    activeProfileId = profile.id;
  }
  const profileView = (profile) => ({
    id: profile.id, name: profile.name, type: profile.type,
    endpoint: profile.endpoint, model: profile.model, authMode: profile.authMode,
    hasApiKey: Boolean(profile.apiKey), active: profile.id === activeProfileId,
    configured: profileComplete(profile),
    status: profileComplete(profile) ? "saved" : "unconfigured",
    connection: { ...profile.connection }, engineering: { ...profile.engineering },
    ephemeralSecret: true,
  });
  const getProfile = (id) => {
    const profile = profiles.get(id);
    if (!profile) throw new ServiceError(404, "SCC-UI-MODEL-001", "Model configuration not found");
    return profile;
  };
  const providerFor = (profile) => {
    assertProfileComplete(profile);
    return createOpenAiCompatibleProvider({
      endpoint: profile.endpoint, model: profile.model,
      apiKey: profile.authMode === "bearer" ? profile.apiKey : "", fetchImpl,
    });
  };
  const proposalValidator = typeof validateProposal === "function" ? validateProposal : async () => {};
  const cadSpecValidator = typeof validateCadSpec === "function" ? validateCadSpec : async (candidate) => candidate;

  return {
    publicConfig: () => publicConfiguration(config),
    engineeringProvider() {
      if (!isConfigured(config) || config.provider !== "openai-compatible") return null;
      return createOpenAiCompatibleProvider({
        endpoint: config.endpoint,
        model: config.model,
        apiKey: config.apiKey,
        fetchImpl,
      });
    },
    update(input) {
      config = updateConfiguration(config, input);
      if (activeProfileId) {
        const profile = getProfile(activeProfileId);
        Object.assign(profile, {
          endpoint: config.endpoint, model: config.model, apiKey: config.apiKey,
          type: config.provider === "scc" ? "scc" : inferProfileType(config.endpoint),
          authMode: requiresBearer(config.provider === "scc" ? "scc" : inferProfileType(config.endpoint)) || config.apiKey ? "bearer" : "none",
          connection: { status: "saved" }, engineering: { status: "saved" },
        });
      } else if (isConfigured(config)) {
        const profile = { id: randomUUID(), name: inferredProfileName(config.endpoint), type: config.provider === "scc" ? "scc" : inferProfileType(config.endpoint),
          endpoint: config.endpoint, model: config.model, apiKey: config.apiKey,
          authMode: requiresBearer(config.provider === "scc" ? "scc" : inferProfileType(config.endpoint)) || config.apiKey ? "bearer" : "none",
          connection: { status: "saved" }, engineering: { status: "saved" } };
        profiles.set(profile.id, profile);
        activeProfileId = profile.id;
      }
      if (config.provider === "disabled") activeProfileId = null;
      return publicConfiguration(config);
    },
    listProfiles() {
      return { activeId: activeProfileId, profiles: [...profiles.values()].map(profileView), ephemeralSecret: true };
    },
    addProfile(input) {
      const profile = validateProfileInput(input);
      profiles.set(profile.id, profile);
      return profileView(profile);
    },
    editProfile(id, input) {
      const current = getProfile(id);
      const next = validateProfileInput(input, current);
      if (id === activeProfileId) assertProfileComplete(next);
      profiles.set(id, next);
      if (id === activeProfileId) config = profileToConfig(next);
      return profileView(next);
    },
    deleteProfile(id) {
      getProfile(id);
      profiles.delete(id);
      if (id === activeProfileId) {
        activeProfileId = null;
        config = initialConfiguration({ intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
      }
      return this.listProfiles();
    },
    activateProfile(id) {
      const profile = getProfile(id);
      assertProfileComplete(profile);
      activeProfileId = id;
      config = profileToConfig(profile);
      return this.listProfiles();
    },
    providerForProfile(id) { return providerFor({ ...getProfile(id) }); },
    async testProfileConnection(id) {
      const profile = getProfile(id);
      assertProfileComplete(profile);
      profile.connection = { status: "testing" };
      const snapshot = { ...profile };
      try {
        await probeOpenAiCompatibleProvider({ endpoint: snapshot.endpoint, model: snapshot.model,
          apiKey: snapshot.authMode === "bearer" ? snapshot.apiKey : "", fetchImpl });
        if (profiles.get(id) === profile) profile.connection = { status: "connected" };
      } catch (error) {
        if (profiles.get(id) === profile) profile.connection = { status: providerTestStatus(error), code: error.code ?? "SCC-UI-PROVIDER-001" };
      }
      return profileView(getProfile(id));
    },
    async testProfileEngineering(id, check) {
      const profile = getProfile(id);
      assertProfileComplete(profile);
      profile.engineering = { status: "testing" };
      const provider = providerFor({ ...profile });
      try {
        await check(provider);
        if (profiles.get(id) === profile) profile.engineering = { status: "connected" };
      } catch (error) {
        if (profiles.get(id) === profile) {
          const transportStatus = providerTestStatus(error);
          const status = error?.code === "SCC-UI-PROVIDER-002" ||
            (transportStatus === "unreachable" && error?.code !== "SCC-UI-PROVIDER-001")
            ? "protocol_failed" : transportStatus;
          profile.engineering = { status, code: error.code ?? "SCC-UI-MODEL-006" };
        }
      }
      return profileView(getProfile(id));
    },
    async compile(body) {
      if (!isConfigured(config)) {
        throw new ServiceError(501, "SCC-UI-INTENT-001", "No intent compiler is configured", {
          failure_class: "PROVIDER_NOT_CONFIGURED",
          suggestion: "Configure an intent provider in Settings or use a structured Proposal template",
        });
      }
      return config.provider === "openai-compatible"
        ? compileWithOpenAiCompatible(fetchImpl, config, body, proposalValidator)
        : compileWithSccEndpoint(fetchImpl, config, body, proposalValidator);
    },
    async compileCad(body) {
      assertConfigured(config, "Configure an intent provider in Settings before generating 3D CAD");
      return config.provider === "openai-compatible"
        ? compileCadWithOpenAiCompatible(fetchImpl, config, body, cadSpecValidator)
        : compileCadWithSccEndpoint(fetchImpl, config, body, cadSpecValidator);
    },
  };
}

const PROFILE_TYPES = new Set(["deepseek", "qwen", "openai-compatible", "local", "scc"]);
function inferProfileType(endpoint) {
  try {
    const host = new URL(endpoint).hostname.toLowerCase();
    if (host === "api.deepseek.com") return "deepseek";
    if (host.endsWith(".aliyuncs.com")) return "qwen";
    if (new Set(["localhost", "127.0.0.1", "::1", "[::1]"]).has(host)) return "local";
  } catch { /* Existing configuration validation reports invalid URLs. */ }
  return "openai-compatible";
}
function inferredProfileName(endpoint) {
  const type = inferProfileType(endpoint);
  return type === "deepseek" ? "DeepSeek (current)" : type === "qwen" ? "Qwen (current)" : "Current provider";
}
const AUTH_MODES = new Set(["bearer", "none"]);

function validateProfileInput(input, current = null) {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new ServiceError(400, "SCC-UI-MODEL-002", "Model configuration must be an object");
  }
  const name = boundedString(input.name ?? current?.name ?? "", "name", 100);
  if (!name) throw new ServiceError(422, "SCC-UI-MODEL-003", "Configuration name is required");
  const type = input.type ?? current?.type ?? "openai-compatible";
  if (!PROFILE_TYPES.has(type)) throw new ServiceError(422, "SCC-UI-MODEL-004", "Unsupported provider type");
  const authMode = input.authMode ?? current?.authMode ?? (type === "local" ? "none" : "bearer");
  if (!AUTH_MODES.has(authMode)) throw new ServiceError(422, "SCC-UI-MODEL-005", "Authentication mode must be bearer or none");
  if ((type === "deepseek" || type === "qwen") && authMode !== "bearer") {
    throw new ServiceError(422, "SCC-UI-MODEL-005", "This cloud provider requires Bearer API Key authentication");
  }
  const endpoint = boundedString(input.endpoint ?? current?.endpoint ?? "", "endpoint", MAX_ENDPOINT_LENGTH);
  const model = boundedString(input.model ?? current?.model ?? "", "model", MAX_MODEL_LENGTH);
  const apiKey = input.clearApiKey ? "" : input.apiKey ? boundedString(input.apiKey, "API key", MAX_API_KEY_LENGTH, false) : current?.apiKey ?? "";
  if (endpoint) validateEndpoint(endpoint, apiKey);
  return { id: current?.id ?? randomUUID(), name, type, authMode, endpoint, model, apiKey,
    connection: { status: "saved" }, engineering: { status: "saved" } };
}

function requiresBearer(type) { return type === "deepseek" || type === "qwen"; }

function profileComplete(profile) {
  return Boolean(profile.endpoint && profile.model && ((!requiresBearer(profile.type) && profile.authMode !== "bearer") || profile.apiKey));
}

function assertProfileComplete(profile) {
  if (profile.type === "scc") throw new ServiceError(422, "SCC-UI-INTENT-016", "SCC Intent API does not support Engineering Tool Calling");
  if (!profile.endpoint || !profile.model) throw new ServiceError(422, "SCC-UI-MODEL-007", "Endpoint and Model ID are required before activation or testing");
  if ((profile.authMode === "bearer" || requiresBearer(profile.type)) && !profile.apiKey) throw new ServiceError(422, "SCC-UI-MODEL-008", "This configuration requires an API key");
}

function profileToConfig(profile) {
  return validateConfiguration({ provider: profile.type === "scc" ? "scc" : "openai-compatible",
    endpoint: profile.endpoint, model: profile.model, apiKey: profile.authMode === "bearer" ? profile.apiKey : "", source: "runtime" });
}

function providerTestStatus(error) {
  if (error?.code === "SCC-UI-PROVIDER-003") return "auth_failed";
  if (error?.code === "SCC-UI-PROVIDER-004") return "model_unavailable";
  if (error?.code === "SCC-UI-PROVIDER-005") return "timeout";
  return "unreachable";
}

function assertConfigured(config, suggestion) {
  if (!isConfigured(config)) {
    throw new ServiceError(501, "SCC-UI-INTENT-001", "No intent compiler is configured", {
      failure_class: "PROVIDER_NOT_CONFIGURED",
      suggestion,
    });
  }
}

function initialConfiguration({ intentEndpoint, intentProvider, intentModel, intentApiKey }) {
  const provider = intentProvider || (intentEndpoint ? "scc" : "disabled");
  return validateConfiguration({
    provider,
    endpoint: intentEndpoint ?? "",
    model: intentModel ?? "",
    apiKey: intentApiKey ?? "",
    source: intentEndpoint || intentProvider || intentModel || intentApiKey ? "environment" : "default",
  });
}

function updateConfiguration(current, input) {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new ServiceError(400, "SCC-UI-INTENT-CONFIG-001", "Intent configuration must be a JSON object");
  }
  const apiKey = input.clearApiKey
    ? ""
    : typeof input.apiKey === "string" && input.apiKey.length > 0 ? input.apiKey : current.apiKey;
  return validateConfiguration({
    provider: input.provider ?? current.provider,
    endpoint: input.endpoint ?? current.endpoint,
    model: input.model ?? current.model,
    apiKey,
    source: "runtime",
  });
}

function validateConfiguration(input) {
  const provider = String(input.provider ?? "");
  if (!PROVIDERS.has(provider)) {
    throw new ServiceError(422, "SCC-UI-INTENT-CONFIG-002", "Intent provider must be disabled, openai-compatible, or scc");
  }
  const endpoint = boundedString(input.endpoint, "endpoint", MAX_ENDPOINT_LENGTH);
  const model = boundedString(input.model, "model", MAX_MODEL_LENGTH);
  const apiKey = boundedString(input.apiKey, "API key", MAX_API_KEY_LENGTH, false);
  if (provider !== "disabled" && !endpoint) {
    throw new ServiceError(422, "SCC-UI-INTENT-CONFIG-003", "Intent API endpoint is required");
  }
  if (provider === "openai-compatible" && !model) {
    throw new ServiceError(422, "SCC-UI-INTENT-CONFIG-004", "Model is required for an OpenAI-compatible provider");
  }
  if (endpoint) validateEndpoint(endpoint, apiKey);
  return { provider, endpoint, model, apiKey, source: input.source };
}

function boundedString(value, label, maximum, trim = true) {
  if (typeof value !== "string") {
    throw new ServiceError(422, "SCC-UI-INTENT-CONFIG-005", `${label} must be a string`);
  }
  const result = trim ? value.trim() : value;
  if (result.length > maximum) {
    throw new ServiceError(422, "SCC-UI-INTENT-CONFIG-006", `${label} is too long`);
  }
  return result;
}

function validateEndpoint(endpoint, apiKey) {
  let url;
  try {
    url = new URL(endpoint);
  } catch {
    throw new ServiceError(422, "SCC-UI-INTENT-CONFIG-007", "Intent API endpoint must be a valid URL");
  }
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) {
    throw new ServiceError(422, "SCC-UI-INTENT-CONFIG-007", "Intent API endpoint must be an HTTP(S) URL without embedded credentials");
  }
  if (apiKey && url.protocol !== "https:" && !isLoopbackHost(url.hostname)) {
    throw new ServiceError(422, "SCC-UI-INTENT-CONFIG-008", "API keys require HTTPS unless the endpoint is local");
  }
}

function isLoopbackHost(hostname) {
  return new Set(["localhost", "127.0.0.1", "::1", "[::1]"]).has(hostname.toLowerCase());
}

function publicConfiguration(config) {
  return {
    provider: config.provider,
    endpoint: config.endpoint,
    model: config.model,
    hasApiKey: Boolean(config.apiKey),
    configured: isConfigured(config),
    source: config.source,
    ephemeralSecret: true,
  };
}

function isConfigured(config) {
  return config.provider !== "disabled" && Boolean(config.endpoint)
    && (config.provider !== "openai-compatible" || Boolean(config.model));
}

async function compileWithSccEndpoint(fetchImpl, config, body, validateProposal) {
  const response = await requestUpstream(fetchImpl, config.endpoint, {
    headers: providerHeaders(config),
    body: JSON.stringify({ ...normalizedIntentBody(body), ...(config.model ? { model: config.model } : {}) }),
  });
  const result = validateIntentResult(await readUpstreamJson(response));
  await validateCandidate(result, validateProposal);
  return result;
}

async function compileWithOpenAiCompatible(fetchImpl, config, body, validateProposal) {
  const normalized = normalizedIntentBody(body);
  const userContent = JSON.stringify(normalized);
  if (userContent.length > MAX_PROMPT_CHARACTERS) {
    throw new ServiceError(413, "SCC-UI-INTENT-005", "Intent context is too large for the configured model");
  }
  let repair;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    let result;
    try {
      result = await requestOpenAiCandidate(fetchImpl, config, userContent, repair);
    } catch (error) {
      if (attempt === 0 && error?.code === "SCC-UI-INTENT-003") {
        repair = { feedback: error.message };
        continue;
      }
      throw error;
    }
    try {
      await validateProposal(result.proposal, result.mutation);
      return result;
    } catch (error) {
      if (error?.status !== 422) throw error;
      if (attempt === 0 && error?.status === 422) {
        repair = { previousResult: result, feedback: summarizeValidationError(error) };
        continue;
      }
      throw rejectedProposalError(error);
    }
  }
  throw new ServiceError(502, "SCC-UI-INTENT-006", "Intent model returned a Proposal rejected by the SCC pipeline");
}

async function compileCadWithSccEndpoint(fetchImpl, config, body, validateCadSpec) {
  const response = await requestUpstream(fetchImpl, config.endpoint, {
    headers: providerHeaders(config),
    body: JSON.stringify({ ...normalizedIntentBody(body), target: "cad_document_1.0", ...(config.model ? { model: config.model } : {}) }),
  });
  const result = validateCadIntentResult(await readUpstreamJson(response));
  result.cadSpec = await validateCadSpec(result.cadSpec);
  return result;
}

async function compileCadWithOpenAiCompatible(fetchImpl, config, body, validateCadSpec) {
  const userContent = JSON.stringify(normalizedIntentBody(body));
  if (userContent.length > MAX_PROMPT_CHARACTERS) {
    throw new ServiceError(413, "SCC-UI-INTENT-005", "Intent context is too large for the configured model");
  }
  let repair;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const result = await requestOpenAiCadCandidate(fetchImpl, config, userContent, repair);
    try {
      result.cadSpec = await validateCadSpec(result.cadSpec);
      return result;
    } catch (error) {
      if (error?.status !== 422) throw error;
      if (attempt === 0) {
        repair = { previousResult: result, feedback: summarizeValidationError(error) };
        continue;
      }
      throw new ServiceError(502, "SCC-UI-INTENT-007", "Intent model returned a CadDocument rejected by the 3D contract", {
        diagnostics: [{ code: error.code, message: error.message }],
      });
    }
  }
  throw new ServiceError(502, "SCC-UI-INTENT-007", "Intent model returned an invalid CadDocument");
}

async function requestOpenAiCandidate(fetchImpl, config, userContent, repair) {
  const response = await requestUpstream(fetchImpl, chatCompletionsUrl(config.endpoint), {
    headers: providerHeaders(config),
    body: JSON.stringify({
      model: config.model,
      temperature: 0,
      response_format: { type: "json_object" },
      messages: modelMessages(SYSTEM_PROMPT, userContent, repair, "SCC pipeline diagnostics"),
    }),
  });
  const upstream = await readUpstreamJson(response);
  const content = upstream?.choices?.[0]?.message?.content;
  if (typeof content !== "string" && !Array.isArray(content)) {
    throw new ServiceError(502, "SCC-UI-INTENT-003", "Intent model response is missing message content");
  }
  return validateIntentResult(parseModelJson(content));
}

async function requestOpenAiCadCandidate(fetchImpl, config, userContent, repair) {
  const response = await requestUpstream(fetchImpl, chatCompletionsUrl(config.endpoint), {
    headers: providerHeaders(config),
    body: JSON.stringify({
      model: config.model,
      temperature: 0,
      response_format: { type: "json_object" },
      messages: modelMessages(CAD_SYSTEM_PROMPT, userContent, repair, "CadSpec diagnostics"),
    }),
  });
  const upstream = await readUpstreamJson(response);
  const content = upstream?.choices?.[0]?.message?.content;
  if (typeof content !== "string" && !Array.isArray(content)) {
    throw new ServiceError(502, "SCC-UI-INTENT-003", "Intent model response is missing message content");
  }
  return validateCadIntentResult(parseModelJson(content));
}

function modelMessages(systemPrompt, userContent, repair, diagnosticLabel) {
  const messages = [
    { role: "system", content: systemPrompt },
    { role: "user", content: userContent },
  ];
  if (!repair) return messages;
  if (repair.previousResult) {
    messages.push({
      role: "assistant",
      content: JSON.stringify(repair.previousResult).slice(0, MAX_REPAIR_CONTEXT_CHARACTERS),
    });
  }
  messages.push({
    role: "user",
    content: `The previous response was invalid. Correct it and return the complete JSON object again. ${diagnosticLabel}: ${repair.feedback}`,
  });
  return messages;
}

async function validateCandidate(result, validateProposal) {
  try {
    await validateProposal(result.proposal, result.mutation);
  } catch (error) {
    if (error?.status !== 422) throw error;
    throw rejectedProposalError(error);
  }
}

function rejectedProposalError(error) {
  return new ServiceError(502, "SCC-UI-INTENT-006", "Intent model returned a Proposal rejected by the SCC pipeline", {
    diagnostics: error?.details?.diagnostics ?? [{ code: error?.code ?? "SCC-UI-INTENT-006", message: error?.message ?? "Unknown SCC pipeline validation error" }],
    suggestion: "Rephrase the design intent with solver-supported constraints or start from a verified template",
  });
}

function summarizeValidationError(error) {
  const messages = [];
  collectDiagnosticMessages(error?.details?.diagnostics, messages);
  if (!messages.length && error?.message) messages.push(error.message);
  return messages.slice(0, 8).join("; ").slice(0, 4_000) || "Proposal rejected by the SCC pipeline";
}

function collectDiagnosticMessages(value, messages) {
  if (!value || messages.length >= 8) return;
  if (Array.isArray(value)) {
    for (const item of value) collectDiagnosticMessages(item, messages);
    return;
  }
  if (typeof value !== "object") return;
  if (typeof value.message === "string") {
    const prefix = [value.code, value.path].filter(Boolean).join(" ");
    messages.push(`${prefix ? `${prefix}: ` : ""}${value.message}`);
  }
  if (value.errors) collectDiagnosticMessages(value.errors, messages);
}

function normalizedIntentBody(body) {
  if (!body || typeof body.intent !== "string" || !body.intent.trim()) {
    throw new ServiceError(400, "SCC-UI-INTENT-004", "Design intent is required");
  }
  const documents = Array.isArray(body.documents) ? body.documents.slice(0, MAX_DOCUMENTS).map((document) => ({
    name: String(document?.name ?? "document").slice(0, 200),
    type: String(document?.type ?? "text").slice(0, 100),
    content: String(document?.content ?? "").slice(0, MAX_DOCUMENT_CHARACTERS),
  })) : [];
  return {
    intent: body.intent.trim(),
    documents,
    currentProposal: body.currentProposal && typeof body.currentProposal === "object" ? body.currentProposal : undefined,
    currentCadSpec: body.currentCadSpec && typeof body.currentCadSpec === "object" ? body.currentCadSpec : undefined,
  };
}

function providerHeaders(config) {
  return {
    "content-type": "application/json",
    ...(config.apiKey ? { authorization: `Bearer ${config.apiKey}` } : {}),
  };
}

async function requestUpstream(fetchImpl, endpoint, options) {
  let response;
  try {
    response = await fetchImpl(endpoint, { method: "POST", ...options, signal: AbortSignal.timeout(60_000) });
  } catch (error) {
    throw new ServiceError(502, "SCC-UI-INTENT-002", "Intent API could not be reached", {
      failure_class: "PROVIDER_UNREACHABLE",
      cause: error.message,
    });
  }
  if (!response.ok) {
    if (response.status === 401 || response.status === 403) {
      throw new ServiceError(502, "SCC-UI-PROVIDER-003", `Intent API authentication failed (HTTP ${response.status})`, {
        failure_class: "PROVIDER_AUTH_FAILED",
        upstreamStatus: response.status,
      });
    }
    throw new ServiceError(502, "SCC-UI-INTENT-002", `Intent API returned HTTP ${response.status}`, {
      failure_class: "PROVIDER_UNREACHABLE",
      upstreamStatus: response.status,
    });
  }
  return response;
}

async function readUpstreamJson(response) {
  try {
    return await response.json();
  } catch {
    throw new ServiceError(502, "SCC-UI-INTENT-003", "Intent API returned unreadable JSON", {
      failure_class: "MODEL_OUTPUT_INVALID",
    });
  }
}

function chatCompletionsUrl(endpoint) {
  const url = new URL(endpoint);
  if (!url.pathname.replace(/\/$/, "").endsWith("/chat/completions")) {
    url.pathname = `${url.pathname.replace(/\/$/, "")}/chat/completions`;
  }
  return url.toString();
}

function parseModelJson(content) {
  const text = Array.isArray(content)
    ? content.map((part) => typeof part === "string" ? part : part?.text ?? "").join("")
    : content;
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end < start) {
    throw new ServiceError(502, "SCC-UI-INTENT-003", "Intent model did not return a JSON object", {
      failure_class: "MODEL_OUTPUT_INVALID",
    });
  }
  try {
    return JSON.parse(text.slice(start, end + 1));
  } catch {
    throw new ServiceError(502, "SCC-UI-INTENT-003", "Intent model returned invalid JSON", {
      failure_class: "MODEL_OUTPUT_INVALID",
    });
  }
}

function validateIntentResult(result) {
  const proposal = result?.proposal ?? result;
  if (!proposal || typeof proposal !== "object" || Array.isArray(proposal) || typeof proposal.version !== "string") {
    throw new ServiceError(502, "SCC-UI-INTENT-003", "Intent compiler response is missing a valid Proposal", {
      failure_class: "MODEL_OUTPUT_INVALID",
    });
  }
  for (const field of ["entities", "relations", "dimensions", "arrays", "anchors"]) {
    if (proposal[field] !== undefined && !Array.isArray(proposal[field])) {
      throw new ServiceError(502, "SCC-UI-INTENT-003", `Intent compiler Proposal field ${field} must be an array`, {
        failure_class: "MODEL_OUTPUT_INVALID",
      });
    }
  }
  return {
    proposal,
    summary: typeof result?.summary === "string" ? result.summary : "Intent compiled into a structured Proposal.",
    mutation: Array.isArray(result?.mutation) ? result.mutation : [],
  };
}

function validateCadIntentResult(result) {
  const cadSpec = result?.cadSpec ?? result;
  if (!cadSpec || typeof cadSpec !== "object" || Array.isArray(cadSpec) || typeof cadSpec.version !== "string") {
    throw new ServiceError(502, "SCC-UI-INTENT-003", "Intent compiler response is missing a valid CadSpec", {
      failure_class: "MODEL_OUTPUT_INVALID",
    });
  }
  return {
    cadSpec,
    summary: typeof result?.summary === "string" ? result.summary : "Intent compiled into a structured 3D CadSpec.",
  };
}
