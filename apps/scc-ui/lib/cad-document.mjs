import { createHash } from "node:crypto";
import { normalizeParameters } from "./cad-parameters.mjs";
import { validateManufacturingSemantics } from "./manufacturing-semantics.mjs";
import { ServiceError } from "./scc-cli.mjs";

const TOP_LEVEL_FIELDS = new Set([
  "version", "kind", "name", "units", "parameters", "features", "requirements",
  "manufacturing", "assumptions", "unresolved", "source_hash",
]);
const FEATURE_BASE_FIELDS = ["id", "type", "operation", "suppressed", "label"];
const FEATURE_FIELDS = {
  box: new Set([...FEATURE_BASE_FIELDS, "size", "origin"]),
  cylinder: new Set([...FEATURE_BASE_FIELDS, "diameter", "length", "axis", "origin"]),
  circular_hole_pattern: new Set([
    ...FEATURE_BASE_FIELDS, "diameter", "length", "count", "pitch_diameter",
    "start_angle", "axis", "origin",
  ]),
};
const REQUIREMENT_FIELDS = new Set([
  "id", "text", "disposition", "feature_ids", "manufacturing_ref", "parameter_ids",
  "assumption_ref", "unresolved_ref", "reason",
]);
const REVIEW_FIELDS = new Set(["id", "severity", "field", "message", "choices", "equation"]);
const REQUIREMENT_DISPOSITIONS = new Set([
  "geometry", "manufacturing", "derived", "assumption", "unresolved", "unsupported",
]);
const ID_PATTERN = /^[a-z][a-z0-9_-]{0,63}$/;
const AXES = new Set(["x", "y", "z"]);

export function normalizeCadDocument(input) {
  requireObject(input, "CadDocument", 400);
  rejectUnknown(input, TOP_LEVEL_FIELDS, "");
  if (input.version !== "1.0") throw cadError("SCC-CAD-010", "CadDocument version must be 1.0");
  if (input.kind !== "feature_graph_v1") throw cadError("SCC-CAD-010", "CadDocument kind must be feature_graph_v1");
  if (input.units !== "mm") throw cadError("SCC-CAD-010", "CadDocument units must be mm");
  if (typeof input.name !== "string" || input.name.trim().length === 0 || input.name.length > 120) {
    throw cadError("SCC-CAD-010", "CadDocument name must be a non-empty string up to 120 characters");
  }

  const { values: parameterValues, normalized: normalizedParameters } = normalizeParameters(input.parameters);
  const features = validateFeatures(input.features, parameterValues);
  const manufacturing = validateManufacturingSemantics(input.manufacturing, {
    features,
    parameters: parameterValues,
  });
  const assumptions = validateReviewItems(input.assumptions, "assumptions", new Set(["assumption"]));
  const unresolved = validateReviewItems(input.unresolved, "unresolved", new Set(["critical", "warning"]));
  validateRequirements(input.requirements, features, manufacturing, assumptions, unresolved, parameterValues);

  const normalized = structuredClone(input);
  delete normalized.source_hash;
  normalized.name = normalized.name.trim();
  normalized.parameters = normalizedParameters;
  normalized.source_hash = hashCanonical(normalized);
  return normalized;
}

function validateFeatures(features, parameters) {
  if (!Array.isArray(features) || features.length === 0 || features.length > 256) {
    throw cadError("SCC-CAD-010", "features must contain between 1 and 256 entries");
  }
  const ids = new Set();
  let firstActiveSeen = false;
  const records = new Map();
  for (const [index, feature] of features.entries()) {
    const path = `features[${index}]`;
    requireObject(feature, path);
    validateId(feature.id, `${path}.id`);
    if (ids.has(feature.id)) throw cadError("SCC-CAD-012", `Duplicate feature ID: ${feature.id}`);
    ids.add(feature.id);
    const allowed = FEATURE_FIELDS[feature.type];
    if (!allowed) throw cadError("SCC-CAD-010", `Unsupported feature type: ${feature.type}`);
    rejectUnknown(feature, allowed, `${path}.`);
    if (typeof feature.suppressed !== "undefined" && typeof feature.suppressed !== "boolean") {
      throw cadError("SCC-CAD-010", `${path}.suppressed must be boolean`);
    }
    if (feature.label !== undefined && typeof feature.label !== "string") {
      throw cadError("SCC-CAD-010", `${path}.label must be a string`);
    }
    if (!new Set(["add", "cut"]).has(feature.operation)) {
      throw cadError("SCC-CAD-010", `${path}.operation must be add or cut`);
    }
    if (feature.type === "circular_hole_pattern" && feature.operation !== "cut") {
      throw cadError("SCC-CAD-010", `${path} circular_hole_pattern must use cut`);
    }
    if (!feature.suppressed && !firstActiveSeen) {
      if (feature.operation !== "add") throw cadError("SCC-CAD-010", "The first active feature must add material");
      firstActiveSeen = true;
    }
    validateFeatureGeometry(feature, path, parameters);
    records.set(feature.id, {
      suppressed: feature.suppressed === true,
      type: feature.type,
      feature,
    });
  }
  if (!firstActiveSeen) throw cadError("SCC-CAD-010", "At least one active additive feature is required");
  return records;
}

