//! EngineeringModel-v2 lowering to the stateless generic CadQuery rebuild.
//!
//! This adapter resolves only the G1B-supported principal/offset datums and
//! ordered Sketch + Extrude history. It never mutates or persists geometry.

use std::collections::BTreeMap;
use std::path::PathBuf;

use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_engineering::binding::FeatureParameterSlot;
use scc_engineering::feature::EngineeringFeature;
use scc_engineering::parametric::{
    AxisReference, DatumAxisDefinition, DatumPlaneDefinition, EdgeReference, ExtrudeDirection,
    ExtrudeExtent, GeneratedFaceSelector, HoleKind, HoleTermination, LoftMode, LoftOperation,
    LoftOrientationPolicy, ParametricFeature, PatternDefinition, PlanarFaceSelector,
    PlaneReference, PrincipalAxis, PrincipalPlane, ProfileEdgeSide, RevolveAngle, RevolveOperation,
    RevolveOrientation, SemanticEdgeCurveKind, SemanticEdgeSelector,
    SemanticGeneratedFaceQualifier, SemanticGeneratedFaceReference, SemanticPlanarFaceReference,
    SemanticSurfaceKind, SketchCurveGeometry, SweepDirection, SweepOperation, SweepOrientation,
};
use scc_engineering::{EngineeringModel, FeatureId};
use scc_spatial::{
    Bounds3D, SpatialBaseProfile, SpatialBody, SpatialFootprint, SpatialModel, SpatialModelSource,
    ZInterval,
};
use sha2::{Digest, Sha256};

use scc_cad_backend::{
    ParametricArcDirection, ParametricBooleanOperation, ParametricCountersinkOperation,
    ParametricCurve, ParametricEdgeCurveKind, ParametricEdgeQualifier, ParametricEdgeReference,
    ParametricEdgeSelector, ParametricExtent, ParametricExtrudeOperation, ParametricFaceReference,
    ParametricGeneratedFaceQualifier, ParametricGeneratedFaceReference,
    ParametricGeneratedFaceSelector, ParametricLoftMode, ParametricLoftOperation,
    ParametricLoftOrientationPolicy, ParametricLoftSection, ParametricOperationTiming,
    ParametricProfileEdgeSide, ParametricProfileEndpoint, ParametricProfilePlane,
    ParametricRebuildOperation, ParametricRebuildRequest, ParametricRevolveAngle,
    ParametricRevolveOperation, ParametricRevolveOrientation, ParametricSectionGeometry,
    ParametricSectionPlane, ParametricSemanticSurfaceKind, ParametricSweepDirection,
    ParametricSweepOperation, ParametricSweepOrientation, PARAMETRIC_REBUILD_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Copy)]
struct PlaneFrame {
    origin: [f64; 3],
    u_axis: [f64; 3],
    v_axis: [f64; 3],
    normal: [f64; 3],
}

