use std::collections::BTreeMap;

use scc_core::ids::EntityId;
use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::feature::{EngineeringFeature, HoleDefinition, HolePattern};
use scc_engineering::lowering::lower_and_solve;
use scc_engineering::spatial::{HoleExtent, HoleStartFace, SourcedLength};
use scc_engineering::{EngineeringModel, FeatureId, PartId};
use scc_geometry::Point2;
use scc_spatial::{
    Bounds3D, CylindricalCut, SlotCut, SpatialBody, SpatialCutId, SpatialCutKind, SpatialFootprint,
    SpatialInputs, SpatialModel, SpatialModelSource, SpatialProfileSegment, ZInterval,
};

use scc_manufacturing::{
    lower_solid_build_spec, SolidArcDirection, SolidBodySource, SolidBodySpec, SolidBuildError,
    SolidBuildSourceMap, SolidBuildSpec, SolidFootprint, SolidOperation, SolidPartSpec,
    SolidPoint2, SolidProfileSegment, SolidZInterval, SOLID_BUILD_SPEC_SCHEMA_VERSION,
};

fn fid(id: &str) -> FeatureId {
    FeatureId(id.into())
}

fn eid(id: &str) -> EntityId {
    EntityId(id.into())
}

fn point(x: f64, y: f64) -> Point2 {
    Point2 { x, y }
}

fn spatial_with_cuts(cuts: BTreeMap<SpatialCutId, CylindricalCut>) -> SpatialModel {
    SpatialModel {
        body: SpatialBody {
            footprint: SpatialFootprint::Rectangle {
                origin: point(-60.0, -40.0),
                width: 120.0,
                height: 80.0,
            },
            profile: scc_spatial::SpatialBaseProfile::Rectangle {
                origin: point(-60.0, -40.0),
                width: 120.0,
                height: 80.0,
            },
            z: ZInterval {
                min: 0.0,
                max: 10.0,
            },
            source_feature: fid("plate"),
            source_entity: eid("plate"),
        },
        cuts,
        slot_cuts: BTreeMap::new(),
        bounds: Bounds3D {
            min_x: -60.0,
            max_x: 60.0,
            min_y: -40.0,
            max_y: 40.0,
            min_z: 0.0,
            max_z: 10.0,
        },
        construction_circles: BTreeMap::new(),
        source: SpatialModelSource {
            geometry_source_root: 1,
            geometry_source_step: 2,
        },
    }
}

fn cut(
    id: &str,
    feature: &str,
    entity: &str,
    center: (f64, f64),
) -> (SpatialCutId, CylindricalCut) {
    (
        SpatialCutId(id.into()),
        CylindricalCut {
            center: point(center.0, center.1),
            radius: 5.0,
            z: ZInterval {
                min: 0.0,
                max: 10.0,
            },
            kind: SpatialCutKind::ThroughAll,
            source_feature: fid(feature),
            source_entity: eid(entity),
        },
    )
}

fn rounded_plate(
    center: (f64, f64),
    width: f64,
    height: f64,
    radius: f64,
    thickness: f64,
) -> SpatialModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("rounded".into()),
            name: "rounded".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("rounded_base"),
                name: "rounded_base".into(),
                profile: scc_engineering::BaseProfile::RoundedRectangle {
                    center: scc_engineering::Point2D::new(center.0, center.1),
                    width,
                    height,
                    corner_radius: radius,
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: thickness }),
        },
    );
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    scc_spatial::lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap()
}

fn real_plate(width: f64, thickness: f64) -> SpatialModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("plate".into()),
            name: "plate".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("plate"),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width,
                    height: 80.0,
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: thickness }),
        },
    );
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    scc_spatial::lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap()
}

