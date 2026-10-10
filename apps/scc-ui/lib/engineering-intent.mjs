// AICAD intent orchestrator (Phase 6, ADR-020).
//
// `runIntent` is the provider-neutral orchestration layer between natural
// language and the two stable front ends:
//   - NewDesign → AICAD DSL or registered Tool Calls → Rust validation/solve
//   - EditDesign → registered Tool Calls → `scc engineering tools --model … --solve`
//
// Every LLM output is treated as untrusted input and goes through the real
// Rust parser / tool validator / model / lowering / SCC. Failures are
// fail-closed: a provider failure, invalid DSL, or invalid tool batch never
// changes an EngineeringModel.

import { randomUUID } from "node:crypto";
import { ServiceError } from "./scc-cli.mjs";
import { loadPrompt } from "./provider-adapter.mjs";

const MAX_TOOL_CALLS_PER_REQUEST = 32;
const MAX_DSL_OUTPUT_CHARS = 40_000;
const MAX_INTENT_INPUT_CHARS = 8_000;
// Initial attempt plus at most one repair.
const MAX_ROUNDS = 2;
const SKILL_POLICY = [
  "AICAD CAD Skills policy:",
  "Treat user-specified dimensions as explicit constraints. Never invent missing critical dimensions; ask for clarification.",
  "Use only the supplied Engineering Intent protocol: new designs may return AICAD DSL or supplied registered tool calls; edits use only supplied registered tool schemas.",
  "Do not create a part when the current EngineeringModel already contains one. Never replace or reset an existing model.",
  "Distinguish Counterbore, Countersink, and edge Chamfer. For Countersink require explicit mouth diameter and included angle; ask for missing values and never assume a standard angle.",
  "Use ExplicitPlacement Pattern only for nonuniform planar positions with a typed principal/Datum reference plane and an earlier Hole or Extrude CUT seed. placements are additional absolute local U/V target anchors; the seed remains instance zero. Keep uniform linear and PCD intent as Linear/Circular Pattern.",
  "Do not generate SVG or DXF. Formal drawing and export are performed only by AICAD's validated backend.",
  "Do not claim a drawing or export succeeded until AICAD returns the formal validated artifact.",
].join("\n");

/**
 * Run an intent operation.
 * @param {{
 *   mode: "new" | "edit",
 *   text: string,
 *   modelJson?: string | null,        // current model JSON (edit only)
 *   provider: IntentProvider,
 *   cli: SccCliAdapter,               // must expose engineering* methods
 *   systemPrompt?: string | null,
 * }} args
 * @returns {Promise<{status: "ok", mode, trace, result} |
 *                  {status: "needs_clarification", mode, trace, fields, reason} |
 *                  {status: "rejected", mode, trace, error}>}
 */