#[derive(Debug, Clone)]
struct SketchFrame {
    plane: PlaneFrame,
    loops: Vec<Vec<ParametricCurve>>,
    profile_entity_ids: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
struct PathFrame {
    plane: PlaneFrame,
    curves: Vec<ParametricCurve>,
    entity_ids: Vec<String>,
}

pub fn is_parametric_model(model: &EngineeringModel) -> bool {
    model
        .features
        .values()
        .any(|feature| matches!(feature, EngineeringFeature::Parametric { .. }))
}

#[derive(Debug, Clone, Default)]
pub struct RebuildTimings {
    pub history_lowering_ms: f64,
    pub kernel_rebuild_ms: f64,
    pub topology_resolution_ms: f64,
    pub base_rebuild_ms: f64,
    pub fillet_rebuild_ms: f64,
    pub chamfer_rebuild_ms: f64,
    pub operation_timings: Vec<ParametricOperationTiming>,
    pub spatial_lowering_ms: f64,
    pub total_rebuild_ms: f64,
}

pub struct RebuildOutput {
    pub spatial: SpatialModel,
    pub timings: RebuildTimings,
    pub section_geometry: Option<ParametricSectionGeometry>,
}

pub fn rebuild_spatial(model: &EngineeringModel) -> Result<RebuildOutput, SccError> {
    let rebuild_started = std::time::Instant::now();
    let lowering_started = std::time::Instant::now();
    let request = lower_parametric_manufacturing_spec(model)?;
    let history_lowering_ms = lowering_started.elapsed().as_secs_f64() * 1000.0;
    let result = cad_backend_client()
        .execute_parametric_rebuild(&request)
        .map_err(map_backend_error)?;
    rebuild_output_from_result(request, result, None, rebuild_started, history_lowering_ms)
}

pub fn rebuild_spatial_with_section(
    model: &EngineeringModel,
    plane: ParametricSectionPlane,
) -> Result<RebuildOutput, SccError> {
    let rebuild_started = std::time::Instant::now();
    let lowering_started = std::time::Instant::now();
    let request = lower_parametric_manufacturing_spec(model)?;
    let history_lowering_ms = lowering_started.elapsed().as_secs_f64() * 1000.0;
    let (result, section) = cad_backend_client()
        .execute_parametric_section(&request, plane)
        .map_err(map_backend_error)?;
    rebuild_output_from_result(
        request,
        result,
        Some(section),
        rebuild_started,
        history_lowering_ms,
    )
}

fn cad_backend_client() -> scc_cad_backend::CadBackendClient {
    let python = std::env::var_os("SCC_CAD_PYTHON").unwrap_or_else(|| "python3".into());
    scc_cad_backend::CadBackendClient::new(PathBuf::from(python))
        .with_args(["-m", "aicad_cad.parametric_rebuild"])
}

fn rebuild_output_from_result(
    request: ParametricRebuildRequest,
    result: scc_cad_backend::ParametricRebuildResult,
    section_geometry: Option<ParametricSectionGeometry>,
    rebuild_started: std::time::Instant,
    history_lowering_ms: f64,
) -> Result<RebuildOutput, SccError> {
    let kernel_rebuild_ms = result.worker_elapsed_ms;
    let operation_timings = result.operation_timings.clone();
    let topology_resolution_ms = operation_timings
        .iter()
        .map(|timing| timing.topology_resolution_ms)
        .sum();
    let base_rebuild_ms = operation_timings
        .iter()
        .filter(|timing| {
            timing.kind.starts_with("extrude_")
                || timing.kind.starts_with("revolve_")
                || timing.kind.starts_with("sweep_")
                || timing.kind.starts_with("loft_")
        })
        .map(|timing| timing.elapsed_ms)
        .sum();
    let fillet_rebuild_ms = operation_timings
        .iter()
        .filter(|timing| timing.kind == "fillet")
        .map(|timing| timing.elapsed_ms)
        .sum();
    let chamfer_rebuild_ms = operation_timings
        .iter()
        .filter(|timing| timing.kind == "chamfer")
        .map(|timing| timing.elapsed_ms)
        .sum();
    let spatial_lowering_started = std::time::Instant::now();
    let bounds = Bounds3D::new(
        [result.bbox.x_min, result.bbox.y_min, result.bbox.z_min],
        [result.bbox.x_max, result.bbox.y_max, result.bbox.z_max],
    )
    .map_err(|error| structured("BODY_INVALID", error.to_string()))?;
    let evidence = result.spatial_evidence;
    let thickness = evidence.frame.axis_max - evidence.frame.axis_min;
    let z = ZInterval::new(0.0, thickness)
        .map_err(|error| structured("BODY_INVALID", error.to_string()))?;
    let first_add = request
        .operations
        .iter()
        .find(|operation| match operation {
            ParametricRebuildOperation::Extrude(extrude) => {
                extrude.operation == ParametricBooleanOperation::Add
            }
            ParametricRebuildOperation::Revolve(revolve) => {
                revolve.operation == ParametricBooleanOperation::Add
            }
            ParametricRebuildOperation::Sweep(sweep) => {
                sweep.operation == ParametricBooleanOperation::Add
            }
            ParametricRebuildOperation::Loft(loft) => {
                loft.operation == ParametricBooleanOperation::Add
            }
            _ => false,
        })
        .ok_or_else(|| structured("NO_BODY", "feature history contains no active ADD"))?;
    let source_feature = FeatureId(first_add.source_feature_id().to_owned());
    let source_step = request.operations.len() as u64;
    let geometry_source_root = u64::from_str_radix(&request.model_sha256[..16], 16)
        .map_err(|error| structured("REBUILD_DEPENDENCY_ERROR", error.to_string()))?;
    let spatial = SpatialModel {
        body: SpatialBody {
            footprint: SpatialFootprint::Parametric {
                evidence: Box::new(evidence),
            },
            profile: SpatialBaseProfile::Parametric,
            z,
            source_entity: scc_core::ids::EntityId(source_feature.0.clone()),
            source_feature,
        },
        cuts: BTreeMap::new(),
        slot_cuts: BTreeMap::new(),
        bounds,
        construction_circles: BTreeMap::new(),
        source: SpatialModelSource {
            geometry_source_root,
            geometry_source_step: source_step,
        },
    };
    let spatial_lowering_ms = spatial_lowering_started.elapsed().as_secs_f64() * 1000.0;
    Ok(RebuildOutput {
        spatial,
        section_geometry,
        timings: RebuildTimings {
            history_lowering_ms,
            kernel_rebuild_ms,
            topology_resolution_ms,
            base_rebuild_ms,
            fillet_rebuild_ms,
            chamfer_rebuild_ms,
            operation_timings,
            spatial_lowering_ms,
            total_rebuild_ms: rebuild_started.elapsed().as_secs_f64() * 1000.0,
        },
    })
}

/// Lower the validated EngineeringModel v2 history to the immutable ordered
/// operation contract shared by drawing rebuild and formal manufacturing.
/// This function performs no kernel work and never reads DrawingModel state.
pub fn lower_parametric_manufacturing_spec(
    model: &EngineeringModel,
) -> Result<ParametricRebuildRequest, SccError> {
    validate_pattern_seeds(model)?;
    model.validate_model().map_err(map_model_validation_error)?;
    let request = build_request(model)?;
    request.validate().map_err(map_backend_error)?;
    Ok(request)
}

fn validate_pattern_seeds(model: &EngineeringModel) -> Result<(), SccError> {
    let ordered = model.ordered_feature_ids();
    let positions = ordered
        .iter()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect::<BTreeMap<_, _>>();
    for (index, id) in ordered.iter().enumerate() {
        let Some(EngineeringFeature::Parametric {
            feature: ParametricFeature::Pattern {
                seed_feature_id, ..
            },
            suppressed,
            ..
        }) = model.features.get(id)
        else {
            continue;
        };
        if *suppressed {
            continue;
        }
        let Some(seed) = model.features.get(seed_feature_id) else {
            return Err(structured(
                "PATTERN_SEED_NOT_FOUND",
                format!("pattern {id} references missing seed feature {seed_feature_id}"),
            ));
        };
        if matches!(
            seed,
            EngineeringFeature::Parametric {
                suppressed: true,
                ..
            }
        ) || positions
            .get(seed_feature_id)
            .is_none_or(|seed_index| *seed_index >= index)
        {
            return Err(structured(
                "PATTERN_SEED_NOT_FOUND",
                format!("pattern {id} requires an active earlier seed feature {seed_feature_id}"),
            ));
        }
        let supported_seed = matches!(
            seed,
            EngineeringFeature::Parametric {
                suppressed: false,
                feature: ParametricFeature::Hole { .. }
                    | ParametricFeature::Extrude {
                        operation: scc_engineering::ExtrudeOperation::Cut,
                        ..
                    },
                ..
            }
        );
        if !supported_seed {
            return Err(structured(
                "PATTERN_UNSUPPORTED_SEED",
                format!("pattern {id} supports only an earlier active Hole or Extrude CUT seed; {seed_feature_id} is a different feature type"),
            ));
        }
    }
    Ok(())
}

fn build_request(model: &EngineeringModel) -> Result<ParametricRebuildRequest, SccError> {
    let mut datums = BTreeMap::<FeatureId, PlaneFrame>::new();
    let mut axes = BTreeMap::<FeatureId, ([f64; 3], [f64; 3])>::new();
    let mut sketches = BTreeMap::<FeatureId, SketchFrame>::new();
    let mut paths = BTreeMap::<FeatureId, PathFrame>::new();
    let mut generated_face_frames = BTreeMap::<(FeatureId, String), PlaneFrame>::new();
    let mut lowered_by_feature = BTreeMap::<FeatureId, Vec<ParametricRebuildOperation>>::new();
    let mut operations = Vec::new();
    for id in model.ordered_feature_ids() {
        let Some(EngineeringFeature::Parametric {
            feature,
            suppressed,
            ..
        }) = model.features.get(&id)
        else {
            return Err(structured(
                "REBUILD_DEPENDENCY_ERROR",
                format!("feature {id} is not part of the generic parametric history"),
            ));
        };
        if *suppressed {
            continue;
        }
        let mut feature_ops = Vec::<ParametricRebuildOperation>::new();
        match feature {
            ParametricFeature::DatumPlane { definition } => {
                let frame = match definition {
                    DatumPlaneDefinition::Principal { plane } => principal_frame(*plane),
                    DatumPlaneDefinition::Offset { base, distance } => {
                        let mut frame = resolve_plane_reference(base, &datums)?;
                        for (axis, origin) in frame.normal.iter().zip(frame.origin.iter_mut()) {
                            *origin += axis * distance;
                        }
                        frame
                    }
                    DatumPlaneDefinition::CustomPlane {
                        origin,
                        normal,
                        x_axis,
                    } => {
                        let normal =
                            normalize(*normal, "INVALID_DATUM", "custom DatumPlane normal")?;
                        let mut u_axis =
                            normalize(*x_axis, "INVALID_DATUM", "custom DatumPlane X axis")?;
                        let projection = dot(u_axis, normal);
                        u_axis = normalize(
                            sub(u_axis, normal.map(|value| value * projection)),
                            "INVALID_DATUM",
                            "custom DatumPlane X axis",
                        )?;
                        let v_axis = cross(normal, u_axis);
                        PlaneFrame {
                            origin: *origin,
                            u_axis,
                            v_axis,
                            normal,
                        }
                    }
                    DatumPlaneDefinition::PlanarFaceReference { .. } => {
                        return Err(structured(
                            "INVALID_DATUM",
                            format!(
                                "datum {id} uses a support that is not executable in this rebuild"
                            ),
                        ));
                    }
                };
                datums.insert(id.clone(), frame);
            }
            ParametricFeature::DatumAxis { definition } => {
                axes.insert(id.clone(), resolve_axis(definition)?);
            }
            ParametricFeature::Sketch { support, profile } => {
                let (plane, _) = resolve_support(support, &datums, &generated_face_frames)?;
                profile
                    .validate()
                    .map_err(|error| structured("INVALID_SKETCH", error.to_string()))?;
                let mut loops = vec![lower_loop(&profile.outer)?];
                let mut profile_entity_ids = vec![profile
                    .outer
                    .entities
                    .iter()
                    .map(|entity| entity.id.0.clone())
                    .collect()];
                for loop_ in &profile.inner_loops {
                    loops.push(lower_loop(loop_)?);
                    profile_entity_ids.push(
                        loop_
                            .entities
                            .iter()
                            .map(|entity| entity.id.0.clone())
                            .collect(),
                    );
                }
                sketches.insert(
                    id.clone(),
                    SketchFrame {
                        plane,
                        loops,
                        profile_entity_ids,
                    },
                );
            }
            ParametricFeature::Path { support, segments } => {
                let (plane, _) = resolve_support(support, &datums, &generated_face_frames)?;
                let curves = segments
                    .iter()
                    .map(|segment| lower_curve(&segment.geometry))
                    .collect::<Result<Vec<_>, _>>()?;
                paths.insert(
                    id.clone(),
                    PathFrame {
                        plane,
                        curves,
                        entity_ids: segments
                            .iter()
                            .map(|segment| segment.id.0.clone())
                            .collect(),
                    },
                );
            }
            ParametricFeature::Loft {
                sections,
                operation,
                mode,
                orientation_policy,
            } => {
                let mut resolved_sections = Vec::with_capacity(sections.len());
                for section_id in sections {
                    let section = sketches.get(section_id).ok_or_else(|| {
                        structured(
                            "LOFT_SECTION_REFERENCE_NOT_FOUND",
                            format!(
                                "Loft {id} references missing or suppressed Sketch {section_id}"
                            ),
                        )
                    })?;
                    if section.loops.len() != 1 {
                        return Err(structured(
                            "LOFT_INNER_LOOPS_UNSUPPORTED",
                            format!("Loft {id} currently supports one outer loop per section; inner-loop correspondence is deferred"),
                        ));
                    }
                    resolved_sections.push((section_id, section));
                }
                validate_loft_sections(&id, &resolved_sections)?;
                let operation = match operation {
                    LoftOperation::Add => ParametricBooleanOperation::Add,
                    LoftOperation::Cut => ParametricBooleanOperation::Cut,
                };
                if operation == ParametricBooleanOperation::Add {
                    let first = resolved_sections
                        .first()
                        .expect("Loft section validation requires a first section")
                        .1
                        .plane;
                    let last = resolved_sections
                        .last()
                        .expect("Loft section validation requires a last section")
                        .1
                        .plane;
                    generated_face_frames.insert(
                        (id.clone(), "loft_start_cap".into()),
                        reversed_cap_frame(first),
                    );
                    generated_face_frames.insert((id.clone(), "loft_end_cap".into()), last);
                }
                let mode = match mode {
                    LoftMode::Ruled => ParametricLoftMode::Ruled,
                };
                let orientation_policy = match orientation_policy {
                    LoftOrientationPolicy::Aligned => ParametricLoftOrientationPolicy::Aligned,
                };
                let sections = resolved_sections
                    .into_iter()
                    .map(|(section_id, sketch)| ParametricLoftSection {
                        sketch_id: section_id.0.clone(),
                        plane: to_profile_plane(sketch.plane),
                        loops: sketch.loops.clone(),
                        profile_entity_ids: sketch.profile_entity_ids.clone(),
                    })
                    .collect();
                feature_ops.push(ParametricRebuildOperation::Loft(Box::new(
                    ParametricLoftOperation {
                        feature_id: id.0.clone(),
                        operation,
                        mode,
                        orientation_policy,
                        sections,
                    },
                )));
            }
            ParametricFeature::Sweep {
                profile,
                path,
                operation,
                direction,
                profile_origin,
                orientation,
            } => {
                let sketch = sketches.get(profile).ok_or_else(|| {
                    structured(
                        "MISSING_PROFILE_REFERENCE",
                        format!("Sweep {id} references missing or suppressed Sketch {profile}"),
                    )
                })?;
                let path_frame = paths.get(path).ok_or_else(|| {
                    structured(
                        "PATH_REFERENCE_NOT_FOUND",
                        format!("Sweep {id} references missing or suppressed Path {path}"),
                    )
                })?;
                validate_sweep_attachment(sketch.plane, *profile_origin, path_frame, *direction)?;
                let sweep_operation = match operation {
                    SweepOperation::Add => ParametricBooleanOperation::Add,
                    SweepOperation::Cut => ParametricBooleanOperation::Cut,
                };
                if sweep_operation == ParametricBooleanOperation::Add {
                    let (start_cap, end_cap) =
                        sweep_cap_frames(path_frame, *direction, *profile_origin)?;
                    generated_face_frames.insert((id.clone(), "sweep_start_cap".into()), start_cap);
                    generated_face_frames.insert((id.clone(), "sweep_end_cap".into()), end_cap);
                }
                let operation = match operation {
                    SweepOperation::Add => ParametricBooleanOperation::Add,
                    SweepOperation::Cut => ParametricBooleanOperation::Cut,
                };
                let direction = match direction {
                    SweepDirection::Forward => ParametricSweepDirection::Forward,
                    SweepDirection::Reverse => ParametricSweepDirection::Reverse,
                };
                let orientation = match orientation {
                    SweepOrientation::FixedBinormal => ParametricSweepOrientation::FixedBinormal,
                };
                feature_ops.push(ParametricRebuildOperation::Sweep(Box::new(
                    ParametricSweepOperation {
                        feature_id: id.0.clone(),
                        sketch_id: profile.0.clone(),
                        path_id: path.0.clone(),
                        operation,
                        plane: to_profile_plane(sketch.plane),
                        loops: sketch.loops.clone(),
                        profile_entity_ids: sketch.profile_entity_ids.clone(),
                        profile_origin: [profile_origin.x, profile_origin.y],
                        path_plane: to_profile_plane(path_frame.plane),
                        path: path_frame.curves.clone(),
                        path_entity_ids: path_frame.entity_ids.clone(),
                        direction,
                        orientation,
                    },
                )));
            }
            ParametricFeature::Extrude {
                profile,
                operation,
                direction,
                extent,
            } => {
                let sketch = sketches.get(profile).ok_or_else(|| {
                    structured(
                        "MISSING_PROFILE_REFERENCE",
                        format!("extrude {id} references missing or suppressed sketch {profile}"),
                    )
                })?;
                let direction = resolve_direction(direction, sketch.plane.normal)?;
                let operation = match operation {
                    scc_engineering::ExtrudeOperation::Add => ParametricBooleanOperation::Add,
                    scc_engineering::ExtrudeOperation::Cut => ParametricBooleanOperation::Cut,
                };
                feature_ops.push(ParametricRebuildOperation::Extrude(Box::new(
                    make_operation(
                        id.0.clone(),
                        &id,
                        "extrude",
                        profile.0.clone(),
                        operation,
                        sketch.plane,
                        sketch.loops.clone(),
                        sketch.profile_entity_ids.clone(),
                        direction,
                        lower_extent(*extent)?,
                        None,
                        false,
                    ),
                )));
            }
            ParametricFeature::Revolve {
                profile,
                axis,
                operation,
                angle,
                orientation,
            } => {
                let sketch = sketches.get(profile).ok_or_else(|| {
                    structured(
                        "MISSING_PROFILE_REFERENCE",
                        format!("revolve {id} references missing or suppressed sketch {profile}"),
                    )
                })?;
                let axis_feature = match axis {
                    AxisReference::DatumAxis { feature } => feature,
                };
                let (axis_origin, direction) =
                    axes.get(axis_feature).copied().ok_or_else(|| {
                        structured(
                            "AXIS_REFERENCE_NOT_FOUND",
                            format!("revolve {id} references unresolved DatumAxis {axis_feature}"),
                        )
                    })?;
                let operation = match operation {
                    RevolveOperation::Add => ParametricBooleanOperation::Add,
                    RevolveOperation::Cut => ParametricBooleanOperation::Cut,
                };
                let angle = match angle {
                    RevolveAngle::Full360 => ParametricRevolveAngle::Full360,
                    RevolveAngle::Partial { degrees } => {
                        ParametricRevolveAngle::Partial { degrees: *degrees }
                    }
                };
                let orientation = match orientation {
                    RevolveOrientation::Forward => ParametricRevolveOrientation::Forward,
                    RevolveOrientation::Reverse => ParametricRevolveOrientation::Reverse,
                };
                operations.push(ParametricRebuildOperation::Revolve(Box::new(
                    ParametricRevolveOperation {
                        feature_id: id.0.clone(),
                        sketch_id: profile.0.clone(),
                        operation,
                        plane: ParametricProfilePlane {
                            origin: sketch.plane.origin,
                            u_axis: sketch.plane.u_axis,
                            v_axis: sketch.plane.v_axis,
                            normal: sketch.plane.normal,
                        },
                        loops: sketch.loops.clone(),
                        profile_entity_ids: sketch.profile_entity_ids.clone(),
                        axis_origin,
                        direction,
                        angle,
                        orientation,
                    },
                )));
            }
            ParametricFeature::Hole {
                support,
                position,
                diameter: _,
                reversed,
                termination,
                kind,
            } => {
                let diameter = model
                    .resolved_feature_parameter_value(&id, FeatureParameterSlot::Diameter)
                    .map_err(map_model_validation_error)?;
                let termination = match termination {
                    HoleTermination::ThroughAll => HoleTermination::ThroughAll,
                    HoleTermination::Blind { .. } => HoleTermination::Blind {
                        depth: model
                            .resolved_feature_parameter_value(&id, FeatureParameterSlot::BlindDepth)
                            .map_err(map_model_validation_error)?,
                    },
                };
                let kind = match kind {
                    HoleKind::Simple => HoleKind::Simple,
                    HoleKind::Counterbore { .. } => HoleKind::Counterbore {
                        diameter: model
                            .resolved_feature_parameter_value(
                                &id,
                                FeatureParameterSlot::CounterboreDiameter,
                            )
                            .map_err(map_model_validation_error)?,
                        depth: model
                            .resolved_feature_parameter_value(
                                &id,
                                FeatureParameterSlot::CounterboreDepth,
                            )
                            .map_err(map_model_validation_error)?,
                    },
                    HoleKind::Countersink { .. } => HoleKind::Countersink {
                        diameter: model
                            .resolved_feature_parameter_value(
                                &id,
                                FeatureParameterSlot::CountersinkMouthDiameter,
                            )
                            .map_err(map_model_validation_error)?,
                        included_angle_degrees: model
                            .resolved_feature_parameter_value(
                                &id,
                                FeatureParameterSlot::CountersinkIncludedAngleDegrees,
                            )
                            .map_err(map_model_validation_error)?,
                    },
                };
                let (mut plane, support_face) =
                    resolve_support(support, &datums, &generated_face_frames)?;
                for index in 0..3 {
                    plane.origin[index] +=
                        plane.u_axis[index] * position.x + plane.v_axis[index] * position.y;
                }
                let direction = if *reversed {
                    plane.normal.map(|v| -v)
                } else {
                    plane.normal
                };
                let (extent, blind_hole) = match termination {
                    HoleTermination::ThroughAll => (ParametricExtent::ThroughAll, false),
                    HoleTermination::Blind { depth } => {
                        (ParametricExtent::Distance { length: depth }, true)
                    }
                };
                let profile = vec![vec![ParametricCurve::Circle {
                    center: [0.0, 0.0],
                    radius: diameter / 2.0,
                }]];
                let operation_prefix = format!("{}::", id.0);
                feature_ops.push(ParametricRebuildOperation::Extrude(Box::new(
                    make_operation(
                        format!("{operation_prefix}main"),
                        &id,
                        "hole",
                        id.0.clone(),
                        ParametricBooleanOperation::Cut,
                        plane,
                        profile.clone(),
                        vec![vec![format!("{}-main-bore", id.0)]],
                        direction,
                        extent,
                        support_face.clone(),
                        blind_hole,
                    ),
                )));
                match kind {
                    HoleKind::Simple => {}
                    HoleKind::Counterbore { diameter, depth } => {
                        if let HoleTermination::Blind { depth: main_depth } = termination {
                            if depth > main_depth {
                                return Err(structured(
                                    "INVALID_COUNTERBORE",
                                    "counterbore depth cannot exceed the blind hole depth",
                                ));
                            }
                        }
                        feature_ops.push(ParametricRebuildOperation::Extrude(Box::new(
                            make_operation(
                                format!("{operation_prefix}counterbore"),
                                &id,
                                "hole",
                                id.0.clone(),
                                ParametricBooleanOperation::Cut,
                                plane,
                                vec![vec![ParametricCurve::Circle {
                                    center: [0.0, 0.0],
                                    radius: diameter / 2.0,
                                }]],
                                vec![vec![format!("{}-counterbore", id.0)]],
                                direction,
                                ParametricExtent::Distance { length: depth },
                                support_face,
                                true,
                            ),
                        )));
                    }
                    HoleKind::Countersink {
                        diameter: mouth_diameter,
                        included_angle_degrees,
                    } => {
                        feature_ops.push(ParametricRebuildOperation::Countersink(Box::new(
                            ParametricCountersinkOperation {
                                feature_id: format!("{operation_prefix}countersink"),
                                source_feature_id: id.0.clone(),
                                source_feature_kind: "hole".into(),
                                plane: ParametricProfilePlane {
                                    origin: plane.origin,
                                    u_axis: plane.u_axis,
                                    v_axis: plane.v_axis,
                                    normal: plane.normal,
                                },
                                direction,
                                main_diameter: diameter,
                                mouth_diameter,
                                included_angle_degrees,
                                support_face,
                            },
                        )));
                    }
                }
            }
            ParametricFeature::Fillet { edge_refs, radius } => {
                operations.push(ParametricRebuildOperation::Fillet {
                    feature_id: id.0.clone(),
                    edge_refs: edge_refs.iter().map(lower_edge_reference).collect(),
                    radius: *radius,
                });
            }
            ParametricFeature::Chamfer {
                edge_refs,
                distance,
            } => {
                operations.push(ParametricRebuildOperation::Chamfer {
                    feature_id: id.0.clone(),
                    edge_refs: edge_refs.iter().map(lower_edge_reference).collect(),
                    distance: *distance,
                });
            }
            ParametricFeature::Pattern {
                seed_feature_id,
                definition,
            } => {
                let seeds = lowered_by_feature.get(seed_feature_id).ok_or_else(|| structured(
                    "PATTERN_SEED_NOT_FOUND", format!("pattern {id} references feature {seed_feature_id}, but its rebuild geometry is unavailable"),
                ))?;
                if seeds.is_empty() {
                    return Err(structured(
                        "PATTERN_SEED_NOT_FOUND",
                        format!("pattern {id} has no active seed operation"),
                    ));
                }
                if seeds
                    .iter()
                    .any(|op| operation_boolean(op) != Some(ParametricBooleanOperation::Cut))
                {
                    return Err(structured(
                        "PATTERN_UNSUPPORTED_SEED",
                        "this single-body pattern capability currently supports CUT seeds only",
                    ));
                }
                match definition {
                    PatternDefinition::Linear {
                        direction,
                        count,
                        spacing,
                    } => {
                        let direction = [direction.x, direction.y, direction.z];
                        for instance in 1..*count {
                            for seed in seeds {
                                let mut operation = seed.clone();
                                retarget_pattern_operation(
                                    &mut operation,
                                    &id.0,
                                    seed_feature_id,
                                    instance,
                                    "pattern:linear",
                                )?;
                                let distance = *spacing * f64::from(instance);
                                translate_rebuild_operation(
                                    &mut operation,
                                    direction.map(|v| v * distance),
                                );
                                feature_ops.push(operation);
                            }
                        }
                    }
                    PatternDefinition::Circular {
                        axis,
                        center,
                        count,
                        radius,
                        start_angle_degrees,
                        angular_span_degrees,
                    } => {
                        let axis = [axis.x, axis.y, axis.z];
                        let center = [center.x, center.y, center.z];
                        for seed in seeds {
                            let direction = operation_direction(seed).ok_or_else(|| {
                                structured(
                                    "PATTERN_UNSUPPORTED_SEED",
                                    "pattern seed operation has no cutting direction",
                                )
                            })?;
                            let plane = operation_plane(seed).ok_or_else(|| {
                                structured(
                                    "PATTERN_UNSUPPORTED_SEED",
                                    "pattern seed operation has no plane frame",
                                )
                            })?;
                            if dot(direction, axis).abs() < 1.0 - 1.0e-8 {
                                return Err(structured("PATTERN_INVALID_AXIS", "circular pattern axis must be parallel to the seed feature axis in this capability"));
                            }
                            let from_center = sub(plane.origin, center);
                            let radial = sub(from_center, axis.map(|v| dot(from_center, axis) * v));
                            if (norm(radial) - radius).abs() > 1.0e-5 {
                                return Err(structured(
                                    "PATTERN_INVALID_RADIUS",
                                    "circular pattern seed must lie on the declared radius",
                                ));
                            }
                            let (pattern_u, pattern_v) = frame_axes(axis);
                            let seed_angle = dot(radial, pattern_v)
                                .atan2(dot(radial, pattern_u))
                                .to_degrees();
                            let delta_start = (start_angle_degrees - seed_angle).to_radians();
                            if delta_start.abs() > 1.0e-5 {
                                return Err(structured("PATTERN_START_ANGLE_MISMATCH", "seed center must already be placed at the declared start angle"));
                            }
                        }
                        for instance in 1..*count {
                            let angle = angular_span_degrees.to_radians() * f64::from(instance)
                                / f64::from(*count);
                            for seed in seeds {
                                let mut operation = seed.clone();
                                retarget_pattern_operation(
                                    &mut operation,
                                    &id.0,
                                    seed_feature_id,
                                    instance,
                                    "pattern:circular",
                                )?;
                                rotate_rebuild_operation(&mut operation, axis, center, angle)?;
                                feature_ops.push(operation);
                            }
                        }
                    }
                    PatternDefinition::ExplicitPlacement {
                        reference_plane,
                        placements,
                    } => {
                        let frame = resolve_plane_reference(reference_plane, &datums)?;
                        let seed_plane = operation_plane(&seeds[0]).ok_or_else(|| {
                            structured(
                                "PATTERN_UNSUPPORTED_SEED",
                                "explicit pattern seed has no plane frame",
                            )
                        })?;
                        let tolerance = 1.0e-7;
                        if dot(seed_plane.normal, frame.normal).abs() < 1.0 - 1.0e-8
                            || dot(sub(seed_plane.origin, frame.origin), frame.normal).abs()
                                > tolerance
                        {
                            return Err(structured(
                                "PATTERN_PLANE_MISMATCH",
                                "explicit pattern seed plane must be parallel and coplanar with its principal or DatumPlane frame",
                            ));
                        }
                        for seed in seeds.iter().skip(1) {
                            let plane = operation_plane(seed).ok_or_else(|| {
                                structured(
                                    "PATTERN_UNSUPPORTED_SEED",
                                    "explicit pattern seed has no plane frame",
                                )
                            })?;
                            if dot(plane.normal, frame.normal).abs() < 1.0 - 1.0e-8
                                || dot(sub(plane.origin, frame.origin), frame.normal).abs()
                                    > tolerance
                            {
                                return Err(structured(
                                    "PATTERN_PLANE_MISMATCH",
                                    "all operations in an explicit pattern seed must share the reference plane",
                                ));
                            }
                        }
                        let seed_local = [
                            dot(sub(seed_plane.origin, frame.origin), frame.u_axis),
                            dot(sub(seed_plane.origin, frame.origin), frame.v_axis),
                        ];
                        for (offset, placement) in placements.iter().enumerate() {
                            if (placement.local_u_mm - seed_local[0])
                                .hypot(placement.local_v_mm - seed_local[1])
                                <= 1.0e-8
                            {
                                return Err(structured(
                                    "PATTERN_DUPLICATE_PLACEMENT",
                                    "explicit pattern placement duplicates the seed anchor",
                                ));
                            }
                            let target = add(
                                add(frame.origin, frame.u_axis.map(|v| v * placement.local_u_mm)),
                                frame.v_axis.map(|v| v * placement.local_v_mm),
                            );
                            let delta = sub(target, seed_plane.origin);
                            let instance = u32::try_from(offset + 1).unwrap_or(u32::MAX);
                            for seed in seeds {
                                let mut operation = seed.clone();
                                retarget_pattern_operation(
                                    &mut operation,
                                    &id.0,
                                    seed_feature_id,
                                    instance,
                                    "pattern:explicit_placement",
                                )?;
                                translate_rebuild_operation(&mut operation, delta);
                                feature_ops.push(operation);
                            }
                        }
                    }
                }
            }
        }
        operations.extend(feature_ops.iter().cloned());
        lowered_by_feature.insert(id, feature_ops);
    }
    if !operations.iter().any(|operation| match operation {
        ParametricRebuildOperation::Extrude(op) => op.operation == ParametricBooleanOperation::Add,
        ParametricRebuildOperation::Revolve(op) => op.operation == ParametricBooleanOperation::Add,
        ParametricRebuildOperation::Sweep(op) => op.operation == ParametricBooleanOperation::Add,
        ParametricRebuildOperation::Loft(op) => op.operation == ParametricBooleanOperation::Add,
        _ => false,
    }) {
        return Err(structured(
            "NO_BODY",
            "feature history has no active ADD operation",
        ));
    }
    let model_bytes = serde_json::to_vec(model).map_err(SccError::from)?;
    let model_sha256 = format!("{:x}", Sha256::digest(&model_bytes));
    Ok(ParametricRebuildRequest {
        schema_version: PARAMETRIC_REBUILD_SCHEMA_VERSION,
        model_sha256,
        operations,
    })
}

// One function maps a resolved engineering operation into the flat worker contract.
#[allow(clippy::too_many_arguments)]
fn make_operation(
    operation_id: String,
    source: &FeatureId,
    source_kind: &str,
    sketch_id: String,
    operation: ParametricBooleanOperation,
    plane: PlaneFrame,
    loops: Vec<Vec<ParametricCurve>>,
    profile_entity_ids: Vec<Vec<String>>,
    direction: [f64; 3],
    extent: ParametricExtent,
    support_face: Option<ParametricFaceReference>,
    blind_hole: bool,
) -> ParametricExtrudeOperation {
    ParametricExtrudeOperation {
        feature_id: operation_id,
        sketch_id,
        operation,
        plane: ParametricProfilePlane {
            origin: plane.origin,
            u_axis: plane.u_axis,
            v_axis: plane.v_axis,
            normal: plane.normal,
        },
        loops,
        profile_entity_ids,
        direction,
        extent,
        source_feature_id: Some(source.0.clone()),
        source_feature_kind: Some(source_kind.to_owned()),
        support_face,
        blind_hole,
    }
}

fn lower_edge_reference(reference: &EdgeReference) -> ParametricEdgeReference {
    let selector = match &reference.selector {
        SemanticEdgeSelector::ProfileBoundary { entity_id, side } => {
            ParametricEdgeSelector::ProfileBoundary {
                entity_id: entity_id.0.clone(),
                side: match side {
                    ProfileEdgeSide::Top => ParametricProfileEdgeSide::Top,
                    ProfileEdgeSide::Bottom => ParametricProfileEdgeSide::Bottom,
                },
            }
        }
        SemanticEdgeSelector::PocketEntrance { entity_id } => {
            ParametricEdgeSelector::PocketEntrance {
                entity_id: entity_id.0.clone(),
            }
        }
        SemanticEdgeSelector::RevolveProfileVertex {
            entity_id,
            endpoint,
        } => ParametricEdgeSelector::RevolveProfileVertex {
            entity_id: entity_id.0.clone(),
            endpoint: match endpoint {
                scc_engineering::SketchEntityEndpoint::Start => ParametricProfileEndpoint::Start,
                scc_engineering::SketchEntityEndpoint::End => ParametricProfileEndpoint::End,
            },
        },
        SemanticEdgeSelector::HoleEntrance => ParametricEdgeSelector::HoleEntrance,
        SemanticEdgeSelector::HoleExit => ParametricEdgeSelector::HoleExit,
        SemanticEdgeSelector::BetweenFaces { first, second } => {
            ParametricEdgeSelector::BetweenFaces {
                first: lower_semantic_face_reference(first),
                second: lower_semantic_face_reference(second),
            }
        }
        SemanticEdgeSelector::Convex { first, second } => ParametricEdgeSelector::Convex {
            first: lower_semantic_face_reference(first),
            second: lower_semantic_face_reference(second),
        },
        SemanticEdgeSelector::Concave { first, second } => ParametricEdgeSelector::Concave {
            first: lower_semantic_face_reference(first),
            second: lower_semantic_face_reference(second),
        },
        SemanticEdgeSelector::BetweenGeneratedFaces { first, second } => {
            ParametricEdgeSelector::BetweenGeneratedFaces {
                first: lower_generated_face_reference(first),
                second: lower_generated_face_reference(second),
            }
        }
    };
    ParametricEdgeReference {
        source_feature_id: reference.source_feature_id.0.clone(),
        selector,
        qualifier: ParametricEdgeQualifier {
            curve_kind: reference.qualifier.curve_kind.map(|kind| match kind {
                SemanticEdgeCurveKind::Line => ParametricEdgeCurveKind::Line,
                SemanticEdgeCurveKind::Circular => ParametricEdgeCurveKind::Circular,
            }),
            radius_mm: reference.qualifier.radius_mm,
            length_mm: reference.qualifier.length_mm,
            midpoint: reference
                .qualifier
                .midpoint
                .map(|point| [point.x, point.y, point.z]),
            tolerance_mm: reference.qualifier.tolerance_mm,
        },
    }
}

fn lower_semantic_face_reference(
    reference: &SemanticPlanarFaceReference,
) -> ParametricFaceReference {
    ParametricFaceReference {
        source_feature_id: reference.source_feature_id.0.clone(),
        selector: match reference.selector {
            PlanarFaceSelector::Top => "top",
            PlanarFaceSelector::Bottom => "bottom",
            PlanarFaceSelector::PositiveX => "positive_x",
            PlanarFaceSelector::NegativeX => "negative_x",
            PlanarFaceSelector::PositiveY => "positive_y",
            PlanarFaceSelector::NegativeY => "negative_y",
        }
        .into(),
        normal: [
            reference.qualifier.normal.x,
            reference.qualifier.normal.y,
            reference.qualifier.normal.z,
        ],
        plane_offset: reference.qualifier.plane_offset,
        tolerance: reference.qualifier.tolerance,
        generated_qualifier: None,
    }
}

fn lower_generated_face_reference(
    reference: &SemanticGeneratedFaceReference,
) -> ParametricGeneratedFaceReference {
    ParametricGeneratedFaceReference {
        source_feature_id: reference.source_feature_id.0.clone(),
        selector: lower_generated_face_selector(&reference.selector),
        qualifier: lower_generated_face_qualifier(reference.qualifier),
    }
}

fn lower_generated_face_selector(
    selector: &GeneratedFaceSelector,
) -> ParametricGeneratedFaceSelector {
    match selector {
        GeneratedFaceSelector::SweepStartCap => ParametricGeneratedFaceSelector::SweepStartCap,
        GeneratedFaceSelector::SweepEndCap => ParametricGeneratedFaceSelector::SweepEndCap,
        GeneratedFaceSelector::SweepLateral { profile_entity_id } => {
            ParametricGeneratedFaceSelector::SweepLateral {
                profile_entity_id: profile_entity_id.0.clone(),
            }
        }
        GeneratedFaceSelector::LoftStartCap => ParametricGeneratedFaceSelector::LoftStartCap,
        GeneratedFaceSelector::LoftEndCap => ParametricGeneratedFaceSelector::LoftEndCap,
        GeneratedFaceSelector::LoftLateral {
            start_section_id,
            end_section_id,
            start_entity_id,
            end_entity_id,
        } => ParametricGeneratedFaceSelector::LoftLateral {
            start_section_id: start_section_id.0.clone(),
            end_section_id: end_section_id.0.clone(),
            start_entity_id: start_entity_id.0.clone(),
            end_entity_id: end_entity_id.0.clone(),
        },
    }
}

fn lower_generated_face_qualifier(
    value: SemanticGeneratedFaceQualifier,
) -> ParametricGeneratedFaceQualifier {
    ParametricGeneratedFaceQualifier {
        surface_kind: value.surface_kind.map(|kind| match kind {
            SemanticSurfaceKind::Plane => ParametricSemanticSurfaceKind::Plane,
            SemanticSurfaceKind::Cylinder => ParametricSemanticSurfaceKind::Cylinder,
            SemanticSurfaceKind::Cone => ParametricSemanticSurfaceKind::Cone,
            SemanticSurfaceKind::RuledSurface => ParametricSemanticSurfaceKind::RuledSurface,
            SemanticSurfaceKind::RevolutionSurface => {
                ParametricSemanticSurfaceKind::RevolutionSurface
            }
            SemanticSurfaceKind::Other => ParametricSemanticSurfaceKind::Other,
        }),
        normal: value.normal.map(|v| [v.x, v.y, v.z]),
        centroid: value.centroid.map(|v| [v.x, v.y, v.z]),
        area_mm2: value.area_mm2,
        area_tolerance_mm2: value.area_tolerance_mm2,
        tolerance_mm: value.tolerance_mm,
    }
}

fn resolve_support(
    reference: &PlaneReference,
    datums: &BTreeMap<FeatureId, PlaneFrame>,
    generated_faces: &BTreeMap<(FeatureId, String), PlaneFrame>,
) -> Result<(PlaneFrame, Option<ParametricFaceReference>), SccError> {
    match reference {
        PlaneReference::Principal { plane } => Ok((principal_frame(*plane), None)),
        PlaneReference::Datum { feature } => datums
            .get(feature)
            .copied()
            .map(|frame| (frame, None))
            .ok_or_else(|| {
                structured(
                    "INVALID_DATUM",
                    format!("datum reference {feature} has not been resolved"),
                )
            }),
        PlaneReference::PlanarFace {
            source_feature_id,
            selector,
            qualifier,
        } => {
            let normal = [qualifier.normal.x, qualifier.normal.y, qualifier.normal.z];
            let (u_axis, v_axis) = frame_axes(normal);
            let plane = PlaneFrame {
                origin: normal.map(|v| v * qualifier.plane_offset),
                u_axis,
                v_axis,
                normal,
            };
            let selector = match selector {
                PlanarFaceSelector::Top => "top",
                PlanarFaceSelector::Bottom => "bottom",
                PlanarFaceSelector::PositiveX => "positive_x",
                PlanarFaceSelector::NegativeX => "negative_x",
                PlanarFaceSelector::PositiveY => "positive_y",
                PlanarFaceSelector::NegativeY => "negative_y",
            };
            Ok((
                plane,
                Some(ParametricFaceReference {
                    source_feature_id: source_feature_id.0.clone(),
                    selector: selector.into(),
                    normal,
                    plane_offset: qualifier.plane_offset,
                    tolerance: qualifier.tolerance,
                    generated_qualifier: None,
                }),
            ))
        }
        PlaneReference::GeneratedFace { reference } => {
            reference
                .validate()
                .map_err(|error| structured("UNSUPPORTED_TOPOLOGY_REFERENCE", error.to_string()))?;
            let selector = generated_face_selector_name(&reference.selector).ok_or_else(|| {
                structured(
                    "UNSUPPORTED_TOPOLOGY_REFERENCE",
                    "only planar Sweep/Loft cap roles may support a Hole",
                )
            })?;
            let plane = generated_faces
                .get(&(reference.source_feature_id.clone(), selector.to_string()))
                .copied()
                .ok_or_else(|| {
                    structured(
                        "TOPOLOGY_REFERENCE_NOT_FOUND",
                        format!(
                            "generated cap role {selector} is unavailable on {}",
                            reference.source_feature_id
                        ),
                    )
                })?;
            let normal = plane.normal;
            Ok((
                plane,
                Some(ParametricFaceReference {
                    source_feature_id: reference.source_feature_id.0.clone(),
                    selector: selector.to_string(),
                    normal,
                    plane_offset: dot(normal, plane.origin),
                    tolerance: reference.qualifier.tolerance_mm,
                    generated_qualifier: Some(lower_generated_face_qualifier(reference.qualifier)),
                }),
            ))
        }
    }
}

fn generated_face_selector_name(selector: &GeneratedFaceSelector) -> Option<&'static str> {
    match selector {
        GeneratedFaceSelector::SweepStartCap => Some("sweep_start_cap"),
        GeneratedFaceSelector::SweepEndCap => Some("sweep_end_cap"),
        GeneratedFaceSelector::LoftStartCap => Some("loft_start_cap"),
        GeneratedFaceSelector::LoftEndCap => Some("loft_end_cap"),
        GeneratedFaceSelector::SweepLateral { .. } | GeneratedFaceSelector::LoftLateral { .. } => {
            None
        }
    }
}

