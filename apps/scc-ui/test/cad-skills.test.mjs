import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { analyzeMechanicalRequest } from "../lib/cad-skills/analysis.mjs";
import { diagnoseEngineeringError } from "../lib/cad-skills/diagnostics.mjs";
import { validateSectionDrawingArtifacts, validateThreeViewArtifacts } from "../lib/cad-skills/export-validation.mjs";
import { loadCadSkills, resolveSkillToolAllowlist } from "../lib/cad-skills/registry.mjs";
import { buildCadSkillPlan } from "../lib/cad-skills/router.mjs";
import { SkillPlanStore, bindSkillPlan } from "../lib/cad-skills/plan-store.mjs";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..", "..");

test("six built-in CAD Skill contracts are declarative, bilingual, and capability-bounded", async () => {
  const skills = await loadCadSkills(resolve(repoRoot, "skills/cad"));
  assert.equal(skills.length, 6);
  for (const skill of skills) {
    for (const field of ["id", "name", "description", "version", "trigger", "input_contract", "required_context", "allowed_tools", "execution_steps", "validation_gates", "failure_policy", "output_contract"]) {
      assert.ok(Object.hasOwn(skill, field), `${skill.id} has ${field}`);
    }
    assert.equal(typeof skill.name["zh-CN"], "string");
    assert.equal(typeof skill.name.en, "string");
    assert.equal(Object.hasOwn(skill, "script"), false);
  }
});

test("request analysis separates explicit, actual derived, ambiguous, conflicting, unsupported, and missing evidence", () => {
  const analysis = analyzeMechanicalRequest({
    text: "安装板宽120 mm，板宽130 mm，长120 mm，厚10 mm，中心Φ38贯穿孔；增加沉头孔和位置度。",
    model: null,
    context: { parameters: [{ id: "derived_d", value: "12", origin: "derived" }, { id: "assumed_d", value: "9", origin: "ai_assumption" }] },
  });
  assert.ok(analysis.explicit.some((item) => item.kind === "base_width" && item.value_mm === 120));
  assert.ok(analysis.derived.some((item) => item.id === "derived_d"));
  assert.equal(analysis.derived.some((item) => item.id === "assumed_d"), false);
  assert.ok(analysis.conflicting.some((item) => item.kind === "base_width"));
  assert.ok(analysis.ambiguous.some((item) => item.type === "countersink_geometry"));
  assert.ok(analysis.unsupported.some((item) => item.type === "gdt"));
  assert.equal(analysis.missing.length, 0);
});

test("MMF-204 full request exposes unsupported requirements without pretending it was generated", async () => {
  const text = await readFile(resolve(repoRoot, "apps/scc-ui/test/fixtures/mmf204-input.txt"), "utf8");
  const analysis = analyzeMechanicalRequest({ text });
  const kinds = new Set(analysis.unsupported.map((item) => item.type));
  for (const expected of ["boss", "rib", "draft", "chamfer", "gdt", "datum", "surface_roughness", "fit_tolerance_class"]) {
    assert.ok(kinds.has(expected), `MMF-204 must identify ${expected}`);
  }
  assert.equal(kinds.has("countersink"), false, "exact Countersink geometry is supported in G6B");
  assert.equal(kinds.has("counterbore"), false, "exact counterbore geometry is supported in G2");
  assert.ok(analysis.explicit.length >= 10, "explicit dimensions remain visible as source evidence");
});

test("G2 analysis recognizes counterbores and accepts a parametric Extrude ADD as the model base", () => {
  const supported = analyzeMechanicalRequest({ text: "在板上加工圆柱沉孔 Φ14 深5mm" });
  assert.equal(supported.unsupported.some((item) => item.type === "counterbore"), false);
  assert.equal(supported.ambiguous.some((item) => item.type === "counterbore_geometry"), false);
  const missingTerm = analyzeMechanicalRequest({ text: "加工一个沉孔", model: null });
  assert.ok(missingTerm.ambiguous.some((item) => item.type === "counterbore_geometry"));
  const parametricModel = {
    part: { id: "plate" },
    features: { base: { type: "parametric", suppressed: false, feature: { type: "extrude", operation: "add", extent: { type: "distance", length: 20 } } } },
  };
  const complete = analyzeMechanicalRequest({ text: "修改板上的孔", model: parametricModel });
  assert.equal(complete.missing.some((item) => item.kind === "base_profile" || item.kind === "thickness"), false);
});