fn real_plate_with_slots() -> SpatialModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("slot_plate".into()),
            name: "slot plate".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("slot_plate"),
                name: "slot_plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    );
    for (id, center, length, width, orientation) in [
        ("slot_a", (13.0, -7.0), 36.0, 12.0, 30.0),
        ("slot_b", (-25.0, 12.0), 24.0, 8.0, 0.0),
    ] {
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Slot {
                    id: fid(id),
                    center: scc_engineering::Point2D::new(center.0, center.1),
                    length,
                    width,
                    orientation_degrees: orientation,
                },
            },
        );
    }
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: fid("center_hole"),
                diameter: 8.0,
            },
        },
    );
    bind_scalar(
        &mut model,
        "center_hole",
        scc_engineering::binding::FeatureParameterSlot::CenterX,
        42.0,
    );
    bind_scalar(
        &mut model,
        "center_hole",
        scc_engineering::binding::FeatureParameterSlot::CenterY,
        0.0,
    );
    apply(
        &mut model,
        EngineeringCommand::SetHoleExtent {
            target: fid("center_hole"),
            extent: Some(HoleExtent::ThroughAll),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    scc_spatial::lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap()
}

fn add_spatial_slot(
    spatial: &mut SpatialModel,
    id: &str,
    center: (f64, f64),
    length: f64,
    width: f64,
    orientation: f64,
) {
    let exact = scc_engineering::slot::slot_profile_segments(
        scc_engineering::Point2D::new(center.0, center.1),
        length,
        width,
        orientation,
    )
    .unwrap();
    let segments = exact
        .into_iter()
        .enumerate()
        .map(|(index, segment)| {
            let source_entity = eid(&format!("{id}:segment:{index:04}"));
            let segment_index = u32::try_from(index).unwrap();
            match segment {
                scc_engineering::profile::ProfileSegment::Line { start, end } => {
                    SpatialProfileSegment::Line {
                        start: point(start.x, start.y),
                        end: point(end.x, end.y),
                        source_entity,
                        segment_index,
                    }
                }
                scc_engineering::profile::ProfileSegment::Arc {
                    start,
                    end,
                    center,
                    radius,
                    direction,
                } => SpatialProfileSegment::Arc {
                    start: point(start.x, start.y),
                    end: point(end.x, end.y),
                    center: point(center.x, center.y),
                    radius,
                    direction: match direction {
                        scc_engineering::profile::ArcDirection::Clockwise => {
                            scc_spatial::SpatialArcDirection::Clockwise
                        }
                        scc_engineering::profile::ArcDirection::CounterClockwise => {
                            scc_spatial::SpatialArcDirection::CounterClockwise
                        }
                    },
                    source_entity,
                    segment_index,
                },
            }
        })
        .collect();
    spatial.slot_cuts.insert(
        SpatialCutId(format!("{id}:cut:0000")),
        SlotCut {
            center: point(center.0, center.1),
            overall_length: length,
            width,
            orientation_degrees: orientation,
            segments,
            z: ZInterval {
                min: spatial.body.z.min,
                max: spatial.body.z.max,
            },
            kind: SpatialCutKind::ThroughAll,
            source_feature: fid(id),
        },
    );
}

fn real_polygon(vertices: &[(f64, f64)], thickness: f64, include_holes: bool) -> SpatialModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("polygon".into()),
            name: "polygon plate".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("main"),
                name: "main".into(),
                profile: scc_engineering::BaseProfile::Polygon {
                    vertices: vertices
                        .iter()
                        .map(|(x, y)| scc_engineering::Point2D::new(*x, *y))
                        .collect(),
                },
            },
        },
    );
    if include_holes {
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: fid("center_hole"),
                    diameter: 10.0,
                },
            },
        );
        bind_scalar(
            &mut model,
            "center_hole",
            scc_engineering::binding::FeatureParameterSlot::CenterX,
            50.0,
        );
        bind_scalar(
            &mut model,
            "center_hole",
            scc_engineering::binding::FeatureParameterSlot::CenterY,
            30.0,
        );
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("bolt_pattern"),
                    pattern: HolePattern::Circular {
                        center: scc_engineering::Point2D::new(50.0, 30.0),
                        pitch_circle_diameter: 20.0,
                        count: 4,
                        start_angle: 0.0,
                        hole: HoleDefinition { diameter: 8.0 },
                    },
                },
            },
        );
    }
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: thickness }),
        },
    );
    if include_holes {
        for feature in ["center_hole", "bolt_pattern"] {
            apply(
                &mut model,
                EngineeringCommand::SetHoleExtent {
                    target: fid(feature),
                    extent: Some(HoleExtent::ThroughAll),
                },
            );
        }
    }
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    scc_spatial::lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap()
}
fn real_composite(translation: (f64, f64), include_holes: bool) -> SpatialModel {
    let (dx, dy) = translation;
    let p = |x: f64, y: f64| scc_engineering::Point2D::new(x + dx, y + dy);
    let notch_center_y = 60.0 + 300.0_f64.sqrt();
    let segments = vec![
        scc_engineering::profile::ProfileSegment::Line {
            start: p(0.0, 0.0),
            end: p(80.0, 0.0),
        },
        scc_engineering::profile::ProfileSegment::Arc {
            start: p(80.0, 0.0),
            end: p(100.0, 20.0),
            center: p(80.0, 20.0),
            radius: 20.0,
            direction: scc_engineering::profile::ArcDirection::CounterClockwise,
        },
        scc_engineering::profile::ProfileSegment::Line {
            start: p(100.0, 20.0),
            end: p(100.0, 60.0),
        },
        scc_engineering::profile::ProfileSegment::Line {
            start: p(100.0, 60.0),
            end: p(60.0, 60.0),
        },
        scc_engineering::profile::ProfileSegment::Arc {
            start: p(60.0, 60.0),
            end: p(40.0, 60.0),
            center: p(50.0, notch_center_y),
            radius: 20.0,
            direction: scc_engineering::profile::ArcDirection::Clockwise,
        },
        scc_engineering::profile::ProfileSegment::Line {
            start: p(40.0, 60.0),
            end: p(0.0, 60.0),
        },
        scc_engineering::profile::ProfileSegment::Line {
            start: p(0.0, 60.0),
            end: p(0.0, 0.0),
        },
    ];
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("composite".into()),
            name: "composite plate".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("main"),
                name: "main".into(),
                profile: scc_engineering::BaseProfile::Composite { segments },
            },
        },
    );
    if include_holes {
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Hole {
                    id: fid("center_hole"),
                    diameter: 10.0,
                },
            },
        );
        bind_scalar(
            &mut model,
            "center_hole",
            scc_engineering::binding::FeatureParameterSlot::CenterX,
            dx + 20.0,
        );
        bind_scalar(
            &mut model,
            "center_hole",
            scc_engineering::binding::FeatureParameterSlot::CenterY,
            dy + 30.0,
        );
        apply(
            &mut model,
            EngineeringCommand::AddFeature {
                feature: EngineeringFeature::HolePattern {
                    id: fid("bolt_pattern"),
                    pattern: HolePattern::Circular {
                        center: scc_engineering::Point2D::new(dx + 20.0, dy + 30.0),
                        pitch_circle_diameter: 20.0,
                        count: 4,
                        start_angle: 0.0,
                        hole: HoleDefinition { diameter: 8.0 },
                    },
                },
            },
        );
    }
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    if include_holes {
        for feature in ["center_hole", "bolt_pattern"] {
            apply(
                &mut model,
                EngineeringCommand::SetHoleExtent {
                    target: fid(feature),
                    extent: Some(HoleExtent::ThroughAll),
                },
            );
        }
    }
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    scc_spatial::lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap()
}

fn bind_scalar(
    model: &mut EngineeringModel,
    feature: &str,
    slot: scc_engineering::binding::FeatureParameterSlot,
    value: f64,
) {
    let parameter = scc_engineering::ParameterId(format!("{feature}_{slot}"));
    apply(
        model,
        EngineeringCommand::DeclareParameter {
            id: parameter.clone(),
            value: scc_engineering::parameter::ParameterValue::Scalar(value),
            origin: scc_engineering::ParameterOrigin::User,
            authority: scc_core::authority::AuthorityLevel::L0,
        },
    );
    apply(
        model,
        EngineeringCommand::BindFeatureParameter {
            feature: fid(feature),
            slot,
            parameter,
        },
    );
}
fn apply(model: &mut EngineeringModel, command: EngineeringCommand) {
    CommandExecutor::apply(model, &command).unwrap();
}

fn corner_plate() -> SpatialModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("plate".into()),
            name: "installation plate".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("plate"),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: fid("corner_holes"),
                pattern: HolePattern::Corner {
                    offset_x: 15.0,
                    offset_y: 15.0,
                    hole: HoleDefinition { diameter: 8.0 },
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetHoleExtent {
            target: fid("corner_holes"),
            extent: Some(HoleExtent::ThroughAll),
        },
    );
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    scc_spatial::lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap()
}

fn line(start: (f64, f64), end: (f64, f64)) -> SolidProfileSegment {
    SolidProfileSegment::Line {
        start: SolidPoint2 {
            x: start.0,
            y: start.1,
        },
        end: SolidPoint2 { x: end.0, y: end.1 },
    }
}

fn closed_profile_spec(
    segments: Vec<SolidProfileSegment>,
    bounds: (f64, f64, f64, f64),
) -> SolidBuildSpec {
    SolidBuildSpec {
        schema_version: SOLID_BUILD_SPEC_SCHEMA_VERSION,
        part: SolidPartSpec {
            body: SolidBodySpec {
                footprint: SolidFootprint::ClosedProfile { segments },
                z: SolidZInterval {
                    min: 0.0,
                    max: 10.0,
                },
            },
            bounds: scc_manufacturing::SolidBounds3D {
                min_x: bounds.0,
                max_x: bounds.1,
                min_y: bounds.2,
                max_y: bounds.3,
                min_z: 0.0,
                max_z: 10.0,
            },
        },
        operations: BTreeMap::new(),
        source_map: SolidBuildSourceMap {
            body: SolidBodySource {
                feature: fid("closed_profile"),
                entity: eid("closed_profile"),
            },
            operations: BTreeMap::new(),
        },
    }
}

fn concave_profile_spec() -> SolidBuildSpec {
    closed_profile_spec(
        vec![
            line((0.0, 0.0), (100.0, 0.0)),
            line((100.0, 0.0), (100.0, 60.0)),
            line((100.0, 60.0), (40.0, 60.0)),
            line((40.0, 60.0), (40.0, 100.0)),
            line((40.0, 100.0), (0.0, 100.0)),
            line((0.0, 100.0), (0.0, 0.0)),
        ],
        (0.0, 100.0, 0.0, 100.0),
    )
}

