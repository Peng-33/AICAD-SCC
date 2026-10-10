// Source-only AICAD project helpers. Derived views and STEP bytes never enter a project fingerprint.

export function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

export function fingerprintModel(model) {
  // Product requests use the server-issued SHA-256 fingerprint carried by
  // every Engineering Workbench snapshot. This fallback is only for isolated
  // client tests and never serves as a cache or authorization key.
  return canonicalJson(model);
}

export function safeProjectFilename(name) {
  const stem = String(name ?? "aicad-project")
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^[.-]+|[.-]+$/g, "")
    .slice(0, 80) || "aicad-project";
  return `${stem}.aicad.json`;
}

export function safeStepFilename(name) {
  return safeProjectFilename(name).replace(/\.aicad\.json$/, ".step");
}

export function sourceProject(model, name, description = undefined) {
  const hasParametricHistory = (model?.feature_order?.length ?? 0) > 0
    || Object.values(model?.features ?? {}).some((feature) => feature?.type === "parametric");
  return {
    schema_version: hasParametricHistory ? 2 : 1,
    metadata: { name, ...(description ? { description } : {}) },
    engineering: model,
  };
}
