// OpenAI-compatible provider adapter (ADR-020 Decision 10/23/24).
//
// This adapter ONLY maps internal requests to an OpenAI-compatible chat
// completion HTTP payload and back. It never modifies an EngineeringModel,
// never builds a Proposal, and never executes tools. Tool schemas are derived
// from the Phase 5 `scc-tools` manifest — never a second hand-maintained copy.

import { ServiceError } from "./scc-cli.mjs";
import { renderContextView } from "./engineering-context.mjs";

const DEFAULT_TIMEOUT_MS = 60_000;
const MAX_ARGUMENTS_CHARS = 200_000;

/**
 * Convert one internal tool definition to an OpenAI function schema.
 * @param {object} tool - a ToolDefinition from `scc engineering manifest`.
 * @returns {object} OpenAI `{ type: "function", function: {...} }`.
 */
export function openAiToolSchema(tool) {
  const properties = {};
  const required = [];
  for (const parameter of tool.parameters ?? []) {
    const schema = { type: openAiType(parameter.param_type), description: parameter.description ?? "" };
    const constraints = parameter.constraints;
    if (constraints) {
      if (constraints.enum_values) schema.enum = constraints.enum_values;
      if (constraints.minimum !== undefined && constraints.minimum !== null) schema.minimum = constraints.minimum;
      if (constraints.maximum !== undefined && constraints.maximum !== null) schema.maximum = constraints.maximum;
      if (constraints.exclusive_minimum !== undefined && constraints.exclusive_minimum !== null) {
        schema.exclusiveMinimum = constraints.exclusive_minimum;
      }
    }
    properties[parameter.name] = schema;
    if (parameter.required) required.push(parameter.name);
  }
  return {
    type: "function",
    function: {
      name: tool.name,
      description: tool.description,
      parameters: {
        type: "object",
        properties,
        required,
        additionalProperties: false,
      },
    },
  };
}

function openAiType(paramType) {
  switch (paramType) {
    case "String":
      return "string";
    case "Integer":
      return "integer";
    case "Boolean":
      return "boolean";
    case "Number":
    case "Any":
      return "number";
    default:
      return "string";
  }
}

/** Convert the whole manifest to OpenAI tool schemas. */
export function openAiToolSchemas(manifest) {
  return manifest.map(openAiToolSchema);
}

function buildMessages(request) {
  const messages = [{ role: "system", content: request.systemPrompt }];
  let user = request.userText;
  if (request.context) {
    user = `Current design:\n${renderContextView(request.context)}\n\nUser request:\n${user}`;
  }
  messages.push({ role: "user", content: user });
  if (request.repair) {
    messages.push({
      role: "user",
      content: `The previous output was invalid. ${request.repair.feedback}`,
    });
  }
  return messages;
}

/**
 * Create an OpenAI-compatible IntentProvider.
 * @param {{ endpoint: string, model: string, apiKey?: string, fetchImpl?: Function, timeoutMs?: number }} config
 */
export function createOpenAiCompatibleProvider({ endpoint, model, apiKey = "", fetchImpl = globalThis.fetch, timeoutMs = DEFAULT_TIMEOUT_MS }) {
  if (!endpoint || !model) {
    throw new ServiceError(422, "SCC-UI-INTENT-014", "An OpenAI-compatible endpoint and model are required");
  }
  return {
    name: "openai-compatible",
    async complete(request) {
      const body = {
        model,
        temperature: 0,
        messages: buildMessages(request),
      };
      if ((request.mode === "edit" || request.mode === "new") && Array.isArray(request.tools) && request.tools.length > 0) {
        body.tools = openAiToolSchemas(request.tools ?? []);
        body.tool_choice = "auto";
      }
      let response;
      try {
        response = await fetchImpl(chatCompletionsUrl(endpoint), {
          method: "POST",
          headers: { "content-type": "application/json", ...(apiKey ? { authorization: `Bearer ${apiKey}` } : {}) },
          body: JSON.stringify(body),
          signal: AbortSignal.timeout(timeoutMs),
        });
      } catch (error) {
        if (error?.name === "TimeoutError" || error?.name === "AbortError") {
          throw new ServiceError(504, "SCC-UI-PROVIDER-005", "Intent provider request timed out", {
            failure_class: "PROVIDER_TIMEOUT",
          });
        }
        throw new ServiceError(502, "SCC-UI-PROVIDER-001", "Intent provider could not be reached", {
          failure_class: "PROVIDER_UNREACHABLE",
        });
      }
      if (!response.ok) {
        if (response.status === 401 || response.status === 403) {
          throw new ServiceError(502, "SCC-UI-PROVIDER-003", `Intent provider authentication failed (HTTP ${response.status})`, {
            failure_class: "PROVIDER_AUTH_FAILED",
            upstreamStatus: response.status,
          });
        }
        if (response.status === 400 || response.status === 404 || response.status === 422) {
          throw new ServiceError(502, "SCC-UI-PROVIDER-004", `Model or request is unavailable (HTTP ${response.status})`, {
            failure_class: "MODEL_UNAVAILABLE",
            upstreamStatus: response.status,
          });
        }
        throw new ServiceError(502, "SCC-UI-PROVIDER-001", `Intent provider returned HTTP ${response.status}`, {
          failure_class: "PROVIDER_UNREACHABLE",
          upstreamStatus: response.status,
        });
      }
      let upstream;
      try {
        upstream = await response.json();
      } catch {
        throw new ServiceError(502, "SCC-UI-PROVIDER-002", "Intent provider returned unreadable JSON", {
          failure_class: "MODEL_OUTPUT_INVALID",
        });
      }
      return parseChatResponse(upstream, request.mode);
    },
  };
}

