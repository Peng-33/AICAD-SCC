import assert from "node:assert/strict";
import test from "node:test";
import { setEngineeringLocale, tr, describeEngineeringError } from "../public/js/engineering-i18n.js";

test("Workbench defaults to Chinese in the page and keeps English switching presentation-only", () => {
  setEngineeringLocale("zh-CN");
  assert.equal(tr("Engineering Drawing"), "工程图纸");
  assert.equal(tr("Front / Top / Left"), "主视图 / 俯视图 / 左视图");
  assert.equal(tr("countersink_mouth_diameter"), "沉头入口直径");
  assert.equal(tr("countersink_included_angle_degrees"), "沉头包含角");
  assert.equal(tr("Countersink mouth diameter"), "沉头入口直径");
  assert.equal(tr("Countersink included angle degrees"), "沉头包含角");
  assert.equal(tr("pattern"), "阵列");
  assert.equal(tr("EngineeringModel"), "EngineeringModel");
  setEngineeringLocale("en");
  assert.equal(tr("Engineering Drawing"), "Engineering Drawing");
});

test("Chinese errors retain raw codes and give an actionable next step", () => {
  setEngineeringLocale("zh-CN");
  for (const code of ["SCC-SPAT-002", "SCC-ENG-009", "SCC-ENG-011", "SCC-UI-PROVIDER-003", "SCC-UI-MODEL-008"]) {
    const message = describeEngineeringError({ code, message: "raw detail" });
    assert.match(message, new RegExp(code));
    assert.match(message, /原始信息：raw detail/);
  }
  assert.match(describeEngineeringError({ code: "SCC-SPAT-002", message: "raw" }), /BaseProfile/);
  setEngineeringLocale("en");
});