fn reversed_cap_frame(frame: PlaneFrame) -> PlaneFrame {
    PlaneFrame {
        origin: frame.origin,
        u_axis: frame.u_axis,
        v_axis: frame.v_axis.map(|value| -value),
        normal: frame.normal.map(|value| -value),
    }
}

fn sweep_cap_frames(
    path: &PathFrame,
    direction: SweepDirection,
    profile_origin: scc_engineering::Point2D,
) -> Result<(PlaneFrame, PlaneFrame), SccError> {
    let mut curves = path.curves.clone();
    if direction == SweepDirection::Reverse {
        curves.reverse();
        for curve in &mut curves {
            match curve {
                ParametricCurve::Line { start, end } => std::mem::swap(start, end),
                ParametricCurve::Arc {
                    start,
                    end,
                    direction,
                    ..
                } => {
                    std::mem::swap(start, end);
                    *direction = match direction {
                        ParametricArcDirection::CounterClockwise => {
                            ParametricArcDirection::Clockwise
                        }
                        ParametricArcDirection::Clockwise => {
                            ParametricArcDirection::CounterClockwise
                        }
                    };
                }
                ParametricCurve::Circle { .. } => {
                    return Err(structured(
                        "PATH_UNSUPPORTED_CURVE",
                        "Sweep path cannot contain a Circle",
                    ))
                }
            }
        }
    }
    let first = curves
        .first()
        .ok_or_else(|| structured("PATH_EMPTY", "Sweep path is empty"))?;
    let last = curves
        .last()
        .ok_or_else(|| structured("PATH_EMPTY", "Sweep path is empty"))?;
    let (start_point, _) = curve_endpoints(first).ok_or_else(|| {
        structured(
            "PATH_UNSUPPORTED_CURVE",
            "Sweep cap needs a Line or Arc path",
        )
    })?;
    let (_, end_point) = curve_endpoints(last).ok_or_else(|| {
        structured(
            "PATH_UNSUPPORTED_CURVE",
            "Sweep cap needs a Line or Arc path",
        )
    })?;
    let start_tangent_2d = curve_tangent(first, true)
        .ok_or_else(|| structured("PATH_DEGENERATE", "Sweep start tangent is degenerate"))?;
    let end_tangent_2d = curve_tangent(last, false)
        .ok_or_else(|| structured("PATH_DEGENERATE", "Sweep end tangent is degenerate"))?;
    let world_point = |point: [f64; 2]| {
        add(
            path.plane.origin,
            add(
                path.plane.u_axis.map(|v| v * point[0]),
                path.plane.v_axis.map(|v| v * point[1]),
            ),
        )
    };
    let world_tangent = |t: [f64; 2]| {
        add(
            path.plane.u_axis.map(|v| v * t[0]),
            path.plane.v_axis.map(|v| v * t[1]),
        )
    };
    let start_tangent = world_tangent(start_tangent_2d);
    let end_tangent = world_tangent(end_tangent_2d);
    let binormal = path.plane.normal;
    let start_u = cross(binormal, start_tangent);
    let end_u = cross(binormal, end_tangent);
    let start_origin = add(
        world_point(start_point),
        add(
            start_u.map(|v| -v * profile_origin.x),
            binormal.map(|v| -v * profile_origin.y),
        ),
    );
    let end_origin = add(
        world_point(end_point),
        add(
            end_u.map(|v| -v * profile_origin.x),
            binormal.map(|v| -v * profile_origin.y),
        ),
    );
    let start = PlaneFrame {
        origin: start_origin,
        u_axis: start_u,
        v_axis: binormal.map(|v| -v),
        normal: start_tangent.map(|v| -v),
    };
    let end = PlaneFrame {
        origin: end_origin,
        u_axis: end_u,
        v_axis: binormal,
        normal: end_tangent,
    };
    Ok((start, end))
}

