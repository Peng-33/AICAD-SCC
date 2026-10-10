//! Versioned AICAD project source files.
//!
//! This crate owns only editable project metadata and the authoritative
//! `EngineeringModel`.  Geometry, spatial lowering, drawings, solid specs,
//! STEP artifacts, and UI session state are deliberately outside this schema.

use scc_engineering::EngineeringModel;
use serde::{Deserialize, Serialize};

pub const AICAD_PROJECT_SCHEMA_VERSION: u32 = 2;
const AICAD_PROJECT_LEGACY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMetadata {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AicadProject {
    pub schema_version: u32,
    pub metadata: ProjectMetadata,
    pub engineering: EngineeringModel,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("unsupported project schema version; supported versions are 1 and {AICAD_PROJECT_SCHEMA_VERSION}")]
    UnsupportedSchemaVersion,
    #[error("project metadata name must be non-empty")]
    InvalidName,
    #[error("project engineering model is invalid: {0}")]
    InvalidEngineeringModel(String),
    #[error("project JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl AicadProject {
    pub fn new(
        metadata: ProjectMetadata,
        engineering: EngineeringModel,
    ) -> Result<Self, ProjectError> {
        if metadata.name.trim().is_empty() || metadata.name != metadata.name.trim() {
            return Err(ProjectError::InvalidName);
        }
        engineering
            .validate_model()
            .map_err(|error| ProjectError::InvalidEngineeringModel(error.to_string()))?;
        let schema_version = if !engineering.feature_order.is_empty()
            || engineering.features.values().any(|feature| {
                matches!(
                    feature,
                    scc_engineering::feature::EngineeringFeature::Parametric { .. }
                )
            }) {
            AICAD_PROJECT_SCHEMA_VERSION
        } else {
            AICAD_PROJECT_LEGACY_SCHEMA_VERSION
        };
        Ok(Self {
            schema_version,
            metadata,
            engineering,
        })
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        match self.schema_version {
            AICAD_PROJECT_LEGACY_SCHEMA_VERSION
                if !self.engineering.feature_order.is_empty()
                    || self.engineering.features.values().any(|feature| {
                        matches!(
                            feature,
                            scc_engineering::feature::EngineeringFeature::Parametric { .. }
                        )
                    }) =>
            {
                return Err(ProjectError::UnsupportedSchemaVersion);
            }
            AICAD_PROJECT_LEGACY_SCHEMA_VERSION | AICAD_PROJECT_SCHEMA_VERSION => {}
            _ => return Err(ProjectError::UnsupportedSchemaVersion),
        }
        if self.metadata.name.trim().is_empty() || self.metadata.name != self.metadata.name.trim() {
            return Err(ProjectError::InvalidName);
        }
        self.engineering
            .validate_model()
            .map_err(|error| ProjectError::InvalidEngineeringModel(error.to_string()))
    }

    pub fn to_json(&self) -> Result<String, ProjectError> {
        self.validate()?;
        Ok(format!("{}\n", serde_json::to_string_pretty(self)?))
    }

    pub fn from_json(input: &str) -> Result<Self, ProjectError> {
        let project: Self = serde_json::from_str(input)?;
        project.validate()?;
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::authority::AuthorityLevel;
    use scc_engineering::command::EngineeringCommand;
    use scc_engineering::executor::CommandExecutor;
    use scc_engineering::feature::{
        EngineeringFeature, HoleDefinition, HolePattern, ProjectionMethod,
    };
    use scc_engineering::spatial::{HoleExtent, HoleStartFace, SourcedLength};
    use scc_engineering::specification::{
        DecimalPlaces, DimensionalSpecification, DimensionalTolerance, EngineeringDimensionRef,
    };
    use scc_engineering::{FeatureId, Parameter, ParameterId, ParameterOrigin, PartId, Point2D};

    fn model() -> EngineeringModel {
        let mut model = EngineeringModel::new();
        for command in [
            EngineeringCommand::CreatePart {
                id: PartId("plate".into()),
                name: "Plate".into(),
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: FeatureId("base".into()),
                    name: "Base".into(),
                    profile: scc_engineering::BaseProfile::Rectangle {
                        width: 120.0,
                        height: 80.0,
                    },
                },
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: FeatureId("corners".into()),
                    pattern: HolePattern::Corner {
                        offset_x: 15.0,
                        offset_y: 15.0,
                        hole: HoleDefinition { diameter: 8.0 },
                    },
                },
            },
            EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Literal { value: 10.0 }),
            },
            EngineeringCommand::SetHoleExtent {
                target: FeatureId("corners".into()),
                extent: Some(HoleExtent::ThroughAll),
            },
        ] {
            CommandExecutor::apply(&mut model, &command).unwrap();
        }
        model.drawing.set(
            scc_engineering::feature::DrawingProperty::ProjectionMethod {
                method: ProjectionMethod::ThirdAngle,
            },
        );
        model
            .create_parameter(Parameter::new(
                ParameterId("thickness_driver".into()),
                scc_engineering::parameter::ParameterValue::Scalar(10.0),
                ParameterOrigin::User,
                AuthorityLevel::L0,
            ))
            .unwrap();
        model
    }

    #[test]
    fn project_roundtrip_preserves_circular_base_source_and_center() {
        let mut source = EngineeringModel::new();
        for command in [
            EngineeringCommand::CreatePart {
                id: PartId("flange".into()),
                name: "Flange".into(),
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: FeatureId("base".into()),
                    name: "Base".into(),
                    profile: scc_engineering::BaseProfile::Circle {
                        center: Point2D::new(30.0, -20.0),
                        diameter: 160.0,
                    },
                },
            },
            EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Literal { value: 20.0 }),
            },
        ] {
            CommandExecutor::apply(&mut source, &command).unwrap();
        }
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Circular Flange".into(),
                description: None,
            },
            source,
        )
        .unwrap();
        let json = project.to_json().unwrap();
        assert!(json.contains("\"diameter\""));
        assert!(json.contains("\"diameter\": 160.0"));
        assert!(json.contains("\"center\""));
        let opened = AicadProject::from_json(&json).unwrap();
        assert_eq!(opened, project);
        assert!(matches!(
            opened.engineering.features.get(&FeatureId("base".into())),
            Some(EngineeringFeature::BaseProfile {
                profile: scc_engineering::BaseProfile::Circle { center, diameter },
                ..
            }) if center.x == 30.0 && center.y == -20.0 && *diameter == 160.0
        ));
    }

    #[test]
    fn project_roundtrip_preserves_rounded_base_source_and_center() {
        let mut source = EngineeringModel::new();
        for command in [
            EngineeringCommand::CreatePart {
                id: PartId("rounded".into()),
                name: "Rounded".into(),
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: FeatureId("base".into()),
                    name: "Base".into(),
                    profile: scc_engineering::BaseProfile::RoundedRectangle {
                        center: Point2D::new(10.0, -5.0),
                        width: 120.0,
                        height: 80.0,
                        corner_radius: 10.0,
                    },
                },
            },
            EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Literal { value: 20.0 }),
            },
        ] {
            CommandExecutor::apply(&mut source, &command).unwrap();
        }
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Rounded Base".into(),
                description: None,
            },
            source,
        )
        .unwrap();
        let json = project.to_json().unwrap();
        assert!(json.contains("rounded_rectangle"));
        assert!(json.contains("corner_radius"));
        assert_eq!(AicadProject::from_json(&json).unwrap(), project);
    }

    #[test]
    fn project_roundtrip_preserves_composite_segment_order_and_arc_direction() {
        use scc_engineering::{ArcDirection, ProfileSegment};
        let closed = vec![
            ProfileSegment::Line {
                start: Point2D::new(20.0, -20.0),
                end: Point2D::new(-40.0, -20.0),
            },
            ProfileSegment::Arc {
                start: Point2D::new(-40.0, -20.0),
                end: Point2D::new(-60.0, 0.0),
                center: Point2D::new(-40.0, 0.0),
                radius: 20.0,
                direction: ArcDirection::Clockwise,
            },
            ProfileSegment::Arc {
                start: Point2D::new(-60.0, 0.0),
                end: Point2D::new(-40.0, 20.0),
                center: Point2D::new(-40.0, 0.0),
                radius: 20.0,
                direction: ArcDirection::Clockwise,
            },
            ProfileSegment::Line {
                start: Point2D::new(-40.0, 20.0),
                end: Point2D::new(20.0, 20.0),
            },
            ProfileSegment::Arc {
                start: Point2D::new(20.0, 20.0),
                end: Point2D::new(40.0, 0.0),
                center: Point2D::new(20.0, 0.0),
                radius: 20.0,
                direction: ArcDirection::Clockwise,
            },
            ProfileSegment::Arc {
                start: Point2D::new(40.0, 0.0),
                end: Point2D::new(20.0, -20.0),
                center: Point2D::new(20.0, 0.0),
                radius: 20.0,
                direction: ArcDirection::Clockwise,
            },
        ];
        let mut source = EngineeringModel::new();
        for command in [
            EngineeringCommand::CreatePart {
                id: PartId("composite".into()),
                name: "Composite".into(),
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: FeatureId("base".into()),
                    name: "Base".into(),
                    profile: scc_engineering::BaseProfile::Composite {
                        segments: closed.clone(),
                    },
                },
            },
            EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Literal { value: 8.0 }),
            },
        ] {
            CommandExecutor::apply(&mut source, &command).unwrap();
        }
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Composite Profile".into(),
                description: None,
            },
            source,
        )
        .unwrap();
        let json = project.to_json().unwrap();
        assert!(json.contains("composite"));
        assert!(json.contains("clockwise"));
        assert!(!json.contains("GeometryModel"));
        assert!(!json.contains("SpatialModel"));
        let opened = AicadProject::from_json(&json).unwrap();
        assert_eq!(opened, project);
        let Some(EngineeringFeature::BaseProfile {
            profile: scc_engineering::BaseProfile::Composite { segments: restored },
            ..
        }) = opened.engineering.features.get(&FeatureId("base".into()))
        else {
            panic!("composite source identity was lost");
        };
        assert_eq!(restored, &closed);
        let mut reordered = closed.clone();
        reordered.swap(0, 1);
        assert_ne!(restored, &reordered);
    }

    #[test]
    fn project_roundtrip_preserves_polygon_vertices_and_source_only_schema() {
        let vertices = scc_engineering::profile::canonicalize_polygon(&[
            Point2D::new(100.0, 0.0),
            Point2D::new(50.0, 80.0),
            Point2D::new(0.0, 0.0),
        ])
        .unwrap();
        let mut source = EngineeringModel::new();
        for command in [
            EngineeringCommand::CreatePart {
                id: PartId("polygon".into()),
                name: "Polygon".into(),
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: FeatureId("base".into()),
                    name: "Base".into(),
                    profile: scc_engineering::BaseProfile::Polygon {
                        vertices: vertices.clone(),
                    },
                },
            },
            EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Literal { value: 8.0 }),
            },
        ] {
            CommandExecutor::apply(&mut source, &command).unwrap();
        }
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Polygon Plate".into(),
                description: None,
            },
            source,
        )
        .unwrap();
        let json = project.to_json().unwrap();
        assert!(json.contains("polygon"));
        assert!(json.contains("vertices"));
        assert!(!json.contains("GeometryModel"));
        assert!(!json.contains("SpatialModel"));
        assert!(!json.contains("SolidBuildSpec"));
        let opened = AicadProject::from_json(&json).unwrap();
        assert_eq!(opened, project);
        let Some(EngineeringFeature::BaseProfile {
            profile: scc_engineering::BaseProfile::Polygon { vertices: restored },
            ..
        }) = opened.engineering.features.get(&FeatureId("base".into()))
        else {
            panic!("polygon source identity was lost");
        };
        assert_eq!(restored, &vertices);
        assert_eq!(restored[0], Point2D::new(0.0, 0.0));
    }

    #[test]
    fn project_roundtrip_preserves_typed_slot_source() {
        let mut source = EngineeringModel::new();
        for command in [
            EngineeringCommand::CreatePart {
                id: PartId("plate".into()),
                name: "Plate".into(),
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: FeatureId("base".into()),
                    name: "Base".into(),
                    profile: scc_engineering::BaseProfile::Rectangle {
                        width: 120.0,
                        height: 80.0,
                    },
                },
            },
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Slot {
                    id: FeatureId("slot".into()),
                    center: Point2D::new(10.0, -5.0),
                    length: 40.0,
                    width: 12.0,
                    orientation_degrees: 45.0,
                },
            },
            EngineeringCommand::SetPartThickness {
                source: Some(SourcedLength::Literal { value: 10.0 }),
            },
        ] {
            CommandExecutor::apply(&mut source, &command).unwrap();
        }
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Slot Plate".into(),
                description: None,
            },
            source,
        )
        .unwrap();
        let json = project.to_json().unwrap();
        assert!(json.contains("\"type\": \"slot\"") || json.contains("\"type\":\"slot\""));
        assert!(json.contains("orientation_degrees"));
        assert!(!json.contains("SLOT 40"));
        assert!(!json.contains("GeometryModel"));
        assert!(!json.contains("SpatialModel"));
        assert!(!json.contains("slot_cuts"));
        let opened = AicadProject::from_json(&json).unwrap();
        assert_eq!(opened, project);
        let Some(EngineeringFeature::Slot {
            center,
            length,
            width,
            orientation_degrees,
            id,
        }) = opened.engineering.features.get(&FeatureId("slot".into()))
        else {
            panic!("typed slot source was lost");
        };
        assert_eq!(id.0, "slot");
        assert_eq!(*length, 40.0);
        assert_eq!(*width, 12.0);
        assert_eq!(*orientation_degrees, 45.0);
        assert_eq!(center.x, 10.0);
        assert_eq!(center.y, -5.0);
        let mut legacy_value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let slot = legacy_value
            .get_mut("engineering")
            .and_then(|engineering| engineering.get_mut("features"))
            .and_then(|features| features.get_mut("slot"))
            .and_then(serde_json::Value::as_object_mut)
            .expect("serialized slot feature must be addressable by stable id");
        assert!(slot.remove("center").is_some());
        assert!(slot.remove("orientation_degrees").is_some());
        let legacy = serde_json::to_string(&legacy_value).unwrap();
        let opened_legacy = AicadProject::from_json(&legacy).unwrap();
        let Some(EngineeringFeature::Slot {
            center,
            orientation_degrees,
            ..
        }) = opened_legacy
            .engineering
            .features
            .get(&FeatureId("slot".into()))
        else {
            panic!("legacy JSON lost the typed slot feature");
        };
        assert_eq!(*center, Point2D::new(0.0, 0.0));
        assert_eq!(*orientation_degrees, 0.0);
    }

    #[test]
    fn project_roundtrip_preserves_source_model_and_pattern() {
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Installation Plate".into(),
                description: None,
            },
            model(),
        )
        .unwrap();
        let json = project.to_json().unwrap();
        let decoded = AicadProject::from_json(&json).unwrap();
        assert_eq!(project, decoded);
        assert!(json.contains("hole_pattern"));
        assert!(!json.contains("dimensional_specifications"));
        assert!(!json.contains("SolidBuildSpec"));
        assert!(!json.contains("step_bytes"));
    }

    #[test]
    fn project_roundtrip_persists_typed_specifications_not_formatted_text() {
        let mut source = model();
        source
            .set_dimensional_specification(
                EngineeringDimensionRef::FeatureSlot {
                    feature: FeatureId("base".into()),
                    slot: scc_engineering::binding::FeatureParameterSlot::Width,
                },
                Some(
                    DimensionalSpecification::new(
                        DimensionalTolerance::symmetric(0.10).unwrap(),
                        Some(DecimalPlaces(2)),
                    )
                    .unwrap(),
                ),
            )
            .unwrap();
        source
            .set_dimensional_specification(
                EngineeringDimensionRef::FeatureSlot {
                    feature: FeatureId("corners".into()),
                    slot: scc_engineering::binding::FeatureParameterSlot::Diameter,
                },
                Some(
                    DimensionalSpecification::new(
                        DimensionalTolerance::bilateral(0.20, -0.05).unwrap(),
                        Some(DecimalPlaces(2)),
                    )
                    .unwrap(),
                ),
            )
            .unwrap();
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Tolerance Plate".into(),
                description: None,
            },
            source,
        )
        .unwrap();
        let json = project.to_json().unwrap();
        assert!(json.contains("dimensional_specifications"));
        assert!(json.contains("symmetric"));
        assert!(json.contains("bilateral"));
        assert!(json.contains("decimal_places") || json.contains("precision"));
        assert!(!json.contains("120.00 ±0.10"));
        let opened = AicadProject::from_json(&json).unwrap();
        assert_eq!(opened, project);
    }

    #[test]
    fn project_roundtrip_preserves_blind_extent_without_derived_callout_data() {
        let mut source = model();
        source.spatial.hole_extents.insert(
            FeatureId("corners".into()),
            HoleExtent::Blind {
                start_face: HoleStartFace::Opposite,
                depth: SourcedLength::Literal { value: 4.0 },
            },
        );
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Blind Plate".into(),
                description: None,
            },
            source,
        )
        .unwrap();
        let json = project.to_json().unwrap();
        assert!(json.contains("\"blind\""));
        assert!(json.contains("\"opposite\""));
        assert!(!json.contains("HoleCallout"));
        assert!(!json.contains("hole_callout"));
        let opened = AicadProject::from_json(&json).unwrap();
        assert_eq!(opened, project);
        assert!(matches!(
            opened
                .engineering
                .spatial
                .hole_extents
                .get(&FeatureId("corners".into())),
            Some(HoleExtent::Blind {
                start_face: HoleStartFace::Opposite,
                depth: SourcedLength::Literal { value: 4.0 },
            })
        ));
    }

    #[test]
    fn parametric_history_uses_project_v2_and_roundtrips_without_claiming_geometry() {
        let mut engineering = EngineeringModel::new();
        CommandExecutor::apply(
            &mut engineering,
            &EngineeringCommand::CreatePart {
                id: PartId("parametric_part".into()),
                name: "Parametric Part".into(),
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut engineering,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Parametric {
                    id: FeatureId("sketch".into()),
                    name: "Sketch".into(),
                    feature: scc_engineering::ParametricFeature::Sketch {
                        support: scc_engineering::PlaneReference::Principal {
                            plane: scc_engineering::PrincipalPlane::XY,
                        },
                        profile: scc_engineering::SketchProfile {
                            outer: scc_engineering::SketchLoop::rectangle(
                                "edge",
                                Point2D::new(12.0, -4.0),
                                30.0,
                                20.0,
                            )
                            .unwrap(),
                            inner_loops: Vec::new(),
                        },
                    },
                    suppressed: false,
                },
            },
        )
        .unwrap();
        CommandExecutor::apply(
            &mut engineering,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Parametric {
                    id: FeatureId("extrude".into()),
                    name: "Extrude".into(),
                    feature: scc_engineering::ParametricFeature::Extrude {
                        profile: FeatureId("sketch".into()),
                        operation: scc_engineering::ExtrudeOperation::Add,
                        direction: scc_engineering::ExtrudeDirection::Normal { reversed: false },
                        extent: scc_engineering::ExtrudeExtent::Distance { length: 8.0 },
                    },
                    suppressed: false,
                },
            },
        )
        .unwrap();
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Parametric".into(),
                description: None,
            },
            engineering,
        )
        .unwrap();
        assert_eq!(project.schema_version, AICAD_PROJECT_SCHEMA_VERSION);
        let json = project.to_json().unwrap();
        assert!(json.contains("\"schema_version\": 2"));
        let opened = AicadProject::from_json(&json).unwrap();
        assert_eq!(opened, project);

        let mut forged_v1: serde_json::Value = serde_json::from_str(&json).unwrap();
        forged_v1["schema_version"] = AICAD_PROJECT_LEGACY_SCHEMA_VERSION.into();
        assert!(matches!(
            AicadProject::from_json(&forged_v1.to_string()),
            Err(ProjectError::UnsupportedSchemaVersion)
        ));
    }

    #[test]
    fn unknown_version_and_unknown_top_level_field_fail_closed() {
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Plate".into(),
                description: None,
            },
            model(),
        )
        .unwrap();
        let mut value: serde_json::Value =
            serde_json::from_str(&project.to_json().unwrap()).unwrap();
        value["schema_version"] = 3.into();
        assert!(matches!(
            AicadProject::from_json(&value.to_string()),
            Err(ProjectError::UnsupportedSchemaVersion)
        ));
        let mut value: serde_json::Value =
            serde_json::from_str(&project.to_json().unwrap()).unwrap();
        value["spatial_model"] = serde_json::json!({});
        assert!(matches!(
            AicadProject::from_json(&value.to_string()),
            Err(ProjectError::Json(_))
        ));
    }

    #[test]
    fn expression_source_survives_and_is_re_evaluated_after_open() {
        let mut source = model();
        source
            .create_parameter(Parameter::new(
                ParameterId("thickness".into()),
                scc_engineering::parameter::ParameterValue::Scalar(10.0),
                ParameterOrigin::User,
                AuthorityLevel::L0,
            ))
            .unwrap();
        source
            .create_parameter(Parameter::new(
                ParameterId("depth".into()),
                scc_engineering::parameter::ParameterValue::Expression(
                    scc_engineering::expression::Expression::Divide {
                        left: Box::new(scc_engineering::expression::Expression::Reference {
                            parameter: ParameterId("thickness".into()),
                        }),
                        right: Box::new(scc_engineering::expression::Expression::Constant {
                            value: 2.0,
                        }),
                    },
                ),
                ParameterOrigin::Derived,
                AuthorityLevel::L0,
            ))
            .unwrap();
        let project = AicadProject::new(
            ProjectMetadata {
                name: "Plate".into(),
                description: None,
            },
            source,
        )
        .unwrap();
        let opened = AicadProject::from_json(&project.to_json().unwrap()).unwrap();
        assert_eq!(
            opened
                .engineering
                .evaluate_parameter(&ParameterId("depth".into()))
                .unwrap(),
            scc_engineering::ResolvedValue::Scalar(5.0)
        );
        let mut changed = opened.engineering.clone();
        changed
            .parameters
            .get_mut(&ParameterId("thickness".into()))
            .unwrap()
            .value = scc_engineering::parameter::ParameterValue::Scalar(12.0);
        let changed = AicadProject::new(
            ProjectMetadata {
                name: "Plate".into(),
                description: None,
            },
            changed,
        )
        .unwrap();
        assert_eq!(
            changed
                .engineering
                .evaluate_parameter(&ParameterId("depth".into()))
                .unwrap(),
            scc_engineering::ResolvedValue::Scalar(6.0)
        );
    }
}