test("G6B recognizes fully specified countersinks and requests missing mouth/angle values without marking the feature unsupported", () => {
  const complete = analyzeMechanicalRequest({ text: "加工沉头孔，主孔径 Ø6 mm，沉头入口直径 Ø10 mm，沉头角 90°。" });
  assert.equal(complete.unsupported.some((item) => item.type === "countersink"), false);
  assert.equal(complete.ambiguous.some((item) => item.type === "countersink_geometry"), false);
  assert.ok(complete.explicit.some((item) => item.kind === "countersink_mouth_diameter" && item.value_mm === 10));
  assert.ok(complete.explicit.some((item) => item.kind === "countersink_included_angle_degrees" && item.value_mm === 90));
  const incomplete = analyzeMechanicalRequest({ text: "加工一个沉头孔，主孔径 Ø6 mm。" });
  assert.equal(incomplete.unsupported.some((item) => item.type === "countersink"), false);
  assert.match(incomplete.ambiguous.find((item) => item.type === "countersink_geometry").message, /入口直径和沉头包含角/);
});

test("G2 diagnostics preserve machine codes and give Chinese recovery guidance", () => {
  for (const code of ["INVALID_FACE_REFERENCE", "AMBIGUOUS_FACE_REFERENCE", "INVALID_TOPOLOGY_REFERENCE", "INVALID_EDGE_REFERENCE", "UNSUPPORTED_TOPOLOGY_REFERENCE", "TOPOLOGY_REFERENCE_NOT_FOUND", "TOPOLOGY_REFERENCE_STALE", "AMBIGUOUS_TOPOLOGY_REFERENCE", "SOURCE_TOPOLOGY_CHANGED", "INVALID_HOLE_DEPTH", "INVALID_COUNTERBORE", "INVALID_COUNTERSINK", "COUNTERSINK_NO_INTERSECTION", "COUNTERSINK_GEOMETRY_FAILED", "PATTERN_INVALID_COUNT"]) {
    const result = diagnoseEngineeringError({ code, message: `raw ${code}` });
    assert.equal(result.code, code);
    assert.equal(result.stage, "parametric_rebuild");
    assert.ok(result.explanation["zh-CN"].length > 0);
    assert.ok(result.recovery["zh-CN"].length > 0);
    assert.equal(result.mutated, false);
  }
});

test("router resolves its permission list against the actual Rust manifest and removes create_part for an existing model", async () => {
  const skills = await loadCadSkills(resolve(repoRoot, "skills/cad"));
  const manifest = [{ name: "create_part" }, { name: "set_parameter" }, { name: "add_hole" }];
  const plan = buildCadSkillPlan({
    skills,
    mode: "edit",
    text: "把板宽改成160 mm",
    model: { part: { id: "mounting_plate" }, features: { plate: { type: "base_profile" } }, spatial: { thickness: { value: 10 } } },
    modelFingerprint: "abc123",
    context: { part: { id: "mounting_plate", name: "Plate" }, features: [{ id: "plate", feature_type: "base_profile" }], parameters: [], unresolved: [] },
    manifest,
  });
  assert.equal(plan.model_state.fingerprint, "abc123");
  assert.deepEqual(plan.allowed_tool_names, ["add_hole", "set_parameter"]);
  assert.ok(plan.allowed_capabilities.includes("drawing.three_view"));
});

test("section drawing capability is explicitly allowlisted and section failures keep actionable codes", async () => {
  const skills = await loadCadSkills(resolve(repoRoot, "skills/cad"));
  const plan = resolveSkillToolAllowlist(skills, ["orthographic-drawing"], []);
  assert.ok(plan.capabilities.includes("drawing.section_a_a"));
  for (const code of ["SECTION_REQUEST_INVALID", "SECTION_NO_INTERSECTION", "SECTION_OPEN_BOUNDARY", "SECTION_GEOMETRY_FAILED"]) {
    const result = diagnoseEngineeringError({ code, message: `raw ${code}` });
    assert.equal(result.code, code);
    assert.equal(result.stage, "section_geometry");
    assert.ok(result.explanation["zh-CN"]);
    assert.ok(result.recovery["zh-CN"]);
    assert.equal(result.mutated, false);
  }
});

