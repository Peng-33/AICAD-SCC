const REQUIRED_VIEWS = ["front", "top", "left"];

export function validateThreeViewArtifacts({ drawing, modelFingerprint, analysis = null } = {}) {
  const svg = drawing?.svg;
  const dxf = drawing?.dxf;
  const report = drawing?.drawing_report;
  const svgViewEvidence = Object.fromEntries(REQUIRED_VIEWS.map((view) => [view, typeof svg === "string" && countMatches(svg, new RegExp(`id=["']view_${view}["']`, "g")) === 1]));
  const svgPass = typeof svg === "string" && /^\s*<svg\b/.test(svg) && Object.values(svgViewEvidence).every(Boolean);
  const dxfUnits = typeof dxf === "string" ? readDxfInsunits(dxf) : null;
  const dxfEntities = typeof dxf === "string" ? countDxfEntities(dxf) : 0;
  const dxfPass = typeof dxf === "string" && dxf.includes("0\nSECTION\n2\nHEADER") && dxfUnits === 4 && dxf.includes("0\nSECTION\n2\nENTITIES") && dxf.includes("0\nENDSEC") && dxf.trimEnd().endsWith("0\nEOF") && dxfEntities > 0;
  const reportViews = (report?.views ?? []).map((view) => view.kind);
  const drawingConsistency = report?.drawing_mode === "three_view" && REQUIRED_VIEWS.every((view) => reportViews.includes(view)) && reportViews.length === 3 && report?.view_count === 3;
  const geometryPass = Number(report?.primitive_count) > 0 && drawingConsistency && Boolean(modelFingerprint);
  const requestedCoverage = classifyRequestedRequirements(analysis, report);
  return {
    status: svgPass && dxfPass && geometryPass ? "PASS" : "FAIL",
    model_fingerprint: modelFingerprint,
    source: report?.source ?? null,
    SVG_EXPORT: { status: svgPass ? "PASS" : "FAIL", nonempty: Boolean(svg?.trim()), view_evidence: svgViewEvidence },
    DXF_EXPORT: { status: dxfPass ? "PASS" : "FAIL", nonempty: Boolean(dxf?.trim()), insunits: dxfUnits, units: dxfUnits === 4 ? "millimeter" : "unknown", entity_count: dxfEntities },
    DRAWING_CONSISTENCY: { status: drawingConsistency ? "PASS" : "FAIL", drawing_mode: report?.drawing_mode ?? null, projection_method: drawing?.projection_method ?? null, view_kinds: reportViews, all_views_share_drawing_model: report?.source === "same_formal_drawing_model" },
    GEOMETRY_VALIDATION: { status: geometryPass ? "PASS" : "FAIL", primitive_count: Number(report?.primitive_count ?? 0), model_fingerprint: modelFingerprint },
    DRAWING_ANNOTATION: buildAnnotationReport(report, analysis, requestedCoverage),
    requested_coverage: requestedCoverage,
  };
}

function buildAnnotationReport(report, analysis, coverage) {
  const views = report?.views ?? [];
  const dimensions = views.flatMap((view) => (view.dimensions ?? []).map((dimension) => ({ ...dimension, view: view.kind })));
  const centers = views.flatMap((view) => (view.center_geometry ?? []).map((item) => ({ ...item, view: view.kind })));
  const unsupported = (analysis?.unsupported ?? []).map((item) => ({ status: "UNSUPPORTED", ...item }));
  const ambiguous = (analysis?.ambiguous ?? []).map((item) => ({ status: "AMBIGUOUS", ...item }));
  return {
    status: "PASS",
    dimensions,
    center_geometry: centers,
    capabilities: {
      linear_dimensions: statusFromPresence(dimensions.some((dimension) => JSON.stringify(dimension.kind).includes("linear"))),
      diameter_callouts: statusFromPresence(dimensions.some((dimension) => /diameter|hole_callout/i.test(`${JSON.stringify(dimension.kind)} ${dimension.formatted ?? ""}`))),
      pattern_callouts: statusFromPresence(dimensions.some((dimension) => /pattern/i.test(String(dimension.kind)) || /×/.test(dimension.formatted ?? ""))),
      hole_center_marks_and_centerlines: statusFromPresence(centers.length > 0),
      numeric_tolerances: statusFromPresence(dimensions.some((dimension) => dimension.has_tolerance_specification === true || (dimension.specification !== null && dimension.specification !== undefined))),
      gdt_and_datum_symbols: "UNSUPPORTED",
      surface_roughness: "UNSUPPORTED",
      standards_compliance: "NOT_RUN",
    },
    unsupported,
    ambiguous,
    recognized_but_not_rendered: coverage.filter((item) => item.status === "RECOGNIZED_BUT_NOT_RENDERED"),
  };
}

