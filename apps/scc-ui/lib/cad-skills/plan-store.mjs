import { randomUUID } from "node:crypto";
import { ServiceError } from "../scc-cli.mjs";

const TTL_MS = 2 * 60 * 1000;
const MAX_PLANS = 32;

export class SkillPlanStore {
  #plans = new Map();

  put(binding, payload) {
    this.#purge();
    while (this.#plans.size >= MAX_PLANS) this.#plans.delete(this.#plans.keys().next().value);
    const id = randomUUID();
    this.#plans.set(id, { binding, payload, expiresAt: Date.now() + TTL_MS });
    return id;
  }

  consume(id, binding) {
    this.#purge();
    const record = this.#plans.get(id);
    this.#plans.delete(id);
    if (!record) throw new ServiceError(409, "SCC-UI-SKILL-001", "CAD Skill plan is missing, expired, or already used");
    if (record.binding !== binding) throw new ServiceError(409, "SCC-UI-SKILL-002", "CAD Skill plan no longer matches the request or model snapshot");
    return record.payload;
  }

  #purge() {
    const now = Date.now();
    for (const [id, record] of this.#plans) if (record.expiresAt <= now) this.#plans.delete(id);
  }
}

export function bindSkillPlan({ mode, text, fingerprint }) {
  return JSON.stringify([mode, text, fingerprint]);
}