test("Section A-A export validation checks actual SVG labels, DXF hatch entities, units, and exact-source evidence", () => {
  const drawing = {
    svg: '<svg><g id="view_front"/><g id="view_top"/><g id="view_left"/><g id="view_section_aa"><g id="section_aa_hatch"/></g><text id="label_esection_aa">剖视 A-A</text></svg>',
    dxf: "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n2\nAICAD_HATCH\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nAICAD_HATCH\n0\nENDSEC\n0\nEOF\n",
    section_view: { axis: "y", offset_mm: 0, view_side: "positive", section_edge_count: 12, boundary_loop_count: 2, sample_chord_tolerance_mm: 0.01 },
    drawing_report: {
      source: "same_formal_drawing_model", drawing_mode: "three_view_section", view_count: 4, primitive_count: 10,
      views: [
        { kind: "front", primitives: [] }, { kind: "top", primitives: [] }, { kind: "left", primitives: [] },
        { kind: "section_aa", primitives: [{ id: "section_edge_0", section_source: { model_sha256: "a".repeat(64) } }] },
      ],
    },
  };
  const report = validateSectionDrawingArtifacts({ drawing, modelFingerprint: "model-v1" });
  assert.equal(report.status, "PASS");
  assert.equal(report.SVG_EXPORT.status, "PASS");
  assert.equal(report.DXF_EXPORT.hatch_layer, true);
  assert.equal(report.DRAWING_COMPLETENESS.status, "PARTIAL");
  assert.equal(report.DRAWING_ANNOTATION.status, "PARTIAL");
  assert.equal(report.UNOBSERVABLE_FEATURE_REPORT[0].formal_dimension_annotation, "UNSUPPORTED");
  const badSvg = validateSectionDrawingArtifacts({ drawing: { ...drawing, svg: drawing.svg.replace("label_esection_aa", "wrong") }, modelFingerprint: "model-v1" });
  assert.equal(badSvg.SVG_EXPORT.status, "FAIL");
  assert.equal(badSvg.DXF_EXPORT.status, "PASS");
});

test("skill plans expire by use and are bound to exact text, mode, and model fingerprint", () => {
  const store = new SkillPlanStore();
  const binding = bindSkillPlan({ mode: "edit", text: "increase width", fingerprint: "same-model" });
  const id = store.put(binding, { ok: true });
  assert.throws(() => store.consume(id, bindSkillPlan({ mode: "edit", text: "other request", fingerprint: "same-model" })), /no longer matches/);
  assert.throws(() => store.consume(id, binding), /missing, expired, or already used/);
});

test("SCC-TOOL-005 explains rejected typed parameter values in Chinese and preserves the raw failure", () => {
  const invalidToolValue = diagnoseEngineeringError({ code: "SCC-TOOL-005", message: "value -1 must be greater than 0" });
  assert.equal(invalidToolValue.code, "SCC-TOOL-005");
  assert.equal(invalidToolValue.stage, "engineering_command");
  assert.match(invalidToolValue.explanation["zh-CN"], /输入校验/);
  assert.match(invalidToolValue.recovery["zh-CN"], /正数的毫米值/);
  assert.equal(invalidToolValue.original_message, "value -1 must be greater than 0");
  assert.equal(invalidToolValue.mutated, false);
});

test("SCC-TOOL-010 explains typed parameter validation without hiding the raw failure", () => {
  const result = diagnoseEngineeringError({ code: "SCC-TOOL-010", message: "parameter reference is stale" });
  assert.equal(result.code, "SCC-TOOL-010");
  assert.equal(result.stage, "engineering_command");
  assert.match(result.explanation["zh-CN"], /参数目标校验/);
  assert.match(result.recovery["zh-CN"], /Feature ID/);
  assert.equal(result.original_message, "parameter reference is stale");
  assert.equal(result.mutated, false);
  const invalidValue = diagnoseEngineeringError({ code: "SCC-ENG-028", message: "length_mm parameter must be positive" });
  assert.equal(invalidValue.stage, "engineering_command");
  assert.match(invalidValue.explanation["zh-CN"], /尺寸值/);
  assert.equal(invalidValue.original_message, "length_mm parameter must be positive");
});

test("planning skill exposes a typed editing tool but requires unique existing Hole selection", async () => {
  const skills = await loadCadSkills(resolve(repoRoot, "skills/cad"));
  const planning = skills.find((skill) => skill.id === "engineering-model-planning");
  assert.ok(planning.allowed_tools.includes("set_feature_parameter"));
  assert.ok(planning.execution_steps.some((step) => step.includes("CREATE_FEATURE") && step.includes("MODIFY_FEATURE_PARAMETER")));
  assert.ok(planning.execution_steps.some((step) => step.includes("multiple existing Hole")));
  assert.ok(planning.validation_gates.some((gate) => gate.includes("through-hole blind depth")));
});

