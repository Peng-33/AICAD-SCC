import { createHash } from "node:crypto";
import { ServiceError } from "./scc-cli.mjs";
import { normalizeCadDocument } from "./cad-document.mjs";

const TOP_LEVEL_FIELDS = new Set(["version", "kind", "units", "parameters", "manufacturing", "assumptions", "unresolved", "source_hash"]);
const MANUFACTURING_FIELDS = new Set(["fits", "tolerances", "datums", "geometric_controls", "surface_finishes", "threads"]);
const PARAMETER_GROUPS = new Set([
  "envelope", "shell", "axes", "mounting", "top_opening", "side_openings",
  "lubrication", "internal_cavities", "reinforcement", "locating", "edge_treatments",
]);
const ENVELOPE_FIELDS = new Set([
  "base_length", "base_width", "total_height", "base_thickness",
  "body_length", "body_width", "body_height",
]);
const SHELL_FIELDS = new Set([
  "wall_thickness", "top_thickness", "bottom_reinforcement", "corner_radius",
  "body_radius", "inner_length", "inner_width", "inner_height",
]);
const AXIS_FIELDS = new Set(["main", "second", "third"]);
const AXIS_VALUE_FIELDS = new Set([
  "x", "z", "through_diameter", "center_distance", "horizontal_side",
  "front_bearing_diameter", "front_bearing_depth", "front_step_diameter", "front_step_depth",
  "rear_bearing_diameter", "rear_bearing_depth", "rear_step_diameter", "rear_step_depth",
]);

export function normalizeCadSpec(input) {
  if (!isObject(input)) throw cadError(400, "SCC-CAD-001", "CadSpec must be a JSON object");
  if (input.kind === "feature_graph_v1") return normalizeCadDocument(input);
  rejectUnknown(input, TOP_LEVEL_FIELDS, "");
  if (input.version !== "1.0") throw cadError(422, "SCC-CAD-002", "CadSpec version must be 1.0");
  if (input.kind !== "precision_gearbox_housing_v1") {
    throw cadError(422, "SCC-CAD-002", "Unsupported CadSpec kind", { supported: ["precision_gearbox_housing_v1"] });
  }
  if (input.units !== "mm") throw cadError(422, "SCC-CAD-002", "CadSpec units must be mm");
  requireObject(input.parameters, "parameters");
  rejectUnknown(input.parameters, PARAMETER_GROUPS, "parameters.");
  requireObject(input.parameters.envelope, "parameters.envelope");
  requireObject(input.parameters.shell, "parameters.shell");
  requireObject(input.parameters.axes, "parameters.axes");
  rejectUnknown(input.parameters.envelope, ENVELOPE_FIELDS, "parameters.envelope.");
  rejectUnknown(input.parameters.shell, SHELL_FIELDS, "parameters.shell.");
  rejectUnknown(input.parameters.axes, AXIS_FIELDS, "parameters.axes.");
  validateDimensions(input.parameters.envelope, ENVELOPE_FIELDS, "parameters.envelope");
  validateDimensions(input.parameters.shell, SHELL_FIELDS, "parameters.shell");
  for (const axis of AXIS_FIELDS) {
    requireObject(input.parameters.axes[axis], `parameters.axes.${axis}`);
    rejectUnknown(input.parameters.axes[axis], AXIS_VALUE_FIELDS, `parameters.axes.${axis}.`);
    validateAxis(input.parameters.axes[axis], `parameters.axes.${axis}`);
  }
  validateManufacturing(input.manufacturing);
  validateReviewItems(input.assumptions, "assumptions");
  validateReviewItems(input.unresolved, "unresolved");

  const normalized = structuredClone(input);
  delete normalized.source_hash;
  normalized.assumptions = normalized.assumptions.filter(
    (item) => item.field !== "parameters.axes.third.x",
  );
  const { main, third } = normalized.parameters.axes;
  const verticalOffset = Math.abs(number(third.z, "parameters.axes.third.z") - number(main.z, "parameters.axes.main.z"));
  const centerDistance = number(third.center_distance, "parameters.axes.third.center_distance");
  if (centerDistance < verticalOffset) {
    throw cadError(422, "SCC-CAD-003", "Third-axis center distance is smaller than its vertical offset", {
      centerDistance,
      verticalOffset,
    });
  }
  if (!new Set(["left", "right"]).has(third.horizontal_side)) {
    throw cadError(422, "SCC-CAD-002", "Third-axis horizontal_side must be left or right");
  }
  const horizontalOffset = Math.sqrt((centerDistance ** 2) - (verticalOffset ** 2));
  third.x = main.x + (third.horizontal_side === "right" ? horizontalOffset : -horizontalOffset);
  normalized.assumptions.push({
    severity: "assumption",
    field: "parameters.axes.third.x",
    message: `Third shaft placed to the ${third.horizontal_side}; x derived from center distance and axis heights`,
    equation: "main.x ± sqrt(center_distance^2 - (main.z - third.z)^2)",
  });
  normalized.source_hash = hashCanonical(normalized);
  return normalized;
}