fn frame_axes(normal: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let reference = if normal[2].abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = if dot(normal, reference).abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        reference
    };
    (u, cross(normal, u))
}

fn operation_boolean(operation: &ParametricRebuildOperation) -> Option<ParametricBooleanOperation> {
    match operation {
        ParametricRebuildOperation::Extrude(operation) => Some(operation.operation),
        ParametricRebuildOperation::Countersink(_) => Some(ParametricBooleanOperation::Cut),
        _ => None,
    }
}

fn operation_plane(operation: &ParametricRebuildOperation) -> Option<ParametricProfilePlane> {
    match operation {
        ParametricRebuildOperation::Extrude(operation) => Some(operation.plane),
        ParametricRebuildOperation::Countersink(operation) => Some(operation.plane),
        _ => None,
    }
}

fn operation_direction(operation: &ParametricRebuildOperation) -> Option<[f64; 3]> {
    match operation {
        ParametricRebuildOperation::Extrude(operation) => Some(operation.direction),
        ParametricRebuildOperation::Countersink(operation) => Some(operation.direction),
        _ => None,
    }
}

fn retarget_pattern_operation(
    operation: &mut ParametricRebuildOperation,
    pattern_id: &str,
    seed_feature_id: &FeatureId,
    instance: u32,
    source_kind: &str,
) -> Result<(), SccError> {
    let instance_prefix = format!(
        "{pattern_id}::instance-{instance:04}::{}",
        seed_feature_id.0
    );
    let original_id = match operation {
        ParametricRebuildOperation::Extrude(operation) => operation.feature_id.as_str(),
        ParametricRebuildOperation::Countersink(operation) => operation.feature_id.as_str(),
        _ => {
            return Err(structured(
                "PATTERN_UNSUPPORTED_SEED",
                "pattern seed operation cannot be copied",
            ))
        }
    };
    let suffix = original_id.strip_prefix(&seed_feature_id.0).unwrap_or("");
    let feature_id = format!("{instance_prefix}{suffix}");
    match operation {
        ParametricRebuildOperation::Extrude(operation) => {
            operation.feature_id = feature_id;
            operation.source_feature_id = Some(pattern_id.to_owned());
            operation.source_feature_kind = Some(source_kind.to_owned());
        }
        ParametricRebuildOperation::Countersink(operation) => {
            operation.feature_id = feature_id;
            operation.source_feature_id = pattern_id.to_owned();
            operation.source_feature_kind = source_kind.to_owned();
        }
        _ => {
            return Err(structured(
                "PATTERN_UNSUPPORTED_SEED",
                "pattern seed operation cannot be copied",
            ))
        }
    }
    Ok(())
}