fn mixed_arc_profile_spec() -> SolidBuildSpec {
    closed_profile_spec(
        vec![
            line((0.0, 0.0), (80.0, 0.0)),
            SolidProfileSegment::Arc {
                start: SolidPoint2 { x: 80.0, y: 0.0 },
                end: SolidPoint2 { x: 100.0, y: 20.0 },
                center: SolidPoint2 { x: 80.0, y: 20.0 },
                radius: 20.0,
                direction: SolidArcDirection::CounterClockwise,
            },
            line((100.0, 20.0), (100.0, 60.0)),
            line((100.0, 60.0), (0.0, 60.0)),
            line((0.0, 60.0), (0.0, 0.0)),
        ],
        (0.0, 100.0, 0.0, 60.0),
    )
}

#[test]
fn closed_profile_foundation_accepts_concave_and_mixed_line_arc_loops() {
    let concave = concave_profile_spec();
    concave.validate().unwrap();
    let mixed = mixed_arc_profile_spec();
    mixed.validate().unwrap();
    let encoded = serde_json::to_value(&mixed).unwrap();
    assert_eq!(
        encoded["part"]["body"]["footprint"]["kind"],
        "closed_profile"
    );
    assert_eq!(
        encoded["part"]["body"]["footprint"]["segments"][1]["kind"],
        "arc"
    );
    let decoded: SolidBuildSpec = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, mixed);
}

#[test]
fn closed_profile_foundation_rejects_disconnected_or_zero_area_loops() {
    let mut disconnected = concave_profile_spec();
    let SolidFootprint::ClosedProfile { segments } = &mut disconnected.part.body.footprint else {
        panic!("expected closed profile");
    };
    segments[1] = line((100.1, 0.0), (100.0, 60.0));
    assert!(matches!(
        disconnected.validate(),
        Err(SolidBuildError::InvalidManufacturingGeometry { detail })
            if detail.contains("disconnected")
    ));

    let zero_area = closed_profile_spec(
        vec![
            line((0.0, 0.0), (10.0, 0.0)),
            line((10.0, 0.0), (20.0, 0.0)),
            line((20.0, 0.0), (0.0, 0.0)),
        ],
        (0.0, 20.0, 0.0, 1.0),
    );
    assert!(matches!(
        zero_area.validate(),
        Err(SolidBuildError::InvalidManufacturingGeometry { detail })
            if detail.contains("zero signed area")
    ));
}

#[test]
fn rounded_rectangle_lowers_to_typed_solid_spec_without_segment_inference() {
    let spec = lower_solid_build_spec(&rounded_plate((0.0, 0.0), 120.0, 80.0, 10.0, 8.0)).unwrap();
    assert_eq!(
        spec.part.body.footprint,
        SolidFootprint::RoundedRectangle {
            center: SolidPoint2 { x: 0.0, y: 0.0 },
            width: 120.0,
            height: 80.0,
            corner_radius: 10.0,
        }
    );
    assert_eq!(
        spec.part.bounds,
        scc_manufacturing::SolidBounds3D {
            min_x: -60.0,
            max_x: 60.0,
            min_y: -40.0,
            max_y: 40.0,
            min_z: 0.0,
            max_z: 8.0,
        }
    );
    assert_eq!(
        serde_json::to_string_pretty(&spec).unwrap(),
        include_str!("goldens/rounded_rectangle_solid_spec.json").trim_end()
    );
}

#[test]
fn translated_and_asymmetric_rounded_rectangle_preserves_dimensions() {
    let spec =
        lower_solid_build_spec(&rounded_plate((25.0, -15.0), 140.0, 60.0, 12.0, 15.0)).unwrap();
    assert_eq!(
        spec.part.body.footprint,
        SolidFootprint::RoundedRectangle {
            center: SolidPoint2 { x: 25.0, y: -15.0 },
            width: 140.0,
            height: 60.0,
            corner_radius: 12.0,
        }
    );
    assert_eq!(spec.part.bounds.min_x, -45.0);
    assert_eq!(spec.part.bounds.max_x, 95.0);
    assert_eq!(spec.part.bounds.min_y, -45.0);
    assert_eq!(spec.part.bounds.max_y, 15.0);
    assert_eq!(spec.part.body.z.max, 15.0);
}

#[test]
fn rounded_rectangle_invalid_radius_fails_closed() {
    let mut spec =
        lower_solid_build_spec(&rounded_plate((0.0, 0.0), 120.0, 80.0, 10.0, 8.0)).unwrap();
    let SolidFootprint::RoundedRectangle {
        center,
        width,
        height,
        ..
    } = spec.part.body.footprint
    else {
        panic!("expected rounded rectangle");
    };
    spec.part.body.footprint = SolidFootprint::RoundedRectangle {
        center,
        width,
        height,
        corner_radius: 0.0,
    };
    assert!(matches!(
        spec.validate(),
        Err(SolidBuildError::InvalidManufacturingGeometry { detail })
            if detail.contains("rounded rectangle")
    ));
}

#[test]
fn circular_base_lowers_to_exact_typed_footprint_without_bbox_approximation() {
    let mut spatial = spatial_with_cuts(BTreeMap::new());
    spatial.body.footprint = SpatialFootprint::Circle {
        center: point(30.0, -20.0),
        radius: 80.0,
    };
    spatial.body.profile = scc_spatial::SpatialBaseProfile::Circle {
        center: point(30.0, -20.0),
        radius: 80.0,
    };
    spatial.bounds = Bounds3D {
        min_x: -50.0,
        max_x: 110.0,
        min_y: -100.0,
        max_y: 60.0,
        min_z: 0.0,
        max_z: 10.0,
    };
    let spec = lower_solid_build_spec(&spatial).unwrap();
    assert_eq!(
        spec.part.body.footprint,
        SolidFootprint::Circle {
            center: SolidPoint2 { x: 30.0, y: -20.0 },
            radius: 80.0,
        }
    );
    assert_eq!(
        spec.part.bounds,
        scc_manufacturing::SolidBounds3D {
            min_x: -50.0,
            max_x: 110.0,
            min_y: -100.0,
            max_y: 60.0,
            min_z: 0.0,
            max_z: 10.0,
        }
    );
}
#[test]
fn invalid_circle_radius_fails_closed_before_backend() {
    let mut spatial = spatial_with_cuts(BTreeMap::new());
    spatial.body.footprint = SpatialFootprint::Circle {
        center: point(0.0, 0.0),
        radius: 0.0,
    };
    spatial.body.profile = scc_spatial::SpatialBaseProfile::Circle {
        center: point(0.0, 0.0),
        radius: 0.0,
    };
    assert!(matches!(
        lower_solid_build_spec(&spatial),
        Err(SolidBuildError::InvalidManufacturingGeometry { detail })
            if detail.contains("circle")
    ));
}

#[test]
fn polygon_identity_rejects_a_non_composite_footprint() {
    let mut spatial = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ],
        8.0,
        false,
    );
    spatial.body.footprint = SpatialFootprint::Rectangle {
        origin: point(0.0, 0.0),
        width: 120.0,
        height: 90.0,
    };
    assert!(matches!(
        lower_solid_build_spec(&spatial),
        Err(SolidBuildError::InvalidManufacturingInput { detail })
            if detail.contains("typed Polygon") && detail.contains("Composite")
    ));
}