export function assertBuildableCadSpec(spec) {
  const unresolved = spec?.unresolved?.filter((item) => item.severity === "critical") ?? [];
  if (unresolved.length > 0) {
    throw cadError(422, "SCC-CAD-004", "CadSpec has unresolved critical design decisions", { unresolved });
  }
  return spec;
}

function validateManufacturing(value) {
  requireObject(value, "manufacturing");
  rejectUnknown(value, MANUFACTURING_FIELDS, "manufacturing.");
  for (const field of MANUFACTURING_FIELDS) {
    if (!Array.isArray(value[field])) throw cadError(422, "SCC-CAD-002", `manufacturing.${field} must be an array`);
  }
}

function validateReviewItems(value, field) {
  if (!Array.isArray(value)) throw cadError(422, "SCC-CAD-002", `${field} must be an array`);
  for (const [index, item] of value.entries()) {
    requireObject(item, `${field}[${index}]`);
    if (typeof item.severity !== "string" || typeof item.field !== "string" || typeof item.message !== "string") {
      throw cadError(422, "SCC-CAD-002", `${field}[${index}] requires severity, field, and message strings`);
    }
  }
}

function validateDimensions(value, fields, path) {
  for (const field of fields) {
    const dimension = number(value[field], `${path}.${field}`);
    if (dimension <= 0 || dimension > 10_000) throw cadError(422, "SCC-CAD-002", `${path}.${field} is outside the supported range`);
  }
}

function validateAxis(axis, path) {
  for (const field of ["z", "through_diameter"]) number(axis[field], `${path}.${field}`);
  if (axis.x !== undefined) number(axis.x, `${path}.x`);
  if (axis.center_distance !== undefined) number(axis.center_distance, `${path}.center_distance`);
  for (const [key, value] of Object.entries(axis)) {
    if (key.endsWith("diameter") || key.endsWith("depth")) {
      if (number(value, `${path}.${key}`) <= 0) throw cadError(422, "SCC-CAD-002", `${path}.${key} must be positive`);
    }
  }
}

function number(value, field) {
  if (typeof value !== "number" || !Number.isFinite(value)) throw cadError(422, "SCC-CAD-002", `${field} must be a finite number`);
  return value;
}

function rejectUnknown(value, allowed, prefix) {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) throw cadError(422, "SCC-CAD-001", `Unknown field: ${prefix}${key}`);
  }
}

function requireObject(value, field) {
  if (!isObject(value)) throw cadError(422, "SCC-CAD-002", `${field} must be an object`);
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function hashCanonical(value) {
  return createHash("sha256").update(JSON.stringify(sortValue(value))).digest("hex");
}

function sortValue(value) {
  if (Array.isArray(value)) return value.map(sortValue);
  if (!isObject(value)) return value;
  return Object.fromEntries(Object.keys(value).sort().map((key) => [key, sortValue(value[key])]));
}

function cadError(status, code, message, details) {
  return new ServiceError(status, code, message, details);
}