fn translate_rebuild_operation(operation: &mut ParametricRebuildOperation, delta: [f64; 3]) {
    match operation {
        ParametricRebuildOperation::Extrude(operation) => translate_operation(operation, delta),
        ParametricRebuildOperation::Countersink(operation) => {
            for (origin, offset) in operation.plane.origin.iter_mut().zip(delta) {
                *origin += offset;
            }
            if let Some(face) = &mut operation.support_face {
                face.plane_offset += dot(face.normal, delta);
            }
        }
        _ => {}
    }
}

fn rotate_rebuild_operation(
    operation: &mut ParametricRebuildOperation,
    axis: [f64; 3],
    center: [f64; 3],
    angle: f64,
) -> Result<(), SccError> {
    match operation {
        ParametricRebuildOperation::Extrude(operation) => {
            rotate_operation(operation, axis, center, angle)
        }
        ParametricRebuildOperation::Countersink(operation) => {
            if operation
                .support_face
                .as_ref()
                .is_some_and(|face| dot(face.normal, axis).abs() < 1.0 - 1.0e-8)
            {
                return Err(structured(
                    "PATTERN_INVALID_AXIS",
                    "face-supported circular pattern must rotate in its support plane",
                ));
            }
            operation.plane.origin = add(
                center,
                rotate(sub(operation.plane.origin, center), axis, angle),
            );
            operation.plane.u_axis = rotate(operation.plane.u_axis, axis, angle);
            operation.plane.v_axis = rotate(operation.plane.v_axis, axis, angle);
            operation.plane.normal = rotate(operation.plane.normal, axis, angle);
            operation.direction = rotate(operation.direction, axis, angle);
            if let Some(face) = &mut operation.support_face {
                face.normal = rotate(face.normal, axis, angle);
                face.plane_offset = dot(face.normal, operation.plane.origin);
            }
            Ok(())
        }
        _ => Err(structured(
            "PATTERN_UNSUPPORTED_SEED",
            "pattern seed operation cannot be rotated",
        )),
    }
}

fn translate_operation(operation: &mut ParametricExtrudeOperation, delta: [f64; 3]) {
    for (origin, offset) in operation.plane.origin.iter_mut().zip(delta) {
        *origin += offset;
    }
    if let Some(face) = &mut operation.support_face {
        face.plane_offset += dot(face.normal, delta);
    }
}