#[test]
fn engineering_polygon_lowers_to_exact_ordered_closed_profile() {
    let spatial = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ],
        8.0,
        false,
    );
    let spec = lower_solid_build_spec(&spatial).unwrap();
    let SolidFootprint::ClosedProfile { segments } = &spec.part.body.footprint else {
        panic!("expected an exact ClosedProfile");
    };
    let endpoints: Vec<_> = segments
        .iter()
        .map(|segment| match segment {
            SolidProfileSegment::Line { start, end } => ((start.x, start.y), (end.x, end.y)),
            SolidProfileSegment::Arc { .. } => panic!("Polygon must remain line-only"),
        })
        .collect();
    assert_eq!(
        endpoints,
        vec![
            ((0.0, 0.0), (100.0, 0.0)),
            ((100.0, 0.0), (120.0, 40.0)),
            ((120.0, 40.0), (80.0, 90.0)),
            ((80.0, 90.0), (0.0, 60.0)),
            ((0.0, 60.0), (0.0, 0.0)),
        ]
    );
    assert_eq!(segments.len(), 5);
    assert_eq!(
        spatial.body.profile,
        scc_spatial::SpatialBaseProfile::Polygon
    );
    assert_eq!(spec.part.bounds.min_x, 0.0);
    assert_eq!(spec.part.bounds.max_x, 120.0);
    assert_eq!(spec.part.bounds.min_y, 0.0);
    assert_eq!(spec.part.bounds.max_y, 90.0);
    assert_eq!(spec.part.bounds.max_z, 8.0);
    assert_eq!(spec.source_map.body.feature, fid("main"));
    assert_eq!(spec.source_map.body.entity, eid("main_segment_0"));
    let serialized = serde_json::to_string_pretty(&spec).unwrap();
    assert_eq!(
        serialized,
        include_str!("goldens/polygon_solid_spec.json").trim_end()
    );
    for _ in 0..100 {
        let repeated = lower_solid_build_spec(&spatial).unwrap();
        assert_eq!(serde_json::to_string_pretty(&repeated).unwrap(), serialized);
    }
}

#[test]
fn concave_and_translated_engineering_polygons_keep_exact_contours_and_bounds() {
    let concave = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 60.0),
            (40.0, 60.0),
            (40.0, 100.0),
            (0.0, 100.0),
        ],
        8.0,
        false,
    );
    let concave_spec = lower_solid_build_spec(&concave).unwrap();
    assert_eq!(concave_spec.part.bounds.min_x, 0.0);
    assert_eq!(concave_spec.part.bounds.max_x, 100.0);
    assert_eq!(concave_spec.part.bounds.min_y, 0.0);
    assert_eq!(concave_spec.part.bounds.max_y, 100.0);
    assert!(matches!(
        concave_spec.part.body.footprint,
        SolidFootprint::ClosedProfile { ref segments } if segments.len() == 6
    ));

    let translated = real_polygon(
        &[
            (30.0, -20.0),
            (130.0, -20.0),
            (150.0, 20.0),
            (110.0, 70.0),
            (30.0, 40.0),
        ],
        8.0,
        false,
    );
    let translated_spec = lower_solid_build_spec(&translated).unwrap();
    assert_eq!(translated_spec.part.bounds.min_x, 30.0);
    assert_eq!(translated_spec.part.bounds.max_x, 150.0);
    assert_eq!(translated_spec.part.bounds.min_y, -20.0);
    assert_eq!(translated_spec.part.bounds.max_y, 70.0);
    let SolidFootprint::ClosedProfile { segments } = translated_spec.part.body.footprint else {
        panic!("expected translated ClosedProfile");
    };
    assert!(matches!(
        &segments[0],
        SolidProfileSegment::Line { start, .. } if (start.x, start.y) == (30.0, -20.0)
    ));
}

#[test]
fn polygon_preserves_real_single_hole_and_resolved_pattern_cuts() {
    let spatial = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ],
        8.0,
        true,
    );
    let spec = lower_solid_build_spec(&spatial).unwrap();
    assert_eq!(spec.operations.len(), 5);
    assert_eq!(spec.source_map.operations.len(), 5);
    assert_eq!(
        spec.operations
            .values()
            .filter(|operation| matches!(
                operation,
                SolidOperation::CylindricalCut { radius: 5.0, .. }
            ))
            .count(),
        1
    );
    assert_eq!(
        spec.operations
            .values()
            .filter(|operation| matches!(
                operation,
                SolidOperation::CylindricalCut { radius: 4.0, .. }
            ))
            .count(),
        4
    );
    assert!(spec.operations.values().all(|operation| matches!(
        operation,
        SolidOperation::CylindricalCut {
            z: SolidZInterval { min: 0.0, max: 8.0 },
            ..
        }
    )));
}

#[test]
fn polygon_rejects_noncanonical_edge_order_unexpected_arc_and_open_loop() {
    let mut out_of_order = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ],
        8.0,
        false,
    );
    let SpatialFootprint::Composite { segments } = &mut out_of_order.body.footprint else {
        panic!("expected a Spatial composite contour");
    };
    if let SpatialProfileSegment::Line { segment_index, .. } = &mut segments[2] {
        *segment_index = 1;
    }
    assert!(matches!(
        lower_solid_build_spec(&out_of_order),
        Err(SolidBuildError::InvalidManufacturingSource { detail })
            if detail.contains("canonical order")
    ));

    let mut unexpected_arc = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ],
        8.0,
        false,
    );
    let SpatialFootprint::Composite { segments } = &mut unexpected_arc.body.footprint else {
        panic!("expected a Spatial composite contour");
    };
    segments[1] = SpatialProfileSegment::Arc {
        start: point(100.0, 0.0),
        end: point(120.0, 40.0),
        center: point(110.0, 20.0),
        radius: 22.360679774997898,
        direction: scc_spatial::SpatialArcDirection::CounterClockwise,
        source_entity: eid("main_segment_1"),
        segment_index: 1,
    };
    assert!(matches!(
        lower_solid_build_spec(&unexpected_arc),
        Err(SolidBuildError::UnsupportedManufacturingFeature { detail })
            if detail.contains("unexpected arc") && detail.contains("line edges only")
    ));

    let mut open = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ],
        8.0,
        false,
    );
    let SpatialFootprint::Composite { segments } = &mut open.body.footprint else {
        panic!("expected a Spatial composite contour");
    };
    if let SpatialProfileSegment::Line { start, .. } = &mut segments[1] {
        *start = point(101.0, 0.0);
    }
    assert!(matches!(
        lower_solid_build_spec(&open),
        Err(SolidBuildError::InvalidManufacturingGeometry { detail })
            if detail.contains("disconnected")
    ));
}