export async function runIntent({ mode, text, modelJson, baseFingerprint = null, provider, cli, systemPrompt = null, toolManifest = null, allowedToolNames = null, engineeringContext = undefined }) {
  if (!["new", "edit"].includes(mode)) {
    throw new ServiceError(400, "SCC-UI-INTENT-008", "Intent mode must be 'new' or 'edit'");
  }
  if (typeof text !== "string" || !text.trim()) {
    throw new ServiceError(400, "SCC-UI-INTENT-008", "Design intent text is required");
  }
  if (text.length > MAX_INTENT_INPUT_CHARS) {
    throw new ServiceError(413, "SCC-UI-INTENT-009", "Intent text is too large");
  }

  const effectiveSystemPrompt = `${systemPrompt ?? loadPrompt(mode === "new" ? "new-design-v1" : "edit-design-v1")}\n\n${SKILL_POLICY}`;
  const trace = {
    mode,
    provider: provider.name ?? "unknown",
    outputKind: null,
    validation: "pending",
    repairCount: 0,
    provider_calls: 0,
    allowed_tool_names: allowedToolNames === null ? null : [...allowedToolNames],
    base_model_fingerprint: baseFingerprint,
  };

  // Edit mode derives a read-only context view and the real tool manifest
  // through the Rust CLI (never a full model dump; never a second tool list).
  let context = null;
  let tools = null;
  if (mode === "edit") {
    if (modelJson === undefined || modelJson === null || typeof modelJson !== "string") {
      throw new ServiceError(400, "SCC-UI-INTENT-008", "Edit intent requires the current model");
    }
    context = engineeringContext ?? await cli.engineeringContext(modelJson);
    tools = toolManifest ?? await cli.engineeringManifest();
  } else {
    tools = toolManifest ?? await cli.engineeringManifest();
  }
  const allowed = allowedToolNames === null ? null : new Set(allowedToolNames);
  if (allowed && tools) tools = tools.filter((tool) => allowed.has(tool.name));

  let repair = null;
  for (let attempt = 0; attempt < MAX_ROUNDS; attempt += 1) {
    const request = {
      mode,
      systemPrompt: effectiveSystemPrompt,
      userText: text,
      context,
      tools,
      repair,
    };

    let result;
    try {
      trace.provider_calls += 1;
      result = await provider.complete(request);
    } catch (error) {
      throw new ServiceError(
        error?.status ?? 502,
        error?.code ?? "SCC-UI-PROVIDER-001",
        error?.message ?? "Intent provider failed",
        error?.details,
      );
    }
    trace.outputKind = result.kind;

    const outcome = await dispatch(mode, result, cli, modelJson, allowed);
    if (outcome.kind === "ok") {
      trace.validation = "passed";
      return { status: "ok", mode, trace, result: outcome.payload };
    }
    if (outcome.kind === "clarification") {
      trace.validation = "clarified";
      return {
        status: "needs_clarification",
        mode,
        trace,
        request_id: randomUUID(),
        base_model_fingerprint: baseFingerprint,
        original_intent: text,
        fields: normalizeClarificationFields(outcome.fields ?? [], outcome.reason),
        reason: outcome.reason ?? "The design request is ambiguous.",
      };
    }
    trace.validation = "failed";
    if (attempt === 0 && repairable(outcome.error)) {
      trace.repairCount += 1;
      repair = { feedback: summarizeError(outcome.error) };
      continue;
    }
    return { status: "rejected", mode, trace, error: outcome.error };
  }

  return {
    status: "rejected",
    mode,
    trace,
    error: { code: "SCC-UI-INTENT-013", message: "Intent output remained invalid after repair" },
  };
}

async function dispatch(mode, result, cli, modelJson, allowedTools) {
  if (result.kind === "clarification") {
    return { kind: "clarification", fields: result.fields ?? [], reason: result.reason };
  }

  if (mode === "new") {
    if (result.kind === "text") {
      const dsl = result.text;
      if (dsl.length > MAX_DSL_OUTPUT_CHARS) {
        return validationError("SCC-UI-INTENT-012", "DSL output is too large");
      }
      try {
        const payload = await cli.engineeringDslSolve(dsl);
        return { kind: "ok", payload };
      } catch (error) {
        return { kind: "error", error: toValidationError(error) };
      }
    }
    if (result.kind !== "tool_calls") {
      return wrongOutputMode("New-design intent must produce AICAD DSL text or registered tool calls");
    }
  }

  // New parametric model creation and all edits use the same Rust-validated
  // tool-call path. New designs start from a fresh model; edits pass the
  // current model snapshot into the atomic Rust executor.
  if (result.kind !== "tool_calls") {
    return wrongOutputMode("Edit intent must produce tool calls, not free text or DSL");
  }
  const toolCalls = result.toolCalls;
  if (!Array.isArray(toolCalls) || toolCalls.length === 0) {
    return wrongOutputMode("Edit intent returned no tool calls");
  }
  if (toolCalls.length > MAX_TOOL_CALLS_PER_REQUEST) {
    return validationError(
      "SCC-UI-INTENT-011",
      `Too many tool calls (${toolCalls.length} > ${MAX_TOOL_CALLS_PER_REQUEST})`,
    );
  }
  if (allowedTools && toolCalls.some((call) => !allowedTools.has(call?.name))) {
    const rejected = [...new Set(toolCalls.map((call) => call?.name).filter((name) => !allowedTools.has(name)))];
    return validationError("SCC-UI-INTENT-017", `Tool call is outside this CAD Skill plan: ${rejected.join(", ")}`);
  }
  try {
    const payload = await cli.engineeringToolsApply(toolCalls, modelJson, true);
    return {
      kind: "ok",
      payload: {
        ...payload,
        tool_calls: toolCalls,
        command_plan: payload.command_plan ?? null,
      },
    };
  } catch (error) {
    return { kind: "error", error: toValidationError(error) };
  }
}