fn rotate_operation(
    operation: &mut ParametricExtrudeOperation,
    axis: [f64; 3],
    center: [f64; 3],
    angle: f64,
) -> Result<(), SccError> {
    if operation
        .support_face
        .as_ref()
        .is_some_and(|face| dot(face.normal, axis).abs() < 1.0 - 1.0e-8)
    {
        return Err(structured(
            "PATTERN_INVALID_AXIS",
            "face-supported circular pattern must rotate in its support plane",
        ));
    }
    let origin = add(
        center,
        rotate(sub(operation.plane.origin, center), axis, angle),
    );
    operation.plane.origin = origin;
    operation.plane.u_axis = rotate(operation.plane.u_axis, axis, angle);
    operation.plane.v_axis = rotate(operation.plane.v_axis, axis, angle);
    operation.plane.normal = rotate(operation.plane.normal, axis, angle);
    operation.direction = rotate(operation.direction, axis, angle);
    if let Some(face) = &mut operation.support_face {
        face.normal = rotate(face.normal, axis, angle);
        face.plane_offset = dot(face.normal, operation.plane.origin);
    }
    Ok(())
}

fn rotate(value: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let cosine = angle.cos();
    let sine = angle.sin();
    add(
        add(
            value.map(|v| v * cosine),
            cross(axis, value).map(|v| v * sine),
        ),
        axis.map(|v| dot(axis, value) * v * (1.0 - cosine)),
    )
}
fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}
fn sub(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}
fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}
fn norm(vector: [f64; 3]) -> f64 {
    dot(vector, vector).sqrt()
}

fn resolve_axis(definition: &DatumAxisDefinition) -> Result<([f64; 3], [f64; 3]), SccError> {
    match definition {
        DatumAxisDefinition::Principal { axis } => {
            let direction = match axis {
                PrincipalAxis::X => [1.0, 0.0, 0.0],
                PrincipalAxis::Y => [0.0, 1.0, 0.0],
                PrincipalAxis::Z => [0.0, 0.0, 1.0],
            };
            Ok(([0.0, 0.0, 0.0], direction))
        }
        DatumAxisDefinition::ThroughTwoPoints { first, second } => {
            let origin = [first.x, first.y, first.z];
            let end = [second.x, second.y, second.z];
            if !origin
                .iter()
                .chain(end.iter())
                .all(|value| value.is_finite())
            {
                return Err(structured(
                    "INVALID_DATUM_AXIS",
                    "datum axis points must be finite",
                ));
            }
            let delta = sub(end, origin);
            if !delta.iter().all(|value| value.is_finite()) {
                return Err(structured(
                    "INVALID_DATUM_AXIS",
                    "datum axis point difference is not finite",
                ));
            }
            let scale = delta.iter().map(|value| value.abs()).fold(0.0, f64::max);
            if !scale.is_finite() || scale <= 1.0e-8 {
                return Err(structured(
                    "INVALID_DATUM_AXIS",
                    "datum axis direction must be nonzero",
                ));
            }
            let scaled = delta.map(|value| value / scale);
            let magnitude = norm(scaled);
            if !magnitude.is_finite() || magnitude <= f64::EPSILON {
                return Err(structured(
                    "INVALID_DATUM_AXIS",
                    "datum axis direction could not be normalized",
                ));
            }
            Ok((origin, scaled.map(|value| value / magnitude)))
        }
    }
}

fn principal_frame(plane: PrincipalPlane) -> PlaneFrame {
    match plane {
        PrincipalPlane::XY => PlaneFrame {
            origin: [0.0, 0.0, 0.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        },
        PrincipalPlane::XZ => PlaneFrame {
            origin: [0.0, 0.0, 0.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 0.0, 1.0],
            normal: [0.0, -1.0, 0.0],
        },
        PrincipalPlane::YZ => PlaneFrame {
            origin: [0.0, 0.0, 0.0],
            u_axis: [0.0, 1.0, 0.0],
            v_axis: [0.0, 0.0, 1.0],
            normal: [1.0, 0.0, 0.0],
        },
    }
}

fn resolve_plane_reference(
    reference: &PlaneReference,
    datums: &BTreeMap<FeatureId, PlaneFrame>,
) -> Result<PlaneFrame, SccError> {
    match reference {
        PlaneReference::Principal { plane } => Ok(principal_frame(*plane)),
        PlaneReference::Datum { feature } => datums.get(feature).copied().ok_or_else(|| {
            structured(
                "INVALID_DATUM",
                format!("datum reference {feature} has not been resolved"),
            )
        }),
        PlaneReference::PlanarFace { .. } | PlaneReference::GeneratedFace { .. } => {
            Err(structured(
                "INVALID_FACE_REFERENCE",
                "face references are resolved by the CAD worker, not as standalone datum offsets",
            ))
        }
    }
}

fn to_profile_plane(frame: PlaneFrame) -> ParametricProfilePlane {
    ParametricProfilePlane {
        origin: frame.origin,
        u_axis: frame.u_axis,
        v_axis: frame.v_axis,
        normal: frame.normal,
    }
}

fn lower_curve(geometry: &SketchCurveGeometry) -> Result<ParametricCurve, SccError> {
    Ok(match geometry {
        SketchCurveGeometry::Line { start, end } => ParametricCurve::Line {
            start: [start.x, start.y],
            end: [end.x, end.y],
        },
        SketchCurveGeometry::Arc {
            start,
            end,
            center,
            radius,
            direction,
        } => ParametricCurve::Arc {
            start: [start.x, start.y],
            end: [end.x, end.y],
            center: [center.x, center.y],
            radius: *radius,
            direction: match direction {
                scc_engineering::ArcDirection::CounterClockwise => {
                    ParametricArcDirection::CounterClockwise
                }
                scc_engineering::ArcDirection::Clockwise => ParametricArcDirection::Clockwise,
            },
        },
        SketchCurveGeometry::Circle { center, radius } => ParametricCurve::Circle {
            center: [center.x, center.y],
            radius: *radius,
        },
    })
}

fn validate_loft_sections(
    feature_id: &FeatureId,
    sections: &[(&FeatureId, &SketchFrame)],
) -> Result<(), SccError> {
    let first = sections
        .first()
        .ok_or_else(|| {
            structured(
                "LOFT_SECTION_COUNT_INVALID",
                "Loft needs at least two ordered sections",
            )
        })?
        .1;
    let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1.0e-8);
    let first_frame = first.plane;
    let mut previous_offset = 0.0;
    let first_signature = first
        .loops
        .first()
        .map(|loop_| loop_.iter().map(curve_kind).collect::<Vec<_>>());
    let mut orientation_sign = 0_i8;
    for (index, (section_id, sketch)) in sections.iter().enumerate() {
        if sketch.loops.len() != 1 || sketch.loops[0].is_empty() {
            return Err(structured(
                "LOFT_PROFILE_INVALID",
                format!("Loft {feature_id} section {section_id} must have one closed outer loop"),
            ));
        }
        if !close(sketch.plane.normal, first_frame.normal)
            || !close(sketch.plane.u_axis, first_frame.u_axis)
            || !close(sketch.plane.v_axis, first_frame.v_axis)
        {
            return Err(structured(
                "LOFT_ORIENTATION_UNSUPPORTED",
                format!("Loft {feature_id} sections must be parallel, co-oriented and use the same local axes"),
            ));
        }
        let delta = [
            sketch.plane.origin[0] - first_frame.origin[0],
            sketch.plane.origin[1] - first_frame.origin[1],
            sketch.plane.origin[2] - first_frame.origin[2],
        ];
        let offset = delta
            .iter()
            .zip(first_frame.normal)
            .map(|(a, b)| a * b)
            .sum::<f64>();
        let lateral = (0..3)
            .map(|axis| (delta[axis] - first_frame.normal[axis] * offset).powi(2))
            .sum::<f64>()
            .sqrt();
        if lateral > 1.0e-7 || (index > 0 && offset <= previous_offset + 1.0e-7) {
            return Err(structured(
                "LOFT_SECTION_ORDER_INVALID",
                format!("Loft {feature_id} sections must advance strictly along one direction without lateral plane offsets"),
            ));
        }
        previous_offset = offset;
        let signature = sketch.loops[0].iter().map(curve_kind).collect::<Vec<_>>();
        if first_signature.as_ref() != Some(&signature) {
            return Err(structured(
                "LOFT_PROFILE_INCOMPATIBLE",
                format!("Loft {feature_id} section {section_id} curve count or ordered Line/Arc/Circle correspondence differs"),
            ));
        }
        let area_sign = signed_loop_area(&sketch.loops[0]).signum() as i8;
        if area_sign == 0 || (orientation_sign != 0 && area_sign != orientation_sign) {
            return Err(structured(
                "LOFT_PROFILE_ORIENTATION_INVALID",
                format!("Loft {feature_id} section {section_id} has a degenerate or reversed outer loop"),
            ));
        }
        orientation_sign = area_sign;
    }
    Ok(())
}

fn curve_kind(curve: &ParametricCurve) -> u8 {
    match curve {
        ParametricCurve::Line { .. } => 0,
        ParametricCurve::Arc { .. } => 1,
        ParametricCurve::Circle { .. } => 2,
    }
}

fn signed_loop_area(curves: &[ParametricCurve]) -> f64 {
    use std::f64::consts::TAU;
    curves
        .iter()
        .map(|curve| match curve {
            ParametricCurve::Line { start, end } => (start[0] * end[1] - start[1] * end[0]) * 0.5,
            ParametricCurve::Circle { radius, .. } => std::f64::consts::PI * radius * radius,
            ParametricCurve::Arc {
                start,
                end,
                center,
                radius,
                direction,
            } => {
                let a0 = (start[1] - center[1]).atan2(start[0] - center[0]);
                let a1 = (end[1] - center[1]).atan2(end[0] - center[0]);
                let delta = if *direction == ParametricArcDirection::CounterClockwise {
                    (a1 - a0).rem_euclid(TAU)
                } else {
                    -(a0 - a1).rem_euclid(TAU)
                };
                0.5 * (radius * center[0] * (a1.sin() - a0.sin())
                    - radius * center[1] * (a1.cos() - a0.cos())
                    + radius * radius * delta)
            }
        })
        .sum()
}

fn validate_sweep_attachment(
    profile: PlaneFrame,
    profile_origin: scc_engineering::Point2D,
    path: &PathFrame,
    direction: SweepDirection,
) -> Result<(), SccError> {
    let mut curves = path.curves.clone();
    if direction == SweepDirection::Reverse {
        curves.reverse();
        for curve in &mut curves {
            match curve {
                ParametricCurve::Line { start, end } => std::mem::swap(start, end),
                ParametricCurve::Arc {
                    start,
                    end,
                    direction,
                    ..
                } => {
                    std::mem::swap(start, end);
                    *direction = match direction {
                        ParametricArcDirection::CounterClockwise => {
                            ParametricArcDirection::Clockwise
                        }
                        ParametricArcDirection::Clockwise => {
                            ParametricArcDirection::CounterClockwise
                        }
                    };
                }
                ParametricCurve::Circle { .. } => {
                    return Err(structured(
                        "PATH_UNSUPPORTED_CURVE",
                        "Sweep path cannot contain a Circle",
                    ))
                }
            }
        }
    }
    let first = curves
        .first()
        .ok_or_else(|| structured("PATH_EMPTY", "Sweep path is empty"))?;
    let (start, _) = curve_endpoints(first).ok_or_else(|| {
        structured(
            "PATH_UNSUPPORTED_CURVE",
            "Sweep path must contain Line or Arc",
        )
    })?;
    let tangent2 = curve_tangent(first, true)
        .ok_or_else(|| structured("PATH_DEGENERATE", "Sweep path start tangent is degenerate"))?;
    let tangent = add(
        path.plane.u_axis.map(|value| value * tangent2[0]),
        path.plane.v_axis.map(|value| value * tangent2[1]),
    );
    let expected_u = cross(path.plane.normal, tangent);
    let world_start = add(
        add(
            path.plane.origin,
            path.plane.u_axis.map(|value| value * start[0]),
        ),
        path.plane.v_axis.map(|value| value * start[1]),
    );
    let profile_anchor = add(
        add(
            profile.origin,
            profile.u_axis.map(|value| value * profile_origin.x),
        ),
        profile.v_axis.map(|value| value * profile_origin.y),
    );
    let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= 1.0e-7);
    if !close(world_start, profile_anchor)
        || !close(tangent, profile.normal)
        || !close(expected_u, profile.u_axis)
        || !close(path.plane.normal, profile.v_axis)
    {
        return Err(structured("SWEEP_PROFILE_ORIENTATION_INVALID", "Sweep Sketch anchor and frame must coincide with the directed Path endpoint, tangent, and fixed-binormal axes"));
    }
    Ok(())
}

