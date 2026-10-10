//! Deterministic lowering from [`EngineeringModel`] to the formal SCC input
//! ([`scc_proposal::schema::Proposal`]), plus a full pipeline runner to a
//! [`scc_geometry::model::GeometryModel`].
//!
//! Architecture (ADR-017): the model lowers **to the Proposal vocabulary** so
//! the existing `scc-analysis → scc-lowering → scc-execution → scc-geometry`
//! pipeline stays fully in the loop — engineering lowering never fabricates
//! `ConstraintIR` or `GraphState` directly, and `scc-analysis` re-validates
//! everything it produced.
//!
//! This module is a pure, deterministic function of its input: no network, no
//! time, no randomness, no global state, no mutation of the model.

use std::collections::BTreeMap;

use scc_core::ids::{ConstraintId, EntityId};
use scc_proposal::schema::{
    Proposal, ProposalAnchor, ProposalArray, ProposalDimension, ProposalEntity, ProposalRelation,
};

use crate::binding::{feature_field_slots, FeatureParameterSlot};
use crate::error::EngineeringError;
use crate::evaluator::ResolvedValue;
use crate::feature::{BaseProfile, EngineeringConstraint, EngineeringFeature, HolePattern};
use crate::model::EngineeringModel;
use crate::profile::{geometry_angles, ProfileSegment};
use crate::source_map::{
    EngineeringSourceMap, ProfileSegmentSemantic, ProfileSegmentSource, RoundedCorner,
};
use crate::{EngineeringConstraintId, ParameterId};

/// A read-only lowering view: the model plus every parameter resolved in
/// dependency order. Never a second persistent truth — the model keeps its
/// source-of-truth expressions (ADR-016 Decision 2 / ADR-017 Decision 3).
pub struct ResolvedEngineeringView<'a> {
    model: &'a EngineeringModel,
    resolved: BTreeMap<ParameterId, ResolvedValue>,
}

impl<'a> ResolvedEngineeringView<'a> {
    /// Build the view: validate the model, then evaluate every parameter.
    pub fn build(model: &'a EngineeringModel) -> Result<Self, EngineeringError> {
        model.validate_model()?;
        let resolved = crate::evaluator::evaluate_all(model)?;
        Ok(Self { model, resolved })
    }

    /// The underlying model.
    pub fn model(&self) -> &'a EngineeringModel {
        self.model
    }

    /// Resolve one typed feature slot to a concrete finite value.
    ///
    /// Priority (ADR-017 Decision 3/4):
    /// 1. a bound parameter's resolved value (a bound-but-`Unresolved`
    ///    parameter is a hard error — never a fallback to the literal);
    /// 2. the feature's embedded literal (its initial default);
    /// 3. position slots (`CenterX`/`CenterY`) default to the origin.
    pub fn slot_value(
        &self,
        feature: &EngineeringFeature,
        slot: FeatureParameterSlot,
    ) -> Result<f64, EngineeringError> {
        let feature_id = feature.id().clone();
        if let Some(parameter) = self
            .model
            .feature_parameters
            .get(&feature_id)
            .and_then(|bindings| bindings.get(&slot))
        {
            let resolved = self.resolved.get(parameter).ok_or_else(|| {
                EngineeringError::ParameterNotFound {
                    parameter_id: parameter.clone(),
                }
            })?;
            return match resolved {
                ResolvedValue::Scalar(value) => Ok(*value),
                ResolvedValue::Integer(value) => Ok(*value as f64),
                ResolvedValue::Unresolved => Err(EngineeringError::UnresolvedRequiredParameter {
                    feature_id,
                    slot,
                    parameter_id: parameter.clone(),
                }),
                ResolvedValue::Boolean(_) | ResolvedValue::Choice(_) => {
                    Err(EngineeringError::LoweringFailed {
                        message: format!(
                            "parameter {} bound to slot {} is not numeric",
                            parameter, slot
                        ),
                    })
                }
            };
        }
        if let Some(value) = feature_field_slots(feature).get(&slot) {
            return Ok(*value);
        }
        match slot {
            FeatureParameterSlot::CenterX | FeatureParameterSlot::CenterY => Ok(0.0),
            _ => Err(EngineeringError::MissingFeatureParameterBinding { feature_id, slot }),
        }
    }
}

/// The result of lowering an [`EngineeringModel`] to the formal SCC input.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineeringLoweringResult {
    pub proposal: Proposal,
    pub source_map: EngineeringSourceMap,
}

/// Errors that can occur while driving the full SCC pipeline after lowering.
///
/// SCC-stage errors are preserved in their own variants (§29): engineering
/// failures stay `SCC-ENG-*`, semantic-analysis conflicts stay `SCC-ANAL-*`,
/// and geometry failures stay `SCC-GEOM-*`.
#[derive(Debug)]
pub enum LoweringPipelineError {
    Engineering(EngineeringError),
    Analysis(scc_analysis::error::AnalysisErrors),
    AnalysisConflict(scc_core::conflict::Conflict),
    Lowering(scc_lowering::error::LoweringError),
    Execution(scc_core::error::SccError),
    Geometry(scc_geometry::error::GeometryError),
}

impl From<LoweringPipelineError> for scc_core::error::SccError {
    fn from(err: LoweringPipelineError) -> Self {
        use scc_core::error::{ErrorStage, SccError, StructuredError};
        match err {
            LoweringPipelineError::Engineering(e) => SccError::Structured(Box::new(e.into())),
            LoweringPipelineError::Analysis(errors) => errors.into(),
            LoweringPipelineError::AnalysisConflict(conflict) => SccError::Structured(Box::new(
                StructuredError::new(
                    "SCC-ANAL-008",
                    ErrorStage::Analysis,
                    format!("semantic conflict: {}", conflict.facts.message),
                )
                .with_recoverable(true)
                .with_suggestion("remove the conflicting intent before solving"),
            )),
            LoweringPipelineError::Lowering(e) => e.into(),
            LoweringPipelineError::Execution(e) => e,
            LoweringPipelineError::Geometry(e) => e.into(),
        }
    }
}

/// Lower an [`EngineeringModel`] to a deterministic [`Proposal`] plus its
/// source map. Pure and side-effect free.
pub fn lower_engineering_model(
    model: &EngineeringModel,
) -> Result<EngineeringLoweringResult, EngineeringError> {
    let view = ResolvedEngineeringView::build(model)?;
    let mut lowerer = Lowerer::new(&view);
    lowerer.lower()?;
    lowerer.finalize_constraint_source_map();
    Ok(lowerer.finish())
}

/// Lower an [`EngineeringModel`] and drive the existing SCC pipeline all the
/// way to a solved [`scc_geometry::model::GeometryModel`].
pub fn lower_and_solve(
    model: &EngineeringModel,
) -> Result<
    (
        EngineeringLoweringResult,
        scc_geometry::model::GeometryModel,
    ),
    LoweringPipelineError,
> {
    let result = lower_engineering_model(model).map_err(LoweringPipelineError::Engineering)?;

    let analysis = scc_analysis::declaration_graph::analyze_detailed(&result.proposal)
        .map_err(LoweringPipelineError::Analysis)?;
    if let Some(conflict) = analysis.conflicts.first() {
        return Err(LoweringPipelineError::AnalysisConflict(conflict.clone()));
    }

    let constraint_ir =
        scc_lowering::try_lower(&analysis.semantic_ir).map_err(LoweringPipelineError::Lowering)?;
    let empty_patch: Vec<scc_execution::overlay_view::MutationOp> = Vec::new();
    let output = scc_execution::driver::execute(&constraint_ir, &empty_patch)
        .map_err(LoweringPipelineError::Execution)?;
    use scc_geometry::GeometrySolver as _;
    let geometry = scc_geometry::DeterministicGeometrySolver
        .solve(&output.final_state)
        .map_err(LoweringPipelineError::Geometry)?;

    Ok((result, geometry))
}

