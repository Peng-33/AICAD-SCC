// Rendering helpers for the derived EngineeringContextView.
//
// The context view itself is derived by the Rust CLI (`scc engineering
// context`) from the real EngineeringModel; this module only renders it to the
// compact text that is sent to a provider. It never modifies the model and is
// not a source of truth (ADR-020 Decision 11).

/** Render a context view object to compact text for the provider. */
export function renderContextView(context) {
  if (!context || typeof context !== "object") return "";
  const lines = [];
  if (context.part) lines.push(`Part: ${context.part.id} (${context.part.name})`);
  if (context.features?.length) {
    lines.push("Features:");
    for (const feature of context.features) lines.push(`- ${feature.id}: ${feature.feature_type}`);
  }
  if (context.parameters?.length) {
    lines.push("Parameters:");
    for (const parameter of context.parameters) {
      lines.push(`- ${parameter.id} = ${parameter.value} (${parameter.origin})`);
    }
  }
  if (context.constraints?.length) {
    lines.push("Constraints:");
    for (const constraint of context.constraints) {
      lines.push(`- ${constraint.id}: ${constraint.kind} ${constraint.source} -> ${constraint.target}`);
    }
  }
  if (context.unresolved?.length) {
    lines.push(`Unresolved: ${context.unresolved.join(", ")}`);
  }
  return lines.join("\n");
}
