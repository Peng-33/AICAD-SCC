const UNSUPPORTED_TERMS = [
  ["boss", /(?:\bboss\b|凸台|台阶凸台)/iu],
  ["rib", /(?:\bribs?\b|加强筋|肋板)/iu],
  ["draft", /(?:\bdraft\b|拔模角?|拔模)/iu],
  ["chamfer", /(?:\bchamfers?\b|倒角)/iu],
  ["gdt", /\bGD\s*&?\s*T\b|形位公差|位置度|平面度|同轴度/iu],
  ["datum", /\bdatum\b|基准符号|基准面|基准[A-Z]/iu],
  ["surface_roughness", /\broughness\b|\bRa\s*\d|表面粗糙度|表面加工纹理/iu],
  ["fit_tolerance_class", /\b(?:H|h)\d{1,2}\b|\bIT\d{1,2}\b/iu],
];

export function analyzeMechanicalRequest({ text, model = null, context = null } = {}) {
  const source = typeof text === "string" ? text : "";
  const explicit = extractExplicitEvidence(source);
  const countersinkRequested = /(?:\bcountersink\b|沉头孔|沉头)/iu.test(source);
  const hasCountersinkMouth = /(?:沉头(?:口径|入口径|直径)|入口(?:直径|径)|countersink\s+mouth(?:\s+(?:diameter|dia))?)\s*(?:为|=|[:：])?\s*(?:Ø|ø|Φ|φ)?\s*\d+(?:\.\d+)?/iu.test(source);
  const hasCountersinkAngle = /(?:沉头角|锥角|included\s+angle|countersink\s+angle)\s*(?:为|=|[:：])?\s*\d+(?:\.\d+)?\s*(?:度|°|deg(?:rees)?)?|\b\d+(?:\.\d+)?\s*°/iu.test(source);
  const ambiguous = findMatches(source, [
    ["counterbore_geometry", /(?<!圆柱)沉孔/gu, "需确认这是圆柱沉孔还是沉头孔，并提供直径和深度。"],
    ["target_feature", /(?:那个孔|该孔|之前的孔|the hole|that hole)/giu, "无法仅从请求确定要修改的部件或孔特征，请指定 Feature ID。"],
  ]);
  if (countersinkRequested && (!hasCountersinkMouth || !hasCountersinkAngle)) {
    const absent = [!hasCountersinkMouth ? "沉头入口直径" : null, !hasCountersinkAngle ? "沉头包含角" : null].filter(Boolean).join("和");
    ambiguous.push({ type: "countersink_geometry", evidence: source.match(/(?:\bcountersink\b|沉头孔|沉头)/iu)?.[0] ?? "", span: [0, source.length], message: `缺少明确的${absent}；请补充数值，系统不会猜测标准角度。` });
  }
  const unsupported = findMatches(source, UNSUPPORTED_TERMS.map(([type, expression]) => [type, expression, `当前正式工程与图纸链路不支持 ${type}。`]))
    .map(({ type, evidence, message }) => ({ type, evidence, message }));
  const conflicting = findLabelConflicts(source);
  const derived = (context?.parameters ?? [])
    .filter((parameter) => parameter?.origin === "derived" && parameter.value !== "UNRESOLVED")
    .map((parameter) => ({ id: parameter.id, value: parameter.value, source: "EngineeringContext", origin: "DERIVED" }));
  const missing = findMissingInputs(source, model);
  return { explicit, derived, ambiguous, conflicting, unsupported, missing };
}

