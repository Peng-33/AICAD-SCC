import assert from "node:assert/strict";
import test from "node:test";
import { createOpenAiCompatibleProvider, openAiToolSchema, openAiToolSchemas } from "../lib/provider-openai-compatible.mjs";

// Fixture tool definition shaped exactly like the Rust manifest output.
const CIRCULAR_PATTERN_TOOL = {
  name: "add_circular_hole_pattern",
  description: "Add a circular hole pattern (center, PCD, count, diameter, start angle).",
  parameters: [
    { name: "id", required: true, param_type: "String", description: "Feature id of the pattern.", constraints: null },
    { name: "center_x", required: true, param_type: "Number", description: "Pattern center X in mm.", constraints: null },
    { name: "count", required: true, param_type: "Integer", description: "Number of holes.", constraints: { minimum: 1 } },
    { name: "pcd", required: true, param_type: "Number", description: "Pitch circle diameter in mm.", constraints: { exclusive_minimum: 0 } },
  ],
};

function fetchReturning(payload) {
  return async () => ({ ok: true, status: 200, json: async () => payload });
}

function provider(config = {}) {
  return createOpenAiCompatibleProvider({
    endpoint: "https://example.test/v1",
    model: "fixture-model",
    apiKey: "test-key",
    fetchImpl: config.fetchImpl,
  });
}

test("openAiToolSchema converts an internal tool definition", () => {
  const schema = openAiToolSchema(CIRCULAR_PATTERN_TOOL);
  assert.equal(schema.type, "function");
  assert.equal(schema.function.name, "add_circular_hole_pattern");
  assert.equal(schema.function.parameters.type, "object");
  assert.equal(schema.function.parameters.additionalProperties, false);
  assert.deepEqual(schema.function.parameters.required, ["id", "center_x", "count", "pcd"]);
  assert.equal(schema.function.parameters.properties.id.type, "string");
  assert.equal(schema.function.parameters.properties.center_x.type, "number");
  assert.equal(schema.function.parameters.properties.count.type, "integer");
  assert.equal(schema.function.parameters.properties.count.minimum, 1);
  assert.equal(schema.function.parameters.properties.pcd.exclusiveMinimum, 0);
});

test("openAiToolSchemas converts the full manifest", () => {
  const schemas = openAiToolSchemas([CIRCULAR_PATTERN_TOOL, { ...CIRCULAR_PATTERN_TOOL, name: "add_hole" }]);
  assert.equal(schemas.length, 2);
  for (const schema of schemas) assert.equal(schema.function.parameters.additionalProperties, false);
});

test("edit response with tool calls parses to ProviderResult", async () => {
  let captured;
  const fetchImpl = async (url, options) => {
    captured = JSON.parse(options.body);
    return {
      ok: true,
      status: 200,
      json: async () => ({
        choices: [{ message: { role: "assistant", content: null, tool_calls: [
          { id: "call_1", type: "function", function: { name: "set_parameter", arguments: '{"target":"plate_width","value":160}' } },
        ] } }],
      }),
    };
  };
  const result = await provider({ fetchImpl }).complete({
    mode: "edit",
    systemPrompt: "prompt",
    userText: "把板宽改成160",
    context: { part: { id: "p", name: "p" } },
    tools: [CIRCULAR_PATTERN_TOOL],
  });
  assert.equal(result.kind, "tool_calls");
  assert.deepEqual(result.toolCalls, [{ name: "set_parameter", arguments: { target: "plate_width", value: 160 } }]);
  // The request carried the converted tools and the context.
  assert.ok(Array.isArray(captured.tools));
  assert.equal(captured.tools[0].function.name, "add_circular_hole_pattern");
  assert.match(captured.messages[1].content, /Current design:/);
  assert.equal(captured.temperature, 0);
});

test("new-design G2 response sends the filtered manifest and parses registered tool calls", async () => {
  let captured;
  const fetchImpl = async (_url, options) => {
    captured = JSON.parse(options.body);
    return {
      ok: true,
      status: 200,
      json: async () => ({ choices: [{ message: { role: "assistant", content: null, tool_calls: [
        { id: "call_1", type: "function", function: { name: "create_parametric_plate", arguments: '{"id":"base","width":140,"height":100,"thickness":20}' } },
      ] } }] }),
    };
  };
  const tools = [{ ...CIRCULAR_PATTERN_TOOL, name: "create_parametric_plate" }];
  const result = await provider({ fetchImpl }).complete({ mode: "new", systemPrompt: "prompt", userText: "创建参数化安装板", tools });
  assert.deepEqual(result, { kind: "tool_calls", toolCalls: [{ name: "create_parametric_plate", arguments: { id: "base", width: 140, height: 100, thickness: 20 } }] });
  assert.equal(captured.tools[0].function.name, "create_parametric_plate");
  assert.equal(captured.tool_choice, "auto");
});