#[test]
fn engineering_rectangle_with_translated_rotated_and_multiple_slots_lowers_deterministically() {
    let spatial = real_plate_with_slots();
    assert_eq!(spatial.slot_cuts.len(), 2);
    let spec = lower_solid_build_spec(&spatial).unwrap();
    assert_eq!(spec.operations.len(), 3);
    assert_eq!(spec.source_map.operations.len(), 3);
    assert_eq!(
        spec.operations
            .values()
            .filter(|operation| matches!(operation, SolidOperation::CylindricalCut { .. }))
            .count(),
        1
    );
    assert_eq!(
        spec.operations
            .values()
            .filter(|operation| matches!(operation, SolidOperation::SlotCut { .. }))
            .count(),
        2
    );
    for (spatial_id, spatial_slot) in &spatial.slot_cuts {
        let (&operation_id, operation_source) = spec
            .source_map
            .operations
            .iter()
            .find(|(_, source)| *source == spatial_id)
            .unwrap();
        assert_eq!(spec.operation_source(operation_id), Some(spatial_id));
        let SolidOperation::SlotCut {
            profile,
            z,
            axis: scc_manufacturing::SolidAxis::PositiveZ,
        } = &spec.operations[&operation_id]
        else {
            panic!("slot must remain typed");
        };
        assert_eq!(
            *z,
            SolidZInterval {
                min: 0.0,
                max: 10.0
            }
        );
        assert_eq!(profile.segments.len(), 4);
        assert_eq!(
            profile
                .segments
                .iter()
                .filter(|segment| matches!(segment, SolidProfileSegment::Arc { .. }))
                .count(),
            2
        );
        assert_eq!(
            profile
                .segments
                .iter()
                .filter(|segment| matches!(segment, SolidProfileSegment::Line { .. }))
                .count(),
            2
        );
        assert_eq!(profile.segments, slot_segments_for_test(spatial_slot));
        assert_eq!(operation_source, spatial_id);
    }
    assert!(spec.operations.values().any(|operation| matches!(operation, SolidOperation::CylindricalCut { radius, .. } if *radius == 4.0)));
    let encoded = serde_json::to_string_pretty(&spec).unwrap();
    assert_eq!(
        encoded,
        include_str!("goldens/slot_solid_spec.json").trim_end()
    );
    for _ in 0..100 {
        assert_eq!(
            serde_json::to_string_pretty(&lower_solid_build_spec(&spatial).unwrap()).unwrap(),
            encoded
        );
    }
}

fn slot_segments_for_test(slot: &SlotCut) -> Vec<SolidProfileSegment> {
    slot.segments
        .iter()
        .map(|segment| match segment {
            SpatialProfileSegment::Line { start, end, .. } => SolidProfileSegment::Line {
                start: SolidPoint2 {
                    x: start.x,
                    y: start.y,
                },
                end: SolidPoint2 { x: end.x, y: end.y },
            },
            SpatialProfileSegment::Arc {
                start,
                end,
                center,
                radius,
                direction,
                ..
            } => SolidProfileSegment::Arc {
                start: SolidPoint2 {
                    x: start.x,
                    y: start.y,
                },
                end: SolidPoint2 { x: end.x, y: end.y },
                center: SolidPoint2 {
                    x: center.x,
                    y: center.y,
                },
                radius: *radius,
                direction: match direction {
                    scc_spatial::SpatialArcDirection::Clockwise => SolidArcDirection::Clockwise,
                    scc_spatial::SpatialArcDirection::CounterClockwise => {
                        SolidArcDirection::CounterClockwise
                    }
                },
            },
        })
        .collect()
}

#[test]
fn polygon_slot_combination_lowers_exact_contour_as_typed_operation() {
    let mut spatial = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ],
        8.0,
        false,
    );
    add_spatial_slot(&mut spatial, "slot", (50.0, 30.0), 20.0, 8.0, 30.0);
    let spec = lower_solid_build_spec(&spatial).unwrap();
    assert!(matches!(
        spec.part.body.footprint,
        SolidFootprint::ClosedProfile { .. }
    ));
    assert_eq!(spec.operations.len(), 1);
    assert!(
        matches!(spec.operations.values().next(), Some(SolidOperation::SlotCut { profile, z, axis: scc_manufacturing::SolidAxis::PositiveZ }) if profile.segments.len() == 4 && *z == SolidZInterval { min: 0.0, max: 8.0 })
    );
    assert_eq!(
        spec.operation_source(*spec.operations.keys().next().unwrap()),
        Some(&SpatialCutId("slot:cut:0000".into()))
    );
}

#[test]
fn polygon_spatial_bounds_are_witnessed_not_substituted() {
    let mut spatial = real_polygon(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ],
        8.0,
        false,
    );
    spatial.bounds.max_x = 121.0;
    assert!(matches!(
        lower_solid_build_spec(&spatial),
        Err(SolidBuildError::InvalidManufacturingInput { detail })
            if detail.contains("bounds do not match")
    ));
}
#[test]
fn manufacturing_rejects_open_slot_contour_without_repair_or_bbox_fallback() {
    let mut spatial = spatial_with_cuts(BTreeMap::new());
    spatial.slot_cuts.insert(
        SpatialCutId("slot:cut:0000".into()),
        SlotCut {
            center: point(0.0, 0.0),
            overall_length: 40.0,
            width: 12.0,
            orientation_degrees: 0.0,
            segments: vec![SpatialProfileSegment::Line {
                start: point(-14.0, 6.0),
                end: point(14.0, 6.0),
                source_entity: eid("slot_0"),
                segment_index: 0,
            }],
            z: ZInterval {
                min: 0.0,
                max: 10.0,
            },
            kind: SpatialCutKind::ThroughAll,
            source_feature: fid("slot"),
        },
    );
    let error = lower_solid_build_spec(&spatial).unwrap_err();
    assert!(matches!(
        error,
        SolidBuildError::InvalidManufacturingGeometry { detail }
            if detail.contains("closed profile") && detail.contains("not closed")
    ));
    assert!(lower_solid_build_spec(&spatial).is_err());
}

#[test]
fn typed_composite_requires_an_exact_composite_footprint() {
    let mut spatial = real_composite((0.0, 0.0), false);
    spatial.body.footprint = SpatialFootprint::Rectangle {
        origin: point(0.0, 0.0),
        width: 100.0,
        height: 60.0,
    };
    assert!(matches!(
        lower_solid_build_spec(&spatial),
        Err(SolidBuildError::InvalidManufacturingInput { detail })
            if detail.contains("typed Composite") && detail.contains("exact Composite")
    ));
}

#[test]
fn engineering_composite_lowers_exact_ordered_lines_and_arcs_to_closed_profile() {
    let spatial = real_composite((30.0, -20.0), false);
    assert_eq!(
        spatial.body.profile,
        scc_spatial::SpatialBaseProfile::Composite
    );
    let spec = lower_solid_build_spec(&spatial).unwrap();
    let SolidFootprint::ClosedProfile { segments } = &spec.part.body.footprint else {
        panic!("expected generic ClosedProfile");
    };
    assert_eq!(segments.len(), 7);
    assert!(matches!(segments[0], SolidProfileSegment::Line { .. }));
    assert!(matches!(
        &segments[1],
        SolidProfileSegment::Arc {
            start,
            end,
            center,
            radius: 20.0,
            direction: SolidArcDirection::CounterClockwise,
        } if (start.x, start.y) == (110.0, -20.0)
            && (end.x, end.y) == (130.0, 0.0)
            && (center.x, center.y) == (110.0, 0.0)
    ));
    assert!(matches!(
        &segments[4],
        SolidProfileSegment::Arc {
            start,
            end,
            center,
            radius: 20.0,
            direction: SolidArcDirection::Clockwise,
        } if (start.x, start.y) == (90.0, 40.0)
            && (end.x, end.y) == (70.0, 40.0)
            && center.x == 80.0
            && center.y == 40.0 + 300.0_f64.sqrt()
    ));
    assert_eq!(
        spec.part.bounds,
        scc_manufacturing::SolidBounds3D {
            min_x: 30.0,
            max_x: 130.0,
            min_y: -20.0,
            max_y: 40.0,
            min_z: 0.0,
            max_z: 10.0,
        }
    );
    assert_eq!(spec.source_map.body.feature, fid("main"));
    assert_eq!(spec.source_map.body.entity, eid("main_segment_0"));
}

