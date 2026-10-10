// Phase 10.5 structured change-preview contract. This module is pure: it
// compares formal before/after EngineeringModel snapshots and never mutates one.

export const PREVIEW_SCHEMA_VERSION = 1;

export function buildChangePreview({
  proposalId,
  baseModel,
  candidateModel,
  baseSnapshot = {},
  candidateSnapshot = {},
  baseFingerprint,
  candidateFingerprint,
  commands,
  intentSummary,
}) {
  return {
    schema_version: PREVIEW_SCHEMA_VERSION,
    proposal_id: proposalId,
    base_model_fingerprint: baseFingerprint,
    candidate_fingerprint: candidateFingerprint,
    intent_summary: intentSummary ?? "Proposed engineering change",
    parameter_changes: parameterChanges(baseModel, candidateModel, baseSnapshot, candidateSnapshot),
    feature_changes: featureChanges(baseModel, candidateModel),
    diagnostics: diagnostics(candidateSnapshot),
    downstream_readiness: {
      principal: candidateSnapshot.solve_error ? "not_ready" : "ready",
      three_view: candidateSnapshot.solve_error ? "not_ready" : "not_evaluated",
      step: candidateSnapshot.solve_error ? "not_ready" : "not_evaluated",
    },
    plan: {
      schema_version: 1,
      base_model_fingerprint: baseFingerprint,
      commands: structuredClone(commands ?? []),
      intent_summary: intentSummary ?? "Proposed engineering change",
    },
  };
}

export function parameterChanges(before, after, beforeSnapshot = {}, afterSnapshot = {}) {
  const ids = new Set([
    ...Object.keys(before?.parameters ?? {}),
    ...Object.keys(after?.parameters ?? {}),
  ]);
  const beforeViews = parameterViews(beforeSnapshot, before);
  const afterViews = parameterViews(afterSnapshot, after);
  return [...ids].sort().flatMap((id) => {
    const oldValue = beforeViews[id] ?? parameterView(id, before?.parameters?.[id]);
    const newValue = afterViews[id] ?? parameterView(id, after?.parameters?.[id]);
    if (canonical(oldValue) === canonical(newValue)) return [];
    return [{
      id,
      display_name: id,
      change_kind: newValue?.source_expression ? "derived" : "direct",
      before: oldValue,
      after: newValue,
    }];
  });
}

export function featureChanges(before, after) {
  const ids = new Set([
    ...Object.keys(before?.features ?? {}),
    ...Object.keys(after?.features ?? {}),
  ]);
  return [...ids].sort().flatMap((id) => {
    const oldFeature = before?.features?.[id];
    const newFeature = after?.features?.[id];
    if (!oldFeature) return [{ id, kind: "added", after: structuredClone(newFeature) }];
    if (!newFeature) return [{ id, kind: "removed", before: structuredClone(oldFeature) }];
    if (canonical(oldFeature) === canonical(newFeature)) return [{ id, kind: "unchanged" }];
    return [{ id, kind: "modified", before: structuredClone(oldFeature), after: structuredClone(newFeature) }];
  });
}

function parameterViews(snapshot, model) {
  const values = Object.create(null);
  for (const parameter of snapshot?.workbench?.parameters ?? []) {
    values[parameter.id] = {
      source_expression: parameter.expression ?? null,
      resolved_value: parameter.value ?? null,
      origin: parameter.origin ?? null,
    };
  }
  for (const [id, parameter] of Object.entries(model?.parameters ?? {})) {
    values[id] ??= parameterView(id, parameter);
  }
  return values;
}

function parameterView(_id, parameter) {
  if (!parameter) return null;
  const value = parameter.value;
  const expression = value?.type === "expression" ? value : null;
  return {
    source_expression: expression,
    resolved_value: value?.value ?? value ?? null,
    origin: parameter.origin ?? null,
  };
}

function diagnostics(snapshot) {
  const result = [];
  if (snapshot?.solve_error) result.push(snapshot.solve_error);
  return result;
}

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}
