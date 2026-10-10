import { analyzeMechanicalRequest } from "./analysis.mjs";
import { CAD_SKILL_IDS, resolveSkillToolAllowlist } from "./registry.mjs";

export function buildCadSkillPlan({ skills, mode, text, model = null, context = null, manifest = [], modelFingerprint = null }) {
  const selectedSkillIds = [
    "mechanical-part-analysis",
    "engineering-model-planning",
    "orthographic-drawing",
    "drawing-annotation",
    "cad-export-validation",
  ];
  const analysis = analyzeMechanicalRequest({ text, model, context });
  const resolved = resolveSkillToolAllowlist(skills, selectedSkillIds, manifest);
  const partExists = Boolean(context?.part ?? model?.part);
  const allowedToolNames = resolved.allowedToolNames.filter((name) => !(mode === "edit" && partExists && name === "create_part"));
  const features = context?.features ?? [];
  const modelState = {
    source: model ? "validated_engineering_model_snapshot" : "empty_workbench",
    fingerprint: model ? modelFingerprint : null,
    part: context?.part ?? model?.part ?? null,
    part_exists: partExists,
    feature_ids: features.map((feature) => feature.id).filter(Boolean),
    base_profile_exists: features.some((feature) => feature.feature_type === "base_profile") || Object.values(model?.features ?? {}).some((feature) => feature?.type === "base_profile"),
    thickness_exists: Number(model?.spatial?.thickness?.value) > 0,
    unresolved: context?.unresolved ?? [],
    features,
  };
  const skillStates = selectedSkillIds.map((id) => ({ id, state: id === "orthographic-drawing" || id === "drawing-annotation" || id === "cad-export-validation" ? "PENDING" : "COMPLETE" }));
  const inactive = CAD_SKILL_IDS.filter((id) => !selectedSkillIds.includes(id));
  return {
    schema_version: 1,
    selected_skill_ids: selectedSkillIds,
    inactive_skill_ids: inactive,
    skill_states: skillStates,
    request_analysis: analysis,
    model_state: modelState,
    allowed_tool_names: allowedToolNames,
    allowed_capabilities: resolved.capabilities,
    unavailable_tool_names: resolved.unavailableToolNames,
    tool_manifest_count: manifest.length,
    provider_calls: 0,
  };
}

export function completeCadSkillRun(plan, result) {
  const run = structuredClone(plan);
  const failed = result?.status === "rejected";
  const succeeded = result?.status === "ok";
  run.provider_calls = result?.trace?.provider_calls ?? null;
  run.intent_status = result?.status ?? "error";
  run.skill_states = run.skill_states.map((item) => {
    if (item.id === "mechanical-part-analysis" || item.id === "engineering-model-planning") return { ...item, state: failed ? "BLOCKED" : "COMPLETE" };
    if (!succeeded) return { ...item, state: "NOT_RUN" };
    return { ...item, state: "PENDING" };
  });
  if (failed && result?.error) {
    run.diagnostic = result.diagnostic;
    run.selected_skill_ids = [...new Set([...run.selected_skill_ids, "engineering-diagnostics"] )];
    run.inactive_skill_ids = run.inactive_skill_ids.filter((id) => id !== "engineering-diagnostics");
    run.skill_states.push({ id: "engineering-diagnostics", state: "COMPLETE" });
  }
  if (result?.status === "needs_clarification") {
    run.intent_status = "needs_clarification";
    run.skill_states = run.skill_states.map((item) => ({ ...item, state: item.id === "mechanical-part-analysis" ? "COMPLETE" : item.state }));
  }
  if (result?.status === "ok") {
    run.command_plan = result.result?.command_plan ?? null;
    run.engineering_commands = result.result?.command_plan?.commands ?? [];
    run.tool_calls = result.result?.tool_calls ?? [];
  }
  return run;
}

export function completeDrawingSkillRun(plan, report) {
  const run = structuredClone(plan);
  run.drawing_report = report;
  if (report.status === "FAIL" && report.diagnostic) {
    run.diagnostic = report.diagnostic;
    run.selected_skill_ids = [...new Set([...run.selected_skill_ids, "engineering-diagnostics"] )];
    run.skill_states.push({ id: "engineering-diagnostics", state: "COMPLETE" });
  }
  run.skill_states = run.skill_states.map((item) => {
    if (item.id === "orthographic-drawing") return { ...item, state: report.DRAWING_CONSISTENCY.status === "PASS" && report.GEOMETRY_VALIDATION.status === "PASS" ? "COMPLETE" : "BLOCKED" };
    if (item.id === "drawing-annotation") return { ...item, state: report.DRAWING_ANNOTATION.status === "PASS" ? "COMPLETE" : "BLOCKED" };
    if (item.id === "cad-export-validation") return { ...item, state: report.SVG_EXPORT.status === "PASS" && report.DXF_EXPORT.status === "PASS" ? "COMPLETE" : "BLOCKED" };
    return item;
  });
  return run;
}