fn curve_endpoints(curve: &ParametricCurve) -> Option<([f64; 2], [f64; 2])> {
    match curve {
        ParametricCurve::Line { start, end } | ParametricCurve::Arc { start, end, .. } => {
            Some((*start, *end))
        }
        ParametricCurve::Circle { .. } => None,
    }
}

fn curve_tangent(curve: &ParametricCurve, at_start: bool) -> Option<[f64; 2]> {
    let tangent = match curve {
        ParametricCurve::Line { start, end } => [end[0] - start[0], end[1] - start[1]],
        ParametricCurve::Arc {
            start,
            end,
            center,
            direction,
            ..
        } => {
            let point = if at_start { start } else { end };
            let sign = if *direction == ParametricArcDirection::CounterClockwise {
                1.0
            } else {
                -1.0
            };
            [
                -sign * (point[1] - center[1]),
                sign * (point[0] - center[0]),
            ]
        }
        ParametricCurve::Circle { .. } => return None,
    };
    let magnitude = (tangent[0] * tangent[0] + tangent[1] * tangent[1]).sqrt();
    (magnitude > 1.0e-8).then_some([tangent[0] / magnitude, tangent[1] / magnitude])
}

fn normalize(vector: [f64; 3], code: &str, label: &str) -> Result<[f64; 3], SccError> {
    let magnitude = norm(vector);
    if !magnitude.is_finite() || magnitude <= 1.0e-8 {
        return Err(structured(
            code,
            format!("{label} must be finite and nonzero"),
        ));
    }
    Ok(vector.map(|value| value / magnitude))
}

fn lower_loop(loop_: &scc_engineering::SketchLoop) -> Result<Vec<ParametricCurve>, SccError> {
    loop_
        .entities
        .iter()
        .map(|entity| {
            let curve = match entity.geometry {
                SketchCurveGeometry::Line { start, end } => ParametricCurve::Line {
                    start: [start.x, start.y],
                    end: [end.x, end.y],
                },
                SketchCurveGeometry::Arc {
                    start,
                    end,
                    center,
                    radius,
                    direction,
                } => ParametricCurve::Arc {
                    start: [start.x, start.y],
                    end: [end.x, end.y],
                    center: [center.x, center.y],
                    radius,
                    direction: match direction {
                        scc_engineering::ArcDirection::CounterClockwise => {
                            ParametricArcDirection::CounterClockwise
                        }
                        scc_engineering::ArcDirection::Clockwise => {
                            ParametricArcDirection::Clockwise
                        }
                    },
                },
                SketchCurveGeometry::Circle { center, radius } => ParametricCurve::Circle {
                    center: [center.x, center.y],
                    radius,
                },
            };
            Ok(curve)
        })
        .collect()
}

fn resolve_direction(direction: &ExtrudeDirection, normal: [f64; 3]) -> Result<[f64; 3], SccError> {
    let vector = match direction {
        ExtrudeDirection::Normal { reversed } => {
            if *reversed {
                normal.map(|value| -value)
            } else {
                normal
            }
        }
        ExtrudeDirection::Vector { value } => [value.x, value.y, value.z],
    };
    let magnitude = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !magnitude.is_finite() || magnitude <= f64::EPSILON {
        return Err(structured(
            "INVALID_EXTRUDE_EXTENT",
            "extrude direction must be finite and nonzero",
        ));
    }
    let normalized = vector.map(|value| value / magnitude);
    let dot = normalized
        .iter()
        .zip(normal)
        .map(|(left, right)| left * right)
        .sum::<f64>();
    if (dot.abs() - 1.0).abs() > 1.0e-8 {
        return Err(structured(
            "INVALID_EXTRUDE_EXTENT",
            "G1B extrude vector must be parallel to its sketch normal",
        ));
    }
    Ok(normalized)
}

fn lower_extent(extent: ExtrudeExtent) -> Result<ParametricExtent, SccError> {
    match extent {
        ExtrudeExtent::Distance { length } => Ok(ParametricExtent::Distance { length }),
        ExtrudeExtent::ThroughAll => Ok(ParametricExtent::ThroughAll),
        ExtrudeExtent::Symmetric { length } => Ok(ParametricExtent::Symmetric { length }),
    }
}

fn map_model_validation_error(error: scc_engineering::error::EngineeringError) -> SccError {
    let message = error.to_string();
    const CODES: &[&str] = &[
        "INVALID_FACE_REFERENCE",
        "AMBIGUOUS_FACE_REFERENCE",
        "INVALID_DATUM_AXIS",
        "INVALID_REVOLVE_ANGLE",
        "UNSUPPORTED_SUPPORT_SURFACE",
        "INVALID_HOLE_AXIS",
        "INVALID_HOLE_DIAMETER",
        "INVALID_HOLE_DEPTH",
        "INVALID_COUNTERBORE",
        "INVALID_COUNTERSINK",
        "INVALID_FILLET_RADIUS",
        "INVALID_CHAMFER_DISTANCE",
        "INVALID_EDGE_REFERENCE",
        "DUPLICATE_EDGE_REFERENCE",
        "PATTERN_SEED_NOT_FOUND",
        "PATTERN_INVALID_COUNT",
        "PATTERN_INVALID_SPACING",
        "PATTERN_INVALID_AXIS",
        "PATTERN_INSTANCE_COLLISION",
        "PATTERN_INVALID_RADIUS",
        "PATTERN_START_ANGLE_MISMATCH",
        "PATTERN_INVALID_PLACEMENT",
        "PATTERN_DUPLICATE_PLACEMENT",
        "PATTERN_UNSUPPORTED_PLACEMENT_PLANE",
        "LOFT_SECTION_REFERENCE_NOT_FOUND",
        "LOFT_SECTION_REFERENCE_INVALID",
        "LOFT_SECTION_ORDER_INVALID",
        "LOFT_DUPLICATE_SECTION",
        "LOFT_SECTION_COUNT_INVALID",
    ];
    let code = CODES
        .iter()
        .find(|code| message.contains(**code))
        .copied()
        .unwrap_or("REBUILD_DEPENDENCY_ERROR");
    structured(code, message)
}

fn map_backend_error(error: scc_cad_backend::CadBackendError) -> SccError {
    match error {
        scc_cad_backend::CadBackendError::Backend(failure) => {
            structured(&failure.code, failure.message)
        }
        other => structured("BOOLEAN_FAILED", other.to_string()),
    }
}