function extractExplicitEvidence(text) {
  const evidence = [];
  const add = (item) => evidence.push(item);
  const patterns = [
    ["base_width", /(?:板宽|宽度|宽)\s*(?:为|=|[:：])?\s*(\d+(?:\.\d+)?)\s*(mm|毫米)?/giu],
    ["base_length", /(?:板长|长度|长)\s*(?:为|=|[:：])?\s*(\d+(?:\.\d+)?)\s*(mm|毫米)?/giu],
    ["thickness", /(?:厚度|板厚|厚)\s*(?:为|=|[:：])?\s*(\d+(?:\.\d+)?)\s*(mm|毫米)?/giu],
    ["corner_radius", /(?:四角\s*)?[Rr]\s*(\d+(?:\.\d+)?)\s*(mm|毫米)?/gu],
    ["diameter", /(?:[ΦφØø]|(?:孔径|直径))\s*(\d+(?:\.\d+)?)\s*(mm|毫米)?/giu],
    ["countersink_mouth_diameter", /(?:沉头(?:口径|入口径|直径)|入口(?:直径|径)|countersink\s+mouth(?:\s+(?:diameter|dia))?)\s*(?:为|=|[:：])?\s*(?:Ø|ø|Φ|φ)?\s*(\d+(?:\.\d+)?)\s*(mm|毫米)?/giu],
    ["countersink_included_angle_degrees", /(?:沉头角|锥角|included\s+angle|countersink\s+angle)\s*(?:为|=|[:：])?\s*(\d+(?:\.\d+)?)\s*(?:度|°|deg(?:rees)?)?/giu],
    ["dimension", /(\d+(?:\.\d+)?)\s*(mm|毫米)\b/giu],
  ];
  for (const [kind, regex] of patterns) {
    for (const match of text.matchAll(regex)) {
      const number = Number(match[1]);
      const start = match.index ?? 0;
      add({ kind, value_mm: number, evidence: text.slice(start, start + match[0].length), span: [start, start + match[0].length] });
    }
  }
  const tuple = /(?<![\d.])(\d+(?:\.\d+)?)\s*(?:mm|毫米)?\s*[×xX]\s*(\d+(?:\.\d+)?)(?:\s*(?:mm|毫米))?(?:\s*[×xX]\s*(\d+(?:\.\d+)?))?/giu;
  for (const match of text.matchAll(tuple)) {
    const start = match.index ?? 0;
    add({ kind: "dimension_tuple", values_mm: match.slice(1).filter((value) => value !== undefined).map(Number), evidence: match[0], span: [start, start + match[0].length] });
  }
  const seen = new Set();
  return evidence.filter((item) => {
    const key = `${item.kind}:${item.span[0]}:${item.span[1]}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  }).sort((a, b) => a.span[0] - b.span[0] || a.kind.localeCompare(b.kind));
}

function findLabelConflicts(text) {
  const labels = [
    ["base_width", /(?:板宽|宽度|宽)\s*(?:为|=|[:：])?\s*(\d+(?:\.\d+)?)/giu],
    ["base_length", /(?:板长|长度|长)\s*(?:为|=|[:：])?\s*(\d+(?:\.\d+)?)/giu],
    ["thickness", /(?:厚度|板厚|厚)\s*(?:为|=|[:：])?\s*(\d+(?:\.\d+)?)/giu],
  ];
  const conflicts = [];
  for (const [kind, regex] of labels) {
    const values = [...text.matchAll(regex)].map((match) => ({ value: Number(match[1]), evidence: match[0] }));
    if (new Set(values.map((entry) => entry.value)).size > 1) conflicts.push({ kind, evidence: values, message: `同一尺寸标识出现不同数值：${kind}` });
  }
  return conflicts;
}

function findMatches(text, definitions) {
  const results = [];
  for (const [type, expression, message] of definitions) {
    const matcher = new RegExp(expression.source, expression.flags.includes("g") ? expression.flags : `${expression.flags}g`);
    for (const match of text.matchAll(matcher)) results.push({ type, evidence: match[0], span: [match.index ?? 0, (match.index ?? 0) + match[0].length], message });
  }
  return results.sort((a, b) => a.span[0] - b.span[0]);
}

function findMissingInputs(text, model) {
  const missing = [];
  const features = Object.values(model?.features ?? {});
  const parametricAdds = features.filter((feature) => feature?.type === "parametric" && feature?.suppressed !== true && ["extrude", "revolve", "sweep", "loft"].includes(feature?.feature?.type) && feature?.feature?.operation === "add");
  const hasProfile = features.some((feature) => feature?.type === "base_profile") || parametricAdds.length > 0;
  const parametricBase = parametricAdds.find((feature) => feature.feature.type === "extrude");
  const parametricThickness = parametricBase?.feature?.extent?.type === "distance" ? parametricBase.feature.extent.length : null;
  const thickness = model?.spatial?.thickness?.value ?? parametricThickness;
  const requestsPlate = /板|plate|零件|part/iu.test(text);
  const labelledWidth = /(?:板宽|宽度|宽)\s*\d/iu.test(text);
  const labelledLength = /(?:板长|长度|长)\s*\d/iu.test(text);
  const hasTuple = /\d+(?:\.\d+)?\s*(?:mm|毫米)?\s*[×xX]\s*\d+(?:\.\d+)?/iu.test(text);
  if (requestsPlate && !hasProfile && !labelledWidth && !labelledLength && !hasTuple) missing.push({ kind: "base_profile_dimensions", message: "缺少基础轮廓尺寸；请给出长宽、直径或有序顶点。" });
  const hasThicknessInText = /(?:厚度|板厚|厚)\s*\d|\d+(?:\.\d+)?\s*(?:mm|毫米)?\s*[×xX]\s*\d+(?:\.\d+)?\s*[×xX]\s*\d/iu.test(text);
  if (requestsPlate && !thickness && !hasThicknessInText) missing.push({ kind: "thickness", message: "缺少厚度；SpatialModel 需要正数厚度才能构建三维空间模型。" });
  if (model?.part && !hasProfile) missing.push({ kind: "base_profile", message: "当前 EngineeringModel 中没有 BaseProfile。" });
  return missing;
}