#[test]
fn line_only_engineering_composite_keeps_identity_through_manufacturing() {
    let source_segments = vec![
        scc_engineering::ProfileSegment::Line {
            start: scc_engineering::Point2D::new(0.0, 0.0),
            end: scc_engineering::Point2D::new(80.0, 0.0),
        },
        scc_engineering::ProfileSegment::Line {
            start: scc_engineering::Point2D::new(80.0, 0.0),
            end: scc_engineering::Point2D::new(80.0, 50.0),
        },
        scc_engineering::ProfileSegment::Line {
            start: scc_engineering::Point2D::new(80.0, 50.0),
            end: scc_engineering::Point2D::new(0.0, 50.0),
        },
        scc_engineering::ProfileSegment::Line {
            start: scc_engineering::Point2D::new(0.0, 50.0),
            end: scc_engineering::Point2D::new(0.0, 0.0),
        },
    ];
    let mut engineering = EngineeringModel::new();
    apply(
        &mut engineering,
        EngineeringCommand::CreatePart {
            id: PartId("line_only_composite".into()),
            name: "line-only composite".into(),
        },
    );
    apply(
        &mut engineering,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("main"),
                name: "main".into(),
                profile: scc_engineering::BaseProfile::Composite {
                    segments: source_segments,
                },
            },
        },
    );
    apply(
        &mut engineering,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 12.0 }),
        },
    );

    let (lowering, geometry) = lower_and_solve(&engineering).unwrap();
    let spatial = scc_spatial::lower_spatial(SpatialInputs {
        engineering: &engineering,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap();

    assert_eq!(
        spatial.body.profile,
        scc_spatial::SpatialBaseProfile::Composite
    );
    let SpatialFootprint::Composite { segments } = &spatial.body.footprint else {
        panic!("typed line-only Composite must remain a Composite spatial footprint");
    };
    assert_eq!(segments.len(), 4);
    for (index, segment) in segments.iter().enumerate() {
        let (segment_index, source_entity) = match segment {
            SpatialProfileSegment::Line {
                segment_index,
                source_entity,
                ..
            } => (*segment_index, source_entity),
            SpatialProfileSegment::Arc { .. } => {
                panic!("line-only Composite must not acquire Arc segments")
            }
        };
        assert_eq!(segment_index, index as u32);
        assert_eq!(source_entity, &eid(&format!("main_segment_{index}")));
        let provenance = lowering.source_map.profile_segment(source_entity).unwrap();
        assert_eq!(provenance.feature, fid("main"));
        assert_eq!(provenance.segment_index, index as u32);
        assert_eq!(
            provenance.semantic,
            scc_engineering::source_map::ProfileSegmentSemantic::CompositeSegment
        );
    }

    let spec = lower_solid_build_spec(&spatial).unwrap();
    let SolidFootprint::ClosedProfile { segments } = &spec.part.body.footprint else {
        panic!("line-only Composite must lower through the generic ClosedProfile")
    };
    assert_eq!(segments.len(), 4);
    assert!(segments
        .iter()
        .all(|segment| matches!(segment, SolidProfileSegment::Line { .. })));
    assert_eq!(spec.source_map.body.feature, fid("main"));
    assert_eq!(spec.source_map.body.entity, eid("main_segment_0"));
}

#[test]
fn composite_manufacturing_preserves_a_valid_directed_major_arc() {
    let profile = scc_engineering::BaseProfile::Composite {
        segments: vec![
            scc_engineering::profile::ProfileSegment::Line {
                start: scc_engineering::Point2D::new(0.0, 0.0),
                end: scc_engineering::Point2D::new(0.0, -50.0),
            },
            scc_engineering::profile::ProfileSegment::Line {
                start: scc_engineering::Point2D::new(0.0, -50.0),
                end: scc_engineering::Point2D::new(100.0, -50.0),
            },
            scc_engineering::profile::ProfileSegment::Line {
                start: scc_engineering::Point2D::new(100.0, -50.0),
                end: scc_engineering::Point2D::new(100.0, 0.0),
            },
            scc_engineering::profile::ProfileSegment::Arc {
                start: scc_engineering::Point2D::new(100.0, 0.0),
                end: scc_engineering::Point2D::new(0.0, 0.0),
                center: scc_engineering::Point2D::new(50.0, 30.0),
                radius: 50.0_f64.hypot(30.0),
                direction: scc_engineering::profile::ArcDirection::CounterClockwise,
            },
        ],
    };
    profile.validate().unwrap();
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("major_arc".into()),
            name: "major arc composite".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("main"),
                name: "main".into(),
                profile,
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let spatial = scc_spatial::lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap();
    let spec = lower_solid_build_spec(&spatial).unwrap();
    let SolidFootprint::ClosedProfile { segments } = spec.part.body.footprint else {
        panic!("expected exact ClosedProfile");
    };
    assert!(matches!(
        segments.last(),
        Some(SolidProfileSegment::Arc {
            start,
            end,
            direction: SolidArcDirection::CounterClockwise,
            ..
        }) if (start.x, start.y) == (100.0, 0.0) && (end.x, end.y) == (0.0, 0.0)
    ));
    assert!(spec.part.bounds.min_x < 0.0);
    assert!(spec.part.bounds.max_x > 100.0);
    assert!(spec.part.bounds.max_y > 80.0);
}

#[test]
fn composite_preserves_typed_hole_and_upstream_resolved_pattern_cuts() {
    let spatial = real_composite((30.0, -20.0), true);
    let spec = lower_solid_build_spec(&spatial).unwrap();
    assert_eq!(
        spatial.body.profile,
        scc_spatial::SpatialBaseProfile::Composite
    );
    assert_eq!(spec.operations.len(), 5);
    assert_eq!(spec.source_map.operations.len(), 5);
    assert_eq!(
        spec.operations
            .values()
            .filter(|operation| matches!(
                operation,
                SolidOperation::CylindricalCut { radius: 5.0, .. }
            ))
            .count(),
        1
    );
    assert_eq!(
        spec.operations
            .values()
            .filter(|operation| matches!(
                operation,
                SolidOperation::CylindricalCut { radius: 4.0, .. }
            ))
            .count(),
        4
    );
    assert!(spec.operations.values().all(|operation| matches!(
        operation,
        SolidOperation::CylindricalCut {
            z: SolidZInterval {
                min: 0.0,
                max: 10.0
            },
            ..
        }
    )));
    let serialized = serde_json::to_string_pretty(&spec).unwrap();
    assert_eq!(
        serialized,
        include_str!("goldens/composite_solid_spec.json").trim_end()
    );
    for _ in 0..100 {
        let repeated = lower_solid_build_spec(&spatial).unwrap();
        assert_eq!(serde_json::to_string_pretty(&repeated).unwrap(), serialized);
    }
}

#[test]
fn composite_plus_slot_preserves_base_arcs_and_exact_slot_operation() {
    let mut spatial = real_composite((0.0, 0.0), false);
    add_spatial_slot(&mut spatial, "slot", (20.0, 30.0), 24.0, 8.0, 30.0);
    let spec = lower_solid_build_spec(&spatial).unwrap();
    let SolidFootprint::ClosedProfile { segments: base } = &spec.part.body.footprint else {
        panic!("expected exact Composite base");
    };
    assert!(base
        .iter()
        .any(|segment| matches!(segment, SolidProfileSegment::Arc { .. })));
    let operation = spec.operations.values().next().unwrap();
    assert!(
        matches!(operation, SolidOperation::SlotCut { profile, .. } if profile.segments.len() == 4 && profile.segments.iter().filter(|segment| matches!(segment, SolidProfileSegment::Arc { .. })).count() == 2)
    );
    assert_eq!(
        spec.operation_source(*spec.operations.keys().next().unwrap()),
        Some(&SpatialCutId("slot:cut:0000".into()))
    );
}

#[test]
fn composite_rejects_order_provenance_and_direction_corruption() {
    let mut out_of_order = real_composite((0.0, 0.0), false);
    let SpatialFootprint::Composite { segments } = &mut out_of_order.body.footprint else {
        panic!("expected spatial Composite");
    };
    if let SpatialProfileSegment::Arc { segment_index, .. } = &mut segments[1] {
        *segment_index = 4;
    }
    assert!(matches!(
        lower_solid_build_spec(&out_of_order),
        Err(SolidBuildError::InvalidManufacturingSource { detail })
            if detail.contains("source order")
    ));

    let mut wrong_body_source = real_composite((0.0, 0.0), false);
    wrong_body_source.body.source_entity = eid("not_segment_zero");
    assert!(matches!(
        lower_solid_build_spec(&wrong_body_source),
        Err(SolidBuildError::InvalidManufacturingSource { detail })
            if detail.contains("first canonical segment")
    ));

    let mut wrong_direction = real_composite((0.0, 0.0), false);
    let SpatialFootprint::Composite { segments } = &mut wrong_direction.body.footprint else {
        panic!("expected spatial Composite");
    };
    if let SpatialProfileSegment::Arc { direction, .. } = &mut segments[4] {
        *direction = scc_spatial::SpatialArcDirection::CounterClockwise;
    }
    assert!(matches!(
        lower_solid_build_spec(&wrong_direction),
        Err(SolidBuildError::InvalidManufacturingInput { detail })
            if detail.contains("bounds do not match")
    ));

    let mut wrong_radius = real_composite((0.0, 0.0), false);
    let SpatialFootprint::Composite { segments } = &mut wrong_radius.body.footprint else {
        panic!("expected spatial Composite");
    };
    if let SpatialProfileSegment::Arc { radius, .. } = &mut segments[1] {
        *radius = 21.0;
    }
    assert!(matches!(
        lower_solid_build_spec(&wrong_radius),
        Err(SolidBuildError::InvalidManufacturingGeometry { .. })
    ));
}

#[test]
fn body_only_has_one_body_and_no_operations() {
    let spec = lower_solid_build_spec(&spatial_with_cuts(BTreeMap::new())).unwrap();
    assert_eq!(spec.operations.len(), 0);
    assert_eq!(
        spec.part.body.z,
        SolidZInterval {
            min: 0.0,
            max: 10.0
        }
    );
    assert_eq!(
        spec.part.body.footprint,
        SolidFootprint::Rectangle {
            origin: SolidPoint2 { x: -60.0, y: -40.0 },
            width: 120.0,
            height: 80.0,
        }
    );
    assert_eq!(spec.source_map.body.feature, fid("plate"));
}

#[test]
fn through_cut_copies_center_radius_and_z_without_extent_semantics() {
    let mut cuts = BTreeMap::new();
    cuts.insert(
        SpatialCutId("hole:cut:0000".into()),
        CylindricalCut {
            center: point(60.0, 40.0),
            radius: 5.0,
            z: ZInterval {
                min: 0.0,
                max: 10.0,
            },
            kind: SpatialCutKind::ThroughAll,
            source_feature: fid("hole"),
            source_entity: eid("hole"),
        },
    );
    let spec = lower_solid_build_spec(&spatial_with_cuts(cuts)).unwrap();
    assert_eq!(spec.operations.len(), 1);
    let operation = spec.operations.values().next().unwrap();
    assert_eq!(
        operation,
        &SolidOperation::CylindricalCut {
            center: SolidPoint2 { x: 60.0, y: 40.0 },
            radius: 5.0,
            z: SolidZInterval {
                min: 0.0,
                max: 10.0
            },
            axis: scc_manufacturing::SolidAxis::PositiveZ,
        }
    );
}

#[test]
fn blind_intervals_are_consumed_directly() {
    let mut cuts = BTreeMap::new();
    let (_, mut base) = cut("base:cut:0000", "base", "base_hole", (10.0, 20.0));
    base.z = ZInterval { min: 0.0, max: 4.0 };
    base.kind = SpatialCutKind::Blind {
        start_face: HoleStartFace::Base,
    };
    cuts.insert(SpatialCutId("base:cut:0000".into()), base);
    let (_, mut opposite) = cut(
        "opposite:cut:0000",
        "opposite",
        "opposite_hole",
        (30.0, 20.0),
    );
    opposite.z = ZInterval {
        min: 6.0,
        max: 10.0,
    };
    opposite.kind = SpatialCutKind::Blind {
        start_face: HoleStartFace::Opposite,
    };
    cuts.insert(SpatialCutId("opposite:cut:0000".into()), opposite);
    let spec = lower_solid_build_spec(&spatial_with_cuts(cuts)).unwrap();
    let z: Vec<SolidZInterval> = spec
        .operations
        .values()
        .map(|operation| match operation {
            SolidOperation::CylindricalCut { z, .. } | SolidOperation::SlotCut { z, .. } => *z,
        })
        .collect();
    assert_eq!(
        z,
        vec![
            SolidZInterval { min: 0.0, max: 4.0 },
            SolidZInterval {
                min: 6.0,
                max: 10.0
            }
        ]
    );
}

#[test]
fn eight_spatial_cuts_become_eight_operations_with_typed_sources() {
    let mut cuts = BTreeMap::new();
    for index in 0..8 {
        let id = format!("pattern:cut:{index:04}");
        cuts.insert(
            SpatialCutId(id),
            cut(
                "unused",
                "pattern",
                &format!("pattern_array[{index}]"),
                (index as f64, 0.0),
            )
            .1,
        );
    }
    let spec = lower_solid_build_spec(&spatial_with_cuts(cuts)).unwrap();
    assert_eq!(spec.operations.len(), 8);
    assert_eq!(spec.source_map.operations.len(), 8);
    assert!(spec
        .source_map
        .operations
        .values()
        .all(|source| source.0.starts_with("pattern:cut:")));
}

#[test]
fn construction_geometry_is_not_a_manufacturing_operation() {
    let mut with_construction = spatial_with_cuts(BTreeMap::new());
    with_construction
        .construction_circles
        .insert(eid("construction"), fid("construction"));
    let without = lower_solid_build_spec(&spatial_with_cuts(BTreeMap::new())).unwrap();
    let with = lower_solid_build_spec(&with_construction).unwrap();
    assert_eq!(without, with);
    assert_eq!(
        serde_json::to_string(&without).unwrap(),
        serde_json::to_string(&with).unwrap()
    );
}

#[test]
fn width_and_thickness_propagate_only_through_upstream_spatial_truth() {
    let original = lower_solid_build_spec(&real_plate(120.0, 10.0)).unwrap();
    let changed = lower_solid_build_spec(&real_plate(160.0, 12.0)).unwrap();
    let width = match changed.part.body.footprint {
        SolidFootprint::Rectangle { width, .. } => width,
        other => panic!("expected rectangle footprint, got {other:?}"),
    };
    assert_eq!(width, 160.0);
    assert_eq!(changed.part.body.z.max, 12.0);
    assert_ne!(changed, original);
}

#[test]
fn drawing_and_visibility_inputs_do_not_exist_at_this_boundary() {
    let spatial = spatial_with_cuts(BTreeMap::new());
    assert_eq!(
        lower_solid_build_spec(&spatial).unwrap(),
        lower_solid_build_spec(&spatial).unwrap()
    );
}

#[test]
fn outside_body_and_overlapping_cuts_are_preserved_not_clipped_or_deleted() {
    let mut cuts = BTreeMap::new();
    let (_, mut first) = cut("first:cut:0000", "first", "first", (500.0, 500.0));
    first.z = ZInterval { min: 2.0, max: 4.0 };
    cuts.insert(SpatialCutId("first:cut:0000".into()), first);
    let (_, mut second) = cut("second:cut:0000", "second", "second", (500.0, 500.0));
    second.z = ZInterval { min: 3.0, max: 5.0 };
    cuts.insert(SpatialCutId("second:cut:0000".into()), second);
    let spec = lower_solid_build_spec(&spatial_with_cuts(cuts)).unwrap();
    assert_eq!(spec.operations.len(), 2);
}

#[test]
fn malformed_numeric_and_source_inputs_fail_closed() {
    let mut nan_body = spatial_with_cuts(BTreeMap::new());
    nan_body.body.z.max = f64::NAN;
    assert!(matches!(
        lower_solid_build_spec(&nan_body),
        Err(SolidBuildError::InvalidManufacturingGeometry { .. })
    ));

    let mut inf_cut = spatial_with_cuts(BTreeMap::new());
    let (_, mut cut_value) = cut("bad:cut:0000", "bad", "bad", (0.0, 0.0));
    cut_value.radius = f64::INFINITY;
    inf_cut
        .cuts
        .insert(SpatialCutId("bad:cut:0000".into()), cut_value);
    assert!(matches!(
        lower_solid_build_spec(&inf_cut),
        Err(SolidBuildError::InvalidManufacturingGeometry { .. })
    ));

    let mut missing_source = spatial_with_cuts(BTreeMap::new());
    let (_, mut source_cut) = cut("missing:cut:0000", "", "entity", (0.0, 0.0));
    source_cut.source_feature = fid("");
    missing_source
        .cuts
        .insert(SpatialCutId("missing:cut:0000".into()), source_cut);
    assert!(matches!(
        lower_solid_build_spec(&missing_source),
        Err(SolidBuildError::InvalidManufacturingSource { .. })
    ));
}

#[test]
fn body_base_plane_mutation_fails_closed() {
    let mut spec = lower_solid_build_spec(&spatial_with_cuts(BTreeMap::new())).unwrap();
    spec.part.body.z.min = 1.0;
    spec.part.bounds.min_z = 1.0;
    assert!(matches!(
        spec.validate(),
        Err(SolidBuildError::InvalidManufacturingGeometry { .. })
    ));
}

#[test]
fn serde_roundtrip_and_one_hundred_runs_are_byte_deterministic() {
    let spatial = corner_plate();
    let first = lower_solid_build_spec(&spatial).unwrap();
    let json = serde_json::to_string(&first).unwrap();
    for _ in 0..100 {
        let next = lower_solid_build_spec(&spatial).unwrap();
        assert_eq!(serde_json::to_string(&next).unwrap(), json);
    }
    let roundtrip: scc_manufacturing::SolidBuildSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip, first);
}

