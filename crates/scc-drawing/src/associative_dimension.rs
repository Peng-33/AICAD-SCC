//! Associative dimensions for validated EngineeringModel v2 rebuilds.
//!
//! Drawing dimensions remain presentation records on DrawingView. This module
//! adds the formal association index that binds each generated dimension to a
//! stable EngineeringModel reference, the validated Spatial evidence, the
//! projected Drawing view, and its legal layout. It never stores kernel
//! ordinals or derives engineering values from pixels/SVG.

use std::collections::{BTreeMap, BTreeSet};

use scc_engineering::feature::{
    EngineeringFeature, ExtrudeExtent, ExtrudeOperation, HoleKind, HoleTermination,
    ParametricFeature, PatternDefinition,
};
use scc_engineering::parametric::{
    DatumPlaneDefinition, EdgeReference as EngineeringEdgeReference, PlaneReference,
    PrincipalPlane, ProfileEdgeSide, SemanticEdgeCurveKind, SemanticEdgeQualifier,
    SemanticEdgeSelector, SemanticPlanarFaceReference,
};
use scc_engineering::{EngineeringModel, FeatureId};
use scc_spatial::{
    ParametricSpatialEvidence, SpatialEdgeEvidence, SpatialFaceEvidence, SpatialFaceSurface,
    SpatialFootprint, SpatialKernelEdgeCurve, SpatialModel,
};
use serde::{Deserialize, Serialize};

use crate::dimension::{
    DimensionAxis3D, DimensionComponent, DimensionValue, DimensionValueSource, DrawingDimension,
    DrawingDimensionId, DrawingDimensionKind, DrawingDimensionTarget, LinearAxis,
    ParametricCounterboreCallout, ParametricDimensionParameter, ParametricHoleCalloutSemantic,
    ParametricHoleExtent, ParametricHolePatternCallout, ResolvedDimensionValue,
};
use crate::dimension_layout::{
    ArrowAnchor, DimensionLayoutGeometry, DrawingDimensionLayout, LayoutSegment,
};
use crate::model::{DrawingBounds, DrawingPrimitive, DrawingPrimitiveRole, DrawingView};
use crate::{DrawingError, DrawingPoint, DrawingViewId};