function statusFromPresence(present) { return present ? "PASS" : "NOT_RUN"; }

function classifyRequestedRequirements(analysis, report) {
  if (!analysis) return [];
  const dimensions = (report?.views ?? []).flatMap((view) => view.dimensions ?? []);
  const result = [];
  for (const item of analysis.explicit ?? []) {
    const value = item.value_mm;
    if (value === undefined) continue;
    const expectedKinds = {
      base_width: ["linear"],
      base_length: ["linear"],
      thickness: ["linear"],
      corner_radius: ["radius"],
      diameter: ["diameter", "overall_diameter"],
    }[item.kind] ?? [];
    const matching = dimensions.find((dimension) => {
      const kind = typeof dimension.kind === "string" ? dimension.kind : dimension.kind?.kind;
      if (!expectedKinds.includes(kind)) return false;
      return (Array.isArray(dimension.value_mm) ? dimension.value_mm : [dimension.value_mm])
        .some((actual) => Number.isFinite(value) && Math.abs(Number(actual) - value) < 1e-8);
    });
    result.push({ requirement: item.kind, evidence: item.evidence, status: matching ? "ANNOTATED" : "RECOGNIZED_BUT_NOT_RENDERED", drawing_dimension_id: matching?.id ?? null });
  }
  for (const item of analysis.unsupported ?? []) result.push({ requirement: item.type, evidence: item.evidence, status: "UNSUPPORTED" });
  for (const item of analysis.ambiguous ?? []) result.push({ requirement: item.type, evidence: item.evidence, status: "AMBIGUOUS" });
  return result;
}

function readDxfInsunits(dxf) {
  const match = dxf.match(/\$INSUNITS\s*\n\s*70\s*\n\s*(\d+)/);
  return match ? Number(match[1]) : null;
}

function countDxfEntities(dxf) {
  const start = dxf.indexOf("0\nSECTION\n2\nENTITIES\n");
  if (start < 0) return 0;
  const end = dxf.indexOf("0\nENDSEC", start);
  if (end < 0) return 0;
  return [...dxf.slice(start, end).matchAll(/(?:^|\n)0\n[A-Z][A-Z0-9_]*\n/g)].length - 1;
}

function countMatches(text, regex) { return [...text.matchAll(regex)].length; }


