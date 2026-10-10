import { ServiceError } from "./scc-cli.mjs";

const PARAMETER_FIELDS = new Set(["value", "source", "description", "expression"]);
const PARAMETER_SOURCES = new Set(["user", "derived", "assumption"]);
const EXPRESSION_FIELDS = new Set(["op", "args"]);
const REFERENCE_FIELDS = new Set(["parameter"]);
const ID_PATTERN = /^[a-z][a-z0-9_-]{0,63}$/;
const MAX_ABSOLUTE_VALUE = 10_000;
const MAX_EXPRESSION_DEPTH = 32;
const MAX_EXPRESSION_NODES = 256;

export function normalizeParameters(parameters) {
  requireObject(parameters, "parameters");
  const definitions = new Map();
  for (const [id, definition] of Object.entries(parameters)) {
    validateId(id, `parameter ID ${id}`);
    requireObject(definition, `parameters.${id}`);
    rejectUnknown(definition, PARAMETER_FIELDS, `parameters.${id}.`);
    if (!PARAMETER_SOURCES.has(definition.source)) {
      throw parameterError("SCC-CAD-010", `parameters.${id}.source must be user, derived, or assumption`);
    }
    if (definition.description !== undefined && typeof definition.description !== "string") {
      throw parameterError("SCC-CAD-010", `parameters.${id}.description must be a string`);
    }
    if (definition.source === "derived") {
      if (definition.expression === undefined) {
        throw parameterError("SCC-CAD-014", `parameters.${id}.expression is required for a derived parameter`);
      }
      if (definition.value !== undefined) finiteNumber(definition.value, `parameters.${id}.value`);
    } else {
      if (definition.expression !== undefined) {
        throw parameterError("SCC-CAD-014", `parameters.${id}.expression is only valid for a derived parameter`);
      }
      finiteNumber(definition.value, `parameters.${id}.value`);
    }
    definitions.set(id, definition);
  }

  const values = new Map();
  const states = new Map();
  const evaluateParameter = (id, trail = []) => {
    if (!definitions.has(id)) {
      throw parameterError("SCC-CAD-011", `Derived expression references missing parameter: ${id}`);
    }
    if (states.get(id) === "done") return values.get(id);
    if (states.get(id) === "visiting") {
      throw parameterError("SCC-CAD-014", `Derived parameter dependency cycle: ${[...trail, id].join(" -> ")}`);
    }
    states.set(id, "visiting");
    const definition = definitions.get(id);
    const value = definition.source === "derived"
      ? evaluateExpression(definition.expression, `parameters.${id}.expression`, evaluateParameter, [...trail, id], { nodes: 0 }, 0)
      : definition.value;
    validateResolvedValue(value, `parameters.${id}.value`);
    values.set(id, value);
    states.set(id, "done");
    return value;
  };

  for (const id of definitions.keys()) evaluateParameter(id);
  const normalized = Object.fromEntries([...definitions].map(([id, definition]) => [
    id,
    { ...structuredClone(definition), value: values.get(id) },
  ]));
  return { values, normalized };
}

function evaluateExpression(expression, path, evaluateParameter, trail, budget, depth) {
  budget.nodes += 1;
  if (budget.nodes > MAX_EXPRESSION_NODES || depth > MAX_EXPRESSION_DEPTH) {
    throw parameterError("SCC-CAD-014", `${path} exceeds the bounded expression complexity`);
  }
  if (typeof expression === "number") return finiteNumber(expression, path);
  requireObject(expression, path);
  if (Object.hasOwn(expression, "parameter")) {
    rejectUnknown(expression, REFERENCE_FIELDS, `${path}.`);
    if (typeof expression.parameter !== "string") {
      throw parameterError("SCC-CAD-011", `${path}.parameter must be a stable parameter ID`);
    }
    return evaluateParameter(expression.parameter, trail);
  }
  rejectUnknown(expression, EXPRESSION_FIELDS, `${path}.`);
  if (typeof expression.op !== "string" || !Array.isArray(expression.args)) {
    throw parameterError("SCC-CAD-014", `${path} requires an op and args array`);
  }
  validateArity(expression.op, expression.args.length, path);
  const args = expression.args.map((argument, index) => evaluateExpression(
    argument,
    `${path}.args[${index}]`,
    evaluateParameter,
    trail,
    budget,
    depth + 1,
  ));
  let result;
  switch (expression.op) {
    case "add": result = args.reduce((sum, value) => sum + value, 0); break;
    case "subtract": result = args[0] - args[1]; break;
    case "multiply": result = args.reduce((product, value) => product * value, 1); break;
    case "divide":
      if (Math.abs(args[1]) <= Number.EPSILON) throw parameterError("SCC-CAD-014", `${path} divides by zero`);
      result = args[0] / args[1];
      break;
    case "sqrt":
      if (args[0] < 0) throw parameterError("SCC-CAD-014", `${path} takes the square root of a negative number`);
      result = Math.sqrt(args[0]);
      break;
    case "negate": result = -args[0]; break;
    case "hypot": result = Math.hypot(...args); break;
    default: throw parameterError("SCC-CAD-014", `${path}.op is unsupported: ${expression.op}`);
  }
  return finiteNumber(result, path);
}

function validateArity(op, length, path) {
  if (new Set(["subtract", "divide"]).has(op) && length === 2) return;
  if (new Set(["sqrt", "negate"]).has(op) && length === 1) return;
  if (new Set(["add", "multiply", "hypot"]).has(op) && length >= 2 && length <= 16) return;
  if (!new Set(["add", "subtract", "multiply", "divide", "sqrt", "negate", "hypot"]).has(op)) {
    throw parameterError("SCC-CAD-014", `${path}.op is unsupported: ${op}`);
  }
  throw parameterError("SCC-CAD-014", `${path}.args has invalid arity for ${op}`);
}

function validateResolvedValue(value, path) {
  finiteNumber(value, path);
  if (Math.abs(value) > MAX_ABSOLUTE_VALUE) {
    throw parameterError("SCC-CAD-014", `${path} is outside the supported range`);
  }
}

function finiteNumber(value, path) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw parameterError("SCC-CAD-010", `${path} must be a finite number`);
  }
  return value;
}

function validateId(value, path) {
  if (typeof value !== "string" || !ID_PATTERN.test(value)) {
    throw parameterError("SCC-CAD-010", `${path} is not a stable ID`);
  }
}

function requireObject(value, path) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw parameterError("SCC-CAD-010", `${path} must be an object`);
  }
}

function rejectUnknown(value, allowed, prefix) {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) throw parameterError("SCC-CAD-010", `Unknown field: ${prefix}${key}`);
  }
}

function parameterError(code, message) {
  return new ServiceError(422, code, message);
}