struct Lowerer<'a> {
    view: &'a ResolvedEngineeringView<'a>,
    entities: Vec<ProposalEntity>,
    relations: Vec<ProposalRelation>,
    dimensions: Vec<ProposalDimension>,
    arrays: Vec<ProposalArray>,
    anchors: Vec<ProposalAnchor>,
    source_map: EngineeringSourceMap,
    relation_index: BTreeMap<EngineeringConstraintId, usize>,
    anchor_index: BTreeMap<EngineeringConstraintId, usize>,
}

impl<'a> Lowerer<'a> {
    fn new(view: &'a ResolvedEngineeringView<'a>) -> Self {
        Self {
            view,
            entities: Vec::new(),
            relations: Vec::new(),
            dimensions: Vec::new(),
            arrays: Vec::new(),
            anchors: Vec::new(),
            source_map: EngineeringSourceMap::new(),
            relation_index: BTreeMap::new(),
            anchor_index: BTreeMap::new(),
        }
    }

    fn lower(&mut self) -> Result<(), EngineeringError> {
        // Use the persisted history when present. Legacy models resolve to
        // BTreeMap order through `ordered_feature_ids()`.
        for id in self.view.model().ordered_feature_ids() {
            let feature = self
                .view
                .model()
                .features
                .get(&id)
                .expect("validated feature order only contains existing IDs");
            self.lower_feature(feature)?;
        }
        // Constraints in deterministic (sorted) id order.
        for constraint in self.view.model().constraints.values() {
            self.lower_constraint(constraint)?;
        }
        // DrawingState intentionally does not participate in Phase 3 geometry
        // lowering (ADR-017 Decision 8).
        Ok(())
    }

    fn finish(self) -> EngineeringLoweringResult {
        let proposal = Proposal {
            version: "1.0".into(),
            entities: self.entities,
            relations: self.relations,
            dimensions: self.dimensions,
            arrays: self.arrays,
            anchors: self.anchors,
            metadata: serde_json::Value::Null,
        };
        EngineeringLoweringResult {
            proposal,
            source_map: self.source_map,
        }
    }

    /// Compute each lowered constraint's SCC constraint id.
    ///
    /// `scc_lowering::lower` processes relations, then dimensions, then arrays
    /// (two constraints each), then anchors — so every id is a deterministic
    /// function of the proposal's emission order. Relations here are only the
    /// constraint-derived ones; features emit dimensions/arrays/anchors only.
    fn finalize_constraint_source_map(&mut self) {
        let relation_count = self.relations.len();
        let dimension_count = self.dimensions.len();
        let array_count = self.arrays.len();
        for (constraint, index) in &self.relation_index {
            self.source_map
                .record_constraint(constraint.clone(), ConstraintId(format!("c_{}", index + 1)));
        }
        for (constraint, index) in &self.anchor_index {
            let id = relation_count + dimension_count + 2 * array_count + index + 1;
            self.source_map
                .record_constraint(constraint.clone(), ConstraintId(format!("c_{}", id)));
        }
    }

    // ---- feature lowering ----

    fn lower_feature(&mut self, feature: &EngineeringFeature) -> Result<(), EngineeringError> {
        match feature {
            EngineeringFeature::BaseProfile { .. } => self.lower_base_profile(feature),
            EngineeringFeature::Hole { .. } => self.lower_circle(feature),
            EngineeringFeature::HolePattern { pattern, .. } => self.lower_pattern(feature, pattern),
            EngineeringFeature::Slot { .. } => self.lower_slot(feature),
            EngineeringFeature::Fillet { .. } => {
                Err(EngineeringError::UnsupportedFeatureLowering {
                    feature_id: feature.id().clone(),
                    feature_type: "Fillet",
                })
            }
            EngineeringFeature::ConstructionCircle { .. } => self.lower_circle(feature),
            EngineeringFeature::Parametric {
                suppressed: true, ..
            } => Ok(()),
            EngineeringFeature::Parametric { id, .. } => {
                Err(EngineeringError::UnsupportedFeatureLowering {
                    feature_id: id.clone(),
                    feature_type: "Parametric (geometry lowering pending)",
                })
            }
        }
    }

    fn entity(&mut self, feature: &EngineeringFeature, kind: &str) -> EntityId {
        let id = EntityId(feature.id().0.clone());
        self.entities.push(ProposalEntity {
            id: id.0.clone(),
            kind: kind.into(),
        });
        self.source_map
            .record_feature_entity(feature.id().clone(), id.clone());
        id
    }

    fn position_dimensions(&mut self, target: &str, x: f64, y: f64) {
        self.dimensions.push(ProposalDimension {
            target: target.into(),
            kind: "x".into(),
            value: x,
        });
        self.dimensions.push(ProposalDimension {
            target: target.into(),
            kind: "y".into(),
            value: y,
        });
    }

    fn dimension(&mut self, target: &str, kind: &str, value: f64) {
        self.dimensions.push(ProposalDimension {
            target: target.into(),
            kind: kind.into(),
            value,
        });
    }

