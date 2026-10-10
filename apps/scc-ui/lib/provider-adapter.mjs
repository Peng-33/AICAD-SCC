// Provider-neutral intent types (ADR-020 Decision 4/5).
//
// The orchestrator depends only on these shapes; a concrete provider (Fake,
// OpenAI-compatible, ...) maps to and from them. Providers never touch an
// EngineeringModel and never see the full model dump — only a derived context
// view.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

/** Load a versioned prompt file. */
export function loadPrompt(name) {
  return readFileSync(join(here, "..", "prompts", `${name}.md`), "utf8");
}

/**
 * A provider-neutral intent request.
 * @typedef {{
 *   mode: "new" | "edit",
 *   systemPrompt: string,
 *   userText: string,
 *   context?: object | null,   // derived EngineeringContextView (edit only)
 *   tools?: object[] | null,   // supplied manifest (new parametric or edit)
 *   repair?: { feedback: string } | null,
 * }} IntentRequest
 */

/**
 * A provider-neutral result.
 * @typedef {{ kind: "text", text: string }}
 *   | { kind: "tool_calls", toolCalls: Array<{ name: string, arguments: object }> }
 *   | { kind: "clarification", reason: string, fields?: string[] }
 * ProviderResult
 */

/** A provider-neutral provider interface (a duck-typed contract). */
export const IntentProvider = {
  /**
   * @param {IntentRequest} request
   * @returns {Promise<ProviderResult>}
   */
  async complete() {
    throw new Error("IntentProvider.complete must be implemented");
  },
};

/** Build the provider-neutral request for a mode. */
export function buildIntentRequest({ mode, systemPrompt, userText, context, tools, repair }) {
  return { mode, systemPrompt, userText, context: context ?? null, tools: tools ?? null, repair: repair ?? null };
}

/** The provider-neutral responses the orchestrator can return. */
export const IntentStatus = {
  OK: "ok",
  NeedsClarification: "needs_clarification",
  Rejected: "rejected",
};