function validateFeatureGeometry(feature, path, parameters) {
  validateOrigin(feature.origin, `${path}.origin`, parameters);
  if (feature.type === "box") {
    requireObject(feature.size, `${path}.size`);
    rejectUnknown(feature.size, new Set(["x", "y", "z"]), `${path}.size.`);
    for (const axis of AXES) positiveScalar(feature.size[axis], `${path}.size.${axis}`, parameters);
    return;
  }
  if (!AXES.has(feature.axis)) throw cadError("SCC-CAD-010", `${path}.axis must be x, y, or z`);
  positiveScalar(feature.diameter, `${path}.diameter`, parameters);
  positiveScalar(feature.length, `${path}.length`, parameters);
  if (feature.type === "circular_hole_pattern") {
    positiveScalar(feature.pitch_diameter, `${path}.pitch_diameter`, parameters);
    if (!Number.isSafeInteger(feature.count) || feature.count < 2 || feature.count > 360) {
      throw cadError("SCC-CAD-010", `${path}.count must be an integer from 2 to 360`);
    }
    if (feature.start_angle !== undefined) finiteNumber(feature.start_angle, `${path}.start_angle`);
  }
}

function validateOrigin(origin, path, parameters) {
  requireObject(origin, path);
  rejectUnknown(origin, AXES, `${path}.`);
  for (const axis of AXES) resolveScalar(origin[axis], `${path}.${axis}`, parameters);
}

function positiveScalar(value, path, parameters) {
  if (resolveScalar(value, path, parameters) <= 0) throw cadError("SCC-CAD-010", `${path} must resolve to a positive number`);
}

function resolveScalar(value, path, parameters) {
  if (typeof value === "number") return finiteNumber(value, path);
  requireObject(value, path);
  rejectUnknown(value, new Set(["parameter"]), `${path}.`);
  if (typeof value.parameter !== "string" || !parameters.has(value.parameter)) {
    throw cadError("SCC-CAD-011", `${path} references missing parameter: ${String(value.parameter)}`);
  }
  return parameters.get(value.parameter);
}

function validateReviewItems(value, path, severities) {
  if (!Array.isArray(value)) throw cadError("SCC-CAD-010", `${path} must be an array`);
  const ids = new Set();
  for (const [index, item] of value.entries()) {
    requireObject(item, `${path}[${index}]`);
    rejectUnknown(item, REVIEW_FIELDS, `${path}[${index}].`);
    validateId(item.id, `${path}[${index}].id`);
    if (ids.has(item.id)) throw cadError("SCC-CAD-012", `Duplicate ${path} ID: ${item.id}`);
    ids.add(item.id);
    if (!severities.has(item.severity) || typeof item.field !== "string" || typeof item.message !== "string") {
      throw cadError("SCC-CAD-010", `${path}[${index}] has an invalid severity, field, or message`);
    }
  }
  return ids;
}