test("new-design text response parses to ProviderResult", async () => {
  const fetchImpl = async () => ({
    ok: true,
    status: 200,
    json: async () => ({ choices: [{ message: { role: "assistant", content: "AICAD 1\nPART p\n" } }] }),
  });
  const result = await provider({ fetchImpl }).complete({
    mode: "new",
    systemPrompt: "prompt",
    userText: "画一块板",
  });
  assert.equal(result.kind, "text");
  assert.match(result.text, /AICAD 1/);
});

test("edit response with typed clarification JSON preserves field kinds", async () => {
  const fetchImpl = async () => ({
    ok: true,
    status: 200,
    json: async () => ({ choices: [{ message: { content: JSON.stringify({
      kind: "clarification",
      reason: "Which face and extent?",
      fields: [
        { key: "extent", kind: "enum", prompt: "Extent", options: ["through_all", "blind"] },
        { key: "blind_depth", kind: "length", prompt: "Blind depth" },
      ],
    }) } }] }),
  });
  const result = await provider({ fetchImpl }).complete({ mode: "edit", systemPrompt: "p", userText: "add a hole", tools: [] });
  assert.deepEqual(result, {
    kind: "clarification",
    reason: "Which face and extent?",
    fields: [
      { key: "extent", kind: "enum", prompt: "Extent", options: ["through_all", "blind"] },
      { key: "blind_depth", kind: "length", prompt: "Blind depth" },
    ],
  });
});

test("edit response with only free text becomes a clarification", async () => {
  const fetchImpl = async () => ({
    ok: true,
    status: 200,
    json: async () => ({ choices: [{ message: { role: "assistant", content: "Which hole?" } }] }),
  });
  const result = await provider({ fetchImpl }).complete({
    mode: "edit",
    systemPrompt: "p",
    userText: "改孔",
    tools: [],
  });
  assert.equal(result.kind, "clarification");
});

test("provider HTTP failure surfaces as a provider error", async () => {
  const fetchImpl = async () => ({ ok: false, status: 500, json: async () => ({}) });
  await assert.rejects(
    () => provider({ fetchImpl }).complete({ mode: "new", systemPrompt: "p", userText: "x" }),
    (error) => error.code === "SCC-UI-PROVIDER-001",
  );
});

test("provider authentication failure is classified without exposing credentials", async () => {
  const fetchImpl = async () => ({ ok: false, status: 401, json: async () => ({}) });
  await assert.rejects(
    () => provider({ fetchImpl }).complete({ mode: "new", systemPrompt: "p", userText: "x" }),
    (error) => error.code === "SCC-UI-PROVIDER-003"
      && error.details?.failure_class === "PROVIDER_AUTH_FAILED"
      && error.details?.upstreamStatus === 401,
  );
});

test("provider model and timeout errors have distinct safe codes", async () => {
  await assert.rejects(
    () => provider({ fetchImpl: async () => ({ ok: false, status: 404 }) }).complete({ mode: "new", systemPrompt: "p", userText: "x" }),
    (error) => error.code === "SCC-UI-PROVIDER-004" && error.details?.upstreamStatus === 404,
  );
  await assert.rejects(
    () => provider({ fetchImpl: async () => { const error = new Error("request with secret test-key timed out"); error.name = "TimeoutError"; throw error; } })
      .complete({ mode: "new", systemPrompt: "p", userText: "x" }),
    (error) => error.code === "SCC-UI-PROVIDER-005" && !JSON.stringify(error).includes("test-key"),
  );
});

test("invalid tool-call JSON arguments are rejected", async () => {
  const fetchImpl = async () => ({
    ok: true,
    status: 200,
    json: async () => ({ choices: [{ message: { role: "assistant", content: null, tool_calls: [
      { function: { name: "set_parameter", arguments: "{not json" } },
    ] } }] }),
  });
  await assert.rejects(
    () => provider({ fetchImpl }).complete({ mode: "edit", systemPrompt: "p", userText: "x", tools: [] }),
    (error) => error.code === "SCC-UI-PROVIDER-002",
  );
});