    fn lower_base_profile(&mut self, feature: &EngineeringFeature) -> Result<(), EngineeringError> {
        let EngineeringFeature::BaseProfile { profile, .. } = feature else {
            return Err(EngineeringError::LoweringFailed {
                message: "base profile lowering received a non-base feature".into(),
            });
        };
        match profile {
            BaseProfile::Rectangle { .. } => {
                let width = self.view.slot_value(feature, FeatureParameterSlot::Width)?;
                let height = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::Height)?;
                let id = self.entity(feature, "rectangle");
                self.dimension(&id.0, "width", width);
                self.dimension(&id.0, "height", height);
                // Pin the rectangle's center at the origin (L0 anchor).
                let anchor_id = format!("{}_anchor", id.0);
                self.anchors.push(ProposalAnchor {
                    id: anchor_id,
                    kind: "fixed_center".into(),
                    entity: Some(id.0),
                });
            }
            BaseProfile::Circle { .. } => {
                let center_x = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::CenterX)?;
                let center_y = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::CenterY)?;
                let diameter = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::Diameter)?;
                let id = self.entity(feature, "circle");
                self.position_dimensions(&id.0, center_x, center_y);
                self.dimension(&id.0, "diameter", diameter);
            }
            BaseProfile::RoundedRectangle { .. } => {
                let center_x = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::CenterX)?;
                let center_y = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::CenterY)?;
                let width = self.view.slot_value(feature, FeatureParameterSlot::Width)?;
                let height = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::Height)?;
                let corner_radius = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::Radius)?;
                let resolved = BaseProfile::RoundedRectangle {
                    center: crate::Point2D::new(center_x, center_y),
                    width,
                    height,
                    corner_radius,
                };
                let segments = resolved.segments()?;
                for (index, segment) in segments.iter().enumerate() {
                    let semantic = match index {
                        1 => ProfileSegmentSemantic::RoundedCorner {
                            corner: RoundedCorner::BottomRight,
                        },
                        3 => ProfileSegmentSemantic::RoundedCorner {
                            corner: RoundedCorner::TopRight,
                        },
                        5 => ProfileSegmentSemantic::RoundedCorner {
                            corner: RoundedCorner::TopLeft,
                        },
                        7 => ProfileSegmentSemantic::RoundedCorner {
                            corner: RoundedCorner::BottomLeft,
                        },
                        _ => ProfileSegmentSemantic::CompositeSegment,
                    };
                    self.lower_profile_segment(feature, index, segment, semantic)?;
                }
            }
            BaseProfile::Composite { segments } => {
                BaseProfile::Composite {
                    segments: segments.clone(),
                }
                .validate()?;
                for (index, segment) in segments.iter().enumerate() {
                    self.lower_profile_segment(
                        feature,
                        index,
                        segment,
                        ProfileSegmentSemantic::CompositeSegment,
                    )?;
                }
            }
            BaseProfile::Polygon { vertices } => {
                let vertices = crate::profile::canonicalize_polygon(vertices)?;
                let segments = BaseProfile::Polygon {
                    vertices: vertices.clone(),
                }
                .segments()?;
                for (index, segment) in segments.iter().enumerate() {
                    self.lower_profile_segment(
                        feature,
                        index,
                        segment,
                        ProfileSegmentSemantic::PolygonEdge {
                            index: index as u32,
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    fn lower_profile_segment(
        &mut self,
        feature: &EngineeringFeature,
        index: usize,
        segment: &ProfileSegment,
        semantic: ProfileSegmentSemantic,
    ) -> Result<(), EngineeringError> {
        let id = EntityId(format!("{}_segment_{index}", feature.id()));
        let kind = match segment {
            ProfileSegment::Line { .. } => "line",
            ProfileSegment::Arc { .. } => "arc",
        };
        self.entities.push(ProposalEntity {
            id: id.0.clone(),
            kind: kind.into(),
        });
        self.source_map
            .record_feature_entity(feature.id().clone(), id.clone());
        self.source_map.record_profile_segment(
            id.clone(),
            ProfileSegmentSource {
                feature: feature.id().clone(),
                segment_index: index as u32,
                semantic,
            },
        );
        match segment {
            ProfileSegment::Line { start, end } => {
                self.dimension(&id.0, "x1", start.x);
                self.dimension(&id.0, "y1", start.y);
                self.dimension(&id.0, "x2", end.x);
                self.dimension(&id.0, "y2", end.y);
            }
            ProfileSegment::Arc {
                start,
                end,
                center,
                radius,
                ..
            } => {
                self.position_dimensions(&id.0, center.x, center.y);
                self.dimension(&id.0, "x1", start.x);
                self.dimension(&id.0, "y1", start.y);
                self.dimension(&id.0, "x2", end.x);
                self.dimension(&id.0, "y2", end.y);
                self.dimension(&id.0, "radius", *radius);
                let (start_angle, end_angle) =
                    geometry_angles(segment).ok_or_else(|| EngineeringError::InvalidProfile {
                        reason: format!("profile segment {index} is not a valid arc"),
                    })?;
                self.dimension(&id.0, "start_angle", start_angle);
                self.dimension(&id.0, "end_angle", end_angle);
            }
        }
        Ok(())
    }

    fn lower_slot(&mut self, feature: &EngineeringFeature) -> Result<(), EngineeringError> {
        let center_x = self
            .view
            .slot_value(feature, FeatureParameterSlot::CenterX)?;
        let center_y = self
            .view
            .slot_value(feature, FeatureParameterSlot::CenterY)?;
        let overall_length = self
            .view
            .slot_value(feature, FeatureParameterSlot::Length)?;
        let width = self.view.slot_value(feature, FeatureParameterSlot::Width)?;
        let orientation = crate::slot::canonicalize_orientation_degrees(
            self.view
                .slot_value(feature, FeatureParameterSlot::Orientation)?,
        )?;
        let segments = crate::slot::slot_profile_segments(
            crate::Point2D::new(center_x, center_y),
            overall_length,
            width,
            orientation,
        )?;
        for (index, segment) in segments.iter().enumerate() {
            self.lower_profile_segment(
                feature,
                index,
                segment,
                crate::source_map::ProfileSegmentSemantic::SlotSegment,
            )?;
        }
        Ok(())
    }

    fn lower_circle(&mut self, feature: &EngineeringFeature) -> Result<(), EngineeringError> {
        let center_x = self
            .view
            .slot_value(feature, FeatureParameterSlot::CenterX)?;
        let center_y = self
            .view
            .slot_value(feature, FeatureParameterSlot::CenterY)?;
        let diameter = self
            .view
            .slot_value(feature, FeatureParameterSlot::Diameter)?;
        let id = self.entity(feature, "circle");
        self.position_dimensions(&id.0, center_x, center_y);
        self.dimension(&id.0, "diameter", diameter);
        Ok(())
    }

    fn lower_pattern(
        &mut self,
        feature: &EngineeringFeature,
        pattern: &HolePattern,
    ) -> Result<(), EngineeringError> {
        match pattern {
            // All pattern values come from the slot resolver (bound parameter
            // or literal), never from the embedded fields directly.
            HolePattern::Circular { .. } => {
                let center_x = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::CenterX)?;
                let center_y = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::CenterY)?;
                let diameter = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::Diameter)?;
                let pcd = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::PitchCircleDiameter)?;
                let count =
                    positive_count(self.view.slot_value(feature, FeatureParameterSlot::Count)?)?;
                let start_angle = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::StartAngle)?;

                if start_angle == 0.0 {
                    // The SCC circular array starts at 0° — reuse ArrayRelation
                    // (radius, not PCD).
                    let id = self.entity(feature, "circle");
                    self.position_dimensions(&id.0, center_x, center_y);
                    self.dimension(&id.0, "diameter", diameter);
                    let array_id = format!("{}_array", id.0);
                    self.arrays.push(ProposalArray {
                        id: array_id.clone(),
                        kind: "circular".into(),
                        count,
                        spacing: None,
                        element: id.0.clone(),
                        rows: None,
                        cols: None,
                        row_spacing: None,
                        col_spacing: None,
                        radius: Some(pcd / 2.0),
                        direction: None,
                    });
                    self.source_map
                        .record_pattern_array(feature.id().clone(), array_id.clone());
                    for index in 0..count {
                        self.source_map.record_feature_entity(
                            feature.id().clone(),
                            EntityId(format!("{}[{}]", array_id, index)),
                        );
                    }
                } else {
                    // SCC cannot express a start angle; expand deterministically
                    // in pure Rust (§18/§19), degrees → radians.
                    let radius = pcd / 2.0;
                    for index in 0..count {
                        let angle = (start_angle + f64::from(index) * 360.0 / f64::from(count))
                            .to_radians();
                        let x = center_x + radius * angle.cos();
                        let y = center_y + radius * angle.sin();
                        let entity_id = format!("{}_expanded_{}", feature.id(), index);
                        self.entities.push(ProposalEntity {
                            id: entity_id.clone(),
                            kind: "circle".into(),
                        });
                        self.position_dimensions(&entity_id, x, y);
                        self.dimension(&entity_id, "diameter", diameter);
                        self.source_map
                            .record_feature_entity(feature.id().clone(), EntityId(entity_id));
                    }
                }
                Ok(())
            }
            HolePattern::Grid { .. } => {
                let center_x = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::CenterX)?;
                let center_y = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::CenterY)?;
                let diameter = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::Diameter)?;
                let rows =
                    positive_count(self.view.slot_value(feature, FeatureParameterSlot::Rows)?)?;
                let columns = positive_count(
                    self.view
                        .slot_value(feature, FeatureParameterSlot::Columns)?,
                )?;
                let spacing_x = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::SpacingX)?;
                let spacing_y = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::SpacingY)?;

                // Guard against rows*columns overflowing u32 (which would panic
                // in debug and wrap in release).
                let total = rows
                    .checked_mul(columns)
                    .ok_or_else(|| EngineeringError::LoweringFailed {
                        message: format!(
                            "grid pattern {} has rows×columns ({rows}×{columns}) exceeding u32::MAX",
                            feature.id()
                        ),
                    })?;
                let id = self.entity(feature, "circle");
                let template_x = center_x - f64::from(columns - 1) * spacing_x / 2.0;
                let template_y = center_y - f64::from(rows - 1) * spacing_y / 2.0;
                self.position_dimensions(&id.0, template_x, template_y);
                self.dimension(&id.0, "diameter", diameter);
                let array_id = format!("{}_array", id.0);
                self.arrays.push(ProposalArray {
                    id: array_id.clone(),
                    kind: "grid".into(),
                    count: total,
                    spacing: None,
                    element: id.0.clone(),
                    rows: Some(rows),
                    cols: Some(columns),
                    row_spacing: Some(spacing_y),
                    col_spacing: Some(spacing_x),
                    radius: None,
                    direction: None,
                });
                self.source_map
                    .record_pattern_array(feature.id().clone(), array_id.clone());
                for index in 0..total {
                    self.source_map.record_feature_entity(
                        feature.id().clone(),
                        EntityId(format!("{}[{}]", array_id, index)),
                    );
                }
                Ok(())
            }
            HolePattern::Corner { .. } => {
                // Corner holes: one hole inset from each of the four corners of
                // the base profile. Expressed as a 2×2 grid array.
                let offset_x = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::OffsetX)?;
                let offset_y = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::OffsetY)?;
                let diameter = self
                    .view
                    .slot_value(feature, FeatureParameterSlot::Diameter)?;

                let (plate_center_x, plate_center_y, plate_width, plate_height) =
                    self.base_profile_frame()?;
                if offset_x < 0.0 || offset_y < 0.0 {
                    return Err(EngineeringError::LoweringFailed {
                        message: format!(
                            "corner pattern {} has negative offset (offset_x {offset_x}, offset_y {offset_y}); corner offsets are distances from the corners",
                            feature.id()
                        ),
                    });
                }
                let spacing_x = plate_width - 2.0 * offset_x;
                let spacing_y = plate_height - 2.0 * offset_y;
                if spacing_x <= 0.0 || spacing_y <= 0.0 {
                    return Err(EngineeringError::LoweringFailed {
                        message: format!(
                            "corner pattern {} needs offsets smaller than half the base profile (got offset_x {offset_x}, offset_y {offset_y} against {} × {plate_height})",
                            feature.id(), plate_width
                        ),
                    });
                }

                let id = self.entity(feature, "circle");
                let template_x = plate_center_x - plate_width / 2.0 + offset_x;
                let template_y = plate_center_y - plate_height / 2.0 + offset_y;
                self.position_dimensions(&id.0, template_x, template_y);
                self.dimension(&id.0, "diameter", diameter);
                let array_id = format!("{}_array", id.0);
                self.arrays.push(ProposalArray {
                    id: array_id.clone(),
                    kind: "grid".into(),
                    count: 4,
                    spacing: None,
                    element: id.0.clone(),
                    rows: Some(2),
                    cols: Some(2),
                    row_spacing: Some(spacing_y),
                    col_spacing: Some(spacing_x),
                    radius: None,
                    direction: None,
                });
                self.source_map
                    .record_pattern_array(feature.id().clone(), array_id.clone());
                for index in 0..4 {
                    self.source_map.record_feature_entity(
                        feature.id().clone(),
                        EntityId(format!("{}[{}]", array_id, index)),
                    );
                }
                Ok(())
            }
        }
    }

    /// The resolved center and width/height of the first `BaseProfile`
    /// feature, required to position corner patterns.
    fn base_profile_frame(&self) -> Result<(f64, f64, f64, f64), EngineeringError> {
        let profile = self
            .view
            .model()
            .features
            .values()
            .find(|feature| matches!(feature, EngineeringFeature::BaseProfile { .. }))
            .ok_or_else(|| EngineeringError::LoweringFailed {
                message: "corner pattern requires a base profile feature in the model".into(),
            })?;
        match profile {
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Rectangle { .. },
                ..
            } => Ok((
                0.0,
                0.0,
                self.view.slot_value(profile, FeatureParameterSlot::Width)?,
                self.view
                    .slot_value(profile, FeatureParameterSlot::Height)?,
            )),
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Circle { .. },
                ..
            } => Err(EngineeringError::LoweringFailed {
                message: "corner pattern requires a rectangular base profile".into(),
            }),
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::RoundedRectangle { .. },
                ..
            } => Ok((
                self.view
                    .slot_value(profile, FeatureParameterSlot::CenterX)?,
                self.view
                    .slot_value(profile, FeatureParameterSlot::CenterY)?,
                self.view.slot_value(profile, FeatureParameterSlot::Width)?,
                self.view
                    .slot_value(profile, FeatureParameterSlot::Height)?,
            )),
            EngineeringFeature::BaseProfile {
                profile: BaseProfile::Composite { .. } | BaseProfile::Polygon { .. },
                ..
            } => Err(EngineeringError::LoweringFailed {
                message:
                    "corner pattern requires a rectangular or rounded rectangular base profile"
                        .into(),
            }),
            _ => Err(EngineeringError::LoweringFailed {
                message: "corner pattern requires a base profile feature".into(),
            }),
        }
    }

    // ---- constraint lowering ----

    fn lower_constraint(
        &mut self,
        constraint: &EngineeringConstraint,
    ) -> Result<(), EngineeringError> {
        let id = constraint.id().clone();
        match constraint {
            EngineeringConstraint::Symmetric { source, target, .. } => {
                self.relation(
                    id,
                    ProposalRelation {
                        kind: "symmetric_about".into(),
                        subject: source.0.clone(),
                        reference: target.0.clone(),
                        axis: None,
                        value: None,
                    },
                );
            }
            EngineeringConstraint::Aligned { source, target, .. } => {
                self.relation(
                    id,
                    ProposalRelation {
                        kind: "aligned_with".into(),
                        subject: source.0.clone(),
                        reference: target.0.clone(),
                        axis: None,
                        value: None,
                    },
                );
            }
            EngineeringConstraint::Distance {
                source,
                target,
                value,
                ..
            } => {
                self.relation(
                    id,
                    ProposalRelation {
                        kind: "distance".into(),
                        subject: source.0.clone(),
                        reference: target.0.clone(),
                        axis: None,
                        value: Some(*value),
                    },
                );
            }
            EngineeringConstraint::Tangent { source, target, .. } => {
                self.relation(
                    id,
                    ProposalRelation {
                        kind: "tangent_to".into(),
                        subject: source.0.clone(),
                        reference: target.0.clone(),
                        axis: None,
                        value: None,
                    },
                );
            }
            EngineeringConstraint::Concentric { .. } => {
                return Err(EngineeringError::UnsupportedConstraintLowering {
                    constraint_id: id,
                    kind: "Concentric",
                });
            }
            EngineeringConstraint::Fixed { target, .. } => {
                // Anchor id is derived from the constraint id so it never
                // collides with the base-profile anchor (`{feature}_anchor`).
                let anchor_id = format!("{}_anchor", id.0);
                let index = self.anchors.len();
                self.anchors.push(ProposalAnchor {
                    id: anchor_id,
                    kind: "fixed_center".into(),
                    entity: Some(target.0.clone()),
                });
                self.anchor_index.insert(id, index);
            }
        }
        Ok(())
    }

    fn relation(&mut self, constraint: EngineeringConstraintId, relation: ProposalRelation) {
        let index = self.relations.len();
        self.relation_index.insert(constraint, index);
        self.relations.push(relation);
    }
}

