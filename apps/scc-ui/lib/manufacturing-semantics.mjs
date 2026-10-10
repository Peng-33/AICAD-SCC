import { ServiceError } from "./scc-cli.mjs";

export const MANUFACTURING_FIELDS = new Set([
  "fits", "tolerances", "datums", "geometric_controls", "surface_finishes", "threads",
]);

const FIELD_RULES = {
  fits: new Set(["id", "feature_id", "nominal_diameter", "designation", "note"]),
  tolerances: new Set([
    "id", "parameter_id", "feature_id", "kind", "lower_deviation", "upper_deviation", "note",
  ]),
  datums: new Set(["id", "label", "kind", "feature_id", "note"]),
  geometric_controls: new Set([
    "id", "kind", "feature_id", "tolerance", "diameter_zone", "datum_ids", "note",
  ]),
  surface_finishes: new Set(["id", "feature_id", "ra_um", "note"]),
  threads: new Set(["id", "feature_id", "designation", "effective_depth", "handedness", "note"]),
};
const TOLERANCE_KINDS = new Set(["bilateral", "limit", "general"]);
const DATUM_KINDS = new Set(["axis", "plane", "surface"]);
const GEOMETRIC_CONTROL_KINDS = new Set([
  "straightness", "flatness", "circularity", "cylindricity", "profile_of_line",
  "profile_of_surface", "parallelism", "perpendicularity", "angularity", "position",
  "concentricity", "symmetry", "runout", "total_runout",
]);
const ID_PATTERN = /^[a-z][a-z0-9_-]{0,63}$/;

export function validateManufacturingSemantics(value, { features, parameters }) {
  requireObject(value, "manufacturing");
  rejectUnknown(value, MANUFACTURING_FIELDS, "manufacturing.");
  const semantics = new Map();
  const datums = new Set();
  for (const field of MANUFACTURING_FIELDS) {
    const items = value[field];
    if (!Array.isArray(items)) throw manufacturingError("SCC-CAD-010", `manufacturing.${field} must be an array`);
    for (const [index, item] of items.entries()) {
      const path = `manufacturing.${field}[${index}]`;
      requireObject(item, path);
      rejectUnknown(item, FIELD_RULES[field], `${path}.`);
      validateId(item.id, `${path}.id`);
      if (semantics.has(item.id)) {
        throw manufacturingError("SCC-CAD-012", `Duplicate manufacturing semantic ID: ${item.id}`);
      }
      validateNote(item.note, `${path}.note`);
      validateManufacturingItem(field, item, path, { features, parameters, datums });
      semantics.set(item.id, { category: field, index });
      if (field === "datums") datums.add(item.id);
    }
  }
  return semantics;
}

function validateManufacturingItem(field, item, path, context) {
  switch (field) {
    case "fits": return validateFit(item, path, context);
    case "tolerances": return validateTolerance(item, path, context);
    case "datums": return validateDatum(item, path, context);
    case "geometric_controls": return validateGeometricControl(item, path, context);
    case "surface_finishes": return validateSurfaceFinish(item, path, context);
    case "threads": return validateThread(item, path, context);
    default: throw manufacturingError("SCC-CAD-015", `Unsupported manufacturing category: ${field}`);
  }
}

function validateFit(item, path, { features, parameters }) {
  const feature = activeFeature(item.feature_id, `${path}.feature_id`, features);
  const nominal = positiveNumber(item.nominal_diameter, `${path}.nominal_diameter`);
  nonEmptyString(item.designation, `${path}.designation`, 40);
  if (!new Set(["cylinder", "circular_hole_pattern"]).has(feature.type)) {
    throw manufacturingError("SCC-CAD-015", `${path} fit must reference cylindrical geometry`);
  }
  const geometricDiameter = resolveScalar(feature.feature.diameter, `${path}.feature_id`, parameters);
  if (Math.abs(geometricDiameter - nominal) > 1e-6) {
    throw manufacturingError(
      "SCC-CAD-015",
      `${path}.nominal_diameter ${nominal} conflicts with feature ${item.feature_id} nominal diameter ${geometricDiameter}`,
    );
  }
}

function validateTolerance(item, path, { features, parameters }) {
  if (!TOLERANCE_KINDS.has(item.kind)) {
    throw manufacturingError("SCC-CAD-015", `${path}.kind must be bilateral, limit, or general`);
  }
  const targets = [item.parameter_id !== undefined, item.feature_id !== undefined].filter(Boolean).length;
  if (targets !== 1) {
    throw manufacturingError("SCC-CAD-015", `${path} must reference exactly one parameter_id or feature_id`);
  }
  if (item.parameter_id !== undefined && !parameters.has(item.parameter_id)) {
    throw manufacturingError("SCC-CAD-011", `${path}.parameter_id references missing parameter: ${item.parameter_id}`);
  }
  if (item.feature_id !== undefined) activeFeature(item.feature_id, `${path}.feature_id`, features);
  const lower = finiteNumber(item.lower_deviation, `${path}.lower_deviation`);
  const upper = finiteNumber(item.upper_deviation, `${path}.upper_deviation`);
  if (lower > upper) throw manufacturingError("SCC-CAD-015", `${path} lower deviation exceeds upper deviation`);
}