test("diagnostics preserve actual codes and inspect the actual model without mutation", () => {
  const model = { part: { id: "mounting_plate" }, features: {}, spatial: {} };
  const result = diagnoseEngineeringError({ code: "SCC-SPAT-002", message: "no BaseProfile" }, model);
  assert.equal(result.stage, "spatial");
  assert.equal(result.code, "SCC-SPAT-002");
  assert.equal(result.model_facts.base_profile_exists, false);
  assert.equal(result.confirmation_required, true);
  assert.equal(result.mutated, false);
});

test("diagnostics preserve parametric rebuild codes wrapped in the SCC error envelope", () => {
  const result = diagnoseEngineeringError({
    code: "SCC-SPAT-017",
    message: "pattern miss",
    details: { diagnostics: [{ code: "SCC-SPAT-017", details: { parametric_code: "PATTERN_INSTANCE_COLLISION" } }] },
  });
  assert.equal(result.code, "PATTERN_INSTANCE_COLLISION");
  assert.equal(result.stage, "parametric_rebuild");
});

test("requested diameter coverage cannot match an equal-valued linear dimension", () => {
  const drawing = {
    svg: '<svg><g id="view_front"/><g id="view_top"/><g id="view_left"/></svg>',
    dxf: "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n0\nENDSEC\n0\nEOF\n",
    drawing_report: {
      source: "same_formal_drawing_model", drawing_mode: "three_view", view_count: 3, primitive_count: 1,
      views: [
        { kind: "front", dimensions: [{ id: "width_38", kind: { kind: "linear", axis: "horizontal" }, value_mm: [38], target_feature: "plate" }] },
        { kind: "top", dimensions: [{ id: "hole_10", kind: { kind: "diameter" }, value_mm: [10], target_feature: "hole" }] },
        { kind: "left", dimensions: [] },
      ],
    },
  };
  const result = validateThreeViewArtifacts({
    drawing,
    modelFingerprint: "fingerprint",
    analysis: { explicit: [{ kind: "diameter", value_mm: 38, evidence: "Φ38" }], unsupported: [], ambiguous: [] },
  });
  assert.equal(result.requested_coverage[0].status, "RECOGNIZED_BUT_NOT_RENDERED");
  assert.equal(result.requested_coverage[0].drawing_dimension_id, null);
});

test("SVG and DXF export validation are independent and DXF units/entities are inspected", () => {
  const drawing = {
    svg: '<svg><g id="view_front"/><g id="view_top"/><g id="view_left"/></svg>',
    dxf: "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n0\nENDSEC\n0\nEOF\n",
    drawing_report: { source: "same_formal_drawing_model", drawing_mode: "three_view", view_count: 3, primitive_count: 1, views: [{ kind: "front" }, { kind: "top" }, { kind: "left" }] },
  };
  const passed = validateThreeViewArtifacts({ drawing, modelFingerprint: "fingerprint" });
  assert.equal(passed.SVG_EXPORT.status, "PASS");
  assert.equal(passed.DXF_EXPORT.status, "PASS");
  assert.equal(passed.DXF_EXPORT.insunits, 4);
  const broken = validateThreeViewArtifacts({ drawing: { ...drawing, dxf: "" }, modelFingerprint: "fingerprint" });
  assert.equal(broken.SVG_EXPORT.status, "PASS");
  assert.equal(broken.DXF_EXPORT.status, "FAIL");
  assert.equal(broken.status, "FAIL");
});


test("Sweep diagnostics retain stable codes and recognize Sweep ADD as an existing body", () => {
  for (const [code, token] of [["PATH_NON_TANGENT_JOIN", "切向"], ["PATH_SELF_INTERSECTION", "相交"], ["SWEEP_PROFILE_ORIENTATION_INVALID", "起始切线"], ["DETACHED_ADD", "融合"]]) {
    const result = diagnoseEngineeringError({ code, message: `raw ${code}` });
    assert.equal(result.code, code);
    assert.match(result.explanation["zh-CN"], new RegExp(token));
    assert.equal(result.mutated, false);
  }
  const model = { part: { id: "pipe" }, features: { sweep: { type: "parametric", suppressed: false, feature: { type: "sweep", operation: "add" } } } };
  const spatial = diagnoseEngineeringError({ code: "SCC-SPAT-002", message: "missing body" }, model);
  assert.match(spatial.explanation["zh-CN"], /存在 BaseProfile/);
});
