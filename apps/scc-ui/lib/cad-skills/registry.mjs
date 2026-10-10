import { readdir, readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REQUIRED_FIELDS = [
  "id", "name", "description", "version", "trigger", "input_contract",
  "required_context", "allowed_tools", "execution_steps", "validation_gates",
  "failure_policy", "output_contract",
];
const INTERNAL_CAPABILITIES = new Set([
  "drawing.three_view", "drawing.section_a_a", "drawing.annotation.read", "drawing.export.svg",
  "drawing.export.dxf", "error.inspect", "model.read",
]);
const SKILL_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../../../skills/cad");

export async function loadCadSkills(root = SKILL_ROOT) {
  const directories = (await readdir(root, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
  const skills = [];
  for (const directory of directories) {
    const text = await readFile(join(root, directory, "skill.json"), "utf8");
    let skill;
    try { skill = JSON.parse(text); } catch { throw new Error(`Invalid CAD Skill JSON: ${directory}`); }
    validateSkill(skill, directory);
    skills.push(skill);
  }
  if (skills.length !== 6) throw new Error(`Expected six built-in CAD Skills, found ${skills.length}`);
  const ids = new Set(skills.map((skill) => skill.id));
  if (ids.size !== skills.length) throw new Error("CAD Skill ids must be unique");
  return skills;
}

function validateSkill(skill, directory) {
  if (!skill || typeof skill !== "object" || Array.isArray(skill)) throw new Error(`Invalid CAD Skill: ${directory}`);
  const keys = Object.keys(skill).sort();
  if (keys.join("|") !== [...REQUIRED_FIELDS].sort().join("|")) throw new Error(`CAD Skill has missing or unknown fields: ${directory}`);
  if (skill.id !== directory || !/^\d+\.\d+\.\d+$/.test(skill.version) || typeof skill.description !== "string") throw new Error(`CAD Skill identity is invalid: ${directory}`);
  if (!skill.name || typeof skill.name["zh-CN"] !== "string" || typeof skill.name.en !== "string") throw new Error(`CAD Skill name must provide zh-CN and en: ${directory}`);
  for (const field of REQUIRED_FIELDS.filter((field) => Array.isArray(skill[field]))) {
    if (!skill[field].every((item) => typeof item === "string" && item.trim())) throw new Error(`CAD Skill ${field} is invalid: ${directory}`);
  }
  if (["trigger", "input_contract", "required_context", "execution_steps", "validation_gates", "failure_policy", "output_contract"].some((field) => !skill[field].length)) {
    throw new Error(`CAD Skill contract arrays must not be empty: ${directory}`);
  }
  if (!Array.isArray(skill.allowed_tools)) throw new Error(`CAD Skill allowed_tools must be an array: ${directory}`);
  for (const tool of skill.allowed_tools) {
    if (typeof tool !== "string" || !(INTERNAL_CAPABILITIES.has(tool) || /^[a-z][a-z0-9_]*$/.test(tool))) {
      throw new Error(`CAD Skill capability is invalid: ${directory}`);
    }
  }
  const serialized = JSON.stringify(skill);
  if (/"(?:script|executable|shell|command_line|permission|arbitrary_code)"\s*:/i.test(serialized)) {
    throw new Error(`CAD Skill cannot contain executable authority: ${directory}`);
  }
}

export function publicCadSkills(skills) {
  return skills.map((skill) => structuredClone(skill));
}

export function resolveSkillToolAllowlist(skills, selectedIds, manifest) {
  const actual = new Set((manifest ?? []).map((tool) => tool?.name).filter((name) => typeof name === "string"));
  const contracts = skills.filter((skill) => selectedIds.includes(skill.id));
  const rustNames = new Set();
  const missing = new Set();
  const capabilities = new Set();
  for (const skill of contracts) for (const name of skill.allowed_tools) {
    if (INTERNAL_CAPABILITIES.has(name)) capabilities.add(name);
    else if (actual.has(name)) rustNames.add(name);
    else missing.add(name);
  }
  return { allowedToolNames: [...rustNames].sort(), capabilities: [...capabilities].sort(), unavailableToolNames: [...missing].sort() };
}

export const CAD_SKILL_IDS = Object.freeze([
  "mechanical-part-analysis", "engineering-model-planning", "orthographic-drawing",
  "drawing-annotation", "cad-export-validation", "engineering-diagnostics",
]);