export function validateSectionDrawingArtifacts({ drawing, modelFingerprint } = {}) {
  const svg = drawing?.svg;
  const dxf = drawing?.dxf;
  const report = drawing?.drawing_report;
  const required = [...REQUIRED_VIEWS, "section_aa"];
  const svgViewEvidence = Object.fromEntries(required.map((view) => [view, typeof svg === "string" && countMatches(svg, new RegExp("id=.view_" + view + ".", "g")) === 1]));
  const svgPass = typeof svg === "string" && /^\s*<svg\b/.test(svg) && Object.values(svgViewEvidence).every(Boolean)
    && /id=["']label_esection_aa["']/.test(svg);
  const dxfUnits = typeof dxf === "string" ? readDxfInsunits(dxf) : null;
  const dxfEntities = typeof dxf === "string" ? countDxfEntities(dxf) : 0;
  const dxfPass = typeof dxf === "string" && dxf.includes("0\nSECTION\n2\nHEADER") && dxfUnits === 4
    && dxf.includes("0\nSECTION\n2\nENTITIES") && dxf.includes("0\nENDSEC")
    && dxf.trimEnd().endsWith("0\nEOF") && dxfEntities > 0;
  const reportViews = (report?.views ?? []).map((view) => view.kind);
  const drawingConsistency = report?.drawing_mode === "three_view_section"
    && required.every((view) => reportViews.includes(view)) && reportViews.length === 4 && report?.view_count === 4;
  const section = drawing?.section_view;
  const sectionPass = Number(section?.section_edge_count) > 0 && Number(section?.boundary_loop_count) > 0
    && Number.isFinite(section?.sample_chord_tolerance_mm) && section.sample_chord_tolerance_mm > 0
    && section.sample_chord_tolerance_mm <= 0.01;
  const sectionView = report?.views?.find((view) => view.kind === "section_aa");
  const sectionProvenance = (sectionView?.primitives ?? []).filter((primitive) => primitive.section_source?.model_sha256);
  const provenancePass = sectionProvenance.length > 0 && sectionProvenance.every((primitive) => typeof primitive.section_source.model_sha256 === "string");
  const geometryPass = Number(report?.primitive_count) > 0 && drawingConsistency && sectionPass && provenancePass && Boolean(modelFingerprint);
  const annotationReport = buildAnnotationReport(report, null, []);
  annotationReport.status = "PARTIAL";
  annotationReport.completeness = {
    status: "PARTIAL",
    reason: "EngineeringModel v2 parametric features do not yet produce formally linked DrawingDimension annotations.",
  };
  const drawingCompleteness = {
    status: "PARTIAL",
    annotated_dimension_count: annotationReport.dimensions.length,
    section_geometry_available: sectionPass && provenancePass,
    reason: "Section geometry is verified, but parametric feature dimensions and callouts are not linked into DrawingModel.",
  };
  const unobservableFeatureReport = [{
    requirement: "EngineeringModel v2 machining depths and feature callouts",
    section_geometry: sectionPass && provenancePass ? "OBSERVABLE_IN_SECTION_GEOMETRY" : "NOT_VERIFIED",
    formal_dimension_annotation: "UNSUPPORTED",
    reason: "The current parametric DrawingModel has no typed feature-linked dimension builder; the section must not be described as a dimensioned manufacturing drawing.",
  }];
  return {
    status: svgPass && dxfPass && geometryPass ? "PASS" : "FAIL",
    model_fingerprint: modelFingerprint,
    source: report?.source ?? null,
    SVG_EXPORT: { status: svgPass ? "PASS" : "FAIL", nonempty: Boolean(svg?.trim()), view_evidence: svgViewEvidence, section_label: /label_esection_aa/.test(svg ?? "") },
    DXF_EXPORT: { status: dxfPass ? "PASS" : "FAIL", nonempty: Boolean(dxf?.trim()), insunits: dxfUnits, units: dxfUnits === 4 ? "millimeter" : "unknown", entity_count: dxfEntities, hatch_layer: typeof dxf === "string" && dxf.includes("\n2\nAICAD_HATCH\n") },
    DRAWING_CONSISTENCY: { status: drawingConsistency ? "PASS" : "FAIL", drawing_mode: report?.drawing_mode ?? null, view_kinds: reportViews, all_views_share_drawing_model: report?.source === "same_formal_drawing_model" },
    SECTION_GEOMETRY: { status: sectionPass && provenancePass ? "PASS" : "FAIL", plane: section ? { axis: section.axis, offset_mm: section.offset_mm, view_side: section.view_side } : null, edge_count: section?.section_edge_count ?? 0, loop_count: section?.boundary_loop_count ?? 0, sample_chord_tolerance_mm: section?.sample_chord_tolerance_mm ?? null, typed_provenance_count: sectionProvenance.length },
    GEOMETRY_VALIDATION: { status: geometryPass ? "PASS" : "FAIL", primitive_count: Number(report?.primitive_count ?? 0), model_fingerprint: modelFingerprint },
    DRAWING_COMPLETENESS: drawingCompleteness,
    UNOBSERVABLE_FEATURE_REPORT: unobservableFeatureReport,
    DRAWING_ANNOTATION: annotationReport,
  };
}
