import assert from "node:assert/strict";
import test from "node:test";
import { loadPrompt } from "../lib/provider-adapter.mjs";

// Static prompt checks: the prompts must never claim capabilities the DSL /
// tool layer cannot execute (ADR-020 Decision 8, task §84).

test("new-design prompt preserves legacy DSL and routes parametric parts through supplied tools", () => {
  const prompt = loadPrompt("new-design-v1");
  assert.match(prompt, /AICAD Engineering DSL v1/);
  assert.match(prompt, /registered tool schemas/);
  assert.match(prompt, /EngineeringModel validator/);
  assert.match(prompt, /UNRESOLVED/);
  assert.match(prompt, /never expand/i);
  assert.match(prompt, /Unsupported in v1/);
  assert.match(prompt, /BASE_ROUNDED_RECT/);
  assert.match(prompt, /THICKNESS/);
  assert.match(prompt, /EXTENT feature_id THRU/);
  // SLOT/FILLET/CONCENTRIC must only appear in the unsupported list.
  assert.match(prompt, /SLOT, FILLET, CONCENTRIC/);
  // No tool/DSL pair may be advertised as supported.
  assert.ok(!/slot|fillet|concentric/i.test(prompt.replace(/Unsupported[\s\S]*$/, "")));
});

test("new-design prompt forbids guessing critical dimensions", () => {
  const prompt = loadPrompt("new-design-v1");
  assert.match(prompt, /do NOT guess it/i);
  assert.match(prompt, /Never invent a formula/);
});

test("edit-design prompt uses registered tools and forbids regeneration", () => {
  const prompt = loadPrompt("edit-design-v1");
  assert.match(prompt, /Use only the tools listed/);
  assert.match(prompt, /never regenerate the whole design/i);
  assert.match(prompt, /never manually update a derived/i);
  assert.match(prompt, /return a clarification/i);
});

test("repair prompt protects confirmed user values", () => {
  const prompt = loadPrompt("repair-v1");
  assert.match(prompt, /confirmed user values/);
  assert.match(prompt, /Fix ONLY the reported error/);
});