function normalizeClarificationFields(fields, reason = "") {
  const source = Array.isArray(fields) ? fields : [];
  if (source.length === 0) {
    return [{ key: "details", kind: "text", prompt: reason || "Please provide the missing engineering details.", required: true }];
  }
  return source.map((field) => {
    if (typeof field === "string") {
      const key = field.trim() || "details";
      return { key, kind: inferFieldKind(key), prompt: field, required: true };
    }
    const key = String(field?.key ?? field?.name ?? "details");
    const kind = String(field?.kind ?? inferFieldKind(key)).toLowerCase();
    return {
      key,
      kind,
      prompt: String(field?.prompt ?? field?.label ?? key),
      ...(Array.isArray(field?.options) ? { options: field.options.map(String) } : {}),
      required: field?.required !== false,
    };
  });
}

function inferFieldKind(key) {
  const value = key.toLowerCase();
  if (value.includes("diameter") || value.includes("width") || value.includes("height") || value.includes("depth") || value.includes("thickness") || value.includes("length")) return "length";
  if (value.includes("feature") || value.includes("hole") || value.includes("target")) return "feature_reference";
  if (value.includes("extent") || value.includes("face") || value.includes("type")) return "enum";
  return "text";
}

function wrongOutputMode(message) {
  return { kind: "error", error: { code: "SCC-UI-INTENT-010", message } };
}

function validationError(code, message) {
  return { kind: "error", error: { code, message } };
}

function toValidationError(error) {
  const diagnostics = error?.details?.diagnostics ?? [];
  const first = diagnostics[0];
  return {
    code: first?.details?.parametric_code ?? first?.code ?? error?.code ?? "SCC-UI-CLI-002",
    message: first?.message ?? error?.message ?? "Engineering validation failed",
    diagnostics,
  };
}

function summarizeError(error) {
  const diagnostics = error?.details?.diagnostics ?? [];
  const messages = diagnostics
    .map((d) => `${d.code ?? ""} ${d.message ?? ""}`.trim())
    .filter(Boolean);
  return (messages.slice(0, 8).join("; ") || error?.message || "Validation failed").slice(0, 4_000);
}

/** SCC geometry and semantic-analysis failures are never auto-repaired: the
 * LLM must not be allowed to "fix" confirmed engineering data to make the
 * geometry solve. Orchestrator-level rejections (wrong mode, too many calls,
 * output too large) are also not repairable. Only DSL/TOOL/ENGINEERING
 * syntactic or local-semantic failures get at most one repair. */
function repairable(error) {
  const code = error?.code ?? "";
  if (code.startsWith("SCC-GEOM-") || code.startsWith("SCC-SPAT-")) return false;
  if (code.startsWith("SCC-ANAL-")) return false;
  if (code.startsWith("SCC-UI-")) return false;
  if ((error?.diagnostics ?? []).some((diagnostic) => diagnostic?.stage === "spatial" || diagnostic?.details?.parametric_code)) return false;
  return true;
}