function parseChatResponse(upstream, mode) {
  const message = upstream?.choices?.[0]?.message;
  if (!message) {
    throw new ServiceError(502, "SCC-UI-PROVIDER-002", "Intent provider response is missing a message", {
      failure_class: "MODEL_OUTPUT_INVALID",
    });
  }
  if ((mode === "edit" || mode === "new") && Array.isArray(message.tool_calls) && message.tool_calls.length > 0) {
    const toolCalls = [];
    for (const call of message.tool_calls) {
      const name = call?.function?.name;
      const argumentsText = call?.function?.arguments ?? "";
      if (typeof name !== "string" || !name) {
        throw new ServiceError(502, "SCC-UI-PROVIDER-002", "Intent provider returned a tool call without a name", {
          failure_class: "MODEL_OUTPUT_INVALID",
        });
      }
      if (argumentsText.length > MAX_ARGUMENTS_CHARS) {
        throw new ServiceError(413, "SCC-UI-INTENT-012", "Tool call arguments are too large");
      }
      let parsed;
      try {
        parsed = JSON.parse(argumentsText);
      } catch {
        throw new ServiceError(502, "SCC-UI-PROVIDER-002", `Intent provider returned invalid JSON arguments for tool ${name}`, {
          failure_class: "MODEL_OUTPUT_INVALID",
        });
      }
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new ServiceError(502, "SCC-UI-PROVIDER-002", `Intent provider arguments for tool ${name} must be an object`, {
          failure_class: "MODEL_OUTPUT_INVALID",
        });
      }
      toolCalls.push({ name, arguments: parsed });
    }
    return { kind: "tool_calls", toolCalls };
  }
  const content = message.content;
  if (typeof content === "string" && content.trim()) {
    try {
      const parsed = JSON.parse(content);
      if (parsed && typeof parsed === "object" && (parsed.kind === "clarification" || parsed.clarification)) {
        return {
          kind: "clarification",
          reason: String(parsed.reason ?? parsed.clarification?.reason ?? "Please provide the missing engineering details."),
          fields: parsed.fields ?? parsed.clarification?.fields ?? [],
        };
      }
    } catch {
      // DSL text is handled below; plain edit text remains a typed clarification.
    }
    if (mode === "edit") return { kind: "clarification", reason: content.slice(0, 4000) };
    return { kind: "text", text: content };
  }
  if (mode === "edit" && Array.isArray(message.tool_calls) && message.tool_calls.length === 0) {
    return { kind: "clarification", reason: "The provider returned no tool calls." };
  }
  throw new ServiceError(502, "SCC-UI-PROVIDER-002", "Intent provider returned neither text nor tool calls", {
    failure_class: "MODEL_OUTPUT_INVALID",
  });
}

function chatCompletionsUrl(endpoint) {
  const url = new URL(endpoint);
  if (!url.pathname.replace(/\/$/, "").endsWith("/chat/completions")) {
    url.pathname = `${url.pathname.replace(/\/$/, "")}/chat/completions`;
  }
  return url.toString();
}

/** One explicit, bounded API probe. A 200 response proves transport only, not Engineering validity. */
export async function probeOpenAiCompatibleProvider({ endpoint, model, apiKey = "", fetchImpl = globalThis.fetch, timeoutMs = 10_000 }) {
  let response;
  try {
    response = await fetchImpl(chatCompletionsUrl(endpoint), {
      method: "POST",
      headers: { "content-type": "application/json", ...(apiKey ? { authorization: `Bearer ${apiKey}` } : {}) },
      body: JSON.stringify({ model, messages: [{ role: "user", content: "Reply OK." }], max_tokens: 1 }),
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (error) {
    if (error?.name === "TimeoutError" || error?.name === "AbortError") {
      throw new ServiceError(504, "SCC-UI-PROVIDER-005", "Intent provider request timed out");
    }
    throw new ServiceError(502, "SCC-UI-PROVIDER-001", "Intent provider could not be reached");
  }
  if (response.status === 401 || response.status === 403) {
    throw new ServiceError(502, "SCC-UI-PROVIDER-003", `Intent provider authentication failed (HTTP ${response.status})`);
  }
  if (response.status === 400 || response.status === 404 || response.status === 422) {
    throw new ServiceError(502, "SCC-UI-PROVIDER-004", `Model or request is unavailable (HTTP ${response.status})`);
  }
  if (!response.ok) throw new ServiceError(502, "SCC-UI-PROVIDER-001", `Intent provider returned HTTP ${response.status}`);
  return { connected: true };
}
