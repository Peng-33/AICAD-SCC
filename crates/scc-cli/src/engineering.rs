//! AICAD engineering intent bridge subcommands.
//!
//! These commands expose the Phase 4/5 engineering pipeline (DSL and tool
//! calls) over the CLI so the Node intent orchestrator can drive the *real*
//! Rust parsers, validators, and solvers. The LLM never talks to Rust directly;
//! it produces DSL text or tool calls that these commands validate and apply.

use scc_core::error::{ErrorStage, SccError, StructuredError};
use std::time::Instant;

fn project_error(error: scc_project::ProjectError) -> SccError {
    SccError::Structured(Box::new(StructuredError::new(
        "SCC-CLI-012",
        ErrorStage::Cli,
        error.to_string(),
    )))
}

fn manufacturing_lowering_error(error: SccError) -> SccError {
    let (cause_code, failure_class, failure_stage) = match &error {
        SccError::Structured(structured) => {
            let parametric_code = structured
                .details
                .as_ref()
                .and_then(|details| details.get("parametric_code"))
                .and_then(serde_json::Value::as_str);
            let cause_code = parametric_code.unwrap_or(&structured.code).to_owned();
            let engineering_validation = is_engineering_validation_code(&cause_code);
            (
                cause_code,
                if engineering_validation {
                    "ENGINEERING_VALIDATION_FAILURE"
                } else {
                    "MANUFACTURING_LOWERING_FAILURE"
                },
                if engineering_validation {
                    "engineering_validation"
                } else {
                    "manufacturing_lowering"
                },
            )
        }
        SccError::StructuredBatch(errors) => {
            let cause_code = errors
                .first()
                .map(|structured| {
                    structured
                        .details
                        .as_ref()
                        .and_then(|details| details.get("parametric_code"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(&structured.code)
                        .to_owned()
                })
                .unwrap_or_else(|| "MANUFACTURING_LOWERING_FAILED".to_owned());
            let engineering_validation = is_engineering_validation_code(&cause_code);
            (
                cause_code,
                if engineering_validation {
                    "ENGINEERING_VALIDATION_FAILURE"
                } else {
                    "MANUFACTURING_LOWERING_FAILURE"
                },
                if engineering_validation {
                    "engineering_validation"
                } else {
                    "manufacturing_lowering"
                },
            )
        }
        _ => (
            "MANUFACTURING_LOWERING_FAILED".to_owned(),
            "MANUFACTURING_LOWERING_FAILURE",
            "manufacturing_lowering",
        ),
    };
    SccError::Structured(Box::new(
        StructuredError::new(
            "SCC-MFG-005",
            ErrorStage::Manufacturing,
            format!("MANUFACTURING_LOWERING_FAILED: {error}"),
        )
        .with_detail(
            "phase_code",
            serde_json::json!("MANUFACTURING_LOWERING_FAILED"),
        )
        .with_detail("failure_class", serde_json::json!(failure_class))
        .with_detail("failure_stage", serde_json::json!(failure_stage))
        .with_detail("error_code", serde_json::json!(cause_code)),
    ))
}

fn is_engineering_validation_code(code: &str) -> bool {
    matches!(
        code,
        "INVALID_FACE_REFERENCE"
            | "AMBIGUOUS_FACE_REFERENCE"
            | "INVALID_DATUM_AXIS"
            | "INVALID_REVOLVE_ANGLE"
            | "UNSUPPORTED_SUPPORT_SURFACE"
            | "INVALID_HOLE_AXIS"
            | "INVALID_HOLE_DIAMETER"
            | "INVALID_HOLE_DEPTH"
            | "INVALID_COUNTERBORE"
            | "INVALID_COUNTERSINK"
            | "INVALID_FILLET_RADIUS"
            | "INVALID_CHAMFER_DISTANCE"
            | "INVALID_EDGE_REFERENCE"
            | "DUPLICATE_EDGE_REFERENCE"
            | "PATTERN_SEED_NOT_FOUND"
            | "PATTERN_INVALID_COUNT"
            | "PATTERN_INVALID_SPACING"
            | "PATTERN_INVALID_AXIS"
            | "PATTERN_INSTANCE_COLLISION"
            | "PATTERN_INVALID_RADIUS"
            | "PATTERN_START_ANGLE_MISMATCH"
    )
}

fn step_backend_error(error: scc_cad_backend::CadBackendError) -> SccError {
    let (code, stage, original_code, original_stage, message) = match error {
        scc_cad_backend::CadBackendError::Backend(failure) => {
            let code = match (failure.stage.as_str(), failure.code.as_str()) {
                ("manufacturing_lowering", "MANUFACTURING_LOWERING_FAILED") => "SCC-MFG-005",
                (_, "COUNTERSINK_GEOMETRY_DEFERRED") | (_, "UNSUPPORTED_MANUFACTURING_FEATURE") => {
                    "SCC-MFG-002"
                }
                (_, "BODY_INVALID")
                | (_, "INVALID_SOLID")
                | (_, "MULTI_BODY_NOT_ALLOWED")
                | (_, "DETACHED_ADD") => "SCC-MFG-006",
                (_, "CAD-STEP-001") => "SCC-MFG-006",
                (_, "CAD-STEP-002") => "SCC-MFG-007",
                (_, "CAD-STEP-003") => "SCC-MFG-008",
                (_, "CAD-STEP-004") | (_, "STEP_EQUIVALENCE_FAILED") => "SCC-MFG-009",
                ("manufacturing_build", _) => "SCC-MFG-006",
                _ => "SCC-MFG-010",
            };
            (
                code,
                ErrorStage::Manufacturing,
                failure.code,
                failure.stage,
                failure.message,
            )
        }
        other => (
            "SCC-MFG-010",
            ErrorStage::Manufacturing,
            "CAD_BACKEND_FAILURE".to_owned(),
            "step_delivery".to_owned(),
            other.to_string(),
        ),
    };
    let failure_class = match original_stage.as_str() {
        "parametric_rebuild" | "manufacturing_build" => "GEOMETRY_BUILD_FAILURE",
        "step_serialization" | "step_reimport" | "step_equivalence" | "step_delivery" => {
            "STEP_VALIDATION_FAILURE"
        }
        _ => "MANUFACTURING_LOWERING_FAILURE",
    };
    SccError::Structured(Box::new(
        StructuredError::new(
            code,
            stage,
            format!("{original_stage}/{original_code}: {message}"),
        )
        .with_detail("backend_stage", serde_json::json!(original_stage))
        .with_detail("backend_code", serde_json::json!(original_code))
        .with_detail("failure_class", serde_json::json!(failure_class))
        .with_detail("failure_stage", serde_json::json!(original_stage))
        .with_detail("error_code", serde_json::json!(original_code)),
    ))
}

/// Validate and serialize the source-only AICAD project envelope.
pub fn project_save(
    model_json: &str,
    name: &str,
    description: Option<&str>,
) -> Result<String, SccError> {
    let model: scc_engineering::model::EngineeringModel =
        serde_json::from_str(model_json).map_err(SccError::from)?;
    scc_project::AicadProject::new(
        scc_project::ProjectMetadata {
            name: name.to_owned(),
            description: description.map(str::to_owned),
        },
        model,
    )
    .map_err(project_error)?
    .to_json()
    .map_err(project_error)
}

/// Validate and canonicalize an AICAD project opened from disk.
pub fn project_open(project_json: &str) -> Result<String, SccError> {
    scc_project::AicadProject::from_json(project_json)
        .map_err(project_error)?
        .to_json()
        .map_err(project_error)
}

/// Lower the source model through spatial/manufacturing and return the exact
/// bytes emitted by the real CadQuery worker after STEP reimport validation.
pub fn export_step(model_json: &str) -> Result<scc_cad_backend::ValidatedStepArtifact, SccError> {
    let model: scc_engineering::model::EngineeringModel =
        serde_json::from_str(model_json).map_err(SccError::from)?;
    if crate::parametric_rebuild::is_parametric_model(&model) {
        let lowering_started = Instant::now();
        let manufacturing = crate::parametric_rebuild::lower_parametric_manufacturing_spec(&model)
            .map_err(manufacturing_lowering_error)?;
        let manufacturing_lowering_ms = lowering_started.elapsed().as_secs_f64() * 1000.0;
        let python = std::env::var_os("SCC_CAD_PYTHON").unwrap_or_else(|| "python3".into());
        let mut artifact = scc_cad_backend::CadBackendClient::new(python)
            .with_args(["-m", "aicad_cad.step_delivery"])
            .execute_formal_step_delivery_artifact(
                scc_cad_backend::FormalManufacturingInput::ProjectV2(&manufacturing),
            )
            .map_err(step_backend_error)?;
        artifact.witness.timings_ms.manufacturing_lowering_ms = manufacturing_lowering_ms;
        artifact.witness.timings_ms.total_ms += manufacturing_lowering_ms;
        return Ok(artifact);
    }
    let lowering_started = Instant::now();
    let solved = scc_engineering::snapshot::SolvedEngineeringSnapshot::solve(&model)
        .map_err(SccError::from)?;
    let spatial = scc_spatial::lower_spatial(scc_spatial::SpatialInputs {
        engineering: &solved.model,
        geometry: &solved.geometry,
        source_map: &solved.source_map,
    })
    .map_err(|error| SccError::Structured(Box::new(StructuredError::from(error))))?;
    let spec = scc_manufacturing::lower_solid_build_spec(&spatial)
        .map_err(|error| SccError::Structured(Box::new(StructuredError::from(error))))?;
    let manufacturing_lowering_ms = lowering_started.elapsed().as_secs_f64() * 1000.0;
    let python = std::env::var_os("SCC_CAD_PYTHON").unwrap_or_else(|| "python3".into());
    let mut artifact = scc_cad_backend::CadBackendClient::new(python)
        .with_args(["-m", "aicad_cad.step_delivery"])
        .execute_step_delivery_artifact(&spec)
        .map_err(step_backend_error)?;
    artifact.witness.timings_ms.manufacturing_lowering_ms = manufacturing_lowering_ms;
    artifact.witness.timings_ms.total_ms += manufacturing_lowering_ms;
    Ok(artifact)
}

/// Emit the provider-neutral tool manifest from the Phase 5 registry.
///
/// The Node side consumes this so its tool schema is always generated from the
/// Rust `scc-tools` registry (no second hand-maintained tool definition).
pub fn tool_manifest() -> Result<String, SccError> {
    let registry = scc_tools::ToolRegistry::standard();
    serde_json::to_string_pretty(&registry.list()).map_err(SccError::from)
}

/// Apply DSL text to a fresh model and emit the resulting model JSON.
pub fn dsl_apply(dsl: &str) -> Result<String, SccError> {
    let commands = scc_dsl::command_builder::compile(dsl)
        .map_err(|e| SccError::Structured(Box::new(e.into())))?;
    let mut model = scc_engineering::model::EngineeringModel::new();
    scc_engineering::executor::CommandExecutor::apply_all(&mut model, &commands)
        .map_err(|e| SccError::Structured(Box::new(e.into())))?;
    serde_json::to_string_pretty(&model).map_err(SccError::from)
}

/// Solve a model and emit `{ model, proposal, geometry, workbench, source_map,
/// entity_to_feature }`.
fn solve_payload(
    model: scc_engineering::model::EngineeringModel,
) -> Result<serde_json::Value, SccError> {
    if crate::parametric_rebuild::is_parametric_model(&model) {
        // Generic parametric history has its own validated CadQuery/OCP and
        // formal three-view path. Do not route it through the legacy SCC
        // Proposal lowering, which cannot represent Sketch/Extrude/Hole/
        // Pattern history and would reject a valid EngineeringModel v2.
        let drawing = drawing_three_view(&serde_json::to_string(&model).map_err(SccError::from)?)?;
        let parametric_drawing: serde_json::Value =
            serde_json::from_str(&drawing).map_err(SccError::from)?;
        let workbench = model.workbench_view();
        return Ok(serde_json::json!({
            "model": model,
            "proposal": null,
            "geometry": null,
            "workbench": workbench,
            "source_map": null,
            "entity_to_feature": null,
            "diagnostics": [],
            "parametric_drawing": parametric_drawing,
        }));
    }
    let snapshot = scc_engineering::snapshot::SolvedEngineeringSnapshot::solve(&model)
        .map_err(SccError::from)?;
    let entity_to_feature = snapshot.source_map.entity_to_feature();
    Ok(serde_json::json!({
        "model": snapshot.model,
        "proposal": snapshot.proposal,
        "geometry": snapshot.geometry,
        "workbench": model.workbench_view(),
        "source_map": snapshot.source_map,
        "entity_to_feature": entity_to_feature,
        "diagnostics": snapshot.diagnostics,
    }))
}

/// Solve an existing source model once and emit a complete derived snapshot.
pub fn snapshot(model_json: &str) -> Result<String, SccError> {
    let model: scc_engineering::model::EngineeringModel =
        serde_json::from_str(model_json).map_err(SccError::from)?;
    serde_json::to_string_pretty(&solve_payload(model)?).map_err(SccError::from)
}

/// Apply DSL text to a fresh model, solve through the SCC pipeline, and emit
/// the [`solve_payload`] shape.
pub fn dsl_solve(dsl: &str) -> Result<String, SccError> {
    let commands = scc_dsl::command_builder::compile(dsl)
        .map_err(|e| SccError::Structured(Box::new(e.into())))?;
    let mut model = scc_engineering::model::EngineeringModel::new();
    scc_engineering::executor::CommandExecutor::apply_all(&mut model, &commands)
        .map_err(|e| SccError::Structured(Box::new(e.into())))?;
    let mut payload = solve_payload(model)?;
    payload["command_plan"] = serde_json::json!({
        "schema_version": 1,
        "commands": commands,
    });
    serde_json::to_string_pretty(&payload).map_err(SccError::from)
}

/// Apply a batch of tool calls (JSON) to a model (or a fresh one) and emit the
/// resulting model JSON (or `{model, proposal, geometry}` with `solve`).
/// All tool calls are applied atomically.
pub fn tools_apply(
    toolcalls_json: &str,
    model_json: Option<&str>,
    solve: bool,
) -> Result<String, SccError> {
    let calls: Vec<scc_tools::ToolCall> =
        serde_json::from_str(toolcalls_json).map_err(SccError::from)?;
    let mut model = match model_json {
        Some(json) => serde_json::from_str(json).map_err(SccError::from)?,
        None => scc_engineering::model::EngineeringModel::new(),
    };
    let applied = scc_tools::executor::apply_tool_calls_atomically(&mut model, &calls)
        .map_err(|e| SccError::Structured(Box::new(e.into())))?;
    let commands = applied
        .into_iter()
        .flat_map(|result| result.applied_commands)
        .collect::<Vec<_>>();
    if solve {
        let mut payload = solve_payload(model)?;
        payload["command_plan"] = serde_json::json!({
            "schema_version": 1,
            "commands": commands,
        });
        serde_json::to_string_pretty(&payload).map_err(SccError::from)
    } else {
        serde_json::to_string_pretty(&model).map_err(SccError::from)
    }
}

/// Derive the read-only workbench view from a model JSON file.
pub fn workbench(model_json: &str) -> Result<String, SccError> {
    let model: scc_engineering::model::EngineeringModel =
        serde_json::from_str(model_json).map_err(SccError::from)?;
    serde_json::to_string_pretty(&model.workbench_view()).map_err(SccError::from)
}

/// Build the Phase 8 presentation drawing for a model and emit
/// `{svg, sheet_width_mm, sheet_height_mm, scale, part_title}`.
///
/// Pure transport over `scc-drawing`: lower + solve, build the derived
/// [`scc_drawing::DrawingModel`], render it to SVG, and report the sheet
/// metadata. No printing, no logging; errors propagate unchanged via the
/// existing `From<DrawingError> for SccError` conversion (SCC-DRW-*).
pub fn drawing(model_json: &str) -> Result<String, SccError> {
    let model: scc_engineering::model::EngineeringModel =
        serde_json::from_str(model_json).map_err(SccError::from)?;
    let solved = scc_engineering::snapshot::SolvedEngineeringSnapshot::solve(&model)
        .map_err(SccError::from)?;
    let inputs = scc_drawing::builder::DrawingInputs::new(
        &solved.geometry,
        &solved.source_map,
        &model.drawing,
        &model.features,
        Some(&model),
        model.part.as_ref(),
    );
    let drawing = scc_drawing::builder::build_drawing(&inputs)?;
    let bounds = drawing.principal_sheet().map(|sheet| sheet.bounds());
    let scale = drawing.principal_view().map(|view| view.transform.scale.0);
    let part_title = drawing.principal_sheet().and_then(|sheet| {
        sheet
            .annotations
            .get(&scc_drawing::DrawingAnnotationId("part_title".into()))
            .map(|annotation| annotation.text.clone())
    });
    let svg = scc_drawing::drawing_to_svg(&drawing)?;
    let dxf = scc_drawing::drawing_to_dxf(&drawing)?;
    let payload = serde_json::json!({
        "svg": svg,
        "dxf": dxf,
        "sheet_width_mm": bounds.map(|b| b.max_x - b.min_x),
        "sheet_height_mm": bounds.map(|b| b.max_y - b.min_y),
        "scale": scale,
        "part_title": part_title,
    });
    serde_json::to_string(&payload).map_err(SccError::from)
}

/// Build the formal Phase 9 Front/Top/Left presentation drawing and emit
/// `{svg, sheet_width_mm, sheet_height_mm, scale, part_title,
/// projection_method, drawing_mode}`.
///
/// This is intentionally only a CLI orchestration boundary. The formal
/// spatial, projection, visibility, consistency, drawing-layout, and SVG
/// semantics remain in their owning crates. In particular,
/// `build_three_view_drawing` owns the single projection-consistency gate.
pub fn drawing_three_view(model_json: &str) -> Result<String, SccError> {
    let model: scc_engineering::model::EngineeringModel =
        serde_json::from_str(model_json).map_err(SccError::from)?;
    if crate::parametric_rebuild::is_parametric_model(&model) {
        let rebuilt = crate::parametric_rebuild::rebuild_spatial(&model)?;
        return render_three_view(&model, &rebuilt.spatial, Some(rebuilt.timings), None);
    }
    let solved = scc_engineering::snapshot::SolvedEngineeringSnapshot::solve(&model)
        .map_err(SccError::from)?;
    let spatial = scc_spatial::lower_spatial(scc_spatial::SpatialInputs {
        engineering: &solved.model,
        geometry: &solved.geometry,
        source_map: &solved.source_map,
    })
    .map_err(|error| SccError::Structured(Box::new(StructuredError::from(error))))?;
    render_three_view(&solved.model, &spatial, None, None)
}

/// Emit a contract-valid drawing code and keep the semantic cause for the UI.
///
/// ADR-002 requires `SCC-DRW-NNN`. The Workbench still keys guidance on the
/// historical `SECTION_*` name, carried in `details.parametric_code`.
fn drawing_semantic_error(code: &str, semantic: &str, message: &str) -> SccError {
    SccError::Structured(Box::new(
        StructuredError::new(code, ErrorStage::Drawing, message).with_detail(
            "parametric_code",
            serde_json::Value::String(semantic.to_owned()),
        ),
    ))
}

pub fn drawing_section_a_a(
    model_json: &str,
    axis: scc_cad_backend::ParametricSectionAxis,
    offset_mm: f64,
    view_side: scc_cad_backend::ParametricSectionViewSide,
) -> Result<String, SccError> {
    let model: scc_engineering::model::EngineeringModel =
        serde_json::from_str(model_json).map_err(SccError::from)?;
    if !crate::parametric_rebuild::is_parametric_model(&model) {
        return Err(drawing_semantic_error(
            "SCC-DRW-024",
            "SECTION_PARAMETRIC_REBUILD_REQUIRED",
            "formal Section A-A requires an EngineeringModel v2 parametric rebuild",
        ));
    }
    let rebuilt = crate::parametric_rebuild::rebuild_spatial_with_section(
        &model,
        scc_cad_backend::ParametricSectionPlane {
            axis,
            offset_mm,
            view_side,
        },
    )?;
    let section = rebuilt.section_geometry.ok_or_else(|| {
        drawing_semantic_error(
            "SCC-DRW-025",
            "SECTION_GEOMETRY_MISSING",
            "OCP worker omitted section geometry",
        )
    })?;
    render_three_view(
        &model,
        &rebuilt.spatial,
        Some(rebuilt.timings),
        Some(section),
    )
}

fn render_three_view(
    engineering: &scc_engineering::model::EngineeringModel,
    spatial: &scc_spatial::SpatialModel,
    rebuild_timings: Option<crate::parametric_rebuild::RebuildTimings>,
    section_geometry: Option<scc_cad_backend::ParametricSectionGeometry>,
) -> Result<String, SccError> {
    let projection_visibility_started = std::time::Instant::now();
    let projection = scc_projection::project_principal_views(spatial)
        .map_err(|error| SccError::Structured(Box::new(StructuredError::from(error))))?;
    let visibility = scc_visibility::classify_visibility(spatial, &projection)
        .map_err(|error| SccError::Structured(Box::new(StructuredError::from(error))))?;
    let projection_visibility_ms = projection_visibility_started.elapsed().as_secs_f64() * 1000.0;
    let drawing_started = std::time::Instant::now();
    let projection_method = match engineering
        .drawing
        .get(scc_engineering::feature::DrawingPropertyKind::ProjectionMethod)
    {
        Some(scc_engineering::feature::DrawingProperty::ProjectionMethod { method }) => {
            Some(match method {
                scc_engineering::feature::ProjectionMethod::FirstAngle => "first_angle",
                scc_engineering::feature::ProjectionMethod::ThirdAngle => "third_angle",
            })
        }
        _ => None,
    };
    let mut drawing =
        scc_drawing::build_three_view_drawing(&scc_drawing::ThreeViewDrawingInputs {
            engineering,
            spatial,
            projection: &projection,
            visibility: &visibility,
        })?;
    let section_report = if let Some(section) = &section_geometry {
        let axis = match section.plane.axis {
            scc_cad_backend::ParametricSectionAxis::X => scc_drawing::SectionAxis::X,
            scc_cad_backend::ParametricSectionAxis::Y => scc_drawing::SectionAxis::Y,
            scc_cad_backend::ParametricSectionAxis::Z => scc_drawing::SectionAxis::Z,
        };
        let view_side = match section.plane.view_side {
            scc_cad_backend::ParametricSectionViewSide::Positive => {
                scc_drawing::SectionViewSide::Positive
            }
            scc_cad_backend::ParametricSectionViewSide::Negative => {
                scc_drawing::SectionViewSide::Negative
            }
        };
        let loops = section
            .loops
            .iter()
            .map(|loop_| {
                loop_
                    .iter()
                    .map(|point| scc_drawing::DrawingPoint::new(point[0], point[1]))
                    .collect()
            })
            .collect();
        Some(scc_drawing::append_section_aa(
            &mut drawing,
            scc_drawing::SectionViewGeometry {
                model_sha256: section.model_sha256.clone(),
                axis,
                offset_mm: section.plane.offset_mm,
                view_side,
                loops,
                sample_chord_tolerance_mm: section.sample_chord_tolerance_mm,
            },
        )?)
    } else {
        None
    };
    let drawing_integration_ms = drawing_started.elapsed().as_secs_f64() * 1000.0;
    let svg_started = std::time::Instant::now();
    let svg = scc_drawing::drawing_to_svg(&drawing)?;
    let svg_export_ms = svg_started.elapsed().as_secs_f64() * 1000.0;
    let dxf_started = std::time::Instant::now();
    let dxf = scc_drawing::drawing_to_dxf(&drawing)?;
    let dxf_export_ms = dxf_started.elapsed().as_secs_f64() * 1000.0;
    let sheet = drawing.principal_sheet().ok_or_else(|| {
        SccError::Structured(Box::new(StructuredError::new(
            "SCC-DRW-021",
            ErrorStage::Drawing,
            "three-view drawing has no sheet",
        )))
    })?;
    let scale = sheet
        .views
        .values()
        .next()
        .map(|view| view.transform.scale.0)
        .ok_or_else(|| {
            SccError::Structured(Box::new(StructuredError::new(
                "SCC-DRW-021",
                ErrorStage::Drawing,
                "three-view drawing has no views",
            )))
        })?;
    let bounds = sheet.bounds();
    let part_title = sheet
        .annotations
        .get(&scc_drawing::DrawingAnnotationId("part_title".into()))
        .map(|annotation| annotation.text.clone());
    let projection_method = projection_method.ok_or_else(|| {
        SccError::Structured(Box::new(StructuredError::new(
            "SCC-DRW-019",
            ErrorStage::Drawing,
            "drawing projection method is missing",
        )))
    })?;
    let drawing_report = three_view_drawing_report(&drawing);
    let parametric_spatial = match &spatial.body.footprint {
        scc_spatial::SpatialFootprint::Parametric { evidence } => serde_json::json!({
            "model_sha256": evidence.model_sha256,
            "volume_mm3": evidence.volume_mm3,
            "operation_features": evidence.operation_features,
            "edge_count": evidence.edges.len(),
            "face_count": evidence.faces.len(),
            "cylindrical_face_count": evidence.faces.iter().filter(|face| matches!(&face.surface, scc_spatial::SpatialFaceSurface::Cylinder { .. })).count(),
            "toroidal_face_count": evidence.faces.iter().filter(|face| matches!(&face.surface, scc_spatial::SpatialFaceSurface::Torus { .. })).count(),
            "conical_face_count": evidence.faces.iter().filter(|face| matches!(&face.surface, scc_spatial::SpatialFaceSurface::Cone { .. })).count(),
            "derived_projected_edge_count": evidence.views.values().flatten().filter(|edge| edge.derived_approximation).count(),
            "projected_edge_count": evidence.views.iter().map(|(view, edges)| (format!("{:?}", view).to_lowercase(), edges.len())).collect::<std::collections::BTreeMap<_, _>>(),
        }),
        _ => serde_json::Value::Null,
    };
    let parametric_timings = rebuild_timings.map(|timings| {
        serde_json::json!({
            "history_lowering_ms": timings.history_lowering_ms,
            "kernel_rebuild_ms": timings.kernel_rebuild_ms,
            "combined_fixture_ms": timings.kernel_rebuild_ms,
            "topology_resolution_ms": timings.topology_resolution_ms,
            "base_rebuild_ms": timings.base_rebuild_ms,
            "fillet_rebuild_ms": timings.fillet_rebuild_ms,
            "chamfer_rebuild_ms": timings.chamfer_rebuild_ms,
            "operation_timings": timings.operation_timings,
            "spatial_lowering_ms": timings.spatial_lowering_ms,
            "total_rebuild_ms": timings.total_rebuild_ms,
            "projection_visibility_ms": projection_visibility_ms,
            "drawing_integration_ms": drawing_integration_ms,
            "three_view_generation_ms": projection_visibility_ms + drawing_integration_ms,
            "svg_export_ms": svg_export_ms,
            "dxf_export_ms": dxf_export_ms,
            "section_geometry_ms": section_geometry.as_ref().map(|section| section.geometry_elapsed_ms),
        })
    });
    let payload = serde_json::json!({
        "svg": svg,
        "dxf": dxf,
        "sheet_width_mm": bounds.max_x - bounds.min_x,
        "sheet_height_mm": bounds.max_y - bounds.min_y,
        "scale": scale,
        "part_title": part_title,
        "projection_method": projection_method,
        "drawing_mode": if section_geometry.is_some() { "three_view_section" } else { "three_view" },
        "section_view": section_geometry.as_ref().map(|section| serde_json::json!({
            "label": "section_a_a",
            "axis": section.plane.axis,
            "offset_mm": section.plane.offset_mm,
            "view_side": section.plane.view_side,
            "section_edge_count": section.edge_count,
            "boundary_loop_count": section.loops.len(),
            "sample_chord_tolerance_mm": section.sample_chord_tolerance_mm,
            "geometry_elapsed_ms": section.geometry_elapsed_ms,
            "drawing_report": section_report,
        })),
        "drawing_report": drawing_report,
        "spatial_summary": {
            "bounds": spatial.bounds,
            "axis_extent_mm": spatial.body.z.max - spatial.body.z.min,
            "parametric": parametric_spatial,
        },
        "parametric_timings_ms": parametric_timings,
    });
    serde_json::to_string(&payload).map_err(SccError::from)
}

/// Summarize typed annotation and view evidence from the exact DrawingModel
/// used by both formal renderers. This is transport metadata only; it is not a
/// second drawing representation or a new validation path.
fn three_view_drawing_report(drawing: &scc_drawing::DrawingModel) -> serde_json::Value {
    let mut views = Vec::new();
    let mut primitive_count = 0usize;
    for sheet in drawing.sheets.values() {
        for view in sheet.views.values() {
            primitive_count += view.primitives.len();
            let dimensions = view.dimensions.values().map(|dimension| {
                let target_feature = match &dimension.target {
                    scc_drawing::DrawingDimensionTarget::Body => "body".to_owned(),
                    scc_drawing::DrawingDimensionTarget::Feature { feature } => feature.0.clone(),
                };
                let value = serde_json::to_value(&dimension.value).unwrap_or(serde_json::Value::Null);
                let mut value_mm = Vec::new();
                collect_mm_values(&value, &mut value_mm);
                serde_json::json!({
                    "id": dimension.id.0.clone(),
                    "kind": serde_json::to_value(&dimension.kind).unwrap_or(serde_json::Value::Null),
                    "target_feature": target_feature,
                    "value_mm": value_mm,
                    "formatted": scc_drawing::format_dimension(dimension),
                    "has_tolerance_specification": dimension.specification.is_some(),
                })
            }).collect::<Vec<_>>();
            let associative_dimensions = drawing
                .associative_dimensions
                .values()
                .filter(|association| association.view == view.id)
                .map(|association| serde_json::json!({
                    "annotation_id": association.annotation_id,
                    "drawing_dimension_id": association.drawing_dimension.as_ref().map(|id| id.0.clone()),
                    "kind": association.kind,
                    "reference": association.reference,
                    "nominal": association.nominal,
                    "unit": association.unit,
                    "sources": association.sources,
                    "provenance": association.provenance,
                    "witness": association.witness,
                    "observability": association.observability,
                    "status": association.status,
                    "diagnostic": association.diagnostic,
                }))
                .collect::<Vec<_>>();
            let primitives = view
                .primitives
                .iter()
                .map(|(id, primitive)| {
                    serde_json::json!({
                        "id": id.0,
                        "geometry": primitive,
                        "projection_primitive_id": drawing.source_map.projected_for_primitive(id).map(|source| source.0.clone()),
                        "section_source": drawing.source_map.section_for_primitive(id),
                    })
                })
                .collect::<Vec<_>>();
            let center_geometry = view
                .center_geometry
                .values()
                .map(|item| {
                    let (kind, feature) = match item {
                        scc_drawing::DrawingCenterGeometry::CenterMark(mark) => {
                            ("center_mark", mark.source_feature.0.clone())
                        }
                        scc_drawing::DrawingCenterGeometry::Centerline(line) => {
                            ("centerline", line.source_feature.0.clone())
                        }
                    };
                    serde_json::json!({ "kind": kind, "source_feature": feature })
                })
                .collect::<Vec<_>>();
            views.push(serde_json::json!({
                "kind": view.kind,
                "primitive_count": view.primitives.len(),
                "primitives": primitives,
                "dimensions": dimensions,
                "associative_dimensions": associative_dimensions,
                "center_geometry": center_geometry,
            }));
        }
    }
    serde_json::json!({
        "source": "same_formal_drawing_model",
        "drawing_mode": if views.iter().any(|view| view["kind"] == "section_aa") { "three_view_section" } else { "three_view" },
        "view_count": views.len(),
        "primitive_count": primitive_count,
        "associative_dimension_count": drawing.associative_dimensions.len(),
        "associative_dimensions": drawing.associative_dimensions.values().map(|association| serde_json::json!({
            "annotation_id": association.annotation_id,
            "drawing_dimension_id": association.drawing_dimension.as_ref().map(|id| id.0.clone()),
            "kind": association.kind,
            "reference": association.reference,
            "nominal": association.nominal,
            "unit": association.unit,
            "sources": association.sources,
            "provenance": association.provenance,
            "witness": association.witness,
            "observability": association.observability,
            "status": association.status,
            "diagnostic": association.diagnostic,
        })).collect::<Vec<_>>(),
        "views": views,
    })
}

fn collect_mm_values(value: &serde_json::Value, output: &mut Vec<f64>) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, item) in fields {
                if key == "mm" {
                    if let Some(number) = item.as_f64() {
                        output.push(number);
                    }
                } else {
                    collect_mm_values(item, output);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_mm_values(item, output);
            }
        }
        _ => {}
    }
}