function validateDatum(item, path, { features }) {
  if (!DATUM_KINDS.has(item.kind)) {
    throw manufacturingError("SCC-CAD-015", `${path}.kind must be axis, plane, or surface`);
  }
  nonEmptyString(item.label, `${path}.label`, 8);
  activeFeature(item.feature_id, `${path}.feature_id`, features);
}

function validateGeometricControl(item, path, { features, datums }) {
  if (!GEOMETRIC_CONTROL_KINDS.has(item.kind)) {
    throw manufacturingError("SCC-CAD-015", `${path}.kind is not a supported geometric control`);
  }
  activeFeature(item.feature_id, `${path}.feature_id`, features);
  positiveNumber(item.tolerance, `${path}.tolerance`);
  if (item.diameter_zone !== undefined && typeof item.diameter_zone !== "boolean") {
    throw manufacturingError("SCC-CAD-015", `${path}.diameter_zone must be boolean`);
  }
  if (!Array.isArray(item.datum_ids)) throw manufacturingError("SCC-CAD-015", `${path}.datum_ids must be an array`);
  for (const datumId of item.datum_ids) {
    if (!datums.has(datumId)) {
      throw manufacturingError("SCC-CAD-015", `${path}.datum_ids references missing datum: ${datumId}`);
    }
  }
}

function validateSurfaceFinish(item, path, { features }) {
  activeFeature(item.feature_id, `${path}.feature_id`, features);
  const ra = positiveNumber(item.ra_um, `${path}.ra_um`);
  if (ra > 1_000) throw manufacturingError("SCC-CAD-015", `${path}.ra_um is outside the supported range`);
}

function validateThread(item, path, { features }) {
  activeFeature(item.feature_id, `${path}.feature_id`, features);
  nonEmptyString(item.designation, `${path}.designation`, 100);
  if (item.effective_depth !== undefined) positiveNumber(item.effective_depth, `${path}.effective_depth`);
  if (item.handedness !== undefined && !new Set(["right", "left"]).has(item.handedness)) {
    throw manufacturingError("SCC-CAD-015", `${path}.handedness must be right or left`);
  }
}

function activeFeature(id, path, features) {
  if (typeof id !== "string" || !features.has(id) || features.get(id).suppressed) {
    throw manufacturingError("SCC-CAD-015", `${path} references missing or suppressed feature: ${String(id)}`);
  }
  return features.get(id);
}

function resolveScalar(value, path, parameters) {
  if (typeof value === "number") return finiteNumber(value, path);
  if (value && typeof value === "object" && !Array.isArray(value)
    && typeof value.parameter === "string" && parameters.has(value.parameter)) {
    return parameters.get(value.parameter);
  }
  throw manufacturingError("SCC-CAD-015", `${path} does not resolve to a geometric diameter`);
}

function validateNote(value, path) {
  if (value !== undefined && (typeof value !== "string" || value.length > 500)) {
    throw manufacturingError("SCC-CAD-015", `${path} must be a string up to 500 characters`);
  }
}

function nonEmptyString(value, path, maximum) {
  if (typeof value !== "string" || value.trim().length === 0 || value.length > maximum) {
    throw manufacturingError("SCC-CAD-015", `${path} must be a non-empty string up to ${maximum} characters`);
  }
}

function positiveNumber(value, path) {
  const number = finiteNumber(value, path);
  if (number <= 0) throw manufacturingError("SCC-CAD-015", `${path} must be positive`);
  return number;
}

function finiteNumber(value, path) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw manufacturingError("SCC-CAD-015", `${path} must be a finite number`);
  }
  return value;
}

function validateId(value, path) {
  if (typeof value !== "string" || !ID_PATTERN.test(value)) {
    throw manufacturingError("SCC-CAD-015", `${path} is not a stable ID`);
  }
}

function requireObject(value, path) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw manufacturingError("SCC-CAD-015", `${path} must be an object`);
  }
}

function rejectUnknown(value, allowed, prefix) {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) throw manufacturingError("SCC-CAD-015", `Unknown field: ${prefix}${key}`);
  }
}

function manufacturingError(code, message) {
  return new ServiceError(422, code, message);
}