fn positive_count(value: f64) -> Result<u32, EngineeringError> {
    if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > f64::from(u32::MAX) {
        return Err(EngineeringError::LoweringFailed {
            message: format!("pattern count must be a positive integer, got {}", value),
        });
    }
    Ok(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::EngineeringCommand;
    use crate::evaluator::ResolvedValue;
    use crate::executor::CommandExecutor;
    use crate::expression::Expression;
    use crate::feature::HoleDefinition;
    use crate::model::{Parameter, ParameterOrigin};
    use crate::parameter::ParameterValue;
    use crate::{FeatureId, PartId, Point2D};
    use scc_core::authority::AuthorityLevel;
    use std::collections::BTreeSet;

    fn fid(id: &str) -> FeatureId {
        FeatureId(id.into())
    }

    fn pid(id: &str) -> ParameterId {
        ParameterId(id.into())
    }

    fn new_part(name: &str) -> EngineeringModel {
        let mut model = EngineeringModel::new();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::CreatePart {
                id: PartId(name.into()),
                name: name.into(),
            },
        )
        .unwrap();
        model
    }

    fn base_profile(model: &mut EngineeringModel, width: f64, height: f64) {
        CommandExecutor::apply(
            model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: fid("plate"),
                    name: "plate".into(),
                    profile: crate::BaseProfile::Rectangle { width, height },
                },
            },
        )
        .unwrap();
    }

    fn add_base_profile(model: &mut EngineeringModel, profile: BaseProfile, id: &str) {
        CommandExecutor::apply(
            model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: fid(id),
                    name: id.into(),
                    profile,
                },
            },
        )
        .unwrap();
    }

    fn corner_pattern(model: &mut EngineeringModel, offset: f64, diameter: f64) {
        CommandExecutor::apply(
            model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("corner_holes"),
                    pattern: HolePattern::Corner {
                        offset_x: offset,
                        offset_y: offset,
                        hole: HoleDefinition { diameter },
                    },
                },
            },
        )
        .unwrap();
    }

    /// §30 fixture: mounting plate + four corner holes.
    fn mounting_plate_model() -> EngineeringModel {
        let mut model = new_part("mounting_plate");
        base_profile(&mut model, 120.0, 80.0);
        corner_pattern(&mut model, 15.0, 10.0);
        model
    }

    fn add_parameter(model: &mut EngineeringModel, id: &str, value: ParameterValue) {
        model
            .create_parameter(Parameter::new(
                ParameterId(id.into()),
                value,
                ParameterOrigin::User,
                AuthorityLevel::L0,
            ))
            .unwrap();
    }

    // ---- Level 1: feature → SCC semantic input ----

    #[test]
    fn base_profile_lowers_to_rectangle_with_dimensions_and_origin_anchor() {
        let model = mounting_plate_model();
        let result = lower_engineering_model(&model).unwrap();
        assert!(result
            .proposal
            .entities
            .iter()
            .any(|e| e.id == "plate" && e.kind == "rectangle"));
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|d| d.target == "plate" && d.kind == "width" && d.value == 120.0));
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|d| d.target == "plate" && d.kind == "height" && d.value == 80.0));
        assert!(result
            .proposal
            .anchors
            .iter()
            .any(|a| a.kind == "fixed_center" && a.entity.as_deref() == Some("plate")));
        assert_eq!(
            result.source_map.entities_for(&fid("plate")),
            BTreeSet::from([EntityId("plate".into())])
        );
    }

    #[test]
    fn rounded_rectangle_lowers_to_exact_line_and_arc_geometry() {
        let mut model = new_part("rounded");
        add_base_profile(
            &mut model,
            BaseProfile::RoundedRectangle {
                center: Point2D::new(10.0, 20.0),
                width: 80.0,
                height: 40.0,
                corner_radius: 8.0,
            },
            "rounded",
        );
        let result = lower_engineering_model(&model).unwrap();
        assert_eq!(
            result
                .proposal
                .entities
                .iter()
                .filter(|entity| entity.id.starts_with("rounded_segment_"))
                .count(),
            8
        );
        assert_eq!(result.source_map.profile_segments().len(), 8);
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|dimension| dimension.kind == "start_angle"));
        let (_, geometry) = lower_and_solve(&model).unwrap();
        assert_eq!(geometry.entities.len(), 8);
        assert!(geometry.entities.values().any(|entity| matches!(
            entity.primitive,
            scc_geometry::GeometryPrimitive::Arc { .. }
        )));
    }

    #[test]
    fn composite_preserves_order_provenance_and_geometry() {
        let mut model = new_part("composite");
        add_base_profile(
            &mut model,
            BaseProfile::Composite {
                segments: vec![
                    ProfileSegment::Line {
                        start: Point2D::new(0.0, 0.0),
                        end: Point2D::new(20.0, 0.0),
                    },
                    ProfileSegment::Arc {
                        start: Point2D::new(20.0, 0.0),
                        end: Point2D::new(0.0, 20.0),
                        center: Point2D::new(0.0, 0.0),
                        radius: 20.0,
                        direction: crate::ArcDirection::CounterClockwise,
                    },
                    ProfileSegment::Line {
                        start: Point2D::new(0.0, 20.0),
                        end: Point2D::new(0.0, 0.0),
                    },
                ],
            },
            "composite",
        );
        let result = lower_engineering_model(&model).unwrap();
        let ids: Vec<_> = result
            .proposal
            .entities
            .iter()
            .filter(|entity| entity.id.starts_with("composite_segment_"))
            .map(|entity| entity.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "composite_segment_0",
                "composite_segment_1",
                "composite_segment_2"
            ]
        );
        assert_eq!(
            result
                .source_map
                .profile_segment(&scc_core::ids::EntityId("composite_segment_1".into()))
                .unwrap()
                .segment_index,
            1
        );
        let (_, geometry) = lower_and_solve(&model).unwrap();
        assert!(geometry.entities.values().any(|entity| matches!(
            entity.primitive,
            scc_geometry::GeometryPrimitive::Arc { .. }
        )));
    }

    #[test]
    fn circular_base_lowers_with_typed_center_diameter_and_expression_binding() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: fid("disk"),
                    name: "disk".into(),
                    profile: crate::BaseProfile::Circle {
                        center: Point2D::new(30.0, -20.0),
                        diameter: 100.0,
                    },
                },
            },
        )
        .unwrap();
        add_parameter(&mut model, "pcd", ParameterValue::Scalar(120.0));
        add_parameter(&mut model, "outer_diameter", ParameterValue::Unresolved);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: pid("outer_diameter"),
                expression: Expression::Add {
                    left: Box::new(Expression::Reference {
                        parameter: pid("pcd"),
                    }),
                    right: Box::new(Expression::Constant { value: 40.0 }),
                },
            },
        )
        .unwrap();
        model
            .bind_feature_parameter(
                fid("disk"),
                FeatureParameterSlot::Diameter,
                pid("outer_diameter"),
            )
            .unwrap();

        let result = lower_engineering_model(&model).unwrap();
        assert!(result
            .proposal
            .entities
            .iter()
            .any(|entity| entity.id == "disk" && entity.kind == "circle"));
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|dimension| dimension.target == "disk"
                && dimension.kind == "x"
                && dimension.value == 30.0));
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|dimension| dimension.target == "disk"
                && dimension.kind == "y"
                && dimension.value == -20.0));
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|dimension| dimension.target == "disk"
                && dimension.kind == "diameter"
                && dimension.value == 160.0));

        let (_result, geometry) = lower_and_solve(&model).unwrap();
        assert!(matches!(
            geometry.entities[&EntityId("disk".into())].primitive,
            scc_geometry::model::GeometryPrimitive::Circle { center, radius }
                if center.x == 30.0 && center.y == -20.0 && radius == 80.0
        ));
    }

    #[test]
    fn circular_base_invalid_diameter_fails_closed() {
        let mut model = new_part("p");
        for (id, diameter) in [("zero", 0.0), ("negative", -1.0), ("nan", f64::NAN)] {
            let error = CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::BaseProfile {
                        id: fid(id),
                        name: id.into(),
                        profile: crate::BaseProfile::Circle {
                            center: Point2D::new(0.0, 0.0),
                            diameter,
                        },
                    },
                },
            )
            .unwrap_err();
            assert!(matches!(
                error,
                EngineeringError::NonPositiveLength { .. }
                    | EngineeringError::NonFiniteValue { .. }
            ));
        }
    }

    #[test]
    fn hole_lowers_to_circle_with_explicit_position_and_diameter() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: fid("center_hole"),
                    diameter: 20.0,
                },
            },
        )
        .unwrap();
        let result = lower_engineering_model(&model).unwrap();
        assert!(result
            .proposal
            .entities
            .iter()
            .any(|e| e.id == "center_hole" && e.kind == "circle"));
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|d| d.target == "center_hole" && d.kind == "x" && d.value == 0.0));
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|d| d.target == "center_hole" && d.kind == "y" && d.value == 0.0));
        assert!(result
            .proposal
            .dimensions
            .iter()
            .any(|d| d.target == "center_hole" && d.kind == "diameter" && d.value == 20.0));
    }

    #[test]
    fn circular_pattern_lowers_to_circular_array_with_radius_pcd_over_two() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("bolt_pattern"),
                    pattern: HolePattern::Circular {
                        center: Point2D::new(0.0, 0.0),
                        pitch_circle_diameter: 126.0,
                        count: 8,
                        start_angle: 0.0,
                        hole: HoleDefinition { diameter: 10.0 },
                    },
                },
            },
        )
        .unwrap();
        let result = lower_engineering_model(&model).unwrap();
        let array = result
            .proposal
            .arrays
            .iter()
            .find(|a| a.kind == "circular")
            .expect("circular array");
        assert_eq!(array.count, 8);
        assert_eq!(array.radius, Some(63.0));
        assert_eq!(array.element, "bolt_pattern");
        // Model still holds ONE pattern feature, never eight holes.
        assert_eq!(
            result.source_map.entities_for(&fid("bolt_pattern")).len(),
            9
        );
    }

    #[test]
    fn corner_pattern_lowers_to_grid_array_covering_the_plate() {
        let model = mounting_plate_model();
        let result = lower_engineering_model(&model).unwrap();
        let array = result
            .proposal
            .arrays
            .iter()
            .find(|a| a.kind == "grid")
            .expect("grid array for corner pattern");
        assert_eq!(array.rows, Some(2));
        assert_eq!(array.cols, Some(2));
        assert_eq!(array.count, 4);
        assert_eq!(array.row_spacing, Some(50.0));
        assert_eq!(array.col_spacing, Some(90.0));
        let template = result
            .proposal
            .dimensions
            .iter()
            .find(|d| d.target == "corner_holes" && d.kind == "x")
            .unwrap();
        assert_eq!(template.value, -45.0);
    }

    // ---- Level 2: constraint → SCC semantic input ----

    #[test]
    fn supported_constraints_lower_to_relations_and_anchor() {
        let mut model = mounting_plate_model();
        for constraint in [
            EngineeringConstraint::Symmetric {
                id: EngineeringConstraintId("c1".into()),
                source: fid("corner_holes"),
                target: fid("corner_holes"),
            },
            EngineeringConstraint::Aligned {
                id: EngineeringConstraintId("c2".into()),
                source: fid("corner_holes"),
                target: fid("corner_holes"),
            },
            EngineeringConstraint::Distance {
                id: EngineeringConstraintId("c3".into()),
                source: fid("corner_holes"),
                target: fid("corner_holes"),
                value: 42.0,
            },
            EngineeringConstraint::Tangent {
                id: EngineeringConstraintId("c4".into()),
                source: fid("corner_holes"),
                target: fid("corner_holes"),
            },
        ] {
            CommandExecutor::apply(
                &mut model,
                &EngineeringCommand::AddConstraint { constraint },
            )
            .unwrap();
        }
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddConstraint {
                constraint: EngineeringConstraint::Fixed {
                    id: EngineeringConstraintId("c5".into()),
                    target: fid("plate"),
                },
            },
        )
        .unwrap();

        let result = lower_engineering_model(&model).unwrap();
        assert!(result
            .proposal
            .relations
            .iter()
            .any(|r| r.kind == "symmetric_about"));
        assert!(result
            .proposal
            .relations
            .iter()
            .any(|r| r.kind == "aligned_with"));
        assert!(result
            .proposal
            .relations
            .iter()
            .any(|r| r.kind == "distance" && r.value == Some(42.0)));
        assert!(result
            .proposal
            .relations
            .iter()
            .any(|r| r.kind == "tangent_to"));
        assert!(result
            .proposal
            .anchors
            .iter()
            .any(|a| a.kind == "fixed_center" && a.entity.as_deref() == Some("plate")));
        // Source map predicts deterministic SCC constraint ids.
        assert_eq!(
            result
                .source_map
                .constraint_ids_for(&EngineeringConstraintId("c1".into())),
            BTreeSet::from([ConstraintId("c_1".into())])
        );
    }

    #[test]
    fn concentric_constraint_fails_closed() {
        let mut model = mounting_plate_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddConstraint {
                constraint: EngineeringConstraint::Concentric {
                    id: EngineeringConstraintId("c1".into()),
                    source: fid("corner_holes"),
                    target: fid("plate"),
                },
            },
        )
        .unwrap();
        assert!(matches!(
            lower_engineering_model(&model).unwrap_err(),
            EngineeringError::UnsupportedConstraintLowering { .. }
        ));
    }

    // ---- Level 3: full pipeline to GeometryModel ----

    #[test]
    fn mounting_plate_solves_to_plate_and_four_corner_holes() {
        let model = mounting_plate_model();
        let (_result, geometry) = lower_and_solve(&model).unwrap();

        match &geometry.entities[&EntityId("plate".into())].primitive {
            scc_geometry::model::GeometryPrimitive::Rectangle {
                origin,
                width,
                height,
            } => {
                assert_eq!(*width, 120.0);
                assert_eq!(*height, 80.0);
                // Center at origin → lower-left origin at (-60, -40).
                assert!((origin.x + 60.0).abs() < 1.0e-9);
                assert!((origin.y + 40.0).abs() < 1.0e-9);
            }
            _ => panic!("expected rectangle plate"),
        }

        let expected = [(-45.0, -25.0), (45.0, -25.0), (-45.0, 25.0), (45.0, 25.0)];
        for (index, (ex, ey)) in expected.iter().enumerate() {
            let key = EntityId(format!("corner_holes_array[{}]", index));
            let circle = geometry
                .entities
                .get(&key)
                .unwrap_or_else(|| panic!("missing derived hole {}", key.0));
            match &circle.primitive {
                scc_geometry::model::GeometryPrimitive::Circle { center, radius } => {
                    assert!((radius - 5.0).abs() < 1.0e-9, "radius {radius}");
                    assert!((center.x - ex).abs() < 1.0e-9, "x {} != {}", center.x, ex);
                    assert!((center.y - ey).abs() < 1.0e-9, "y {} != {}", center.y, ey);
                }
                _ => panic!("expected circle"),
            }
        }
    }

    #[test]
    fn circular_pattern_solves_to_eight_holes_on_the_pitch_circle() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("bolt_pattern"),
                    pattern: HolePattern::Circular {
                        center: Point2D::new(0.0, 0.0),
                        pitch_circle_diameter: 126.0,
                        count: 8,
                        start_angle: 0.0,
                        hole: HoleDefinition { diameter: 10.0 },
                    },
                },
            },
        )
        .unwrap();
        let (_result, geometry) = lower_and_solve(&model).unwrap();

        for index in 0..8 {
            let key = EntityId(format!("bolt_pattern_array[{}]", index));
            let circle = geometry
                .entities
                .get(&key)
                .unwrap_or_else(|| panic!("missing {}", key.0));
            match &circle.primitive {
                scc_geometry::model::GeometryPrimitive::Circle { center, radius } => {
                    let angle = std::f64::consts::TAU * f64::from(index) / 8.0;
                    let expected_x = 63.0 * angle.cos();
                    let expected_y = 63.0 * angle.sin();
                    assert!((radius - 5.0).abs() < 1.0e-9);
                    assert!(
                        (center.x - expected_x).abs() < 1.0e-6,
                        "x {} vs {expected_x}",
                        center.x
                    );
                    assert!(
                        (center.y - expected_y).abs() < 1.0e-6,
                        "y {} vs {expected_y}",
                        center.y
                    );
                }
                _ => panic!("expected circle"),
            }
        }
    }

    #[test]
    fn grid_pattern_solves_to_rows_times_columns_circles() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("grid_holes"),
                    pattern: HolePattern::Grid {
                        rows: 2,
                        columns: 2,
                        spacing_x: 30.0,
                        spacing_y: 40.0,
                        hole: HoleDefinition { diameter: 6.0 },
                    },
                },
            },
        )
        .unwrap();
        let (_result, geometry) = lower_and_solve(&model).unwrap();
        let mut circle_count = 0;
        for entity in geometry.entities.values() {
            if matches!(
                entity.primitive,
                scc_geometry::model::GeometryPrimitive::Circle { .. }
            ) {
                circle_count += 1;
            }
        }
        assert_eq!(circle_count, 4);
    }

    // ---- Level 4: parameter update → re-lower → geometry change ----

    #[test]
    fn parameter_update_moves_holes_after_relowering() {
        let mut model = mounting_plate_model();
        add_parameter(&mut model, "plate_width", ParameterValue::Scalar(120.0));
        add_parameter(&mut model, "plate_height", ParameterValue::Scalar(80.0));
        add_parameter(&mut model, "offset", ParameterValue::Scalar(15.0));
        add_parameter(&mut model, "right_hole_x", ParameterValue::Unresolved);
        model
            .bind_feature_parameter(
                fid("plate"),
                FeatureParameterSlot::Width,
                pid("plate_width"),
            )
            .unwrap();
        model
            .bind_feature_parameter(
                fid("plate"),
                FeatureParameterSlot::Height,
                pid("plate_height"),
            )
            .unwrap();
        model
            .bind_feature_parameter(
                fid("corner_holes"),
                FeatureParameterSlot::OffsetX,
                pid("offset"),
            )
            .unwrap();
        model
            .bind_feature_parameter(
                fid("corner_holes"),
                FeatureParameterSlot::OffsetY,
                pid("offset"),
            )
            .unwrap();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetExpression {
                target: pid("right_hole_x"),
                expression: Expression::Subtract {
                    left: Box::new(Expression::Divide {
                        left: Box::new(Expression::Reference {
                            parameter: pid("plate_width"),
                        }),
                        right: Box::new(Expression::Constant { value: 2.0 }),
                    }),
                    right: Box::new(Expression::Reference {
                        parameter: pid("offset"),
                    }),
                },
            },
        )
        .unwrap();

        // §31: right_hole_x = plate_width/2 − offset = 45.
        assert_eq!(
            model.evaluate_parameter(&pid("right_hole_x")).unwrap(),
            ResolvedValue::Scalar(45.0)
        );
        let (_result, geometry) = lower_and_solve(&model).unwrap();
        let corner = geometry.entities[&EntityId("corner_holes_array[0]".into())].clone();
        match &corner.primitive {
            scc_geometry::model::GeometryPrimitive::Circle { center, .. } => {
                assert!((center.x + 45.0).abs() < 1.0e-9);
            }
            _ => panic!("expected circle"),
        }

        // Change plate_width → right_hole_x = 65 and the hole moves.
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::SetParameter {
                target: pid("plate_width"),
                value: ParameterValue::Scalar(160.0),
            },
        )
        .unwrap();
        assert_eq!(
            model.evaluate_parameter(&pid("right_hole_x")).unwrap(),
            ResolvedValue::Scalar(65.0)
        );
        let (_result, geometry) = lower_and_solve(&model).unwrap();
        let corner = geometry.entities[&EntityId("corner_holes_array[0]".into())].clone();
        match &corner.primitive {
            scc_geometry::model::GeometryPrimitive::Circle { center, .. } => {
                assert!((center.x + 65.0).abs() < 1.0e-9);
            }
            _ => panic!("expected circle"),
        }
    }

    // ---- Level 5: unsupported / unresolved fail closed ----

    #[test]
    fn fillet_fails_closed() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Fillet {
                    id: fid("f1"),
                    radius: 5.0,
                },
            },
        )
        .unwrap();
        assert!(matches!(
            lower_engineering_model(&model).unwrap_err(),
            EngineeringError::UnsupportedFeatureLowering { .. }
        ));
    }

    #[test]
    fn slot_lowers_to_two_lines_and_two_directed_arcs() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Slot {
                    id: fid("s1"),
                    center: crate::Point2D::new(10.0, -5.0),
                    length: 40.0,
                    width: 12.0,
                    orientation_degrees: 45.0,
                },
            },
        )
        .unwrap();
        let result = lower_engineering_model(&model).unwrap();
        assert_eq!(result.source_map.profile_segments().len(), 4);
        assert!(result.source_map.profile_segments().values().all(
            |source| source.semantic == crate::source_map::ProfileSegmentSemantic::SlotSegment
        ));
        let (_, geometry) = lower_and_solve(&model).unwrap();
        assert_eq!(geometry.entities.len(), 4);
        assert_eq!(
            geometry
                .entities
                .values()
                .filter(|entity| matches!(
                    entity.primitive,
                    scc_geometry::GeometryPrimitive::Arc { .. }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn construction_circle_lowers_as_reference_circle() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::ConstructionCircle {
                    id: fid("cc_1"),
                    diameter: 60.0,
                },
            },
        )
        .unwrap();
        let result = lower_engineering_model(&model).unwrap();
        assert!(result
            .proposal
            .entities
            .iter()
            .any(|e| e.id == "cc_1" && e.kind == "circle"));
    }

    #[test]
    fn unresolved_bound_parameter_fails_closed_with_feature_slot_parameter() {
        let mut model = new_part("p");
        base_profile(&mut model, 120.0, 80.0);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("bolt_pattern"),
                    pattern: HolePattern::Circular {
                        center: Point2D::new(0.0, 0.0),
                        pitch_circle_diameter: 126.0,
                        count: 8,
                        start_angle: 0.0,
                        hole: HoleDefinition { diameter: 10.0 },
                    },
                },
            },
        )
        .unwrap();
        add_parameter(&mut model, "bolt_pcd", ParameterValue::Unresolved);
        model
            .bind_feature_parameter(
                fid("bolt_pattern"),
                FeatureParameterSlot::PitchCircleDiameter,
                pid("bolt_pcd"),
            )
            .unwrap();

        let err = lower_engineering_model(&model).unwrap_err();
        match err {
            EngineeringError::UnresolvedRequiredParameter {
                feature_id,
                slot,
                parameter_id,
            } => {
                assert_eq!(feature_id, fid("bolt_pattern"));
                assert_eq!(slot, FeatureParameterSlot::PitchCircleDiameter);
                assert_eq!(parameter_id, pid("bolt_pcd"));
            }
            _ => panic!("expected UnresolvedRequiredParameter, got {err}"),
        }
        // The model is unchanged.
        assert!(model.features.contains_key(&fid("bolt_pattern")));
    }

    // ---- Level 3b: reuse existing SVG/DXF export (no new drawing engine) ----

    #[test]
    fn solved_geometry_exports_through_existing_svg_and_dxf() {
        let model = mounting_plate_model();
        let (_result, geometry) = lower_and_solve(&model).unwrap();
        let svg = scc_geometry::export_svg(&geometry).unwrap();
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("id=\"plate\""));
        let dxf = scc_geometry::export_dxf(&geometry).unwrap();
        assert!(dxf.contains("SECTION"));
    }

    // ---- Level 6: source map ----

    #[test]
    fn source_map_tracks_feature_to_entities_strongly() {
        let model = mounting_plate_model();
        let result = lower_engineering_model(&model).unwrap();
        let entities = result.source_map.entities_for(&fid("corner_holes"));
        assert!(entities.contains(&EntityId("corner_holes".into())));
        for index in 0..4 {
            assert!(entities.contains(&EntityId(format!("corner_holes_array[{}]", index))));
        }
        assert_eq!(
            result.source_map.pattern_array(&fid("corner_holes")),
            Some("corner_holes_array")
        );
    }

    // ---- Level 7: determinism ----

    #[test]
    fn lowering_is_deterministic_over_many_runs() {
        let model = mounting_plate_model();
        let reference = lower_engineering_model(&model).unwrap();
        let reference_json = serde_json::to_string(&reference.proposal).unwrap();
        let reference_geometry = lower_and_solve(&model).unwrap().1;
        let reference_geometry_json = serde_json::to_string(&reference_geometry).unwrap();

        for _ in 0..100 {
            let candidate = lower_engineering_model(&model).unwrap();
            assert_eq!(candidate.proposal, reference.proposal);
            assert_eq!(candidate.source_map, reference.source_map);
            assert_eq!(
                serde_json::to_string(&candidate.proposal).unwrap(),
                reference_json
            );

            let geometry = lower_and_solve(&model).unwrap().1;
            assert_eq!(geometry, reference_geometry);
            assert_eq!(
                serde_json::to_string(&geometry).unwrap(),
                reference_geometry_json
            );
        }
    }

    // ---- regression tests from the adversarial review ----

    #[test]
    fn circular_pattern_at_non_origin_center_survives_lowering() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("bolt_pattern"),
                    pattern: HolePattern::Circular {
                        center: Point2D::new(40.0, 30.0),
                        pitch_circle_diameter: 126.0,
                        count: 8,
                        start_angle: 0.0,
                        hole: HoleDefinition { diameter: 10.0 },
                    },
                },
            },
        )
        .unwrap();
        let result = lower_engineering_model(&model).unwrap();
        let x = result
            .proposal
            .dimensions
            .iter()
            .find(|d| d.target == "bolt_pattern" && d.kind == "x")
            .unwrap();
        let y = result
            .proposal
            .dimensions
            .iter()
            .find(|d| d.target == "bolt_pattern" && d.kind == "y")
            .unwrap();
        assert_eq!(x.value, 40.0);
        assert_eq!(y.value, 30.0);
    }

    #[test]
    fn grid_overflow_fails_closed_instead_of_panicking() {
        let mut model = new_part("p");
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("grid_holes"),
                    pattern: HolePattern::Grid {
                        rows: u32::MAX,
                        columns: u32::MAX,
                        spacing_x: 1.0,
                        spacing_y: 1.0,
                        hole: HoleDefinition { diameter: 1.0 },
                    },
                },
            },
        )
        .unwrap();
        assert!(matches!(
            lower_engineering_model(&model).unwrap_err(),
            EngineeringError::LoweringFailed { .. }
        ));
    }

    #[test]
    fn corner_negative_offset_fails_closed() {
        let mut model = new_part("p");
        base_profile(&mut model, 120.0, 80.0);
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("corner_holes"),
                    pattern: HolePattern::Corner {
                        offset_x: -5.0,
                        offset_y: 15.0,
                        hole: HoleDefinition { diameter: 10.0 },
                    },
                },
            },
        )
        .unwrap();
        assert!(matches!(
            lower_engineering_model(&model).unwrap_err(),
            EngineeringError::LoweringFailed { .. }
        ));
    }

    #[test]
    fn fixed_constraint_anchor_id_does_not_collide_with_base_profile_anchor() {
        let mut model = mounting_plate_model();
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddConstraint {
                constraint: EngineeringConstraint::Fixed {
                    id: EngineeringConstraintId("pin".into()),
                    target: fid("plate"),
                },
            },
        )
        .unwrap();
        let result = lower_engineering_model(&model).unwrap();
        let mut anchor_ids = result
            .proposal
            .anchors
            .iter()
            .map(|a| a.id.clone())
            .collect::<Vec<_>>();
        anchor_ids.sort();
        anchor_ids.dedup();
        assert_eq!(
            anchor_ids.len(),
            result.proposal.anchors.len(),
            "anchor ids must be unique"
        );
    }

    #[test]
    fn slot_value_prefers_binding_over_literal_and_origin_default() {
        let mut model = EngineeringModel::new();
        model
            .create_parameter(crate::model::Parameter::new(
                ParameterId("plate_width".into()),
                crate::parameter::ParameterValue::Scalar(120.0),
                crate::model::ParameterOrigin::User,
                scc_core::authority::AuthorityLevel::L0,
            ))
            .unwrap();
        crate::executor::CommandExecutor::apply(
            &mut model,
            &crate::command::EngineeringCommand::CreatePart {
                id: crate::PartId("p".into()),
                name: "p".into(),
            },
        )
        .unwrap();
        let profile = EngineeringFeature::BaseProfile {
            id: FeatureId("plate".into()),
            name: "plate".into(),
            profile: crate::BaseProfile::Rectangle {
                width: 80.0,
                height: 80.0,
            },
        };
        crate::executor::CommandExecutor::apply(
            &mut model,
            &crate::command::EngineeringCommand::AddFeature {
                feature: profile.clone(),
            },
        )
        .unwrap();
        model
            .bind_feature_parameter(
                FeatureId("plate".into()),
                FeatureParameterSlot::Width,
                ParameterId("plate_width".into()),
            )
            .unwrap();

        let view = ResolvedEngineeringView::build(&model).unwrap();
        // Bound parameter wins over the literal.
        assert_eq!(
            view.slot_value(&profile, FeatureParameterSlot::Width)
                .unwrap(),
            120.0
        );
        // Unbound slot falls back to the literal.
        assert_eq!(
            view.slot_value(&profile, FeatureParameterSlot::Height)
                .unwrap(),
            80.0
        );
        // Position slots default to origin.
        let hole = EngineeringFeature::Hole {
            id: FeatureId("h".into()),
            diameter: 10.0,
        };
        assert_eq!(
            view.slot_value(&hole, FeatureParameterSlot::CenterX)
                .unwrap(),
            0.0
        );
    }
}