const ASSOCIATIVE_TOLERANCE_MM: f64 = 1.0e-5;
const DIMENSION_OFFSET_MM: f64 = 8.0;
const EXTENSION_GAP_MM: f64 = 1.0;
const EXTENSION_OVERHANG_MM: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reference", rename_all = "snake_case")]
// EdgeReference stores a full engineering edge selector inline. Boxing it
// would change the public drawing model only to satisfy a size lint.
#[allow(clippy::large_enum_variant)]
pub enum AssociativeDimensionReference {
    BodyEnvelope {
        axis: DimensionAxis3D,
    },
    FeatureParameter {
        feature: FeatureId,
        parameter: ParametricDimensionParameter,
    },
    FeatureReference {
        feature: FeatureId,
    },
    FaceReference {
        face: SemanticPlanarFaceReference,
    },
    EdgeReference {
        edge: EngineeringEdgeReference,
    },
    DatumReference {
        feature: FeatureId,
        kind: AssociativeDatumKind,
    },
    ValidatedGeometryReference {
        model_sha256: String,
        evidence_ids: Vec<String>,
    },
    FeatureSemantic {
        feature: FeatureId,
        semantic: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociativeDatumKind {
    Plane,
    Axis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociativeDimensionUnit {
    Millimetres,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionValueProvenance {
    EngineeringDeclared,
    GeometryDerived,
    UserNote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionObservability {
    Observable,
    AnnotatedFromValidatedFeature,
    RequiresSectionView,
    NotObservable,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociativeDimensionStatus {
    Resolved,
    Invalidated,
    Unsupported,
}

/// Exact canonical B-Rep evidence plus an independent Drawing observation.
/// `evidence_ids` are opaque stable kernel geometry descriptors, never indices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionGeometryWitness {
    pub model_sha256: String,
    pub evidence_ids: Vec<String>,
    pub geometry_measurement_mm: f64,
    pub drawing_measurement_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub component_measurements_mm: BTreeMap<ParametricDimensionParameter, f64>,
    pub tolerance_mm: f64,
}

/// Association metadata indexed by the stable annotation id. The DrawingView
/// dimension and layout are the visible rendering; this record binds them to
/// validated engineering and geometry evidence without introducing a second
/// editable geometry model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssociativeDimension {
    pub annotation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing_dimension: Option<DrawingDimensionId>,
    pub kind: DrawingDimensionKind,
    pub reference: AssociativeDimensionReference,
    pub view: DrawingViewId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nominal: Option<ResolvedDimensionValue>,
    pub unit: AssociativeDimensionUnit,
    pub sources: Vec<DimensionValueSource>,
    pub provenance: DimensionValueProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub witness: Option<DimensionGeometryWitness>,
    pub observability: DimensionObservability,
    pub status: AssociativeDimensionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

/// Build associative dimensions only for an already validated parametric
/// spatial rebuild. Unsupported/non-unique references fail closed.
pub(crate) fn build_parametric_associative_dimensions(
    engineering: &EngineeringModel,
    spatial: &SpatialModel,
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
) -> Result<BTreeMap<DrawingDimensionId, AssociativeDimension>, DrawingError> {
    let evidence = match &spatial.body.footprint {
        SpatialFootprint::Parametric { evidence } => evidence.as_ref(),
        _ => return Ok(BTreeMap::new()),
    };
    let extents = body_extents(evidence)?;
    let mut result = BTreeMap::new();

    add_body_dimension(
        &mut result,
        views,
        evidence,
        DimensionAxis3D::X,
        DrawingViewId("front".into()),
        LinearAxis::Horizontal,
        DrawingDimensionId("associative.body.length".into()),
        extents[0],
        extents[3],
    )?;
    add_body_dimension(
        &mut result,
        views,
        evidence,
        DimensionAxis3D::Y,
        DrawingViewId("left".into()),
        LinearAxis::Vertical,
        DrawingDimensionId("associative.body.width".into()),
        extents[1],
        extents[4],
    )?;

    let feature_thickness = if frame_axis_is_z(&evidence.frame.axis) {
        add_base_extrude_thickness(&mut result, views, engineering, evidence)?
    } else {
        false
    };
    if !feature_thickness {
        add_body_dimension(
            &mut result,
            views,
            evidence,
            DimensionAxis3D::Z,
            DrawingViewId("top".into()),
            LinearAxis::Vertical,
            DrawingDimensionId("associative.body.thickness".into()),
            extents[2],
            extents[5],
        )?;
    }

    let patterned_seeds = engineering
        .ordered_feature_ids()
        .into_iter()
        .filter_map(|id| match engineering.features.get(&id) {
            Some(EngineeringFeature::Parametric {
                feature:
                    ParametricFeature::Pattern {
                        seed_feature_id, ..
                    },
                suppressed: false,
                ..
            }) => Some(seed_feature_id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    for id in engineering.ordered_feature_ids() {
        let Some(EngineeringFeature::Parametric {
            feature,
            suppressed: false,
            ..
        }) = engineering.features.get(&id)
        else {
            continue;
        };
        match feature {
            ParametricFeature::Hole {
                diameter,
                termination,
                kind,
                ..
            } if !patterned_seeds.contains(&id) => {
                add_hole_callout(
                    &mut result,
                    views,
                    engineering,
                    spatial,
                    evidence,
                    &id,
                    &id,
                    *diameter,
                    *termination,
                    *kind,
                    None,
                )?;
            }
            ParametricFeature::Pattern {
                seed_feature_id,
                definition,
            } => {
                let Some(EngineeringFeature::Parametric {
                    feature:
                        ParametricFeature::Hole {
                            diameter,
                            termination,
                            kind,
                            ..
                        },
                    suppressed: false,
                    ..
                }) = engineering.features.get(seed_feature_id)
                else {
                    continue;
                };
                add_hole_callout(
                    &mut result,
                    views,
                    engineering,
                    spatial,
                    evidence,
                    &id,
                    seed_feature_id,
                    *diameter,
                    *termination,
                    *kind,
                    Some(definition),
                )?;
            }
            ParametricFeature::Extrude {
                profile,
                operation: ExtrudeOperation::Add,
                direction,
                ..
            } => {
                if let Some((entity_id, diameter)) =
                    crate::dimension::circular_add_extrude_profile(engineering, profile, direction)
                {
                    add_cylindrical_boss_diameter(
                        &mut result,
                        views,
                        engineering,
                        evidence,
                        &id,
                        &entity_id,
                        diameter,
                    )?;
                }
            }
            ParametricFeature::Fillet { radius, .. } => {
                add_fillets_as_unsupported(&mut result, evidence, &id, *radius)?;
            }
            _ => {}
        }
    }
    Ok(result)
}

/// Verify every association against the same EngineeringModel, B-Rep evidence,
/// Drawing view, and visual layout consumed by SVG/DXF.
pub(crate) fn validate_parametric_associative_dimensions(
    drawing: &crate::DrawingModel,
    engineering: &EngineeringModel,
    spatial: &SpatialModel,
) -> Result<(), DrawingError> {
    let evidence = match &spatial.body.footprint {
        SpatialFootprint::Parametric { evidence } => evidence.as_ref(),
        _ => return Ok(()),
    };
    for (id, association) in &drawing.associative_dimensions {
        if association.annotation_id != id.0 {
            return Err(invalid(
                "associative annotation id does not match its stable key",
            ));
        }
        if let Some(witness) = &association.witness {
            validate_geometry_witness(witness, association, engineering, evidence)?;
        } else if association.status == AssociativeDimensionStatus::Resolved {
            return Err(invalid(
                "resolved associative dimension has no geometry witness",
            ));
        }
        for source in &association.sources {
            if let DimensionValueSource::EngineeringFeatureParameter { feature, parameter } = source
            {
                if expected_parameter_value(engineering, feature, *parameter).is_none() {
                    return Err(invalid(&format!(
                        "{} references a missing, suppressed, or incompatible feature parameter",
                        association.annotation_id
                    )));
                }
            }
            if let DimensionValueSource::VerifiedBodyGeometry { model_sha256, .. } = source {
                if model_sha256 != &evidence.model_sha256 {
                    return Err(invalid(
                        "body-envelope source digest does not match the current rebuild",
                    ));
                }
            }
        }
        match association.status {
            AssociativeDimensionStatus::Unsupported => {
                if association.drawing_dimension.is_some() || association.diagnostic.is_none() {
                    return Err(invalid("unsupported association must not claim a rendered dimension and must explain why"));
                }
                continue;
            }
            AssociativeDimensionStatus::Invalidated => {
                return Err(invalid(&format!(
                    "{} references an invalidated feature",
                    association.annotation_id
                )));
            }
            AssociativeDimensionStatus::Resolved => {}
        }
        let dimension_id = association
            .drawing_dimension
            .as_ref()
            .ok_or_else(|| invalid("resolved association has no DrawingDimension"))?;
        let view = drawing
            .sheets
            .values()
            .flat_map(|sheet| sheet.views.values())
            .find(|view| view.id == association.view)
            .ok_or_else(|| invalid("associative dimension view is missing"))?;
        let dimension = view
            .dimensions
            .get(dimension_id)
            .ok_or_else(|| invalid("associative DrawingDimension is missing"))?;
        if dimension.id != *dimension_id || dimension.kind != association.kind {
            return Err(invalid(
                "associative dimension and DrawingDimension semantics disagree",
            ));
        }
        let layout = view
            .dimension_layouts
            .get(dimension_id)
            .ok_or_else(|| invalid("resolved associative dimension has no legal layout"))?;
        let sources = dimension.sources();
        if sources.len() != association.sources.len()
            || sources
                .iter()
                .zip(&association.sources)
                .any(|(left, right)| *left != right)
        {
            return Err(invalid(
                "associative sources disagree with the DrawingDimension",
            ));
        }
        let dimension_nominal = dimension_nominal(&dimension.value)
            .ok_or_else(|| invalid("associative DrawingDimension has no nominal value"))?;
        if !resolved_values_equal(
            dimension_nominal,
            association
                .nominal
                .ok_or_else(|| invalid("resolved association has no nominal value"))?,
            ASSOCIATIVE_TOLERANCE_MM,
        ) {
            return Err(invalid(
                "association nominal value disagrees with DrawingDimension",
            ));
        }
        for source in &association.sources {
            match source {
                DimensionValueSource::EngineeringFeatureParameter { feature, parameter } => {
                    let expected = expected_parameter_value(engineering, feature, *parameter)
                        .ok_or_else(|| invalid("feature parameter does not resolve"))?;
                    let actual = value_for_source(&dimension.value, source).ok_or_else(|| {
                        invalid("DrawingDimension omits a referenced EngineeringModel parameter")
                    })?;
                    if !resolved_values_equal(expected, actual, ASSOCIATIVE_TOLERANCE_MM) {
                        return Err(invalid("DrawingDimension parameter value is stale or disagrees with EngineeringModel v2"));
                    }
                }
                DimensionValueSource::VerifiedBodyGeometry { axis, .. } => {
                    let expected = ResolvedDimensionValue::LengthMm {
                        mm: axis_span(evidence, *axis)?,
                    };
                    let actual = value_for_source(&dimension.value, source).ok_or_else(|| {
                        invalid("body envelope source is detached from its value")
                    })?;
                    if !resolved_values_equal(expected, actual, ASSOCIATIVE_TOLERANCE_MM) {
                        return Err(invalid(
                            "body envelope dimension disagrees with current B-Rep evidence",
                        ));
                    }
                }
                _ => {
                    return Err(invalid(
                        "parametric associative dimension uses an unsupported source type",
                    ))
                }
            }
        }
        validate_drawing_observation(association, view, layout, dimension, engineering, evidence)?;
    }
    Ok(())
}

fn expected_parameter_value(
    engineering: &EngineeringModel,
    feature: &FeatureId,
    parameter: ParametricDimensionParameter,
) -> Option<ResolvedDimensionValue> {
    let value = parametric_feature_parameter_value(engineering, feature, parameter)?;
    if parameter == ParametricDimensionParameter::PatternCount {
        if value < 1.0 || value.fract() != 0.0 || value > f64::from(u32::MAX) {
            return None;
        }
        Some(ResolvedDimensionValue::Count {
            count: value as u32,
        })
    } else {
        Some(ResolvedDimensionValue::LengthMm { mm: value })
    }
}

fn dimension_nominal(value: &DimensionValue) -> Option<ResolvedDimensionValue> {
    match value {
        DimensionValue::Measured { resolved, .. } => Some(*resolved),
        DimensionValue::ParametricHoleCallout { semantic } => Some(semantic.diameter.resolved),
        _ => None,
    }
}

fn value_for_source(
    value: &DimensionValue,
    source: &DimensionValueSource,
) -> Option<ResolvedDimensionValue> {
    match value {
        DimensionValue::Measured {
            source: actual,
            resolved,
        } if actual == source => Some(*resolved),
        DimensionValue::ParametricHoleCallout { semantic } => {
            let mut components = vec![&semantic.diameter];
            if let ParametricHoleExtent::Blind { depth } = &semantic.extent {
                components.push(depth);
            }
            if let Some(cb) = &semantic.counterbore {
                components.extend([&cb.diameter, &cb.depth]);
            }
            if let Some(pattern) = &semantic.pattern {
                match pattern {
                    ParametricHolePatternCallout::Linear { count, spacing } => {
                        components.extend([count, spacing])
                    }
                    ParametricHolePatternCallout::Circular { count, radius } => {
                        components.extend([count, radius])
                    }
                }
            }
            components
                .into_iter()
                .find(|component| &component.source == source)
                .map(|component| component.resolved)
        }
        _ => None,
    }
}

fn resolved_values_equal(
    first: ResolvedDimensionValue,
    second: ResolvedDimensionValue,
    tolerance: f64,
) -> bool {
    match (first, second) {
        (
            ResolvedDimensionValue::LengthMm { mm: a },
            ResolvedDimensionValue::LengthMm { mm: b },
        ) => (a - b).abs() <= tolerance,
        (
            ResolvedDimensionValue::Count { count: a },
            ResolvedDimensionValue::Count { count: b },
        ) => a == b,
        _ => false,
    }
}

fn validate_geometry_witness(
    witness: &DimensionGeometryWitness,
    association: &AssociativeDimension,
    engineering: &EngineeringModel,
    evidence: &ParametricSpatialEvidence,
) -> Result<(), DrawingError> {
    if witness.model_sha256 != evidence.model_sha256
        || !witness.geometry_measurement_mm.is_finite()
        || witness.tolerance_mm <= 0.0
    {
        return Err(invalid(
            "associative geometry witness digest or measurement is invalid",
        ));
    }
    if witness.evidence_ids.is_empty()
        || witness.evidence_ids.iter().any(|id| {
            !evidence.faces.iter().any(|face| &face.id == id)
                && !evidence.edges.iter().any(|edge| &edge.id == id)
        })
    {
        return Err(invalid(
            "associative dimension references missing canonical face or edge evidence",
        ));
    }
    let faces = evidence
        .faces
        .iter()
        .filter(|face| witness.evidence_ids.contains(&face.id))
        .collect::<Vec<_>>();
    let edges = evidence
        .edges
        .iter()
        .filter(|edge| witness.evidence_ids.contains(&edge.id))
        .collect::<Vec<_>>();
    match &association.reference {
        AssociativeDimensionReference::BodyEnvelope { axis } => {
            let actual = axis_span(evidence, *axis)?;
            if (actual - witness.geometry_measurement_mm).abs() > witness.tolerance_mm { return Err(invalid("body envelope witness does not re-resolve")); }
        }
        AssociativeDimensionReference::FeatureParameter { feature, parameter: ParametricDimensionParameter::ExtrudeDistance } => {
            let attributed = faces.iter().filter(|face| face.source_features.iter().any(|source| source == &feature.0)).collect::<Vec<_>>();
            if attributed.is_empty() { return Err(invalid("extrude thickness evidence lost its feature provenance")); }
            let min = attributed.iter().map(|face| face.bounds_min[2]).fold(f64::INFINITY, f64::min);
            let max = attributed.iter().map(|face| face.bounds_max[2]).fold(f64::NEG_INFINITY, f64::max);
            if ((max-min) - witness.geometry_measurement_mm).abs() > witness.tolerance_mm { return Err(invalid("extrude thickness witness does not re-resolve from current face evidence")); }
        }
        AssociativeDimensionReference::FeatureParameter { feature, parameter: ParametricDimensionParameter::FilletRadius } => {
            if !faces.iter().any(|face| face.source_features.iter().any(|source| source == &feature.0) && matches!(face.surface, SpatialFaceSurface::Torus { minor_radius, .. } if (minor_radius-witness.geometry_measurement_mm).abs() <= witness.tolerance_mm)) {
                return Err(invalid("fillet radius witness does not match its source feature"));
            }
        }
        AssociativeDimensionReference::EdgeReference { edge } => {
            validate_boss_edge_reference(
                edge,
                association,
                engineering,
                evidence,
                &faces,
                &edges,
                witness,
            )?;
        }
        AssociativeDimensionReference::FeatureSemantic { feature, semantic } if semantic == "hole_callout" => {
            let diameter = association.nominal.and_then(|value| match value { ResolvedDimensionValue::LengthMm { mm } => Some(mm), _ => None }).ok_or_else(|| invalid("hole callout has no diameter nominal"))?;
            let main_faces = faces.iter().copied().filter(|face| matches!(face.surface, SpatialFaceSurface::Cylinder { radius, .. } if (2.0*radius-diameter).abs() <= witness.tolerance_mm)).collect::<Vec<_>>();
            if main_faces.is_empty() { return Err(invalid("hole diameter has no matching exact cylindrical witness")); }
            if let Some(seed) = hole_seed_and_pattern(engineering, feature) {
                if let Some(expected_axes) = expected_hole_locations(engineering, &seed.0, seed.1.as_ref()) {
                    if main_faces.iter().any(|face| match face.surface {
                        SpatialFaceSurface::Cylinder { axis_origin, axis, .. } => !expected_axes
                            .iter()
                            .any(|expected| same_axis_line(axis_origin, axis, *expected)),
                        _ => true,
                    }) {
                        return Err(invalid("cylindrical evidence is not bound to the referenced EngineeringModel hole location"));
                    }
                }
            }
            let measured = main_faces.iter().find_map(|face| match face.surface { SpatialFaceSurface::Cylinder { radius, .. } => Some(2.0*radius), _ => None }).unwrap();
            if (measured-witness.geometry_measurement_mm).abs() > witness.tolerance_mm { return Err(invalid("hole diameter geometry measurement does not re-resolve")); }
            validate_hole_component_witnesses(association, engineering, &main_faces, &faces, witness)?;
            if let Some(observed) = witness.drawing_measurement_mm {
                if (observed-measured).abs() > witness.tolerance_mm { return Err(invalid("projected circle diameter disagrees with cylindrical face measurement")); }
            }
            let _ = feature;
        }
        _ => {}
    }
    if let Some(observed) = witness.drawing_measurement_mm {
        if !observed.is_finite()
            || (observed - witness.geometry_measurement_mm).abs() > witness.tolerance_mm
        {
            return Err(invalid(
                "Drawing projection measurement disagrees with validated B-Rep evidence",
            ));
        }
    }
    Ok(())
}

fn validate_boss_edge_reference(
    reference: &EngineeringEdgeReference,
    association: &AssociativeDimension,
    engineering: &EngineeringModel,
    evidence: &ParametricSpatialEvidence,
    witness_faces: &[&SpatialFaceEvidence],
    witness_edges: &[&SpatialEdgeEvidence],
    witness: &DimensionGeometryWitness,
) -> Result<(), DrawingError> {
    reference
        .validate()
        .map_err(|_| invalid("semantic EdgeReference failed validation"))?;
    let SemanticEdgeSelector::ProfileBoundary { entity_id, .. } = &reference.selector else {
        return Err(invalid(
            "this associative EdgeReference is not a supported circular profile boundary",
        ));
    };
    let Some(EngineeringFeature::Parametric {
        feature:
            ParametricFeature::Extrude {
                profile,
                operation: ExtrudeOperation::Add,
                direction,
                ..
            },
        suppressed: false,
        ..
    }) = engineering.features.get(&reference.source_feature_id)
    else {
        return Err(invalid(
            "circular EdgeReference source is not an active Extrude ADD feature",
        ));
    };
    let Some((actual_entity, diameter)) =
        crate::dimension::circular_add_extrude_profile(engineering, profile, direction)
    else {
        return Err(invalid(
            "circular EdgeReference no longer resolves to an XY Sketch circle and Z-axis ADD",
        ));
    };
    if &actual_entity != entity_id {
        return Err(invalid(
            "circular EdgeReference sketch entity was removed or changed",
        ));
    }
    let expected = association
        .nominal
        .and_then(|value| match value {
            ResolvedDimensionValue::LengthMm { mm } => Some(mm),
            ResolvedDimensionValue::Count { .. } => None,
        })
        .ok_or_else(|| invalid("boss diameter EdgeReference has no length nominal"))?;
    if (diameter - expected).abs() > witness.tolerance_mm
        || (witness.geometry_measurement_mm - expected).abs() > witness.tolerance_mm
        || reference.qualifier.curve_kind != Some(SemanticEdgeCurveKind::Circular)
        || reference
            .qualifier
            .radius_mm
            .is_none_or(|value| (value - expected / 2.0).abs() > witness.tolerance_mm)
    {
        return Err(invalid(
            "boss diameter EdgeReference no longer matches its typed sketch parameter",
        ));
    }

    let face_ids = evidence
        .faces
        .iter()
        .filter(|face| {
            face.source_features
                .iter()
                .any(|source| source == &reference.source_feature_id.0)
        })
        .filter(|face| {
            matches!(
                face.surface,
                SpatialFaceSurface::Cylinder { radius, axis, .. }
                    if (2.0 * radius - expected).abs() <= witness.tolerance_mm
                        && axis[0].abs() <= witness.tolerance_mm
                        && axis[1].abs() <= witness.tolerance_mm
                        && (axis[2].abs() - 1.0).abs() <= witness.tolerance_mm
            )
        })
        .map(|face| face.id.clone())
        .collect::<BTreeSet<_>>();
    let edge_records = evidence
        .edges
        .iter()
        .filter(|edge| {
            edge.source_features
                .iter()
                .any(|source| source == &reference.source_feature_id.0)
        })
        .filter(|edge| {
            matches!(
                edge.curve,
                SpatialKernelEdgeCurve::Circular { radius, axis, .. }
                    if (2.0 * radius - expected).abs() <= witness.tolerance_mm
                        && axis[0].abs() <= witness.tolerance_mm
                        && axis[1].abs() <= witness.tolerance_mm
                        && (axis[2].abs() - 1.0).abs() <= witness.tolerance_mm
            )
        })
        .collect::<Vec<_>>();
    if face_ids.is_empty() || edge_records.is_empty() {
        return Err(invalid(
            "boss diameter has no feature-attributed cylindrical face and circular edge evidence",
        ));
    }
    let actual_face_ids = witness_faces
        .iter()
        .map(|face| face.id.clone())
        .collect::<BTreeSet<_>>();
    if actual_face_ids != face_ids {
        return Err(invalid(
            "boss diameter witness is missing or includes unrelated cylindrical faces",
        ));
    }
    let expected_edge_ids = edge_records
        .iter()
        .map(|edge| edge.id.clone())
        .collect::<BTreeSet<_>>();
    let actual_edge_ids = witness_edges
        .iter()
        .map(|edge| edge.id.clone())
        .collect::<BTreeSet<_>>();
    if actual_edge_ids != expected_edge_ids {
        return Err(invalid(
            "boss diameter witness is missing or includes unrelated circular edges",
        ));
    }
    let centers = edge_records
        .iter()
        .filter_map(|edge| match edge.curve {
            SpatialKernelEdgeCurve::Circular { center, .. } => Some((
                (center[0] / witness.tolerance_mm).round() as i64,
                (center[1] / witness.tolerance_mm).round() as i64,
            )),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if centers.len() != 1 {
        return Err(invalid(
            "boss diameter semantic edge resolves to multiple cylinder axes",
        ));
    }
    let edge_ids = edge_records
        .iter()
        .map(|edge| edge.id.clone())
        .collect::<BTreeSet<_>>();
    let expected_ids = face_ids.union(&edge_ids).cloned().collect::<BTreeSet<_>>();
    let actual_ids = witness
        .evidence_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual_ids != expected_ids {
        return Err(invalid(
            "boss diameter witness contains missing or unrelated B-Rep evidence",
        ));
    }
    Ok(())
}

fn validate_hole_component_witnesses(
    association: &AssociativeDimension,
    engineering: &EngineeringModel,
    main_faces: &[&SpatialFaceEvidence],
    witness_faces: &[&SpatialFaceEvidence],
    witness: &DimensionGeometryWitness,
) -> Result<(), DrawingError> {
    let Some(dimension_id) = &association.drawing_dimension else {
        return Ok(());
    };
    let _ = dimension_id;
    let main_depth = main_faces
        .iter()
        .map(|face| face.bounds_max[2] - face.bounds_min[2])
        .fold(0.0_f64, f64::max);
    if let Some(source) = association.sources.iter().find(|source| {
        matches!(
            source,
            DimensionValueSource::EngineeringFeatureParameter {
                parameter: ParametricDimensionParameter::CounterboreDiameter,
                ..
            }
        )
    }) {
        let DimensionValueSource::EngineeringFeatureParameter { feature, .. } = source else {
            unreachable!()
        };
        let diameter = parametric_feature_parameter_value(
            engineering,
            feature,
            ParametricDimensionParameter::CounterboreDiameter,
        )
        .ok_or_else(|| invalid("counterbore diameter source is unresolved"))?;
        let faces = witness_faces.iter().filter(|face| matches!(face.surface, SpatialFaceSurface::Cylinder { radius, .. } if (2.0*radius-diameter).abs() <= witness.tolerance_mm)).collect::<Vec<_>>();
        if faces.is_empty() {
            return Err(invalid(
                "counterbore diameter has no exact cylindrical witness",
            ));
        }
        let measured_diameter = faces
            .iter()
            .find_map(|face| match face.surface {
                SpatialFaceSurface::Cylinder { radius, .. } => Some(2.0 * radius),
                _ => None,
            })
            .unwrap();
        if witness
            .component_measurements_mm
            .get(&ParametricDimensionParameter::CounterboreDiameter)
            .is_none_or(|cached| (cached - measured_diameter).abs() > witness.tolerance_mm)
        {
            return Err(invalid(
                "counterbore diameter measurement does not re-resolve",
            ));
        }
        let measured_depth = faces
            .iter()
            .map(|face| face.bounds_max[2] - face.bounds_min[2])
            .fold(0.0_f64, f64::max);
        if witness
            .component_measurements_mm
            .get(&ParametricDimensionParameter::CounterboreDepth)
            .is_none_or(|cached| (cached - measured_depth).abs() > witness.tolerance_mm)
        {
            return Err(invalid("counterbore depth measurement does not re-resolve"));
        }
    }
    if let Some(measured) = witness
        .component_measurements_mm
        .get(&ParametricDimensionParameter::HoleDepth)
    {
        let counterbore_depth = witness
            .component_measurements_mm
            .get(&ParametricDimensionParameter::CounterboreDepth)
            .copied()
            .unwrap_or(0.0);
        if (main_depth + counterbore_depth - measured).abs() > witness.tolerance_mm {
            return Err(invalid(
                "blind-hole total depth witness does not re-resolve",
            ));
        }
    }
    if let AssociativeDimensionReference::FeatureSemantic { feature, .. } = &association.reference {
        if let Some(EngineeringFeature::Parametric {
            feature: ParametricFeature::Pattern { definition, .. },
            suppressed: false,
            ..
        }) = engineering.features.get(feature)
        {
            let main_diameter = main_faces
                .iter()
                .find_map(|face| match face.surface {
                    SpatialFaceSurface::Cylinder { radius, .. } => Some(2.0 * radius),
                    _ => None,
                })
                .unwrap_or(0.0);
            let selected = witness_faces.iter().filter(|face| matches!(face.surface, SpatialFaceSurface::Cylinder { radius, .. } if (2.0*radius-main_diameter).abs() <= witness.tolerance_mm)).copied().collect::<Vec<_>>();
            let measured =
                measured_pattern_components(definition, &selected)?.ok_or_else(|| {
                    invalid("pattern geometry is not observable in the supported reference plane")
                })?;
            for (parameter, value) in measured {
                if witness
                    .component_measurements_mm
                    .get(&parameter)
                    .is_none_or(|cached| (cached - value).abs() > witness.tolerance_mm)
                {
                    return Err(invalid(
                        "pattern count or placement measurement does not re-resolve",
                    ));
                }
            }
        }
    }
    // Geometry IDs in the single witness are exact; each counterbore cylinder
    // is independently checked by the component-measurement cache below.
    for (parameter, cached) in &witness.component_measurements_mm {
        if let Some(source) = association.sources.iter().find(|source| matches!(source, DimensionValueSource::EngineeringFeatureParameter { parameter: actual, .. } if actual == parameter)) {
            let DimensionValueSource::EngineeringFeatureParameter { feature, .. } = source else { continue; };
            let expected = expected_parameter_value(engineering, feature, *parameter).ok_or_else(|| invalid("component source is unresolved"))?;
            let expected_mm = match expected { ResolvedDimensionValue::LengthMm { mm } => mm, ResolvedDimensionValue::Count { count } => f64::from(count) };
            if (expected_mm-cached).abs() > witness.tolerance_mm { return Err(invalid("B-Rep component measurement disagrees with declared EngineeringModel value")); }
        }
    }
    Ok(())
}

fn validate_drawing_observation(
    association: &AssociativeDimension,
    view: &DrawingView,
    layout: &DrawingDimensionLayout,
    dimension: &DrawingDimension,
    engineering: &EngineeringModel,
    evidence: &ParametricSpatialEvidence,
) -> Result<(), DrawingError> {
    let witness = association
        .witness
        .as_ref()
        .ok_or_else(|| invalid("resolved dimension has no geometry witness"))?;
    match &association.reference {
        AssociativeDimensionReference::BodyEnvelope { axis } => {
            let observed = match (axis, view.id.0.as_str()) {
                (DimensionAxis3D::X, "front") => view.model_bounds.max_x - view.model_bounds.min_x,
                (DimensionAxis3D::Y, "left") => view.model_bounds.max_y - view.model_bounds.min_y,
                (DimensionAxis3D::Z, "top") => view.model_bounds.max_y - view.model_bounds.min_y,
                _ => {
                    return Err(invalid(
                        "body envelope dimension is attached to an inapplicable view",
                    ))
                }
            };
            if witness
                .drawing_measurement_mm
                .is_none_or(|cached| (cached - observed).abs() > witness.tolerance_mm)
            {
                return Err(invalid("body envelope Drawing observation is stale"));
            }
        }
        AssociativeDimensionReference::FeatureSemantic { semantic, .. }
            if semantic == "hole_callout" =>
        {
            let DimensionLayoutGeometry::PatternCallout {
                anchor_on_pattern,
                footprint,
                ..
            } = &layout.geometry
            else {
                return Err(invalid("hole callout has a non-callout layout"));
            };
            let DimensionValue::ParametricHoleCallout { semantic: callout } = &dimension.value
            else {
                return Err(invalid("hole callout lost typed semantic data"));
            };
            let main_diameter = match callout.diameter.resolved {
                ResolvedDimensionValue::LengthMm { mm } => mm,
                _ => return Err(invalid("hole callout diameter is not a length")),
            };
            let main_faces = evidence
                .faces
                .iter()
                .filter(|face| {
                    witness.evidence_ids.contains(&face.id)
                        && matches!(face.surface, SpatialFaceSurface::Cylinder { radius, .. } if (2.0 * radius - main_diameter).abs() <= witness.tolerance_mm)
                })
                .collect::<Vec<_>>();
            let projected_circles = projected_hole_circles(view, &main_faces, main_diameter / 2.0)
                .ok_or_else(|| {
                    invalid("resolved hole callout is not represented by its exact Drawing circles")
                })?;
            let observed_footprint = projected_circles_footprint(&projected_circles)?;
            let paper_tolerance = witness.tolerance_mm * view.transform.scale.0;
            if (footprint.min_x - observed_footprint.min_x).abs() > paper_tolerance
                || (footprint.min_y - observed_footprint.min_y).abs() > paper_tolerance
                || (footprint.max_x - observed_footprint.max_x).abs() > paper_tolerance
                || (footprint.max_y - observed_footprint.max_y).abs() > paper_tolerance
            {
                return Err(invalid(
                    "hole callout layout does not cover the full projected pattern footprint",
                ));
            }
            if !projected_circles.iter().any(|circle| {
                let dx = anchor_on_pattern.x - circle.center.x;
                let dy = anchor_on_pattern.y - circle.center.y;
                let distance = dx.hypot(dy);
                let angle = dy.atan2(dx);
                (distance - circle.sheet_radius).abs() <= paper_tolerance
                    && circle
                        .arc_angles
                        .is_none_or(|(start, end)| angle_is_on_arc(angle, start, end))
            }) {
                return Err(invalid(
                    "hole callout leader does not start on a projected feature boundary",
                ));
            }
        }
        AssociativeDimensionReference::FeatureParameter {
            feature,
            parameter: ParametricDimensionParameter::ExtrudeDistance,
        } => {
            if view.id.0 != "top" {
                return Err(invalid(
                    "base thickness is not attached to its observed Top view",
                ));
            }
            let drawing_observation = witness
                .evidence_ids
                .iter()
                .filter_map(|id| evidence.faces.iter().find(|face| &face.id == id))
                .filter(|face| {
                    face.source_features
                        .iter()
                        .any(|source| source == &feature.0)
                })
                .filter_map(|face| base_thickness_projection_line(view, face))
                .map(|(start, end)| (end.y - start.y).abs())
                .find(|observed| {
                    (*observed - witness.geometry_measurement_mm).abs() <= witness.tolerance_mm
                })
                .ok_or_else(|| {
                    invalid("base thickness has no matching visible Top-view side edge")
                })?;
            let layout_measurement = layout_linear_measurement(layout, view)?;
            if witness.drawing_measurement_mm.is_none_or(|cached| {
                (cached - drawing_observation).abs() > witness.tolerance_mm
                    || (cached - layout_measurement).abs() > witness.tolerance_mm
            }) {
                return Err(invalid(
                    "base thickness layout or Top-view geometry is stale",
                ));
            }
        }
        AssociativeDimensionReference::EdgeReference { edge } => {
            validate_boss_diameter_drawing_observation(
                edge,
                association,
                view,
                layout,
                dimension,
                evidence,
                witness,
            )?;
        }
        _ => {}
    }
    let _ = engineering;
    Ok(())
}

fn validate_boss_diameter_drawing_observation(
    reference: &EngineeringEdgeReference,
    association: &AssociativeDimension,
    view: &DrawingView,
    layout: &DrawingDimensionLayout,
    dimension: &DrawingDimension,
    evidence: &ParametricSpatialEvidence,
    witness: &DimensionGeometryWitness,
) -> Result<(), DrawingError> {
    if view.id.0 != "front" || dimension.kind != DrawingDimensionKind::Diameter {
        return Err(invalid(
            "boss diameter must be attached to its Front circular projection",
        ));
    }
    let mut circles = BTreeSet::new();
    for edge in &evidence.edges {
        if !witness.evidence_ids.contains(&edge.id)
            || !edge
                .source_features
                .iter()
                .any(|source| source == &reference.source_feature_id.0)
        {
            continue;
        }
        if let SpatialKernelEdgeCurve::Circular {
            center,
            axis,
            radius,
            ..
        } = edge.curve
        {
            if axis[0].abs() <= witness.tolerance_mm
                && axis[1].abs() <= witness.tolerance_mm
                && (axis[2].abs() - 1.0).abs() <= witness.tolerance_mm
            {
                circles.insert((
                    (center[0] / witness.tolerance_mm).round() as i64,
                    (center[1] / witness.tolerance_mm).round() as i64,
                    (radius / witness.tolerance_mm).round() as i64,
                ));
            }
        }
    }
    if circles.len() != 1 {
        return Err(invalid(
            "boss diameter projection does not resolve to one circular B-Rep edge",
        ));
    }
    let (cx_key, cy_key, radius_key) = *circles.iter().next().unwrap();
    let center = [
        cx_key as f64 * witness.tolerance_mm,
        cy_key as f64 * witness.tolerance_mm,
    ];
    let radius = radius_key as f64 * witness.tolerance_mm;
    let projected = view
        .primitives
        .values()
        .filter_map(|primitive| match primitive {
            DrawingPrimitive::Circle {
                center: p,
                radius: r,
                role: DrawingPrimitiveRole::Object,
            } if (p.x - center[0]).abs() <= witness.tolerance_mm
                && (p.y - center[1]).abs() <= witness.tolerance_mm
                && (r - radius).abs() <= witness.tolerance_mm =>
            {
                Some((p, r))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if projected.len() != 1 {
        return Err(invalid(
            "Front Drawing has missing or ambiguous circle geometry for the boss EdgeReference",
        ));
    }
    let DimensionLayoutGeometry::Diameter {
        center: layout_center,
        axis_line,
        ..
    } = &layout.geometry
    else {
        return Err(invalid("boss diameter has no formal Diameter layout"));
    };
    let expected_center = view
        .transform
        .model_to_sheet(DrawingPoint::new(center[0], center[1]));
    let paper_tolerance = witness.tolerance_mm * view.transform.scale.0;
    let line_diameter = (axis_line.end.x - axis_line.start.x)
        .hypot(axis_line.end.y - axis_line.start.y)
        / view.transform.scale.0;
    let observed_diameter = 2.0 * projected[0].1;
    let nominal = association
        .nominal
        .and_then(|value| match value {
            ResolvedDimensionValue::LengthMm { mm } => Some(mm),
            ResolvedDimensionValue::Count { .. } => None,
        })
        .ok_or_else(|| invalid("boss diameter Drawing association has no length nominal"))?;
    if (layout_center.x - expected_center.x).abs() > paper_tolerance
        || (layout_center.y - expected_center.y).abs() > paper_tolerance
        || (line_diameter - nominal).abs() > witness.tolerance_mm
        || (observed_diameter - nominal).abs() > witness.tolerance_mm
        || witness
            .drawing_measurement_mm
            .is_none_or(|value| (value - observed_diameter).abs() > witness.tolerance_mm)
    {
        return Err(invalid(
            "boss diameter layout, Front Drawing circle, and engineering nominal disagree",
        ));
    }
    Ok(())
}

fn layout_linear_measurement(
    layout: &DrawingDimensionLayout,
    view: &DrawingView,
) -> Result<f64, DrawingError> {
    let DimensionLayoutGeometry::Linear { dimension_line, .. } = layout.geometry else {
        return Err(invalid("feature linear dimension has a non-linear layout"));
    };
    let scale = view.transform.scale.0;
    if !scale.is_finite() || scale <= 0.0 {
        return Err(invalid("drawing scale for dimension layout is invalid"));
    }
    Ok((dimension_line.end.x - dimension_line.start.x)
        .abs()
        .max((dimension_line.end.y - dimension_line.start.y).abs())
        / scale)
}

#[allow(clippy::too_many_arguments)]
fn add_body_dimension(
    output: &mut BTreeMap<DrawingDimensionId, AssociativeDimension>,
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    evidence: &ParametricSpatialEvidence,
    axis: DimensionAxis3D,
    view_id: DrawingViewId,
    linear_axis: LinearAxis,
    id: DrawingDimensionId,
    min: f64,
    max: f64,
) -> Result<(), DrawingError> {
    let nominal = max - min;
    let view = views
        .get_mut(&view_id)
        .ok_or_else(|| invalid("required principal view is missing"))?;
    let (drawing_min, drawing_max, start, end) = match axis {
        DimensionAxis3D::X => (
            view.model_bounds.min_x,
            view.model_bounds.max_x,
            DrawingPoint::new(view.model_bounds.min_x, view.model_bounds.max_y),
            DrawingPoint::new(view.model_bounds.max_x, view.model_bounds.max_y),
        ),
        DimensionAxis3D::Y => (
            view.model_bounds.min_y,
            view.model_bounds.max_y,
            DrawingPoint::new(view.model_bounds.max_x, view.model_bounds.min_y),
            DrawingPoint::new(view.model_bounds.max_x, view.model_bounds.max_y),
        ),
        DimensionAxis3D::Z => (
            view.model_bounds.min_y,
            view.model_bounds.max_y,
            DrawingPoint::new(view.model_bounds.min_x, view.model_bounds.min_y),
            DrawingPoint::new(view.model_bounds.min_x, view.model_bounds.max_y),
        ),
    };
    let drawing_measurement = drawing_max - drawing_min;
    if (drawing_measurement - nominal).abs() > ASSOCIATIVE_TOLERANCE_MM {
        return Err(invalid(&format!(
            "{} projection span {} on view {} bounds [{}, {}, {}, {}] disagrees with spatial span {}",
            id,
            drawing_measurement,
            view_id.0,
            view.model_bounds.min_x,
            view.model_bounds.min_y,
            view.model_bounds.max_x,
            view.model_bounds.max_y,
            nominal
        )));
    }
    let source = DimensionValueSource::VerifiedBodyGeometry {
        model_sha256: evidence.model_sha256.clone(),
        axis,
    };
    let dimension = DrawingDimension {
        id: id.clone(),
        target: DrawingDimensionTarget::Body,
        kind: DrawingDimensionKind::Linear { axis: linear_axis },
        specification: None,
        value: DimensionValue::Measured {
            source: source.clone(),
            resolved: ResolvedDimensionValue::LengthMm { mm: nominal },
        },
    };
    let layout = linear_layout(
        &view.transform,
        start,
        end,
        linear_axis,
        view.model_bounds,
        true,
        id.clone(),
    );
    view.dimensions.insert(id.clone(), dimension);
    view.dimension_layouts.insert(id.clone(), layout);
    let evidence_ids = extreme_face_ids(evidence, axis, min, max);
    if evidence_ids.is_empty() {
        return Err(invalid("overall dimension has no exact face witnesses"));
    }
    let record = AssociativeDimension {
        annotation_id: id.0.clone(),
        drawing_dimension: Some(id.clone()),
        kind: DrawingDimensionKind::Linear { axis: linear_axis },
        reference: AssociativeDimensionReference::BodyEnvelope { axis },
        view: view_id,
        nominal: Some(ResolvedDimensionValue::LengthMm { mm: nominal }),
        unit: AssociativeDimensionUnit::Millimetres,
        sources: vec![source],
        provenance: DimensionValueProvenance::GeometryDerived,
        witness: Some(DimensionGeometryWitness {
            model_sha256: evidence.model_sha256.clone(),
            evidence_ids,
            geometry_measurement_mm: nominal,
            drawing_measurement_mm: Some(drawing_measurement),
            component_measurements_mm: BTreeMap::new(),
            tolerance_mm: ASSOCIATIVE_TOLERANCE_MM,
        }),
        observability: DimensionObservability::Observable,
        status: AssociativeDimensionStatus::Resolved,
        diagnostic: None,
    };
    output.insert(id, record);
    Ok(())
}

fn add_base_extrude_thickness(
    output: &mut BTreeMap<DrawingDimensionId, AssociativeDimension>,
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    engineering: &EngineeringModel,
    evidence: &ParametricSpatialEvidence,
) -> Result<bool, DrawingError> {
    let candidate = engineering
        .ordered_feature_ids()
        .into_iter()
        .find_map(|id| match engineering.features.get(&id) {
            Some(EngineeringFeature::Parametric {
                feature:
                    ParametricFeature::Extrude {
                        operation: ExtrudeOperation::Add,
                        extent,
                        ..
                    },
                suppressed: false,
                ..
            }) => {
                let length = match extent {
                    ExtrudeExtent::Distance { length } | ExtrudeExtent::Symmetric { length } => {
                        *length
                    }
                    ExtrudeExtent::ThroughAll => return None,
                };
                Some((id, length))
            }
            _ => None,
        });
    let Some((feature, declared)) = candidate else {
        return Ok(false);
    };
    let view_id = DrawingViewId("top".into());
    let view = views
        .get_mut(&view_id)
        .ok_or_else(|| invalid("Top view is missing for base thickness"))?;
    let observed = evidence.faces.iter().find_map(|face| {
        let belongs_to_base = face
            .source_features
            .iter()
            .any(|source| source == &feature.0);
        let side_face = matches!(
            face.surface,
            SpatialFaceSurface::Plane { normal, .. }
                if normal[1] > 1.0 - ASSOCIATIVE_TOLERANCE_MM
        );
        let face_span = face.bounds_max[2] - face.bounds_min[2];
        if !belongs_to_base || !side_face || (face_span - declared).abs() > ASSOCIATIVE_TOLERANCE_MM
        {
            return None;
        }
        base_thickness_projection_line(view, face).map(|(start, end)| (face, start, end, face_span))
    });
    let Some((face, start, end, measured)) = observed else {
        return Ok(false);
    };
    let drawing_measurement = (end.y - start.y).abs();
    if (drawing_measurement - measured).abs() > ASSOCIATIVE_TOLERANCE_MM
        || (measured - declared).abs() > ASSOCIATIVE_TOLERANCE_MM
    {
        return Ok(false);
    }
    let id = DrawingDimensionId("associative.base.thickness".into());
    let source = DimensionValueSource::EngineeringFeatureParameter {
        feature: feature.clone(),
        parameter: ParametricDimensionParameter::ExtrudeDistance,
    };
    let dimension = DrawingDimension {
        id: id.clone(),
        target: DrawingDimensionTarget::Feature {
            feature: feature.clone(),
        },
        kind: DrawingDimensionKind::Linear {
            axis: LinearAxis::Vertical,
        },
        specification: None,
        value: DimensionValue::Measured {
            source: source.clone(),
            resolved: ResolvedDimensionValue::LengthMm { mm: declared },
        },
    };
    let layout = linear_layout(
        &view.transform,
        start,
        end,
        LinearAxis::Vertical,
        view.model_bounds,
        true,
        id.clone(),
    );
    view.dimensions.insert(id.clone(), dimension);
    view.dimension_layouts.insert(id.clone(), layout);
    let record = AssociativeDimension {
        annotation_id: id.0.clone(),
        drawing_dimension: Some(id.clone()),
        kind: DrawingDimensionKind::Linear {
            axis: LinearAxis::Vertical,
        },
        reference: AssociativeDimensionReference::FeatureParameter {
            feature: feature.clone(),
            parameter: ParametricDimensionParameter::ExtrudeDistance,
        },
        view: view_id,
        nominal: Some(ResolvedDimensionValue::LengthMm { mm: declared }),
        unit: AssociativeDimensionUnit::Millimetres,
        sources: vec![source],
        provenance: DimensionValueProvenance::EngineeringDeclared,
        witness: Some(DimensionGeometryWitness {
            model_sha256: evidence.model_sha256.clone(),
            evidence_ids: vec![face.id.clone()],
            geometry_measurement_mm: measured,
            drawing_measurement_mm: Some(drawing_measurement),
            component_measurements_mm: BTreeMap::new(),
            tolerance_mm: ASSOCIATIVE_TOLERANCE_MM,
        }),
        observability: DimensionObservability::Observable,
        status: AssociativeDimensionStatus::Resolved,
        diagnostic: None,
    };
    output.insert(id, record);
    Ok(true)
}

/// Match the EngineeringModel base extrusion side-face boundary to an actual
/// visible line in the Top DrawingView (u=X, v=-Z).
fn base_thickness_projection_line(
    view: &DrawingView,
    face: &SpatialFaceEvidence,
) -> Option<(DrawingPoint, DrawingPoint)> {
    let start = DrawingPoint::new(face.bounds_max[0], -face.bounds_max[2]);
    let end = DrawingPoint::new(face.bounds_max[0], -face.bounds_min[2]);
    view.primitives
        .values()
        .find_map(|primitive| match primitive {
            DrawingPrimitive::Line {
                role: DrawingPrimitiveRole::Object,
                start: actual_start,
                end: actual_end,
            } if segment_matches(*actual_start, *actual_end, start, end) => Some((start, end)),
            _ => None,
        })
}

fn segment_matches(
    actual_start: DrawingPoint,
    actual_end: DrawingPoint,
    expected_start: DrawingPoint,
    expected_end: DrawingPoint,
) -> bool {
    let close = |a: DrawingPoint, b: DrawingPoint| {
        (a.x - b.x).abs() <= ASSOCIATIVE_TOLERANCE_MM
            && (a.y - b.y).abs() <= ASSOCIATIVE_TOLERANCE_MM
    };
    (close(actual_start, expected_start) && close(actual_end, expected_end))
        || (close(actual_start, expected_end) && close(actual_end, expected_start))
}

#[allow(clippy::too_many_arguments)]
fn add_hole_callout(
    output: &mut BTreeMap<DrawingDimensionId, AssociativeDimension>,
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    engineering: &EngineeringModel,
    spatial: &SpatialModel,
    evidence: &ParametricSpatialEvidence,
    owner: &FeatureId,
    seed: &FeatureId,
    diameter: f64,
    termination: HoleTermination,
    kind: HoleKind,
    pattern: Option<&PatternDefinition>,
) -> Result<(), DrawingError> {
    let diameter = parametric_feature_parameter_value(
        engineering,
        seed,
        ParametricDimensionParameter::HoleDiameter,
    )
    .unwrap_or(diameter);
    if matches!(pattern, Some(PatternDefinition::ExplicitPlacement { .. })) {
        add_unsupported_hole_callout(output, evidence, owner, seed, diameter, pattern, &[]);
        return Ok(());
    }
    let termination = match termination {
        HoleTermination::ThroughAll => HoleTermination::ThroughAll,
        HoleTermination::Blind { depth } => HoleTermination::Blind {
            depth: parametric_feature_parameter_value(
                engineering,
                seed,
                ParametricDimensionParameter::HoleDepth,
            )
            .unwrap_or(depth),
        },
    };
    let kind = match kind {
        HoleKind::Counterbore { diameter, depth } => HoleKind::Counterbore {
            diameter: parametric_feature_parameter_value(
                engineering,
                seed,
                ParametricDimensionParameter::CounterboreDiameter,
            )
            .unwrap_or(diameter),
            depth: parametric_feature_parameter_value(
                engineering,
                seed,
                ParametricDimensionParameter::CounterboreDepth,
            )
            .unwrap_or(depth),
        },
        kind => kind,
    };
    let main_faces =
        feature_hole_faces(engineering, evidence, owner, seed, diameter / 2.0, pattern);
    if main_faces.is_empty() {
        add_unsupported_hole_callout(output, evidence, owner, seed, diameter, pattern, &[]);
        return Ok(());
    }
    if main_faces.iter().any(|face| !axis_is_z(face)) {
        add_unsupported_hole_callout(
            output,
            evidence,
            owner,
            seed,
            diameter,
            pattern,
            &main_faces,
        );
        return Ok(());
    }
    let depth_mm = main_faces
        .iter()
        .map(|face| face.bounds_max[2] - face.bounds_min[2])
        .fold(0.0_f64, f64::max);
    let mut hole_component_measurements = BTreeMap::new();
    let mut counterbore_depth_mm = 0.0;
    let counterbore = match kind {
        HoleKind::Simple => None,
        HoleKind::Counterbore {
            diameter: cb_diameter,
            depth,
        } => {
            let faces = feature_hole_faces(
                engineering,
                evidence,
                owner,
                seed,
                cb_diameter / 2.0,
                pattern,
            );
            if faces.is_empty() || faces.iter().any(|face| !axis_is_z(face)) {
                return Err(invalid(&format!(
                    "counterbore {} lacks a matching cylindrical witness",
                    owner
                )));
            }
            let measured_depth = faces
                .iter()
                .map(|face| face.bounds_max[2] - face.bounds_min[2])
                .fold(0.0_f64, f64::max);
            if (measured_depth - depth).abs() > ASSOCIATIVE_TOLERANCE_MM {
                return Err(invalid(&format!(
                    "counterbore {} depth disagrees with B-Rep evidence",
                    owner
                )));
            }
            let measured_diameter = faces
                .iter()
                .find_map(|face| match face.surface {
                    SpatialFaceSurface::Cylinder { radius, .. } => Some(2.0 * radius),
                    _ => None,
                })
                .ok_or_else(|| invalid("counterbore face is not cylindrical"))?;
            counterbore_depth_mm = measured_depth;
            hole_component_measurements.insert(
                ParametricDimensionParameter::CounterboreDiameter,
                measured_diameter,
            );
            hole_component_measurements.insert(
                ParametricDimensionParameter::CounterboreDepth,
                measured_depth,
            );
            Some(ParametricCounterboreCallout {
                diameter: length_component(
                    seed,
                    ParametricDimensionParameter::CounterboreDiameter,
                    cb_diameter,
                ),
                depth: length_component(
                    seed,
                    ParametricDimensionParameter::CounterboreDepth,
                    depth,
                ),
            })
        }
        HoleKind::Countersink { .. } => {
            add_unsupported_hole_callout(
                output,
                evidence,
                owner,
                seed,
                diameter,
                pattern,
                &main_faces,
            );
            let id = DrawingDimensionId(format!("associative.hole.{}.callout", owner.0));
            if let Some(callout) = output.get_mut(&id) {
                callout.diagnostic = Some(
                    "COUNTERSINK_ANNOTATION_UNSUPPORTED: exact conical geometry is present in Spatial and orthographic/section projections, but this Drawing contract cannot yet express a standard countersink diameter-and-included-angle callout; use the EngineeringModel Hole parameters and a section view for manufacturing evidence".into(),
                );
            }
            return Ok(());
        }
    };
    let extent = match termination {
        // A chamfer or counterbore can split a through wall into multiple
        // analytic faces. The feature's validated ThroughAll operation is the
        // semantic source; the exact bore diameter remains independently
        // measured from the final cylinder faces.
        HoleTermination::ThroughAll => ParametricHoleExtent::Through,
        HoleTermination::Blind { depth } => {
            let measured_depth = depth_mm + counterbore_depth_mm;
            if (measured_depth - depth).abs() > ASSOCIATIVE_TOLERANCE_MM {
                return Err(invalid(&format!(
                    "blind-hole {} depth disagrees with its final cylindrical B-Rep evidence",
                    owner
                )));
            }
            hole_component_measurements
                .insert(ParametricDimensionParameter::HoleDepth, measured_depth);
            ParametricHoleExtent::Blind {
                depth: length_component(seed, ParametricDimensionParameter::HoleDepth, depth),
            }
        }
    };
    let pattern_measurements = if let Some(definition) = pattern {
        let Some(measurements) = measured_pattern_components(definition, &main_faces)? else {
            add_unsupported_hole_callout(
                output,
                evidence,
                owner,
                seed,
                diameter,
                pattern,
                &main_faces,
            );
            return Ok(());
        };
        Some(measurements)
    } else {
        None
    };
    let pattern_callout = pattern.and_then(|definition| match definition {
        PatternDefinition::Linear { count, spacing, .. } => {
            Some(ParametricHolePatternCallout::Linear {
                count: count_component(owner, *count),
                spacing: length_component(
                    owner,
                    ParametricDimensionParameter::PatternSpacing,
                    *spacing,
                ),
            })
        }
        PatternDefinition::Circular { count, radius, .. } => {
            Some(ParametricHolePatternCallout::Circular {
                count: count_component(owner, *count),
                radius: length_component(
                    owner,
                    ParametricDimensionParameter::PatternRadius,
                    *radius,
                ),
            })
        }
        PatternDefinition::ExplicitPlacement { .. } => None,
    });
    let main_source = DimensionValueSource::EngineeringFeatureParameter {
        feature: seed.clone(),
        parameter: ParametricDimensionParameter::HoleDiameter,
    };
    let semantic = ParametricHoleCalloutSemantic {
        feature: owner.clone(),
        diameter: length_component(seed, ParametricDimensionParameter::HoleDiameter, diameter),
        extent,
        counterbore,
        pattern: pattern_callout,
    };
    let view_id = DrawingViewId("front".into());
    let view = views
        .get_mut(&view_id)
        .ok_or_else(|| invalid("Front view is missing for the Z-axis hole callout"))?;
    let projected_circles = projected_hole_circles(view, &main_faces, diameter / 2.0);
    let Some(projected_circles) = projected_circles else {
        add_unsupported_hole_callout(
            output,
            evidence,
            owner,
            seed,
            diameter,
            pattern,
            &main_faces,
        );
        return Ok(());
    };
    if pattern.is_some_and(|definition| {
        let expected = match definition {
            PatternDefinition::Linear { count, .. } | PatternDefinition::Circular { count, .. } => {
                *count as usize
            }
            PatternDefinition::ExplicitPlacement { placements, .. } => placements.len() + 1,
        };
        projected_circles.len() != expected
    }) {
        add_unsupported_hole_callout(
            output,
            evidence,
            owner,
            seed,
            diameter,
            pattern,
            &main_faces,
        );
        return Ok(());
    }
    let footprint = projected_circles_footprint(&projected_circles)?;
    let attachment = projected_circles
        .iter()
        .max_by(|first, second| {
            (first.center.x + first.center.y).total_cmp(&(second.center.x + second.center.y))
        })
        .copied()
        .ok_or_else(|| invalid("hole callout has no projected circles"))?;
    let ProjectedHoleCircleWitness {
        center: center_sheet,
        model_radius,
        sheet_radius,
        arc_angles,
    } = attachment;
    let angle = arc_angles.map_or_else(
        || 8.0_f64.atan2(9.0),
        |(start, end)| {
            let span = (end - start).rem_euclid(std::f64::consts::TAU);
            start + span / 2.0
        },
    );
    let direction = DrawingPoint::new(angle.cos(), angle.sin());
    let anchor = DrawingPoint::new(
        center_sheet.x + sheet_radius * direction.x,
        center_sheet.y + sheet_radius * direction.y,
    );
    let leader_end = DrawingPoint::new(
        center_sheet.x + (sheet_radius + 12.0) * direction.x,
        center_sheet.y + (sheet_radius + 12.0) * direction.y,
    );
    let id = DrawingDimensionId(format!("associative.hole.{}.callout", owner.0));
    let dimension_kind = if pattern.is_some() {
        DrawingDimensionKind::Pattern
    } else {
        DrawingDimensionKind::Diameter
    };
    let dimension = DrawingDimension {
        id: id.clone(),
        target: DrawingDimensionTarget::Feature {
            feature: owner.clone(),
        },
        kind: dimension_kind.clone(),
        specification: None,
        value: DimensionValue::ParametricHoleCallout {
            semantic: semantic.clone(),
        },
    };
    let layout = DrawingDimensionLayout {
        dimension: id.clone(),
        geometry: DimensionLayoutGeometry::PatternCallout {
            anchor_on_pattern: anchor,
            leader: LayoutSegment {
                start: anchor,
                end: leader_end,
            },
            text_anchor: DrawingPoint::new(leader_end.x + 1.5, leader_end.y),
            footprint,
        },
    };
    view.dimensions.insert(id.clone(), dimension);
    view.dimension_layouts.insert(id.clone(), layout);
    let mut sources = vec![main_source];
    if let ParametricHoleExtent::Blind { depth } = &semantic.extent {
        sources.push(depth.source.clone());
    }
    if let Some(cb) = &semantic.counterbore {
        sources.extend([cb.diameter.source.clone(), cb.depth.source.clone()]);
    }
    if let Some(pattern) = &semantic.pattern {
        match pattern {
            ParametricHolePatternCallout::Linear { count, spacing } => {
                sources.extend([count.source.clone(), spacing.source.clone()])
            }
            ParametricHolePatternCallout::Circular { count, radius } => {
                sources.extend([count.source.clone(), radius.source.clone()])
            }
        }
    }
    let mut witness_ids = main_faces
        .iter()
        .map(|face| face.id.clone())
        .collect::<Vec<_>>();
    if let Some(cb) = &semantic.counterbore {
        if let ResolvedDimensionValue::LengthMm { mm } = cb.diameter.resolved {
            witness_ids.extend(
                feature_hole_faces(engineering, evidence, owner, seed, mm / 2.0, pattern)
                    .iter()
                    .map(|face| face.id.clone()),
            );
        }
    }
    let geometry_measurement = diameter;
    let mut component_measurements = hole_component_measurements;
    if let Some(pattern_measurements) = pattern_measurements {
        component_measurements.extend(pattern_measurements);
    }
    let record = AssociativeDimension {
        annotation_id: id.0.clone(),
        drawing_dimension: Some(id.clone()),
        kind: dimension_kind,
        reference: AssociativeDimensionReference::FeatureSemantic {
            feature: owner.clone(),
            semantic: "hole_callout".into(),
        },
        view: view_id,
        nominal: Some(ResolvedDimensionValue::LengthMm { mm: diameter }),
        unit: AssociativeDimensionUnit::Millimetres,
        sources,
        provenance: DimensionValueProvenance::EngineeringDeclared,
        witness: Some(DimensionGeometryWitness {
            model_sha256: evidence.model_sha256.clone(),
            evidence_ids: witness_ids,
            geometry_measurement_mm: geometry_measurement,
            drawing_measurement_mm: Some(2.0 * model_radius),
            component_measurements_mm: component_measurements,
            tolerance_mm: ASSOCIATIVE_TOLERANCE_MM,
        }),
        observability: DimensionObservability::AnnotatedFromValidatedFeature,
        status: AssociativeDimensionStatus::Resolved,
        diagnostic: None,
    };
    output.insert(id, record);
    let _ = spatial;
    Ok(())
}

fn measured_pattern_components(
    definition: &PatternDefinition,
    faces: &[&SpatialFaceEvidence],
) -> Result<Option<BTreeMap<ParametricDimensionParameter, f64>>, DrawingError> {
    let count = match definition {
        PatternDefinition::Linear { count, .. } | PatternDefinition::Circular { count, .. } => {
            *count
        }
        PatternDefinition::ExplicitPlacement { .. } => return Ok(None),
    };
    let mut unique = BTreeMap::new();
    for face in faces {
        if let Some(point) = cylinder_center(face) {
            let key = (
                (point[0] / ASSOCIATIVE_TOLERANCE_MM).round() as i64,
                (point[1] / ASSOCIATIVE_TOLERANCE_MM).round() as i64,
            );
            unique.entry(key).or_insert(point);
        }
    }
    if unique.len() != count as usize {
        return Err(invalid(
            "pattern instance count disagrees with unique cylindrical B-Rep witnesses",
        ));
    }
    let mut measured = BTreeMap::new();
    measured.insert(
        ParametricDimensionParameter::PatternCount,
        unique.len() as f64,
    );
    let points = unique.values().copied().collect::<Vec<_>>();
    match definition {
        PatternDefinition::Linear {
            direction, spacing, ..
        } => {
            if direction.z.abs() > ASSOCIATIVE_TOLERANCE_MM {
                return Ok(None);
            }
            let norm = direction.x.hypot(direction.y);
            if !norm.is_finite() || norm <= ASSOCIATIVE_TOLERANCE_MM {
                return Err(invalid("linear pattern direction is degenerate"));
            }
            let mut projections = points
                .iter()
                .map(|point| (point[0] * direction.x + point[1] * direction.y) / norm)
                .collect::<Vec<_>>();
            projections.sort_by(f64::total_cmp);
            let intervals = projections
                .windows(2)
                .map(|pair| pair[1] - pair[0])
                .collect::<Vec<_>>();
            if intervals.is_empty()
                || intervals
                    .iter()
                    .any(|actual| (*actual - *spacing).abs() > ASSOCIATIVE_TOLERANCE_MM)
            {
                return Err(invalid(
                    "linear pattern spacing disagrees with unique cylindrical B-Rep locations",
                ));
            }
            measured.insert(
                ParametricDimensionParameter::PatternSpacing,
                intervals.iter().sum::<f64>() / intervals.len() as f64,
            );
        }
        PatternDefinition::Circular {
            axis,
            center,
            radius,
            ..
        } => {
            if axis.x.abs() > ASSOCIATIVE_TOLERANCE_MM
                || axis.y.abs() > ASSOCIATIVE_TOLERANCE_MM
                || (axis.z.abs() - 1.0).abs() > ASSOCIATIVE_TOLERANCE_MM
            {
                return Ok(None);
            }
            let radii = points
                .iter()
                .map(|point| (point[0] - center.x).hypot(point[1] - center.y))
                .collect::<Vec<_>>();
            if radii
                .iter()
                .any(|actual| (*actual - *radius).abs() > ASSOCIATIVE_TOLERANCE_MM)
            {
                return Err(invalid(
                    "circular pattern radius disagrees with B-Rep locations",
                ));
            }
            measured.insert(
                ParametricDimensionParameter::PatternRadius,
                radii.iter().sum::<f64>() / radii.len() as f64,
            );
        }
        PatternDefinition::ExplicitPlacement { .. } => return Ok(None),
    }
    Ok(Some(measured))
}

fn add_unsupported_hole_callout(
    output: &mut BTreeMap<DrawingDimensionId, AssociativeDimension>,
    evidence: &ParametricSpatialEvidence,
    owner: &FeatureId,
    seed: &FeatureId,
    diameter: f64,
    pattern: Option<&PatternDefinition>,
    faces: &[&SpatialFaceEvidence],
) {
    let id = DrawingDimensionId(format!("associative.hole.{}.callout", owner.0));
    let source = DimensionValueSource::EngineeringFeatureParameter {
        feature: seed.clone(),
        parameter: ParametricDimensionParameter::HoleDiameter,
    };
    output.insert(
        id.clone(),
        AssociativeDimension {
            annotation_id: id.0.clone(),
            drawing_dimension: None,
            kind: if pattern.is_some() { DrawingDimensionKind::Pattern } else { DrawingDimensionKind::Diameter },
            reference: AssociativeDimensionReference::FeatureSemantic { feature: owner.clone(), semantic: "hole_callout".into() },
            view: DrawingViewId("front".into()),
            nominal: Some(ResolvedDimensionValue::LengthMm { mm: diameter }),
            unit: AssociativeDimensionUnit::Millimetres,
            sources: vec![source],
            provenance: DimensionValueProvenance::EngineeringDeclared,
            witness: if faces.is_empty() {
                None
            } else {
                Some(DimensionGeometryWitness {
                    model_sha256: evidence.model_sha256.clone(),
                    evidence_ids: faces.iter().map(|face| face.id.clone()).collect(),
                    geometry_measurement_mm: 2.0 * faces.iter().find_map(|face| match face.surface {
                        SpatialFaceSurface::Cylinder { radius, .. } => Some(radius), _ => None,
                    }).unwrap_or(diameter / 2.0),
                    drawing_measurement_mm: None,
                    component_measurements_mm: BTreeMap::new(),
                    tolerance_mm: ASSOCIATIVE_TOLERANCE_MM,
                })
            },
            observability: DimensionObservability::Unsupported,
            status: AssociativeDimensionStatus::Unsupported,
            diagnostic: Some("current Front/Top/Left projections do not provide a unique feature-linked circular witness; use a supported principal or section view".into()),
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn add_cylindrical_boss_diameter(
    output: &mut BTreeMap<DrawingDimensionId, AssociativeDimension>,
    views: &mut BTreeMap<DrawingViewId, DrawingView>,
    _engineering: &EngineeringModel,
    evidence: &ParametricSpatialEvidence,
    feature: &FeatureId,
    sketch_entity: &scc_engineering::parametric::SketchEntityId,
    diameter: f64,
) -> Result<(), DrawingError> {
    let id = DrawingDimensionId(format!("associative.boss.{}.diameter", feature.0));
    let source = DimensionValueSource::EngineeringFeatureParameter {
        feature: feature.clone(),
        parameter: ParametricDimensionParameter::BossDiameter,
    };
    let radius = diameter / 2.0;
    let edge_reference = EngineeringEdgeReference {
        source_feature_id: feature.clone(),
        selector: SemanticEdgeSelector::ProfileBoundary {
            entity_id: sketch_entity.clone(),
            side: ProfileEdgeSide::Top,
        },
        qualifier: SemanticEdgeQualifier {
            curve_kind: Some(SemanticEdgeCurveKind::Circular),
            radius_mm: Some(radius),
            ..SemanticEdgeQualifier::default()
        },
    };
    let reference = AssociativeDimensionReference::EdgeReference {
        edge: edge_reference.clone(),
    };

    let cylinders = evidence
        .faces
        .iter()
        .filter(|face| {
            face.source_features
                .iter()
                .any(|source| source == &feature.0)
        })
        .filter(|face| {
            matches!(
                face.surface,
                SpatialFaceSurface::Cylinder { radius: actual, axis, .. }
                    if (actual - radius).abs() <= ASSOCIATIVE_TOLERANCE_MM
                        && axis[0].abs() <= ASSOCIATIVE_TOLERANCE_MM
                        && axis[1].abs() <= ASSOCIATIVE_TOLERANCE_MM
                        && (axis[2].abs() - 1.0).abs() <= ASSOCIATIVE_TOLERANCE_MM
            )
        })
        .collect::<Vec<_>>();
    let Some(center) = cylinders.first().and_then(|face| match face.surface {
        SpatialFaceSurface::Cylinder { axis_origin, .. } => Some([axis_origin[0], axis_origin[1]]),
        _ => None,
    }) else {
        add_unsupported_boss_diameter(
            output,
            &id,
            &reference,
            &source,
            diameter,
            "圆形 ADD 的重建实体没有精确、特征归属的 Z 轴圆柱面证据",
        );
        return Ok(());
    };
    if cylinders.iter().any(|face| match face.surface {
        SpatialFaceSurface::Cylinder { axis_origin, .. } => {
            (axis_origin[0] - center[0]).abs() > ASSOCIATIVE_TOLERANCE_MM
                || (axis_origin[1] - center[1]).abs() > ASSOCIATIVE_TOLERANCE_MM
        }
        _ => true,
    }) {
        add_unsupported_boss_diameter(
            output,
            &id,
            &reference,
            &source,
            diameter,
            "同一 ADD 特征产生多个候选圆柱轴线，凸台直径引用存在歧义",
        );
        return Ok(());
    }

    let circular_edges = evidence
        .edges
        .iter()
        .filter(|edge| {
            edge.source_features
                .iter()
                .any(|source| source == &feature.0)
        })
        .filter(|edge| match edge.curve {
            SpatialKernelEdgeCurve::Circular {
                center: edge_center,
                axis,
                radius: actual,
                ..
            } => {
                (actual - radius).abs() <= ASSOCIATIVE_TOLERANCE_MM
                    && axis[0].abs() <= ASSOCIATIVE_TOLERANCE_MM
                    && axis[1].abs() <= ASSOCIATIVE_TOLERANCE_MM
                    && (axis[2].abs() - 1.0).abs() <= ASSOCIATIVE_TOLERANCE_MM
                    && (edge_center[0] - center[0]).abs() <= ASSOCIATIVE_TOLERANCE_MM
                    && (edge_center[1] - center[1]).abs() <= ASSOCIATIVE_TOLERANCE_MM
            }
            _ => false,
        })
        .collect::<Vec<_>>();
    if circular_edges.is_empty() {
        add_unsupported_boss_diameter(
            output,
            &id,
            &reference,
            &source,
            diameter,
            "未找到与语义草图边匹配的稳定圆弧 B-Rep 边证据",
        );
        return Ok(());
    }

    let view_id = DrawingViewId("front".into());
    let Some(view) = views.get_mut(&view_id) else {
        add_unsupported_boss_diameter(
            output,
            &id,
            &reference,
            &source,
            diameter,
            "当前 DrawingModel 缺少可表达凸台圆形投影的 Front 视图",
        );
        return Ok(());
    };
    let candidates = view
        .primitives
        .values()
        .filter_map(|primitive| match primitive {
            DrawingPrimitive::Circle {
                center: projected,
                radius: projected_radius,
                role: DrawingPrimitiveRole::Object,
            } if (projected.x - center[0]).abs() <= ASSOCIATIVE_TOLERANCE_MM
                && (projected.y - center[1]).abs() <= ASSOCIATIVE_TOLERANCE_MM
                && (projected_radius - radius).abs() <= ASSOCIATIVE_TOLERANCE_MM =>
            {
                Some((*projected, *projected_radius))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        add_unsupported_boss_diameter(
            output,
            &id,
            &reference,
            &source,
            diameter,
            if candidates.is_empty() {
                "特征圆柱没有对应的 Front Drawing 圆形投影"
            } else {
                "Front Drawing 存在多个同尺寸圆形候选，凸台直径引用存在歧义"
            },
        );
        return Ok(());
    }

    let (projected_center, projected_radius) = candidates[0];
    let drawing_measurement = 2.0 * projected_radius;
    if (drawing_measurement - diameter).abs() > ASSOCIATIVE_TOLERANCE_MM {
        add_unsupported_boss_diameter(
            output,
            &id,
            &reference,
            &source,
            diameter,
            "Front Drawing 圆形测量与 Sketch 声明直径不一致",
        );
        return Ok(());
    }

    let dimension = DrawingDimension {
        id: id.clone(),
        target: DrawingDimensionTarget::Feature {
            feature: feature.clone(),
        },
        kind: DrawingDimensionKind::Diameter,
        specification: None,
        value: DimensionValue::Measured {
            source: source.clone(),
            resolved: ResolvedDimensionValue::LengthMm { mm: diameter },
        },
    };
    let center_sheet = view.transform.model_to_sheet(projected_center);
    let layout = DrawingDimensionLayout {
        dimension: id.clone(),
        geometry: crate::three_view_annotations::diameter_layout(
            &view.transform,
            center_sheet,
            projected_radius,
            &id,
        ),
    };
    view.dimensions.insert(id.clone(), dimension.clone());
    view.dimension_layouts.insert(id.clone(), layout);

    let mut evidence_ids = cylinders
        .iter()
        .map(|face| face.id.clone())
        .collect::<Vec<_>>();
    evidence_ids.extend(circular_edges.iter().map(|edge| edge.id.clone()));
    evidence_ids.sort();
    evidence_ids.dedup();
    output.insert(
        id.clone(),
        AssociativeDimension {
            annotation_id: id.0.clone(),
            drawing_dimension: Some(id.clone()),
            kind: DrawingDimensionKind::Diameter,
            reference,
            view: view_id,
            nominal: Some(ResolvedDimensionValue::LengthMm { mm: diameter }),
            unit: AssociativeDimensionUnit::Millimetres,
            sources: vec![source],
            provenance: DimensionValueProvenance::EngineeringDeclared,
            witness: Some(DimensionGeometryWitness {
                model_sha256: evidence.model_sha256.clone(),
                evidence_ids,
                geometry_measurement_mm: diameter,
                drawing_measurement_mm: Some(drawing_measurement),
                component_measurements_mm: BTreeMap::new(),
                tolerance_mm: ASSOCIATIVE_TOLERANCE_MM,
            }),
            observability: DimensionObservability::Observable,
            status: AssociativeDimensionStatus::Resolved,
            diagnostic: None,
        },
    );
    Ok(())
}

fn add_unsupported_boss_diameter(
    output: &mut BTreeMap<DrawingDimensionId, AssociativeDimension>,
    id: &DrawingDimensionId,
    reference: &AssociativeDimensionReference,
    source: &DimensionValueSource,
    diameter: f64,
    diagnostic: &str,
) {
    output.insert(
        id.clone(),
        AssociativeDimension {
            annotation_id: id.0.clone(),
            drawing_dimension: None,
            kind: DrawingDimensionKind::Diameter,
            reference: reference.clone(),
            view: DrawingViewId("front".into()),
            nominal: Some(ResolvedDimensionValue::LengthMm { mm: diameter }),
            unit: AssociativeDimensionUnit::Millimetres,
            sources: vec![source.clone()],
            provenance: DimensionValueProvenance::EngineeringDeclared,
            witness: None,
            observability: DimensionObservability::Unsupported,
            status: AssociativeDimensionStatus::Unsupported,
            diagnostic: Some(diagnostic.into()),
        },
    );
}

fn add_fillets_as_unsupported(
    output: &mut BTreeMap<DrawingDimensionId, AssociativeDimension>,
    evidence: &ParametricSpatialEvidence,
    feature: &FeatureId,
    radius: f64,
) -> Result<(), DrawingError> {
    let witnesses = evidence
        .faces
        .iter()
        .filter(|face| face.source_features.iter().any(|source| source == &feature.0))
        .filter(|face| {
            matches!(face.surface, SpatialFaceSurface::Torus { minor_radius, .. } if (minor_radius - radius).abs() <= ASSOCIATIVE_TOLERANCE_MM)
        })
        .map(|face| face.id.clone())
        .collect::<Vec<_>>();
    let has_exact_geometry = !witnesses.is_empty();
    let id = DrawingDimensionId(format!("associative.fillet.{}.radius", feature.0));
    let source = DimensionValueSource::EngineeringFeatureParameter {
        feature: feature.clone(),
        parameter: ParametricDimensionParameter::FilletRadius,
    };
    output.insert(
        id.clone(),
        AssociativeDimension {
            annotation_id: id.0.clone(),
            drawing_dimension: None,
            kind: DrawingDimensionKind::Radius,
            reference: AssociativeDimensionReference::FeatureParameter {
                feature: feature.clone(),
                parameter: ParametricDimensionParameter::FilletRadius,
            },
            view: DrawingViewId("section_aa".into()),
            nominal: Some(ResolvedDimensionValue::LengthMm { mm: radius }),
            unit: AssociativeDimensionUnit::Millimetres,
            sources: vec![source],
            provenance: DimensionValueProvenance::EngineeringDeclared,
            witness: has_exact_geometry.then(|| DimensionGeometryWitness {
                model_sha256: evidence.model_sha256.clone(),
                evidence_ids: witnesses,
                geometry_measurement_mm: radius,
                drawing_measurement_mm: None,
                component_measurements_mm: BTreeMap::new(),
                tolerance_mm: ASSOCIATIVE_TOLERANCE_MM,
            }),
            observability: DimensionObservability::RequiresSectionView,
            status: AssociativeDimensionStatus::Unsupported,
            diagnostic: Some(if has_exact_geometry {
                "validated fillet radius has exact B-Rep support but no feature-linked section arc for a legal radius annotation".into()
            } else {
                "EngineeringModel declares this fillet radius, but the rebuilt B-Rep has no exact torus-radius witness; radius annotation is omitted".into()
            }),
        },
    );
    Ok(())
}

fn length_component(
    feature: &FeatureId,
    parameter: ParametricDimensionParameter,
    value: f64,
) -> DimensionComponent {
    DimensionComponent {
        source: DimensionValueSource::EngineeringFeatureParameter {
            feature: feature.clone(),
            parameter,
        },
        resolved: ResolvedDimensionValue::LengthMm { mm: value },
        specification: None,
    }
}
fn count_component(feature: &FeatureId, count: u32) -> DimensionComponent {
    DimensionComponent {
        source: DimensionValueSource::EngineeringFeatureParameter {
            feature: feature.clone(),
            parameter: ParametricDimensionParameter::PatternCount,
        },
        resolved: ResolvedDimensionValue::Count { count },
        specification: None,
    }
}

#[derive(Debug, Clone, Copy)]
struct FeatureHoleAxis {
    origin: [f64; 3],
    direction: [f64; 3],
}

#[derive(Debug, Clone, Copy)]
struct FeaturePlaneFrame {
    origin: [f64; 3],
    u_axis: [f64; 3],
    v_axis: [f64; 3],
    normal: [f64; 3],
}

fn feature_hole_faces<'a>(
    engineering: &EngineeringModel,
    evidence: &'a ParametricSpatialEvidence,
    owner: &FeatureId,
    seed: &FeatureId,
    radius: f64,
    pattern: Option<&PatternDefinition>,
) -> Vec<&'a SpatialFaceEvidence> {
    let Some(locations) = expected_hole_locations(engineering, seed, pattern) else {
        return cylinder_faces(evidence, owner, radius);
    };
    evidence
        .faces
        .iter()
        .filter(|face| match face.surface {
            SpatialFaceSurface::Cylinder {
                axis_origin,
                axis,
                radius: actual_radius,
            } if (actual_radius - radius).abs() <= ASSOCIATIVE_TOLERANCE_MM => locations
                .iter()
                .any(|expected| same_axis_line(axis_origin, axis, *expected)),
            _ => false,
        })
        .collect()
}

fn hole_seed_and_pattern(
    engineering: &EngineeringModel,
    owner: &FeatureId,
) -> Option<(FeatureId, Option<PatternDefinition>)> {
    match engineering.features.get(owner)? {
        EngineeringFeature::Parametric {
            feature:
                ParametricFeature::Pattern {
                    seed_feature_id,
                    definition,
                },
            suppressed: false,
            ..
        } => Some((seed_feature_id.clone(), Some(definition.clone()))),
        EngineeringFeature::Parametric {
            feature: ParametricFeature::Hole { .. },
            suppressed: false,
            ..
        } => Some((owner.clone(), None)),
        _ => None,
    }
}

fn expected_hole_locations(
    engineering: &EngineeringModel,
    seed: &FeatureId,
    pattern: Option<&PatternDefinition>,
) -> Option<Vec<FeatureHoleAxis>> {
    let Some(EngineeringFeature::Parametric {
        feature:
            ParametricFeature::Hole {
                support,
                position,
                reversed,
                ..
            },
        suppressed: false,
        ..
    }) = engineering.features.get(seed)
    else {
        return None;
    };
    let mut visited = BTreeSet::new();
    let frame = resolve_plane_reference(engineering, support, &mut visited, 0)?;
    let origin = std::array::from_fn(|index| {
        frame.origin[index] + frame.u_axis[index] * position.x + frame.v_axis[index] * position.y
    });
    let direction = if *reversed {
        frame.normal.map(|value| -value)
    } else {
        frame.normal
    };
    let seed_axis = FeatureHoleAxis { origin, direction };
    match pattern {
        None => Some(vec![seed_axis]),
        Some(PatternDefinition::Linear {
            direction,
            count,
            spacing,
        }) => {
            let vector = [direction.x, direction.y, direction.z];
            Some(
                (0..*count)
                    .map(|index| FeatureHoleAxis {
                        origin: std::array::from_fn(|axis| {
                            seed_axis.origin[axis] + vector[axis] * *spacing * f64::from(index)
                        }),
                        direction: seed_axis.direction,
                    })
                    .collect(),
            )
        }
        Some(PatternDefinition::Circular {
            axis,
            center,
            count,
            angular_span_degrees,
            ..
        }) => {
            let axis = [axis.x, axis.y, axis.z];
            let center = [center.x, center.y, center.z];
            let step = angular_span_degrees.to_radians() / f64::from(*count);
            Some(
                (0..*count)
                    .map(|index| {
                        let angle = step * f64::from(index);
                        Some(FeatureHoleAxis {
                            origin: rotate_about_axis(seed_axis.origin, center, axis, angle),
                            direction: normalize3(rotate_vector(seed_axis.direction, axis, angle))?,
                        })
                    })
                    .collect::<Option<Vec<_>>>(),
            )?
        }
        Some(PatternDefinition::ExplicitPlacement {
            reference_plane,
            placements,
        }) => {
            let frame =
                resolve_plane_reference(engineering, reference_plane, &mut BTreeSet::new(), 0)?;
            let offset =
                std::array::from_fn::<_, 3, _>(|axis| seed_axis.origin[axis] - frame.origin[axis]);
            if dot3(offset, frame.normal).abs() > ASSOCIATIVE_TOLERANCE_MM
                || dot3(seed_axis.direction, frame.normal).abs() < 1.0 - ASSOCIATIVE_TOLERANCE_MM
            {
                return None;
            }
            let seed_u = dot3(offset, frame.u_axis);
            let seed_v = dot3(offset, frame.v_axis);
            let mut expected = vec![seed_axis];
            expected.extend(placements.iter().map(|placement| FeatureHoleAxis {
                origin: std::array::from_fn(|axis| {
                    frame.origin[axis]
                        + frame.u_axis[axis] * placement.local_u_mm
                        + frame.v_axis[axis] * placement.local_v_mm
                }),
                direction: seed_axis.direction,
            }));
            if placements.iter().any(|placement| {
                (placement.local_u_mm - seed_u).hypot(placement.local_v_mm - seed_v)
                    <= ASSOCIATIVE_TOLERANCE_MM
            }) {
                return None;
            }
            Some(expected)
        }
    }
}

fn resolve_plane_reference(
    engineering: &EngineeringModel,
    reference: &PlaneReference,
    visited: &mut BTreeSet<FeatureId>,
    depth: usize,
) -> Option<FeaturePlaneFrame> {
    if depth > engineering.features.len() {
        return None;
    }
    match reference {
        PlaneReference::Principal { plane } => Some(principal_plane_frame(*plane)),
        PlaneReference::PlanarFace { qualifier, .. } => {
            let normal = [qualifier.normal.x, qualifier.normal.y, qualifier.normal.z];
            let normal = normalize3(normal)?;
            let (u_axis, v_axis) = frame_axes_for_normal(normal)?;
            Some(FeaturePlaneFrame {
                origin: normal.map(|value| value * qualifier.plane_offset),
                u_axis,
                v_axis,
                normal,
            })
        }
        // Drawing association does not yet derive generated Sweep/Loft face
        // frames from OCP witnesses; do not guess a dimension plane.
        PlaneReference::GeneratedFace { .. } => None,
        PlaneReference::Datum { feature } => {
            if !visited.insert(feature.clone()) {
                return None;
            }
            let plane = match engineering.features.get(feature) {
                Some(EngineeringFeature::Parametric {
                    feature: ParametricFeature::DatumPlane { definition },
                    suppressed: false,
                    ..
                }) => match definition {
                    DatumPlaneDefinition::Principal { plane } => {
                        Some(principal_plane_frame(*plane))
                    }
                    DatumPlaneDefinition::Offset { base, distance } => {
                        let mut base =
                            resolve_plane_reference(engineering, base, visited, depth + 1)?;
                        for index in 0..3 {
                            base.origin[index] += base.normal[index] * distance;
                        }
                        Some(base)
                    }
                    DatumPlaneDefinition::CustomPlane {
                        origin,
                        normal,
                        x_axis,
                    } => {
                        let normal = normalize3(*normal)?;
                        let u_axis = normalize3(*x_axis)?;
                        let v_axis = normalize3(cross3(normal, u_axis))?;
                        Some(FeaturePlaneFrame {
                            origin: *origin,
                            u_axis,
                            v_axis,
                            normal,
                        })
                    }
                    DatumPlaneDefinition::PlanarFaceReference { .. } => None,
                },
                _ => None,
            };
            visited.remove(feature);
            plane
        }
    }
}

fn principal_plane_frame(plane: PrincipalPlane) -> FeaturePlaneFrame {
    match plane {
        PrincipalPlane::XY => FeaturePlaneFrame {
            origin: [0.0, 0.0, 0.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        },
        PrincipalPlane::XZ => FeaturePlaneFrame {
            origin: [0.0, 0.0, 0.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 0.0, 1.0],
            normal: [0.0, -1.0, 0.0],
        },
        PrincipalPlane::YZ => FeaturePlaneFrame {
            origin: [0.0, 0.0, 0.0],
            u_axis: [0.0, 1.0, 0.0],
            v_axis: [0.0, 0.0, 1.0],
            normal: [1.0, 0.0, 0.0],
        },
    }
}

fn frame_axes_for_normal(normal: [f64; 3]) -> Option<([f64; 3], [f64; 3])> {
    let reference = if normal[2].abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u_axis = if dot3(normal, reference).abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        reference
    };
    Some((u_axis, normalize3(cross3(normal, u_axis))?))
}

fn same_axis_line(origin: [f64; 3], axis: [f64; 3], expected: FeatureHoleAxis) -> bool {
    let Some(actual_axis) = normalize3(axis) else {
        return false;
    };
    if dot3(actual_axis, expected.direction).abs() < 1.0 - ASSOCIATIVE_TOLERANCE_MM {
        return false;
    }
    norm3(cross3(
        std::array::from_fn(|index| expected.origin[index] - origin[index]),
        actual_axis,
    )) <= ASSOCIATIVE_TOLERANCE_MM * 4.0
}

fn rotate_about_axis(point: [f64; 3], center: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let vector = std::array::from_fn(|index| point[index] - center[index]);
    let rotated = rotate_vector(vector, axis, angle);
    std::array::from_fn(|index| center[index] + rotated[index])
}

fn rotate_vector(vector: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let cosine = angle.cos();
    let sine = angle.sin();
    let cross = cross3(axis, vector);
    let projection = dot3(axis, vector) * (1.0 - cosine);
    std::array::from_fn(|index| {
        vector[index] * cosine + cross[index] * sine + axis[index] * projection
    })
}

fn normalize3(vector: [f64; 3]) -> Option<[f64; 3]> {
    let length = norm3(vector);
    (length.is_finite() && length > ASSOCIATIVE_TOLERANCE_MM)
        .then(|| vector.map(|value| value / length))
}

fn dot3(first: [f64; 3], second: [f64; 3]) -> f64 {
    first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

fn cross3(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn norm3(vector: [f64; 3]) -> f64 {
    dot3(vector, vector).sqrt()
}

fn cylinder_faces<'a>(
    evidence: &'a ParametricSpatialEvidence,
    owner: &FeatureId,
    radius: f64,
) -> Vec<&'a SpatialFaceEvidence> {
    evidence.faces.iter().filter(|face| face.source_features.iter().any(|source| source == &owner.0)).filter(|face| matches!(face.surface, SpatialFaceSurface::Cylinder { radius: actual, .. } if (actual-radius).abs() <= ASSOCIATIVE_TOLERANCE_MM)).collect()
}
fn axis_is_z(face: &SpatialFaceEvidence) -> bool {
    matches!(face.surface, SpatialFaceSurface::Cylinder { axis, .. } if axis[0].abs() <= ASSOCIATIVE_TOLERANCE_MM && axis[1].abs() <= ASSOCIATIVE_TOLERANCE_MM && (axis[2].abs()-1.0).abs() <= ASSOCIATIVE_TOLERANCE_MM)
}
fn cylinder_center(face: &SpatialFaceEvidence) -> Option<[f64; 3]> {
    match face.surface {
        SpatialFaceSurface::Cylinder { axis_origin, .. } => Some(axis_origin),
        _ => None,
    }
}
/// Exact circle/arc evidence extracted from DrawingModel primitives.
#[derive(Debug, Clone, Copy)]
struct ProjectedHoleCircleWitness {
    center: DrawingPoint,
    model_radius: f64,
    sheet_radius: f64,
    arc_angles: Option<(f64, f64)>,
}

/// Match every source cylinder center to an actual DrawingModel circle or arc.
fn projected_hole_circles(
    view: &DrawingView,
    faces: &[&SpatialFaceEvidence],
    expected_radius: f64,
) -> Option<Vec<ProjectedHoleCircleWitness>> {
    let mut centers = BTreeMap::new();
    for face in faces {
        let Some(center) = cylinder_center(face) else {
            continue;
        };
        let key = (
            (center[0] / ASSOCIATIVE_TOLERANCE_MM).round() as i64,
            (center[1] / ASSOCIATIVE_TOLERANCE_MM).round() as i64,
        );
        centers.entry(key).or_insert(center);
    }
    if centers.is_empty() {
        return None;
    }
    let mut circles = Vec::with_capacity(centers.len());
    for center in centers.values() {
        let found = view.primitives.values().find_map(|primitive| {
            let (projected_center, radius, arc_angles) = match primitive {
                DrawingPrimitive::Circle {
                    center,
                    radius,
                    role: DrawingPrimitiveRole::Object,
                } => (*center, *radius, None),
                DrawingPrimitive::Arc {
                    center,
                    radius,
                    start_angle,
                    end_angle,
                    role: DrawingPrimitiveRole::Object,
                } => (*center, *radius, Some((*start_angle, *end_angle))),
                _ => return None,
            };
            ((radius - expected_radius).abs() <= ASSOCIATIVE_TOLERANCE_MM
                && (projected_center.x - center[0]).abs() <= ASSOCIATIVE_TOLERANCE_MM
                && (projected_center.y - center[1]).abs() <= ASSOCIATIVE_TOLERANCE_MM)
                .then_some((projected_center, radius, arc_angles))
        })?;
        let sheet_center = view.transform.model_to_sheet(found.0);
        circles.push(ProjectedHoleCircleWitness {
            center: sheet_center,
            model_radius: found.1,
            sheet_radius: found.1 * view.transform.scale.0,
            arc_angles: found.2,
        });
    }
    Some(circles)
}

fn angle_is_on_arc(angle: f64, start: f64, end: f64) -> bool {
    let span = (end - start).rem_euclid(std::f64::consts::TAU);
    let offset = (angle - start).rem_euclid(std::f64::consts::TAU);
    offset <= span + ASSOCIATIVE_TOLERANCE_MM
}

fn projected_circles_footprint(
    circles: &[ProjectedHoleCircleWitness],
) -> Result<DrawingBounds, DrawingError> {
    let mut bounds: Option<DrawingBounds> = None;
    for circle_witness in circles {
        let center = circle_witness.center;
        let radius = circle_witness.sheet_radius;
        let circle = DrawingBounds {
            min_x: center.x - radius,
            min_y: center.y - radius,
            max_x: center.x + radius,
            max_y: center.y + radius,
        };
        match &mut bounds {
            Some(bounds) => bounds.include(&circle),
            None => bounds = Some(circle),
        }
    }
    bounds.ok_or_else(|| invalid("hole callout pattern footprint is empty"))
}

fn body_extents(evidence: &ParametricSpatialEvidence) -> Result<[f64; 6], DrawingError> {
    if evidence.faces.is_empty() {
        return Err(invalid("parametric spatial evidence contains no faces"));
    }
    let mut result = [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for face in &evidence.faces {
        for axis in 0..3 {
            result[axis] = result[axis].min(face.bounds_min[axis]);
            result[axis + 3] = result[axis + 3].max(face.bounds_max[axis]);
        }
    }
    if result.iter().any(|value| !value.is_finite()) {
        return Err(invalid("parametric B-Rep envelope is non-finite"));
    }
    Ok(result)
}
fn axis_span(
    evidence: &ParametricSpatialEvidence,
    axis: DimensionAxis3D,
) -> Result<f64, DrawingError> {
    let extents = body_extents(evidence)?;
    let index = match axis {
        DimensionAxis3D::X => 0,
        DimensionAxis3D::Y => 1,
        DimensionAxis3D::Z => 2,
    };
    Ok(extents[index + 3] - extents[index])
}

fn extreme_face_ids(
    evidence: &ParametricSpatialEvidence,
    axis: DimensionAxis3D,
    min: f64,
    max: f64,
) -> Vec<String> {
    let index = match axis {
        DimensionAxis3D::X => 0,
        DimensionAxis3D::Y => 1,
        DimensionAxis3D::Z => 2,
    };
    evidence
        .faces
        .iter()
        .filter(|face| {
            (face.bounds_min[index] - min).abs() <= ASSOCIATIVE_TOLERANCE_MM
                || (face.bounds_max[index] - max).abs() <= ASSOCIATIVE_TOLERANCE_MM
        })
        .map(|face| face.id.clone())
        .collect()
}
fn frame_axis_is_z(axis: &[f64; 3]) -> bool {
    axis[0].abs() <= ASSOCIATIVE_TOLERANCE_MM
        && axis[1].abs() <= ASSOCIATIVE_TOLERANCE_MM
        && (axis[2].abs() - 1.0).abs() <= ASSOCIATIVE_TOLERANCE_MM
}
fn linear_layout(
    transform: &crate::DrawingViewTransform,
    start: DrawingPoint,
    end: DrawingPoint,
    axis: LinearAxis,
    bounds: DrawingBounds,
    above_or_right: bool,
    id: DrawingDimensionId,
) -> DrawingDimensionLayout {
    let first = transform.model_to_sheet(start);
    let second = transform.model_to_sheet(end);
    let geometry = match axis {
        LinearAxis::Horizontal => {
            let dim_y = if above_or_right {
                transform
                    .model_to_sheet(DrawingPoint::new(0.0, bounds.max_y))
                    .y
                    + DIMENSION_OFFSET_MM
            } else {
                first.y - DIMENSION_OFFSET_MM
            };
            DimensionLayoutGeometry::Linear {
                dimension_line: LayoutSegment {
                    start: DrawingPoint::new(first.x, dim_y),
                    end: DrawingPoint::new(second.x, dim_y),
                },
                extension_lines: [
                    LayoutSegment {
                        start: DrawingPoint::new(first.x, first.y + EXTENSION_GAP_MM),
                        end: DrawingPoint::new(first.x, dim_y + EXTENSION_OVERHANG_MM),
                    },
                    LayoutSegment {
                        start: DrawingPoint::new(second.x, second.y + EXTENSION_GAP_MM),
                        end: DrawingPoint::new(second.x, dim_y + EXTENSION_OVERHANG_MM),
                    },
                ],
                arrows: [
                    ArrowAnchor {
                        point: first,
                        direction: DrawingPoint::new(1.0, 0.0),
                    },
                    ArrowAnchor {
                        point: second,
                        direction: DrawingPoint::new(-1.0, 0.0),
                    },
                ],
                text_anchor: DrawingPoint::new((first.x + second.x) / 2.0, dim_y + 1.5),
                text_orientation: crate::TextOrientation::Horizontal,
            }
        }
        LinearAxis::Vertical => {
            let dim_x = transform
                .model_to_sheet(DrawingPoint::new(bounds.max_x, 0.0))
                .x
                + DIMENSION_OFFSET_MM;
            DimensionLayoutGeometry::Linear {
                dimension_line: LayoutSegment {
                    start: DrawingPoint::new(dim_x, first.y),
                    end: DrawingPoint::new(dim_x, second.y),
                },
                extension_lines: [
                    LayoutSegment {
                        start: DrawingPoint::new(first.x + EXTENSION_GAP_MM, first.y),
                        end: DrawingPoint::new(dim_x + EXTENSION_OVERHANG_MM, first.y),
                    },
                    LayoutSegment {
                        start: DrawingPoint::new(second.x + EXTENSION_GAP_MM, second.y),
                        end: DrawingPoint::new(dim_x + EXTENSION_OVERHANG_MM, second.y),
                    },
                ],
                arrows: [
                    ArrowAnchor {
                        point: first,
                        direction: DrawingPoint::new(0.0, 1.0),
                    },
                    ArrowAnchor {
                        point: second,
                        direction: DrawingPoint::new(0.0, -1.0),
                    },
                ],
                text_anchor: DrawingPoint::new(dim_x + 1.5, (first.y + second.y) / 2.0),
                text_orientation: crate::TextOrientation::Vertical,
            }
        }
    };
    DrawingDimensionLayout {
        dimension: id,
        geometry,
    }
}
fn parametric_feature_parameter_value(
    model: &EngineeringModel,
    feature_id: &FeatureId,
    parameter: ParametricDimensionParameter,
) -> Option<f64> {
    crate::dimension::parametric_feature_parameter_value(model, feature_id, parameter)
}
fn invalid(reason: &str) -> DrawingError {
    DrawingError::InvalidDrawingDimension {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_feature_parameter_reference_fails_closed() {
        use scc_spatial::{
            Bounds3D, ParametricSpatialEvidence, SpatialBaseProfile, SpatialBody, SpatialBodyFrame,
            SpatialFootprint, SpatialModelSource, ZInterval,
        };

        let evidence = ParametricSpatialEvidence {
            model_sha256: "test-digest".into(),
            frame: SpatialBodyFrame {
                origin: [0.0; 3],
                u_axis: [1.0, 0.0, 0.0],
                v_axis: [0.0, 1.0, 0.0],
                axis: [0.0, 0.0, 1.0],
                axis_min: 0.0,
                axis_max: 1.0,
            },
            volume_mm3: 1.0,
            operation_features: Vec::new(),
            edges: Vec::new(),
            faces: Vec::new(),
            views: BTreeMap::new(),
        };
        let spatial = SpatialModel {
            body: SpatialBody {
                footprint: SpatialFootprint::Parametric {
                    evidence: Box::new(evidence),
                },
                profile: SpatialBaseProfile::Parametric,
                z: ZInterval { min: 0.0, max: 1.0 },
                source_feature: FeatureId("body".into()),
                source_entity: scc_core::ids::EntityId("body".into()),
            },
            cuts: BTreeMap::new(),
            slot_cuts: BTreeMap::new(),
            bounds: Bounds3D::new([0.0; 3], [1.0; 3]).unwrap(),
            construction_circles: BTreeMap::new(),
            source: SpatialModelSource {
                geometry_source_root: 0,
                geometry_source_step: 0,
            },
        };
        let mut drawing = crate::DrawingModel {
            sheets: BTreeMap::new(),
            source_map: crate::DrawingSourceMap::new(),
            associative_dimensions: BTreeMap::new(),
        };
        let missing_feature = FeatureId("missing_hole".into());
        let id = DrawingDimensionId("associative.invalid".into());
        drawing.associative_dimensions.insert(
            id.clone(),
            AssociativeDimension {
                annotation_id: id.0.clone(),
                drawing_dimension: None,
                kind: DrawingDimensionKind::Diameter,
                reference: AssociativeDimensionReference::FeatureParameter {
                    feature: missing_feature.clone(),
                    parameter: ParametricDimensionParameter::HoleDiameter,
                },
                view: DrawingViewId("front".into()),
                nominal: Some(ResolvedDimensionValue::LengthMm { mm: 8.0 }),
                unit: AssociativeDimensionUnit::Millimetres,
                sources: vec![DimensionValueSource::EngineeringFeatureParameter {
                    feature: missing_feature,
                    parameter: ParametricDimensionParameter::HoleDiameter,
                }],
                provenance: DimensionValueProvenance::EngineeringDeclared,
                witness: None,
                observability: DimensionObservability::Unsupported,
                status: AssociativeDimensionStatus::Unsupported,
                diagnostic: Some("test invalid feature reference".into()),
            },
        );

        let error = validate_parametric_associative_dimensions(
            &drawing,
            &EngineeringModel::new(),
            &spatial,
        )
        .unwrap_err();
        assert_eq!(error.code(), "SCC-DRW-009");
    }

    #[test]
    fn body_length_dimension_on_inapplicable_view_fails_closed() {
        use crate::dimension_layout::{ArrowAnchor, DimensionLayoutGeometry, LayoutSegment};
        use crate::model::{DrawingBounds, DrawingViewTransform};
        use scc_spatial::{ParametricSpatialEvidence, SpatialBodyFrame};

        let model_sha256 = "view-applicability-test".to_string();
        let id = DrawingDimensionId("associative.body.length".into());
        let source = DimensionValueSource::VerifiedBodyGeometry {
            model_sha256: model_sha256.clone(),
            axis: DimensionAxis3D::X,
        };
        let dimension = DrawingDimension {
            id: id.clone(),
            target: DrawingDimensionTarget::Body,
            kind: DrawingDimensionKind::Linear {
                axis: LinearAxis::Horizontal,
            },
            specification: None,
            value: DimensionValue::Measured {
                source: source.clone(),
                resolved: ResolvedDimensionValue::LengthMm { mm: 100.0 },
            },
        };
        let association = AssociativeDimension {
            annotation_id: id.0.clone(),
            drawing_dimension: Some(id.clone()),
            kind: dimension.kind.clone(),
            reference: AssociativeDimensionReference::BodyEnvelope {
                axis: DimensionAxis3D::X,
            },
            view: DrawingViewId("left".into()),
            nominal: Some(ResolvedDimensionValue::LengthMm { mm: 100.0 }),
            unit: AssociativeDimensionUnit::Millimetres,
            sources: vec![source],
            provenance: DimensionValueProvenance::GeometryDerived,
            witness: Some(DimensionGeometryWitness {
                model_sha256: model_sha256.clone(),
                evidence_ids: vec!["face-1".into()],
                geometry_measurement_mm: 100.0,
                drawing_measurement_mm: Some(100.0),
                component_measurements_mm: BTreeMap::new(),
                tolerance_mm: ASSOCIATIVE_TOLERANCE_MM,
            }),
            observability: DimensionObservability::Observable,
            status: AssociativeDimensionStatus::Resolved,
            diagnostic: None,
        };
        let view = DrawingView {
            id: DrawingViewId("left".into()),
            kind: crate::DrawingViewKind::Left,
            display_label: None,
            transform: DrawingViewTransform::identity(),
            primitives: BTreeMap::new(),
            dimensions: BTreeMap::from([(id.clone(), dimension.clone())]),
            dimension_layouts: BTreeMap::new(),
            center_geometry: BTreeMap::new(),
            model_bounds: DrawingBounds::zero(),
        };
        let point = DrawingPoint::new(0.0, 0.0);
        let layout = DrawingDimensionLayout {
            dimension: id,
            geometry: DimensionLayoutGeometry::Linear {
                dimension_line: LayoutSegment {
                    start: point,
                    end: DrawingPoint::new(100.0, 0.0),
                },
                extension_lines: [LayoutSegment {
                    start: point,
                    end: point,
                }; 2],
                arrows: [ArrowAnchor {
                    point,
                    direction: point,
                }; 2],
                text_anchor: point,
                text_orientation: crate::TextOrientation::Horizontal,
            },
        };
        let evidence = ParametricSpatialEvidence {
            model_sha256,
            frame: SpatialBodyFrame {
                origin: [0.0; 3],
                u_axis: [1.0, 0.0, 0.0],
                v_axis: [0.0, 1.0, 0.0],
                axis: [0.0, 0.0, 1.0],
                axis_min: 0.0,
                axis_max: 10.0,
            },
            volume_mm3: 1000.0,
            operation_features: Vec::new(),
            edges: Vec::new(),
            faces: Vec::new(),
            views: BTreeMap::new(),
        };
        let error = validate_drawing_observation(
            &association,
            &view,
            &layout,
            &dimension,
            &EngineeringModel::new(),
            &evidence,
        )
        .unwrap_err();
        assert!(error.to_string().contains("inapplicable view"));
    }
}