/// Derive the read-only LLM context view from a model JSON file.
pub fn context(model_json: &str) -> Result<String, SccError> {
    let model: scc_engineering::model::EngineeringModel =
        serde_json::from_str(model_json).map_err(SccError::from)?;
    serde_json::to_string_pretty(&model.context_view()).map_err(SccError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_engineering::command::EngineeringCommand;
    use scc_engineering::executor::CommandExecutor;
    use scc_engineering::feature::{DrawingProperty, EngineeringFeature, ProjectionMethod};
    use scc_engineering::spatial::{HoleExtent, SourcedLength};
    use scc_engineering::{
        DecimalPlaces, DimensionalSpecification, DimensionalTolerance, EngineeringDimensionRef,
        EngineeringModel, FeatureId, PartId,
    };

    const DSL_PLATE: &str =
        "AICAD 1\nPART mounting_plate\nBASE_RECT main\n  WIDTH 120\n  HEIGHT 80\n";

    const DSL_TITLED_PLATE: &str = "AICAD 1\nPART mounting_plate \"Mounting Plate\"\n\
PARAM plate_width = 120\nPARAM plate_height = 80\nPARAM edge_offset = 15\n\
PARAM hole_diameter := plate_height / 8\nBASE_RECT main\n  WIDTH 120\n  HEIGHT 80\n\
BIND main WIDTH <- plate_width\nBIND main HEIGHT <- plate_height\n\
HOLE_PATTERN mounting CORNER\n  BASE main\n  OFFSET_X 15\n  OFFSET_Y 15\n  DIA 10\n\
BIND mounting OFFSET_X <- edge_offset\nBIND mounting OFFSET_Y <- edge_offset\n\
BIND mounting DIAMETER <- hole_diameter\n";

    fn model_json_from_dsl(dsl: &str) -> String {
        dsl_apply(dsl).unwrap()
    }

    #[test]
    fn tool_manifest_lists_the_thirty_five_tools_including_path_and_sweep() {
        let json = tool_manifest().unwrap();
        let tools: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(tools.len(), 37);
        let names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"add_hole".to_string()));
        assert!(names.contains(&"add_slot".to_string()));
        assert!(names.contains(&"create_base_rounded_rectangle".to_string()));
        assert!(names.contains(&"create_path".to_string()));
        assert!(names.contains(&"create_sweep".to_string()));
        assert!(names.contains(&"create_loft".to_string()));
        assert!(names.contains(&"create_datum_plane".to_string()));
        assert!(names.contains(&"set_hole_extent".to_string()));
        assert!(names.contains(&"set_part_thickness".to_string()));
        assert!(names.contains(&"set_expression".to_string()));
        assert!(names.contains(&"create_fillet".to_string()));
        assert!(names.contains(&"create_chamfer".to_string()));
        assert!(names.contains(&"add_parametric_rectangular_add".to_string()));
        assert!(!names.contains(&"add_fillet".to_string()));
    }

    #[test]
    fn dsl_apply_produces_a_valid_model() {
        let json = dsl_apply(DSL_PLATE).unwrap();
        let model: scc_engineering::model::EngineeringModel = serde_json::from_str(&json).unwrap();
        model.validate_model().unwrap();
        assert_eq!(model.features.len(), 1);
    }

    #[test]
    fn v1_section_fails_closed_with_a_contract_code() {
        let json = model_json_from_dsl(DSL_PLATE);
        let err = drawing_section_a_a(
            &json,
            scc_cad_backend::ParametricSectionAxis::X,
            0.0,
            scc_cad_backend::ParametricSectionViewSide::Positive,
        )
        .unwrap_err();
        let scc_core::error::SccError::Structured(error) = err else {
            panic!("section rejection must stay a structured error");
        };
        assert_eq!(error.code, "SCC-DRW-024");
        error.validate_contract().expect("ADR-002 contract");
        assert_eq!(
            error.details.as_ref().unwrap()["parametric_code"],
            "SECTION_PARAMETRIC_REBUILD_REQUIRED"
        );
    }

    #[test]
    fn dsl_solve_produces_model_and_geometry() {
        let json = dsl_solve(DSL_PLATE).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["geometry"]["entities"].is_object());
        assert!(value["model"].is_object());
        assert_eq!(value["command_plan"]["commands"][0]["op"], "create_part");
    }

    #[test]
    fn invalid_dsl_returns_structured_error() {
        // CENTER with a single number is a DSL syntax error.
        let err = dsl_apply("AICAD 1\nHOLE h\n  CENTER 0\n").unwrap_err();
        match err {
            scc_core::error::SccError::Structured(structured) => {
                assert_eq!(structured.stage, scc_core::error::ErrorStage::Dsl);
                assert!(structured.code.starts_with("SCC-DSL-"));
            }
            other => panic!("expected structured DSL error, got {other:?}"),
        }
    }

    #[test]
    fn tools_apply_applies_a_batch_to_a_fresh_model() {
        let calls = serde_json::json!([
            { "name": "create_part", "arguments": { "id": "p" } },
            { "name": "create_base_rectangle", "arguments": { "id": "main", "width": 10.0, "height": 10.0 } }
        ]);
        let json = tools_apply(&calls.to_string(), None, false).unwrap();
        let model: scc_engineering::model::EngineeringModel = serde_json::from_str(&json).unwrap();
        assert!(model
            .features
            .contains_key(&scc_engineering::FeatureId("main".into())));
    }

    #[test]
    fn drawing_emits_svg_with_groups_and_metadata() {
        let model_json = model_json_from_dsl(DSL_TITLED_PLATE);
        let json = drawing(&model_json).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let svg = value["svg"].as_str().unwrap();
        assert!(svg.starts_with("<svg"), "svg must start with <svg");
        for group in [
            "objects",
            "construction",
            "centers",
            "dimensions",
            "annotations",
        ] {
            assert!(
                svg.contains(&format!("id=\"{}\"", group)),
                "missing group id={}",
                group
            );
        }
        assert_eq!(value["part_title"].as_str().unwrap(), "Mounting Plate");
        assert!(value["sheet_width_mm"].as_f64().unwrap() > 0.0);
        assert!(value["sheet_height_mm"].as_f64().unwrap() > 0.0);
        assert!(value["scale"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn empty_geometry_returns_drawing_error() {
        // A part with no features lowers to zero geometry → SCC-DRW-005.
        let json = dsl_apply("AICAD 1\nPART empty_plate\n").unwrap();
        let err = drawing(&json).unwrap_err();
        match err {
            scc_core::error::SccError::Structured(structured) => {
                assert_eq!(structured.code, "SCC-DRW-005");
            }
            other => panic!("expected structured SCC-DRW-005 error, got {other:?}"),
        }
    }

    fn spatial_model_json(method: Option<ProjectionMethod>) -> String {
        let mut model = EngineeringModel::new();
        let commands = [
            EngineeringCommand::CreatePart {
                id: PartId("three_view_plate".into()),
                name: "Three View Plate".into(),
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: FeatureId("plate".into()),
                    name: "plate".into(),
                    profile: scc_engineering::BaseProfile::Rectangle {
                        width: 120.0,
                        height: 80.0,
                    },
                },
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: FeatureId("center_hole".into()),
                    diameter: 10.0,
                },
            },
            EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Literal { value: 10.0 }),
            },
            EngineeringCommand::SetHoleExtent {
                target: FeatureId("center_hole".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        ];
        for command in commands {
            CommandExecutor::apply(&mut model, &command).unwrap();
        }
        if let Some(method) = method {
            model
                .drawing
                .set(DrawingProperty::ProjectionMethod { method });
        }
        serde_json::to_string(&model).unwrap()
    }

    #[test]
    fn drawing_three_view_emits_formal_svg_and_metadata() {
        let value: serde_json::Value = serde_json::from_str(
            &drawing_three_view(&spatial_model_json(Some(ProjectionMethod::ThirdAngle))).unwrap(),
        )
        .unwrap();
        assert!(value["svg"].as_str().unwrap().starts_with("<svg"));
        assert!(value["dxf"].as_str().unwrap().contains("SECTION"));
        assert!(value["sheet_width_mm"].as_f64().unwrap() > 0.0);
        assert!(value["sheet_height_mm"].as_f64().unwrap() > 0.0);
        assert_eq!(value["scale"], 1.0);
        assert_eq!(value["part_title"], "Three View Plate");
        assert_eq!(value["projection_method"], "third_angle");
        assert_eq!(value["drawing_mode"], "three_view");
        assert_eq!(
            value["drawing_report"]["source"],
            "same_formal_drawing_model"
        );
        assert_eq!(value["drawing_report"]["view_count"], 3);
        assert!(value["drawing_report"]["views"]
            .as_array()
            .unwrap()
            .iter()
            .any(|view| !view["dimensions"].as_array().unwrap().is_empty()));
        assert!(value["drawing_report"]["views"]
            .as_array()
            .unwrap()
            .iter()
            .any(|view| !view["center_geometry"].as_array().unwrap().is_empty()));
        assert!(value["svg"].as_str().unwrap().contains("id=\"view_front\""));
        assert!(value["svg"].as_str().unwrap().contains("id=\"view_top\""));
        assert!(value["svg"].as_str().unwrap().contains("id=\"view_left\""));
    }

    #[test]
    fn drawing_three_view_reports_typed_tolerance_from_engineering_model() {
        let mut model: EngineeringModel =
            serde_json::from_str(&spatial_model_json(Some(ProjectionMethod::ThirdAngle))).unwrap();
        let target = EngineeringDimensionRef::FeatureSlot {
            feature: FeatureId("plate".into()),
            slot: scc_engineering::binding::FeatureParameterSlot::Width,
        };
        let specification = DimensionalSpecification::new(
            DimensionalTolerance::bilateral(0.2, -0.1).unwrap(),
            Some(DecimalPlaces::new(2).unwrap()),
        )
        .unwrap();
        model
            .set_dimensional_specification(target, Some(specification))
            .unwrap();

        let value: serde_json::Value = serde_json::from_str(
            &drawing_three_view(&serde_json::to_string(&model).unwrap()).unwrap(),
        )
        .unwrap();
        let dimensions = value["drawing_report"]["views"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|view| view["dimensions"].as_array().unwrap())
            .collect::<Vec<_>>();
        assert!(dimensions.iter().any(|dimension| {
            dimension["target_feature"] == "plate"
                && dimension["has_tolerance_specification"] == true
        }));
    }

    #[test]
    fn drawing_three_view_preserves_first_angle_metadata() {
        let value: serde_json::Value = serde_json::from_str(
            &drawing_three_view(&spatial_model_json(Some(ProjectionMethod::FirstAngle))).unwrap(),
        )
        .unwrap();
        assert_eq!(value["projection_method"], "first_angle");
    }

    #[test]
    fn drawing_three_view_fails_closed_without_projection_method() {
        let err = drawing_three_view(&spatial_model_json(None)).unwrap_err();
        match err {
            SccError::Structured(structured) => assert_eq!(structured.code, "SCC-DRW-019"),
            other => panic!("expected structured missing projection method error, got {other:?}"),
        }
    }

    #[test]
    fn context_view_is_derived_from_a_model() {
        let model_json = dsl_apply(DSL_PLATE).unwrap();
        let json = context(&model_json).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["part"]["id"], "mounting_plate");
        assert_eq!(value["features"][0]["feature_type"], "base_profile");
        // No SCC internals leak into the context.
        assert!(value.get("graph_state").is_none());
        assert!(value.get("constraint_ir").is_none());
    }
}