function validateRequirements(requirements, features, manufacturing, assumptions, unresolved, parameters) {
  if (!Array.isArray(requirements) || requirements.length === 0) {
    throw cadError("SCC-CAD-013", "requirements must explicitly cover at least one design-intent requirement");
  }
  const ids = new Set();
  const coveredFeatures = new Set();
  for (const [index, requirement] of requirements.entries()) {
    const path = `requirements[${index}]`;
    requireObject(requirement, path);
    rejectUnknown(requirement, REQUIREMENT_FIELDS, `${path}.`);
    validateId(requirement.id, `${path}.id`);
    if (ids.has(requirement.id)) throw cadError("SCC-CAD-012", `Duplicate requirement ID: ${requirement.id}`);
    ids.add(requirement.id);
    if (typeof requirement.text !== "string" || requirement.text.trim().length === 0) {
      throw cadError("SCC-CAD-013", `${path}.text must be non-empty`);
    }
    if (!REQUIREMENT_DISPOSITIONS.has(requirement.disposition)) {
      throw cadError("SCC-CAD-013", `${path}.disposition is unsupported`);
    }
    validateRequirementEvidence(requirement, path, features, manufacturing, assumptions, unresolved, parameters);
    if (requirement.disposition === "geometry") {
      requirement.feature_ids.forEach((id) => coveredFeatures.add(id));
    }
  }
  for (const [id, feature] of features) {
    if (!feature.suppressed && !coveredFeatures.has(id)) {
      throw cadError("SCC-CAD-013", `Active feature has no design-intent requirement evidence: ${id}`);
    }
  }
}

function validateRequirementEvidence(requirement, path, features, manufacturing, assumptions, unresolved, parameters) {
  if (requirement.disposition === "geometry") {
    if (!Array.isArray(requirement.feature_ids) || requirement.feature_ids.length === 0) {
      throw cadError("SCC-CAD-013", `${path}.feature_ids must identify geometric evidence`);
    }
    for (const id of requirement.feature_ids) {
      if (!features.has(id) || features.get(id).suppressed) {
        throw cadError("SCC-CAD-013", `${path} references missing or suppressed feature: ${id}`);
      }
    }
    return;
  }
  if (requirement.disposition === "manufacturing") {
    if (typeof requirement.manufacturing_ref !== "string" || !manufacturing.has(requirement.manufacturing_ref)) {
      throw cadError("SCC-CAD-013", `${path}.manufacturing_ref does not resolve`);
    }
    return;
  }
  if (requirement.disposition === "derived") {
    if (!Array.isArray(requirement.parameter_ids) || requirement.parameter_ids.length === 0
      || requirement.parameter_ids.some((id) => !parameters.has(id))) {
      throw cadError("SCC-CAD-013", `${path}.parameter_ids must resolve derived evidence`);
    }
    return;
  }
  if (requirement.disposition === "assumption" && !assumptions.has(requirement.assumption_ref)) {
    throw cadError("SCC-CAD-013", `${path}.assumption_ref does not resolve`);
  }
  if (requirement.disposition === "unresolved" && !unresolved.has(requirement.unresolved_ref)) {
    throw cadError("SCC-CAD-013", `${path}.unresolved_ref does not resolve`);
  }
  if (requirement.disposition === "unsupported"
    && (typeof requirement.reason !== "string" || requirement.reason.trim().length === 0)) {
    throw cadError("SCC-CAD-013", `${path}.reason is required for unsupported intent`);
  }
}

function validateId(value, path) {
  if (typeof value !== "string" || !ID_PATTERN.test(value)) throw cadError("SCC-CAD-010", `${path} is not a stable ID`);
}

function finiteNumber(value, path) {
  if (typeof value !== "number" || !Number.isFinite(value)) throw cadError("SCC-CAD-010", `${path} must be a finite number`);
  return value;
}

function rejectUnknown(value, allowed, prefix) {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) throw cadError("SCC-CAD-010", `Unknown field: ${prefix}${key}`);
  }
}

function requireObject(value, path, status = 422) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new ServiceError(status, "SCC-CAD-010", `${path} must be an object`);
  }
}

function hashCanonical(value) {
  return createHash("sha256").update(JSON.stringify(sortValue(value))).digest("hex");
}

function sortValue(value) {
  if (Array.isArray(value)) return value.map(sortValue);
  if (value === null || typeof value !== "object") return value;
  return Object.fromEntries(Object.keys(value).sort().map((key) => [key, sortValue(value[key])]));
}

function cadError(code, message, details) {
  return new ServiceError(422, code, message, details);
}