fn structured(code: &str, message: impl Into<String>) -> SccError {
    let message = message.into();
    let error = if code.starts_with("SCC-") {
        StructuredError::new(code, ErrorStage::Spatial, message)
    } else {
        StructuredError::new("SCC-SPAT-017", ErrorStage::Spatial, message)
            .with_detail("parametric_code", serde_json::json!(code))
    };
    SccError::Structured(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_engineering::command::EngineeringCommand;
    use scc_engineering::executor::CommandExecutor;
    use scc_engineering::feature::{
        EngineeringFeature, ExtrudeDirection, ExtrudeOperation, ParametricFeature, SketchLoop,
        SketchProfile,
    };
    use scc_engineering::{PartId, Point2D};

    fn model() -> EngineeringModel {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::CreatePart {
                id: PartId("part".into()),
                name: "part".into(),
            },
        )
        .unwrap();
        model
    }

    fn add(model: &mut EngineeringModel, id: &str, feature: ParametricFeature) {
        CommandExecutor::apply(
            model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Parametric {
                    id: FeatureId(id.into()),
                    name: id.into(),
                    feature,
                    suppressed: false,
                },
            },
        )
        .unwrap();
    }

    fn rect_profile(name: &str, width: f64, height: f64) -> SketchProfile {
        SketchProfile {
            outer: SketchLoop::rectangle(name, Point2D::new(0.0, 0.0), width, height).unwrap(),
            inner_loops: Vec::new(),
        }
    }

    fn add_rectangle_history() -> EngineeringModel {
        let mut model = model();
        add(
            &mut model,
            "sketch",
            ParametricFeature::Sketch {
                support: PlaneReference::Principal {
                    plane: PrincipalPlane::XY,
                },
                profile: rect_profile("edge", 100.0, 60.0),
            },
        );
        add(
            &mut model,
            "base",
            ParametricFeature::Extrude {
                profile: FeatureId("sketch".into()),
                operation: scc_engineering::ExtrudeOperation::Add,
                direction: ExtrudeDirection::Normal { reversed: false },
                extent: ExtrudeExtent::Distance { length: 10.0 },
            },
        );
        model
    }

    #[test]
    fn g7b_missing_or_suppressed_section_sketch_keeps_specific_error_code() {
        use scc_engineering::parametric::{LoftMode, LoftOperation, LoftOrientationPolicy};

        let valid_history = || {
            let mut model = model();
            for id in ["section_a", "section_b"] {
                add(
                    &mut model,
                    id,
                    ParametricFeature::Sketch {
                        support: scc_engineering::PlaneReference::Principal {
                            plane: scc_engineering::PrincipalPlane::XY,
                        },
                        profile: rect_profile(id, 20.0, 20.0),
                    },
                );
            }
            add(
                &mut model,
                "loft",
                ParametricFeature::Loft {
                    sections: vec![FeatureId("section_a".into()), FeatureId("section_b".into())],
                    operation: LoftOperation::Add,
                    mode: LoftMode::Ruled,
                    orientation_policy: LoftOrientationPolicy::Aligned,
                },
            );
            model
        };

        let mut missing = valid_history();
        let missing_id = FeatureId("section_b".into());
        missing.features.remove(&missing_id);
        missing.feature_order.retain(|id| id != &missing_id);
        let SccError::Structured(missing_error) =
            lower_parametric_manufacturing_spec(&missing).unwrap_err()
        else {
            panic!("missing section references must remain structured");
        };
        assert_eq!(
            missing_error.details.as_ref().unwrap()["parametric_code"],
            "LOFT_SECTION_REFERENCE_NOT_FOUND"
        );

        let mut suppressed = valid_history();
        let EngineeringFeature::Parametric {
            suppressed: is_suppressed,
            ..
        } = suppressed
            .features
            .get_mut(&FeatureId("section_b".into()))
            .unwrap()
        else {
            panic!("section must remain a parametric feature");
        };
        *is_suppressed = true;
        let SccError::Structured(suppressed_error) =
            lower_parametric_manufacturing_spec(&suppressed).unwrap_err()
        else {
            panic!("suppressed section references must remain structured");
        };
        assert_eq!(
            suppressed_error.details.as_ref().unwrap()["parametric_code"],
            "LOFT_SECTION_REFERENCE_NOT_FOUND"
        );
    }

    #[test]
    fn non_scc_parametric_diagnostics_keep_raw_code_in_valid_spatial_envelope() {
        let SccError::Structured(error) =
            structured("PATTERN_INSTANCE_COLLISION", "pattern misses body")
        else {
            panic!("structured parametric error must remain structured");
        };
        assert_eq!(error.code, "SCC-SPAT-017");
        assert_eq!(
            error.details.as_ref().unwrap()["parametric_code"],
            "PATTERN_INSTANCE_COLLISION"
        );
        error.validate_contract().unwrap();
    }

    #[test]
    fn ordered_generic_history_lowers_exact_curve_data_and_is_deterministic() {
        let model = add_rectangle_history();
        let first = build_request(&model).unwrap();
        let second = build_request(&model).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.operations.len(), 1);
        let first_extrude = first.operations[0].as_extrude().unwrap();
        assert_eq!(first_extrude.feature_id, "base");
        assert_eq!(first_extrude.loops[0].len(), 4);
        assert_eq!(
            first_extrude.profile_entity_ids[0],
            vec!["edge-0000", "edge-0001", "edge-0002", "edge-0003"]
        );
        assert_eq!(first_extrude.direction, [0.0, 0.0, 1.0]);
        assert_eq!(
            first_extrude.extent,
            ParametricExtent::Distance { length: 10.0 }
        );
        first.validate().unwrap();
    }

    #[test]
    fn g4_datum_axis_and_revolve_lower_to_normalized_versioned_operation() {
        let mut model = model();
        add(
            &mut model,
            "profile",
            ParametricFeature::Sketch {
                support: PlaneReference::Principal {
                    plane: PrincipalPlane::XZ,
                },
                profile: SketchProfile {
                    outer: SketchLoop::rectangle(
                        "revolve-edge",
                        Point2D::new(10.0, 5.0),
                        20.0,
                        10.0,
                    )
                    .unwrap(),
                    inner_loops: Vec::new(),
                },
            },
        );
        add(
            &mut model,
            "axis",
            ParametricFeature::DatumAxis {
                definition: scc_engineering::DatumAxisDefinition::ThroughTwoPoints {
                    first: scc_engineering::Vector3D {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    second: scc_engineering::Vector3D {
                        x: 0.0,
                        y: 0.0,
                        z: 4.0,
                    },
                },
            },
        );
        add(
            &mut model,
            "body",
            ParametricFeature::Revolve {
                profile: FeatureId("profile".into()),
                axis: scc_engineering::AxisReference::DatumAxis {
                    feature: FeatureId("axis".into()),
                },
                operation: scc_engineering::RevolveOperation::Add,
                angle: scc_engineering::RevolveAngle::Partial { degrees: 90.0 },
                orientation: scc_engineering::RevolveOrientation::Reverse,
            },
        );
        let request = build_request(&model).unwrap();
        assert_eq!(request.schema_version, 6);
        assert_eq!(request.operations.len(), 1);
        let revolve = request.operations[0].as_revolve().unwrap();
        assert_eq!(revolve.feature_id, "body");
        assert_eq!(revolve.sketch_id, "profile");
        assert_eq!(revolve.axis_origin, [0.0, 0.0, 0.0]);
        assert_eq!(revolve.direction, [0.0, 0.0, 1.0]);
        assert_eq!(
            revolve.angle,
            ParametricRevolveAngle::Partial { degrees: 90.0 }
        );
        assert_eq!(revolve.orientation, ParametricRevolveOrientation::Reverse);
        request.validate().unwrap();
    }

    #[test]
    fn g6b_countersink_and_explicit_pattern_lower_to_native_transient_operations() {
        use scc_engineering::parametric::PatternPlacement;
        let mut model = add_rectangle_history();
        add(
            &mut model,
            "sink_hole",
            ParametricFeature::Hole {
                support: PlaneReference::Principal {
                    plane: PrincipalPlane::XY,
                },
                position: Point2D::new(0.0, 0.0),
                diameter: 5.0,
                reversed: false,
                termination: HoleTermination::ThroughAll,
                kind: HoleKind::Countersink {
                    diameter: 10.0,
                    included_angle_degrees: 90.0,
                },
            },
        );
        add(
            &mut model,
            "hole_pattern",
            ParametricFeature::Pattern {
                seed_feature_id: FeatureId("sink_hole".into()),
                definition: PatternDefinition::ExplicitPlacement {
                    reference_plane: PlaneReference::Principal {
                        plane: PrincipalPlane::XY,
                    },
                    placements: vec![
                        PatternPlacement {
                            local_u_mm: 20.0,
                            local_v_mm: 0.0,
                        },
                        PatternPlacement {
                            local_u_mm: -20.0,
                            local_v_mm: 0.0,
                        },
                    ],
                },
            },
        );

        let request = lower_parametric_manufacturing_spec(&model).unwrap();
        let countersinks = request
            .operations
            .iter()
            .filter_map(|operation| match operation {
                ParametricRebuildOperation::Countersink(operation) => Some(operation.as_ref()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(countersinks.len(), 3);
        assert_eq!(countersinks[0].feature_id, "sink_hole::countersink");
        assert_eq!(countersinks[0].source_feature_id, "sink_hole");
        assert_eq!(countersinks[0].main_diameter, 5.0);
        assert_eq!(countersinks[0].mouth_diameter, 10.0);
        assert_eq!(countersinks[0].included_angle_degrees, 90.0);
        assert_eq!(
            countersinks
                .iter()
                .map(|operation| operation.plane.origin)
                .collect::<Vec<_>>(),
            vec![[0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [-20.0, 0.0, 0.0]],
        );
        assert_eq!(countersinks[1].source_feature_id, "hole_pattern");
        assert_eq!(
            countersinks[1].source_feature_kind,
            "pattern:explicit_placement"
        );
        assert_eq!(
            countersinks[1].feature_id,
            "hole_pattern::instance-0001::sink_hole::countersink"
        );
        let ids = request
            .operations
            .iter()
            .map(ParametricRebuildOperation::feature_id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), request.operations.len());
        request.validate().unwrap();
    }

    #[test]
    fn g3_treatments_lower_as_ordered_features_and_obey_edit_suppress_and_reorder() {
        use scc_engineering::parametric::{
            EdgeReference, ProfileEdgeSide, SemanticEdgeQualifier, SemanticEdgeSelector,
        };
        let mut model = add_rectangle_history();
        let reference = EdgeReference {
            source_feature_id: FeatureId("base".into()),
            selector: SemanticEdgeSelector::ProfileBoundary {
                entity_id: scc_engineering::SketchEntityId("edge-0000".into()),
                side: ProfileEdgeSide::Top,
            },
            qualifier: SemanticEdgeQualifier {
                curve_kind: Some(SemanticEdgeCurveKind::Line),
                ..Default::default()
            },
        };
        add(
            &mut model,
            "round",
            ParametricFeature::Fillet {
                edge_refs: vec![reference.clone()],
                radius: 2.0,
            },
        );
        add(
            &mut model,
            "break",
            ParametricFeature::Chamfer {
                edge_refs: vec![reference],
                distance: 0.5,
            },
        );
        let request = build_request(&model).unwrap();
        assert!(
            matches!(request.operations[1], ParametricRebuildOperation::Fillet { ref feature_id, radius: 2.0, .. } if feature_id == "round")
        );
        assert!(
            matches!(request.operations[2], ParametricRebuildOperation::Chamfer { ref feature_id, distance: 0.5, .. } if feature_id == "break")
        );

        let edge_refs = match &model.features[&FeatureId("round".into())] {
            EngineeringFeature::Parametric {
                feature: ParametricFeature::Fillet { edge_refs, .. },
                ..
            } => edge_refs.clone(),
            _ => unreachable!(),
        };
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::ReplaceFeature {
                feature: EngineeringFeature::Parametric {
                    id: FeatureId("round".into()),
                    name: "round".into(),
                    feature: ParametricFeature::Fillet {
                        edge_refs,
                        radius: 3.0,
                    },
                    suppressed: false,
                },
            },
        )
        .unwrap();
        let modified = build_request(&model).unwrap();
        assert!(matches!(
            modified.operations[1],
            ParametricRebuildOperation::Fillet { radius: 3.0, .. }
        ));
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureSuppressed {
                target: FeatureId("round".into()),
                suppressed: true,
            },
        )
        .unwrap();
        assert!(build_request(&model)
            .unwrap()
            .operations
            .iter()
            .all(|operation| !matches!(operation, ParametricRebuildOperation::Fillet { .. })));
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureSuppressed {
                target: FeatureId("round".into()),
                suppressed: false,
            },
        )
        .unwrap();
        let previous = model.feature_order.clone();
        assert!(CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::ReorderFeature {
                target: FeatureId("round".into()),
                before: Some(FeatureId("base".into()))
            }
        )
        .is_err());
        assert_eq!(model.feature_order, previous);
    }

    #[test]
    fn principal_xz_and_yz_and_signed_offset_are_transformed_without_z_hardcoding() {
        let mut model = model();
        add(
            &mut model,
            "offset",
            ParametricFeature::DatumPlane {
                definition: DatumPlaneDefinition::Offset {
                    base: PlaneReference::Principal {
                        plane: PrincipalPlane::XZ,
                    },
                    distance: -12.0,
                },
            },
        );
        add(
            &mut model,
            "sketch",
            ParametricFeature::Sketch {
                support: PlaneReference::Datum {
                    feature: FeatureId("offset".into()),
                },
                profile: rect_profile("edge", 20.0, 30.0),
            },
        );
        add(
            &mut model,
            "base",
            ParametricFeature::Extrude {
                profile: FeatureId("sketch".into()),
                operation: ExtrudeOperation::Add,
                direction: ExtrudeDirection::Normal { reversed: false },
                extent: ExtrudeExtent::Distance { length: 6.0 },
            },
        );
        let request = build_request(&model).unwrap();
        let extrude = request.operations[0].as_extrude().unwrap();
        assert_eq!(extrude.plane.origin, [0.0, 12.0, 0.0]);
        assert_eq!(extrude.plane.normal, [0.0, -1.0, 0.0]);
        assert_eq!(extrude.direction, [0.0, -1.0, 0.0]);
    }

    #[test]
    fn suppressed_cut_is_omitted_from_the_fresh_rebuild_history() {
        let mut model = add_rectangle_history();
        add(
            &mut model,
            "cut_sketch",
            ParametricFeature::Sketch {
                support: PlaneReference::Principal {
                    plane: PrincipalPlane::XY,
                },
                profile: SketchProfile {
                    outer: SketchLoop::circle("cut-circle", Point2D::new(0.0, 0.0), 5.0),
                    inner_loops: Vec::new(),
                },
            },
        );
        add(
            &mut model,
            "cut",
            ParametricFeature::Extrude {
                profile: FeatureId("cut_sketch".into()),
                operation: ExtrudeOperation::Cut,
                direction: ExtrudeDirection::Normal { reversed: false },
                extent: ExtrudeExtent::Distance { length: 2.0 },
            },
        );
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetFeatureSuppressed {
                target: FeatureId("cut".into()),
                suppressed: true,
            },
        )
        .unwrap();
        let request = build_request(&model).unwrap();
        assert_eq!(
            request
                .operations
                .iter()
                .map(|op| op.feature_id())
                .collect::<Vec<_>>(),
            vec!["base"]
        );
    }
}
