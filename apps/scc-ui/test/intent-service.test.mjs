import assert from "node:assert/strict";
import test from "node:test";
import { createIntentService } from "../lib/intent-service.mjs";
import { ServiceError } from "../lib/scc-cli.mjs";

test("runtime intent settings never expose the API key", () => {
  const service = createIntentService({ intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
  const updated = service.update({
    provider: "openai-compatible",
    endpoint: "https://models.example.test/v1",
    model: "design-model",
    apiKey: "secret-value",
  });

  assert.deepEqual(updated, {
    provider: "openai-compatible",
    endpoint: "https://models.example.test/v1",
    model: "design-model",
    hasApiKey: true,
    configured: true,
    source: "runtime",
    ephemeralSecret: true,
  });
  assert.equal(JSON.stringify(updated).includes("secret-value"), false);
  assert.equal(service.publicConfig().hasApiKey, true);
  assert.equal(service.update({ clearApiKey: true }).hasApiKey, false);
});

test("OpenAI-compatible provider converts model JSON into the intent contract", async () => {
  let upstreamRequest;
  const service = createIntentService({
    intentProvider: "openai-compatible",
    intentEndpoint: "http://localhost:11434/v1",
    intentModel: "local-design-model",
    intentApiKey: "local-secret",
    fetchImpl: async (url, options) => {
      upstreamRequest = { url, options, body: JSON.parse(options.body) };
      return new Response(JSON.stringify({
        choices: [{ message: { content: "```json\n{\"proposal\":{\"version\":\"1.0\",\"entities\":[{\"id\":\"p\",\"kind\":\"point\"}],\"relations\":[],\"dimensions\":[],\"arrays\":[],\"anchors\":[],\"metadata\":{}},\"summary\":\"Created a point\"}\n```" } }],
      }), { status: 200, headers: { "content-type": "application/json" } });
    },
  });

  const result = await service.compile({
    intent: "Create a point",
    documents: [{ name: "brief.md", type: "text/markdown", content: "Use an origin point" }],
    currentProposal: { version: "1.0", entities: [] },
  });

  assert.equal(upstreamRequest.url, "http://localhost:11434/v1/chat/completions");
  assert.equal(upstreamRequest.options.headers.authorization, "Bearer local-secret");
  assert.equal(upstreamRequest.body.model, "local-design-model");
  assert.equal(upstreamRequest.body.response_format.type, "json_object");
  assert.match(upstreamRequest.body.messages[1].content, /Create a point/);
  assert.equal(result.proposal.entities[0].id, "p");
  assert.equal(result.summary, "Created a point");
  assert.deepEqual(result.mutation, []);
});

test("OpenAI-compatible provider repairs a Proposal rejected by SCC M1", async () => {
  const upstreamRequests = [];
  const candidates = [
    { proposal: { version: "1.0", entities: [{ id: "p", type: "point" }], relations: [], dimensions: [], arrays: [], anchors: [], metadata: {} } },
    { proposal: { version: "1.0", entities: [{ id: "p", kind: "point" }], relations: [], dimensions: [], arrays: [], anchors: [], metadata: {} }, summary: "Created a valid point" },
  ];
  const service = createIntentService({
    intentProvider: "openai-compatible",
    intentEndpoint: "https://models.example.test/v1",
    intentModel: "design-model",
    fetchImpl: async (_url, options) => {
      upstreamRequests.push(JSON.parse(options.body));
      return new Response(JSON.stringify({
        choices: [{ message: { content: JSON.stringify(candidates.shift()) } }],
      }), { status: 200, headers: { "content-type": "application/json" } });
    },
    validateProposal: async (proposal) => {
      if (!proposal.entities[0]?.kind) {
        throw new ServiceError(422, "SCC-UI-CLI-002", "SCC command failed", {
          diagnostics: [{ errors: [{ code: "SCC-PROP-002", message: "missing required field: /kind", path: "/kind" }] }],
        });
      }
    },
  });

  const result = await service.compile({ intent: "Create a point", documents: [] });

  assert.equal(upstreamRequests.length, 2);
  assert.match(upstreamRequests[0].messages[0].content, /"kind":"point"/);
  assert.match(upstreamRequests[1].messages.at(-1).content, /missing required field: \/kind/);
  assert.equal(result.proposal.entities[0].kind, "point");
  assert.equal(result.summary, "Created a valid point");
});

test("OpenAI-compatible provider repairs a Proposal rejected by M3 geometry", async () => {
  const upstreamRequests = [];
  const candidates = [
    { proposal: { version: "1.0", entities: [{ id: "hole", kind: "circle" }, { id: "plate", kind: "rectangle" }], relations: [{ kind: "inside", subject: "hole", reference: "plate" }], dimensions: [], arrays: [], anchors: [], metadata: {} } },
    { proposal: { version: "1.0", entities: [{ id: "hole", kind: "circle" }, { id: "plate", kind: "rectangle" }], relations: [{ kind: "centered_in", subject: "hole", reference: "plate", axis: "both" }], dimensions: [], arrays: [], anchors: [], metadata: {} }, summary: "Used a solver-supported center relation" },
  ];
  const service = createIntentService({
    intentProvider: "openai-compatible",
    intentEndpoint: "https://models.example.test/v1",
    intentModel: "design-model",
    fetchImpl: async (_url, options) => {
      upstreamRequests.push(JSON.parse(options.body));
      return new Response(JSON.stringify({
        choices: [{ message: { content: JSON.stringify(candidates.shift()) } }],
      }), { status: 200, headers: { "content-type": "application/json" } });
    },
    validateProposal: async (proposal) => {
      if (proposal.relations[0]?.kind === "inside") {
        throw new ServiceError(422, "SCC-GEOM-005", "SCC command failed", {
          diagnostics: [{ code: "SCC-GEOM-005", message: "constraint kind On is not supported by the current solver" }],
        });
      }
    },
  });

  const result = await service.compile({ intent: "Put a hole inside a plate", documents: [] });

  assert.equal(upstreamRequests.length, 2);
  assert.match(upstreamRequests[0].messages[0].content, /Do not emit inside, outside, tangent_to, or symmetric_about/);
  assert.match(upstreamRequests[0].messages[0].content, /2D top-view geometry/);
  assert.match(upstreamRequests[0].messages[0].content, /metadata\.unmodeled_requirements/);
  assert.match(upstreamRequests[1].messages.at(-1).content, /constraint kind On is not supported/);
  assert.equal(result.proposal.relations[0].kind, "centered_in");
});

test("OpenAI-compatible provider reports persistent M1 diagnostics after one failed repair", async () => {
  let calls = 0;
  const service = createIntentService({
    intentProvider: "openai-compatible",
    intentEndpoint: "https://models.example.test/v1",
    intentModel: "design-model",
    fetchImpl: async () => {
      calls += 1;
      return new Response(JSON.stringify({
        choices: [{ message: { content: JSON.stringify({
          proposal: { version: "1.0", entities: [{ id: "p", type: "point" }], relations: [], dimensions: [], arrays: [], anchors: [], metadata: {} },
        }) } }],
      }), { status: 200, headers: { "content-type": "application/json" } });
    },
    validateProposal: async () => {
      throw new ServiceError(422, "SCC-UI-CLI-002", "SCC command failed", {
        diagnostics: [{ errors: [{ code: "SCC-PROP-002", message: "missing required field: /kind", path: "/kind" }] }],
      });
    },
  });

  await assert.rejects(
    () => service.compile({ intent: "Create a point", documents: [] }),
    (error) => error.code === "SCC-UI-INTENT-006"
      && error.message.includes("rejected by the SCC pipeline")
      && error.details.diagnostics[0].errors[0].message.includes("/kind"),
  );
  assert.equal(calls, 2);
});

test("OpenAI-compatible provider does not disguise local validator outages as model errors", async () => {
  let calls = 0;
  const service = createIntentService({
    intentProvider: "openai-compatible",
    intentEndpoint: "https://models.example.test/v1",
    intentModel: "design-model",
    fetchImpl: async () => {
      calls += 1;
      return new Response(JSON.stringify({
        choices: [{ message: { content: JSON.stringify({
          proposal: { version: "1.0", entities: [{ id: "p", kind: "point" }], relations: [], dimensions: [], arrays: [], anchors: [], metadata: {} },
        }) } }],
      }), { status: 200, headers: { "content-type": "application/json" } });
    },
    validateProposal: async () => {
      throw new ServiceError(503, "SCC-UI-CLI-001", "SCC binary is not available");
    },
  });

  await assert.rejects(
    () => service.compile({ intent: "Create a point", documents: [] }),
    (error) => error.code === "SCC-UI-CLI-001" && error.status === 503,
  );
  assert.equal(calls, 1);
});

test("OpenAI-compatible provider converts natural language into a validated generic CadDocument", async () => {
  let upstreamRequest;
  let validated;
  const cadSpec = {
    version: "1.0",
    kind: "feature_graph_v1",
    name: "mounting-bracket",
    units: "mm",
    parameters: { width: { value: 80, source: "user" } },
    features: [{ id: "base", type: "box", operation: "add", size: { x: { parameter: "width" }, y: 50, z: 10 }, origin: { x: -40, y: -25, z: 0 } }],
    requirements: [{ id: "req_base", text: "80 mm wide base", disposition: "geometry", feature_ids: ["base"] }],
    manufacturing: { fits: [], tolerances: [], datums: [], geometric_controls: [], surface_finishes: [], threads: [] },
    assumptions: [],
    unresolved: [],
  };
  const service = createIntentService({
    intentProvider: "openai-compatible",
    intentEndpoint: "https://models.example.test/v1",
    intentModel: "design-model",
    fetchImpl: async (_url, options) => {
      upstreamRequest = JSON.parse(options.body);
      return new Response(JSON.stringify({
        choices: [{ message: { content: JSON.stringify({ cadSpec, summary: "Created a parametric mounting bracket" }) } }],
      }), { status: 200, headers: { "content-type": "application/json" } });
    },
    validateCadSpec: async (candidate) => {
      validated = candidate;
      return { ...candidate, source_hash: "normalized-hash" };
    },
  });

  const result = await service.compileCad({ intent: "生成一个带安装底座的支架", documents: [] });

  assert.match(upstreamRequest.messages[0].content, /CadDocument 1\.0 FeatureGraph/);
  assert.match(upstreamRequest.messages[0].content, /Never emit executable code/);
  assert.match(upstreamRequest.messages[0].content, /Never silently omit intent/);
  assert.match(upstreamRequest.messages[0].content, /circular_hole_pattern/);
  assert.equal(validated.kind, "feature_graph_v1");
  assert.equal(result.cadSpec.source_hash, "normalized-hash");
  assert.equal(result.summary, "Created a parametric mounting bracket");
});

test("API keys are rejected over non-local plain HTTP", () => {
  const service = createIntentService({ intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
  assert.throws(
    () => service.update({ provider: "openai-compatible", endpoint: "http://models.example.test/v1", model: "model", apiKey: "secret" }),
    (error) => error.code === "SCC-UI-INTENT-CONFIG-008",
  );
});