#[test]
fn formal_installation_plate_fixture_has_four_corner_operations() {
    let spec = lower_solid_build_spec(&corner_plate()).unwrap();
    let serialized = serde_json::to_string_pretty(&spec).unwrap();
    assert_eq!(
        serialized,
        include_str!("goldens/installation_plate_solid_spec.json").trim_end()
    );
    assert_eq!(spec.operations.len(), 4);
    assert!(spec.operations.values().all(|operation| matches!(
        operation,
        SolidOperation::CylindricalCut { radius: 4.0, .. }
    )));
}

#[test]
fn circular_disk_fixture_matches_solid_build_spec_golden() {
    let mut spatial = spatial_with_cuts(BTreeMap::new());
    spatial.body.footprint = SpatialFootprint::Circle {
        center: point(0.0, 0.0),
        radius: 80.0,
    };
    spatial.body.profile = scc_spatial::SpatialBaseProfile::Circle {
        center: point(0.0, 0.0),
        radius: 80.0,
    };
    spatial.body.z.max = 20.0;
    spatial.bounds = Bounds3D {
        min_x: -80.0,
        max_x: 80.0,
        min_y: -80.0,
        max_y: 80.0,
        min_z: 0.0,
        max_z: 20.0,
    };
    let spec = lower_solid_build_spec(&spatial).unwrap();
    assert_eq!(
        serde_json::to_string_pretty(&spec).unwrap(),
        include_str!("goldens/circular_disk_solid_spec.json").trim_end()
    );
}
#[test]
fn structured_errors_use_the_manufacturing_namespace() {
    let mut spatial = spatial_with_cuts(BTreeMap::new());
    spatial.body.z.max = f64::NAN;
    let error = lower_solid_build_spec(&spatial).unwrap_err();
    let structured: scc_core::error::StructuredError = error.into();
    assert_eq!(structured.code, "SCC-MFG-004");
    assert_eq!(structured.stage, scc_core::error::ErrorStage::Manufacturing);
    assert!(!structured.recoverable);
    structured.validate_contract().unwrap();
}
#[test]
fn closed_profile_internal_fixture_matches_solid_build_spec_golden() {
    let spec = mixed_arc_profile_spec();
    spec.validate().unwrap();
    let SolidFootprint::ClosedProfile { segments } = &spec.part.body.footprint else {
        panic!("expected the internal ClosedProfile fixture");
    };
    assert_eq!(segments.len(), 5);
    assert_eq!(
        segments
            .iter()
            .filter(|segment| matches!(segment, SolidProfileSegment::Arc { .. }))
            .count(),
        1
    );
    let golden: SolidBuildSpec =
        serde_json::from_str(include_str!("goldens/closed_profile_solid_spec.json")).unwrap();
    assert_eq!(spec, golden);
}
#[test]
fn open_closed_profile_fails_manufacturing_validation_before_face_construction() {
    let mut open = mixed_arc_profile_spec();
    let SolidFootprint::ClosedProfile { segments } = &mut open.part.body.footprint else {
        panic!("expected a ClosedProfile");
    };
    segments.pop();

    assert!(matches!(
        open.validate(),
        Err(SolidBuildError::InvalidManufacturingGeometry { detail })
            if detail.contains("not closed")
    ));
}
