import assert from "node:assert/strict";
import test from "node:test";
import { createApp } from "../server.mjs";
import { createIntentService } from "../lib/intent-service.mjs";

const json = (method, body = {}) => ({ method, headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
async function withServer(run, options = {}) {
  const server = createApp({ adapter: { version: async () => "scc test" }, ...options });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try { await run(`http://127.0.0.1:${server.address().port}`); }
  finally { await new Promise((resolve) => server.close(resolve)); }
}

test("existing DeepSeek runtime configuration is imported without exposing its key", () => {
  const service = createIntentService({
    intentProvider: "openai-compatible", intentEndpoint: "https://api.deepseek.com",
    intentModel: "deepseek-flash", intentApiKey: "existing-secret",
  });
  const list = service.listProfiles();
  assert.equal(list.profiles.length, 1);
  assert.equal(list.profiles[0].type, "deepseek");
  assert.equal(list.profiles[0].active, true);
  assert.equal(list.profiles[0].hasApiKey, true);
  assert.equal(JSON.stringify(list).includes("existing-secret"), false);
  assert.equal(JSON.stringify(service.publicConfig()).includes("existing-secret"), false);
});

test("imported cloud configuration without a key remains visible but cannot be activated", () => {
  const service = createIntentService({
    intentProvider: "openai-compatible", intentEndpoint: "https://api.deepseek.com",
    intentModel: "deepseek-flash", intentApiKey: "",
  });
  const profile = service.listProfiles().profiles[0];
  assert.equal(profile.type, "deepseek");
  assert.equal(profile.authMode, "bearer");
  assert.equal(profile.status, "unconfigured");
  assert.throws(() => service.activateProfile(profile.id), { code: "SCC-UI-MODEL-008" });
});

test("named profiles keep independent keys and an in-flight request keeps its original provider snapshot", async () => {
  const calls = [];
  let releaseFirst;
  const firstGate = new Promise((resolve) => { releaseFirst = resolve; });
  const service = createIntentService({
    intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "",
    fetchImpl: async (url, options) => {
      const body = JSON.parse(options.body);
      calls.push({ url, model: body.model, authorization: options.headers.authorization });
      if (body.model === "model-a") await firstGate;
      return new Response(JSON.stringify({ choices: [{ message: { content: "AICAD 1\nPART p\n" } }] }), { status: 200 });
    },
  });
  const a = service.addProfile({ name: "A", type: "deepseek", endpoint: "https://api.deepseek.com", model: "model-a", apiKey: "key-a", authMode: "bearer" });
  const b = service.addProfile({ name: "B", type: "qwen", endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "model-b", apiKey: "key-b", authMode: "bearer" });
  assert.equal(JSON.stringify(service.listProfiles()).includes("key-a"), false);
  assert.equal(JSON.stringify(service.listProfiles()).includes("key-b"), false);
  service.activateProfile(a.id);
  const pending = service.engineeringProvider().complete({ mode: "new", systemPrompt: "AICAD DSL", userText: "plate" });
  await new Promise((resolve) => setImmediate(resolve));
  service.activateProfile(b.id);
  const second = await service.engineeringProvider().complete({ mode: "new", systemPrompt: "AICAD DSL", userText: "plate" });
  releaseFirst();
  const first = await pending;
  assert.equal(first.kind, "text");
  assert.equal(second.kind, "text");
  assert.deepEqual(calls.map(({ model, authorization }) => [model, authorization]), [
    ["model-a", "Bearer key-a"], ["model-b", "Bearer key-b"],
  ]);
  const edited = service.editProfile(a.id, { name: "A updated", endpoint: "https://api.deepseek.com/v1" });
  assert.equal(edited.hasApiKey, true, "editing without a key retains the original one");
  assert.equal(service.listProfiles().activeId, b.id);
  assert.equal(service.publicConfig().model, "model-b");
});

test("activation rejects incomplete credentials, never falls back, and deletion does not select another paid provider", () => {
  const service = createIntentService({ intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
  const missing = service.addProfile({ name: "No key", type: "deepseek", endpoint: "https://api.deepseek.com", model: "a", authMode: "bearer" });
  assert.equal(missing.status, "unconfigured");
  assert.throws(() => service.activateProfile(missing.id), { code: "SCC-UI-MODEL-008" });
  assert.equal(service.listProfiles().activeId, null);
  const local = service.addProfile({ name: "Local", type: "local", endpoint: "http://127.0.0.1:11434/v1", model: "local-a", authMode: "none" });
  service.activateProfile(local.id);
  service.deleteProfile(local.id);
  assert.equal(service.listProfiles().activeId, null);
  assert.equal(service.publicConfig().configured, false);
  assert.equal(service.listProfiles().profiles[0].id, missing.id);
});

test("API profile responses are sanitized and connection and engineering statuses remain separate", async () => {
  const calls = [];
  await withServer(async (base) => {
    const created = await fetch(`${base}/api/model-configs`, json("POST", {
      name: "Local test", type: "local", endpoint: "http://127.0.0.1:11434/v1",
      model: "local-model", authMode: "bearer", apiKey: "api-secret",
    }));
    assert.equal(created.status, 201);
    const profile = await created.json();
    assert.equal(profile.hasApiKey, true);
    assert.equal(JSON.stringify(profile).includes("api-secret"), false);
    const list = await fetch(`${base}/api/model-configs`).then((response) => response.json());
    assert.equal(JSON.stringify(list).includes("api-secret"), false);
    const tested = await fetch(`${base}/api/model-configs/${profile.id}/test-connection`, json("POST"));
    assert.equal(tested.status, 200);
    assert.equal((await tested.json()).connection.status, "connected");
    const after = await fetch(`${base}/api/model-configs`).then((response) => response.json());
    assert.equal(after.profiles[0].engineering.status, "saved");
    assert.equal(calls[0].model, "local-model");
    assert.equal(calls[0].authorization, "Bearer api-secret");
  }, {
    intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "",
    fetchImpl: async (_url, options) => {
      const body = JSON.parse(options.body);
      calls.push({ model: body.model, authorization: options.headers.authorization });
      return new Response(JSON.stringify({ choices: [{ message: { content: "OK" } }] }), { status: 200 });
    },
  });
});

test("engineering protocol test is distinct from the transport probe and validates a real three-view model", async (t) => {
  const { resolveSccBinary, SccCliAdapter } = await import("../lib/scc-cli.mjs");
  const { resolve } = await import("node:path");
  const { fileURLToPath } = await import("node:url");
  const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
  const binary = await resolveSccBinary(repoRoot);
  if (!binary) { t.skip("SCC binary unavailable"); return; }
  const dsl = `AICAD 1
PART test_plate "Test Plate"
BASE_RECT main
  WIDTH 120
  HEIGHT 80
THICKNESS 10
DRAWING PROJECTION_METHOD THIRD_ANGLE
`;
  await withServer(async (base) => {
    const profile = await fetch(`${base}/api/model-configs`, json("POST", {
      name: "Local engineering", type: "local", endpoint: "http://127.0.0.1:11434/v1",
      model: "test-model", authMode: "none",
    })).then((response) => response.json());
    const response = await fetch(`${base}/api/model-configs/${profile.id}/test-engineering`, json("POST"));
    assert.equal(response.status, 200);
    const tested = await response.json();
    assert.equal(tested.connection.status, "saved");
    assert.equal(tested.engineering.status, "connected");
    assert.equal(tested.active, false);
  }, {
    adapter: new SccCliAdapter(binary),
    intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "",
    fetchImpl: async () => new Response(JSON.stringify({ choices: [{ message: { content: dsl } }] }), { status: 200 }),
  });
});

test("invalid model output is reported as an engineering protocol failure", async () => {
  const service = createIntentService({ intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
  const profile = service.addProfile({ name: "Local", type: "local", endpoint: "http://127.0.0.1:11434/v1", model: "test", authMode: "none" });
  const result = await service.testProfileEngineering(profile.id, async () => {
    const error = new Error("Invalid provider output"); error.code = "SCC-UI-PROVIDER-002"; throw error;
  });
  assert.equal(result.engineering.status, "protocol_failed");
  assert.equal(result.connection.status, "saved");
});

test("Engineering Intent API uses the activated profile without replacing an existing model", async (t) => {
  const { resolveSccBinary, SccCliAdapter } = await import("../lib/scc-cli.mjs");
  const { resolve } = await import("node:path");
  const { fileURLToPath } = await import("node:url");
  const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");
  const binary = await resolveSccBinary(repoRoot);
  if (!binary) { t.skip("SCC binary unavailable"); return; }
  const calls = [];
  const dsl = `AICAD 1
PART test_plate "Test Plate"
BASE_RECT main
  WIDTH 120
  HEIGHT 80
THICKNESS 10
DRAWING PROJECTION_METHOD THIRD_ANGLE
`;
  await withServer(async (base) => {
    const a = await fetch(`${base}/api/model-configs`, json("POST", {
      name: "Provider A", type: "deepseek", endpoint: "https://api.deepseek.com", model: "model-a", apiKey: "secret-a",
    })).then((response) => response.json());
    const b = await fetch(`${base}/api/model-configs`, json("POST", {
      name: "Provider B", type: "qwen", endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "model-b", apiKey: "secret-b",
    })).then((response) => response.json());
    await fetch(`${base}/api/model-configs/${a.id}/activate`, json("POST"));
    const first = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "new", text: "创建 120 × 80 mm、厚 10 mm 安装板" })).then((response) => response.json());
    assert.equal(first.status, "new_ready");
    const modelBefore = JSON.stringify(first.model);
    await fetch(`${base}/api/model-configs/${b.id}/activate`, json("POST"));
    const second = await fetch(`${base}/api/engineering/intent`, json("POST", { mode: "new", text: "创建另一块安装板" })).then((response) => response.json());
    assert.equal(second.status, "new_ready");
    assert.equal(JSON.stringify(first.model), modelBefore);
    assert.deepEqual(calls.map(({ model, authorization }) => [model, authorization]), [
      ["model-a", "Bearer secret-a"], ["model-b", "Bearer secret-b"],
    ]);
    assert.equal(JSON.stringify(await fetch(`${base}/api/model-configs`).then((response) => response.json())).includes("secret-a"), false);
  }, {
    adapter: new SccCliAdapter(binary),
    intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "",
    fetchImpl: async (_url, options) => {
      const body = JSON.parse(options.body);
      calls.push({ model: body.model, authorization: options.headers.authorization });
      return new Response(JSON.stringify({ choices: [{ message: { content: dsl } }] }), { status: 200 });
    },
  });
});

test("runtime profiles are absent after a fresh service instance unless explicitly initialized", () => {
  const one = createIntentService({ intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
  one.addProfile({ name: "Transient", type: "local", endpoint: "http://127.0.0.1:11434/v1", model: "test", authMode: "none" });
  const restarted = createIntentService({ intentProvider: "disabled", intentEndpoint: "", intentModel: "", intentApiKey: "" });
  assert.equal(restarted.listProfiles().profiles.length, 0);
  assert.equal(restarted.listProfiles().ephemeralSecret, true);
});
