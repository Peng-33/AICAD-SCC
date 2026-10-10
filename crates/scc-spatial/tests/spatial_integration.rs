//! Phase 9.2 integration tests — 2.5D spatial lowering end-to-end (ADR-033).
//!
//! Pipeline tests drive the real `CommandExecutor` → `lower_and_solve` →
//! `lower_spatial` chain. Tamper tests construct `GeometryModel` and
//! `EngineeringSourceMap` directly (both are pub data) to reach faults the
//! real pipeline cannot produce — fail-closed integrity must hold there too.

use scc_core::authority::AuthorityLevel;
use scc_core::ids::EntityId;
use scc_engineering::binding::FeatureParameterSlot;
use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::expression::Expression;
use scc_engineering::feature::{EngineeringFeature, HoleDefinition, HolePattern};
use scc_engineering::lowering::lower_and_solve;
use scc_engineering::parameter::ParameterValue;
use scc_engineering::source_map::EngineeringSourceMap;
use scc_engineering::spatial::{HoleExtent, HoleStartFace, SourcedLength, SpatialIssue};
use scc_engineering::{EngineeringModel, FeatureId, ParameterOrigin, PartId};
use scc_geometry::{GeometryEntity, GeometryModel, GeometryPrimitive, Point2};
use scc_spatial::error::{InvalidBaseReason, InvalidGeometryReason, SpatialError};
use scc_spatial::{
    lower_spatial, Bounds3D, SpatialCutKind, SpatialInputs, SpatialModel, ZInterval,
};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn fid(id: &str) -> FeatureId {
    FeatureId(id.into())
}

fn eid(id: &str) -> EntityId {
    EntityId(id.into())
}

fn pid(id: &str) -> scc_engineering::ParameterId {
    scc_engineering::ParameterId(id.into())
}

fn point(x: f64, y: f64) -> Point2 {
    Point2 { x, y }
}

fn apply(model: &mut EngineeringModel, command: EngineeringCommand) {
    CommandExecutor::apply(model, &command).unwrap();
}

fn declare(model: &mut EngineeringModel, id: &str, value: ParameterValue) {
    apply(
        model,
        EngineeringCommand::DeclareParameter {
            id: pid(id),
            value,
            origin: ParameterOrigin::User,
            authority: AuthorityLevel::L0,
        },
    );
}

fn set_parameter(model: &mut EngineeringModel, id: &str, value: f64) {
    apply(
        model,
        EngineeringCommand::SetParameter {
            target: pid(id),
            value: ParameterValue::Scalar(value),
        },
    );
}

fn plate(thickness: SourcedLength) -> EngineeringModel {
    plate_with(thickness, &[])
}

/// A plate model whose `prelude` parameters are declared before the
/// thickness is set — the executor validates parameter existence eagerly
/// (ADR-032), so parameter-backed thickness needs its parameter to exist
/// first.
fn plate_with(thickness: SourcedLength, prelude: &[(&str, ParameterValue)]) -> EngineeringModel {
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
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    );
    for (id, value) in prelude {
        declare(&mut model, id, value.clone());
    }
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(thickness),
        },
    );
    model
}

fn add_hole(model: &mut EngineeringModel, id: &str, diameter: f64) {
    apply(
        model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: fid(id),
                diameter,
            },
        },
    );
}

fn add_circular_pattern(
    model: &mut EngineeringModel,
    id: &str,
    center: (f64, f64),
    pcd: f64,
    count: u32,
    start_angle: f64,
    diameter: f64,
) {
    apply(
        model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: fid(id),
                pattern: HolePattern::Circular {
                    center: scc_engineering::Point2D::new(center.0, center.1),
                    pitch_circle_diameter: pcd,
                    count,
                    start_angle,
                    hole: HoleDefinition { diameter },
                },
            },
        },
    );
}

fn add_grid_pattern(
    model: &mut EngineeringModel,
    id: &str,
    rows: u32,
    columns: u32,
    spacing_x: f64,
    spacing_y: f64,
    diameter: f64,
) {
    apply(
        model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: fid(id),
                pattern: HolePattern::Grid {
                    rows,
                    columns,
                    spacing_x,
                    spacing_y,
                    hole: HoleDefinition { diameter },
                },
            },
        },
    );
}

fn add_corner_pattern(model: &mut EngineeringModel, id: &str, diameter: f64) {
    apply(
        model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: fid(id),
                pattern: HolePattern::Corner {
                    offset_x: 15.0,
                    offset_y: 15.0,
                    hole: HoleDefinition { diameter },
                },
            },
        },
    );
}

fn add_construction(model: &mut EngineeringModel, id: &str, diameter: f64) {
    apply(
        model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::ConstructionCircle {
                id: fid(id),
                diameter,
            },
        },
    );
}

fn set_extent(model: &mut EngineeringModel, target: &str, extent: HoleExtent) {
    apply(
        model,
        EngineeringCommand::SetHoleExtent {
            target: fid(target),
            extent: Some(extent),
        },
    );
}

/// Solve the 2D pipeline and return the provenance + solved geometry.
fn solve(model: &EngineeringModel) -> (EngineeringSourceMap, GeometryModel) {
    let (lowering, geometry) = lower_and_solve(model).unwrap();
    (lowering.source_map, geometry)
}

/// Run the spatial lowering on explicit inputs.
fn run(
    model: &EngineeringModel,
    geometry: &GeometryModel,
    source_map: &EngineeringSourceMap,
) -> Result<SpatialModel, SpatialError> {
    lower_spatial(SpatialInputs {
        engineering: model,
        geometry,
        source_map,
    })
}

/// Pipeline convenience: solve then lower.
fn lower(model: &EngineeringModel) -> Result<SpatialModel, SpatialError> {
    let (source_map, geometry) = solve(model);
    run(model, &geometry, &source_map)
}

/// Direct-construction geometry model (tamper tests).
fn geom_model(entities: Vec<(&str, GeometryPrimitive)>) -> GeometryModel {
    let mut geometry = GeometryModel::new(1, 0, 0);
    for (id, primitive) in entities {
        geometry.entities.insert(
            eid(id),
            GeometryEntity {
                id: eid(id),
                primitive,
            },
        );
    }
    geometry
}

/// Direct-construction source map (tamper tests).
fn map_of(pairs: Vec<(&str, &str)>) -> EngineeringSourceMap {
    let mut source_map = EngineeringSourceMap::default();
    for (feature, entity) in pairs {
        source_map.record_feature_entity(fid(feature), eid(entity));
    }
    source_map
}

fn plate_rectangle() -> GeometryPrimitive {
    GeometryPrimitive::Rectangle {
        origin: point(0.0, 0.0),
        width: 120.0,
        height: 80.0,
    }
}

fn circle(center: (f64, f64), radius: f64) -> GeometryPrimitive {
    GeometryPrimitive::Circle {
        center: point(center.0, center.1),
        radius,
    }
}

fn assert_issues(error: &SpatialError, expected: &[SpatialIssue]) {
    match error {
        SpatialError::EngineeringNotSpatiallyReady { issues, .. } => {
            for issue in expected {
                assert!(
                    issues.contains(issue),
                    "expected {issue:?} in readiness issues, got {issues:?}"
                );
            }
        }
        other => panic!("expected SCC-SPAT-001, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// readiness gate (Phase 9.1 is the mandatory precondition)
// ---------------------------------------------------------------------------

/// 1: A hole without an extent blocks lowering with its readiness issue.
#[test]
fn readiness_gate_blocks_lowering() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    // No SetHoleExtent: readiness must report MissingHoleExtent.
    let error = lower(&model).unwrap_err();
    assert_issues(
        &error,
        &[SpatialIssue::MissingHoleExtent {
            feature: fid("center_hole"),
        }],
    );
}

/// 2: A legacy Phase-8 model (no spatial data at all) is 2D-valid but is
/// rejected by spatial lowering only.
#[test]
fn legacy_incomplete_rejected() {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("legacy".into()),
            name: "legacy".into(),
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
    add_hole(&mut model, "center_hole", 10.0);
    let error = lower(&model).unwrap_err();
    assert_issues(
        &error,
        &[
            SpatialIssue::MissingThickness,
            SpatialIssue::MissingHoleExtent {
                feature: fid("center_hole"),
            },
        ],
    );
}

// ---------------------------------------------------------------------------
// base body + bounds
// ---------------------------------------------------------------------------

/// 3: The base body footprint is the solved rectangle, verbatim.
#[test]
fn base_slice_120x80x10() {
    let model = plate(SourcedLength::Literal { value: 10.0 });
    let (source_map, geometry) = solve(&model);
    let spatial = run(&model, &geometry, &source_map).unwrap();

    assert!(spatial.cuts.is_empty());
    assert!(spatial.construction_circles.is_empty());
    let scc_spatial::SpatialFootprint::Rectangle {
        origin,
        width,
        height,
    } = spatial.body.footprint
    else {
        panic!("rectangle fixture unexpectedly lowered as a circle");
    };
    let expected = geometry
        .entities
        .get(&eid("plate"))
        .map(|entity| match &entity.primitive {
            GeometryPrimitive::Rectangle {
                origin,
                width,
                height,
            } => (*origin, *width, *height),
            other => panic!("expected solved rectangle, got {other:?}"),
        })
        .unwrap();
    assert_eq!((origin, width, height), expected);
    assert_eq!(
        spatial.body.z,
        ZInterval {
            min: 0.0,
            max: 10.0
        }
    );
    assert_eq!(spatial.body.source_feature, fid("plate"));
    assert_eq!(spatial.body.source_entity, eid("plate"));
}

/// 4: Bounds come from the solved footprint × [0, thickness].
#[test]
fn bounds_from_body_only() {
    let model = plate(SourcedLength::Literal { value: 10.0 });
    let (source_map, geometry) = solve(&model);
    let spatial = run(&model, &geometry, &source_map).unwrap();
    let scc_spatial::SpatialFootprint::Rectangle {
        origin,
        width,
        height,
    } = spatial.body.footprint
    else {
        panic!("rectangle fixture unexpectedly lowered as a circle");
    };
    assert_eq!(
        spatial.bounds,
        Bounds3D {
            min_x: origin.x,
            max_x: origin.x + width,
            min_y: origin.y,
            max_y: origin.y + height,
            min_z: 0.0,
            max_z: 10.0,
        }
    );
}

// ---------------------------------------------------------------------------
// thickness resolution paths
// ---------------------------------------------------------------------------

/// 5: Literal thickness.
#[test]
fn literal_thickness() {
    let model = plate(SourcedLength::Literal { value: 10.0 });
    let spatial = lower(&model).unwrap();
    assert_eq!(spatial.body.z.max, 10.0);
}

/// 6: Parameter-backed thickness.
#[test]
fn parameter_thickness() {
    let model = plate_with(
        SourcedLength::Parameter { id: pid("t") },
        &[("t", ParameterValue::Scalar(10.0))],
    );
    let spatial = lower(&model).unwrap();
    assert_eq!(spatial.body.z.max, 10.0);
}

/// 7: Derived (expression) thickness.
#[test]
fn derived_thickness() {
    let mut model = plate_with(
        SourcedLength::Parameter { id: pid("t") },
        &[
            ("base", ParameterValue::Scalar(5.0)),
            ("t", ParameterValue::Unresolved),
        ],
    );
    apply(
        &mut model,
        EngineeringCommand::SetExpression {
            target: pid("t"),
            expression: Expression::Multiply {
                left: Box::new(Expression::Reference {
                    parameter: pid("base"),
                }),
                right: Box::new(Expression::Constant { value: 2.0 }),
            },
        },
    );
    let spatial = lower(&model).unwrap();
    assert_eq!(spatial.body.z.max, 10.0);
}

// ---------------------------------------------------------------------------
// hole cuts and Z-interval rules
// ---------------------------------------------------------------------------

/// 8: ThroughAll lowers to the full plate interval [0, T].
#[test]
fn through_all_interval() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let spatial = lower(&model).unwrap();
    assert_eq!(spatial.cuts.len(), 1);
    let cut = spatial
        .cuts
        .get(&scc_spatial::SpatialCutId("center_hole:cut:0000".into()))
        .unwrap();
    assert_eq!(
        cut.z,
        ZInterval {
            min: 0.0,
            max: 10.0
        }
    );
    assert_eq!(cut.kind, SpatialCutKind::ThroughAll);
}

/// 9: Blind from the base face: [0, D].
#[test]
fn blind_base_interval() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(
        &mut model,
        "center_hole",
        HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Literal { value: 4.0 },
        },
    );
    let spatial = lower(&model).unwrap();
    let cut = spatial
        .cuts
        .get(&scc_spatial::SpatialCutId("center_hole:cut:0000".into()))
        .unwrap();
    assert_eq!(cut.z, ZInterval { min: 0.0, max: 4.0 });
    assert_eq!(
        cut.kind,
        SpatialCutKind::Blind {
            start_face: HoleStartFace::Base
        }
    );
}

/// 10: Blind from the opposite face: [T−D, T].
#[test]
fn blind_opposite_interval() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(
        &mut model,
        "center_hole",
        HoleExtent::Blind {
            start_face: HoleStartFace::Opposite,
            depth: SourcedLength::Literal { value: 4.0 },
        },
    );
    let spatial = lower(&model).unwrap();
    let cut = spatial
        .cuts
        .get(&scc_spatial::SpatialCutId("center_hole:cut:0000".into()))
        .unwrap();
    assert_eq!(
        cut.z,
        ZInterval {
            min: 6.0,
            max: 10.0
        }
    );
    assert_eq!(
        cut.kind,
        SpatialCutKind::Blind {
            start_face: HoleStartFace::Opposite
        }
    );
}

/// 11: A derived blind depth re-resolves per run: T=10, D=T/2 ⇒ [0, 5].
#[test]
fn derived_depth_interval() {
    let mut model = plate_with(
        SourcedLength::Parameter { id: pid("t") },
        &[("t", ParameterValue::Scalar(10.0))],
    );
    add_hole(&mut model, "center_hole", 10.0);
    declare(&mut model, "d", ParameterValue::Unresolved);
    apply(
        &mut model,
        EngineeringCommand::SetExpression {
            target: pid("d"),
            expression: Expression::divide_parameter_by_constant(pid("t"), 2.0),
        },
    );
    set_extent(
        &mut model,
        "center_hole",
        HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Parameter { id: pid("d") },
        },
    );
    let spatial = lower(&model).unwrap();
    let cut = spatial
        .cuts
        .get(&scc_spatial::SpatialCutId("center_hole:cut:0000".into()))
        .unwrap();
    assert_eq!(cut.z, ZInterval { min: 0.0, max: 5.0 });
}

/// 12: Parameter drift (T 10→6 with literal D=8) is blocked by the Phase 9.1
/// readiness gate; the specific issue is carried verbatim.
#[test]
fn parameter_drift_blocked() {
    let mut model = plate_with(
        SourcedLength::Parameter { id: pid("t") },
        &[("t", ParameterValue::Scalar(10.0))],
    );
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(
        &mut model,
        "center_hole",
        HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Literal { value: 8.0 },
        },
    );
    let (source_map, geometry) = solve(&model);
    assert!(run(&model, &geometry, &source_map).is_ok());

    set_parameter(&mut model, "t", 6.0);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_issues(
        &error,
        &[SpatialIssue::InvalidBlindDepth {
            feature: fid("center_hole"),
            depth: 8.0,
        }],
    );
}

/// 13: The cut center/radius come from the solved circle, witnessed against
/// the engineering diameter.
#[test]
fn hole_cut_center_and_witness() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    declare(&mut model, "hx", ParameterValue::Scalar(20.0));
    declare(&mut model, "hy", ParameterValue::Scalar(30.0));
    add_hole(&mut model, "center_hole", 10.0);
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: fid("center_hole"),
            slot: FeatureParameterSlot::CenterX,
            parameter: pid("hx"),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: fid("center_hole"),
            slot: FeatureParameterSlot::CenterY,
            parameter: pid("hy"),
        },
    );
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let (source_map, geometry) = solve(&model);
    let spatial = run(&model, &geometry, &source_map).unwrap();
    let cut = spatial
        .cut_for_entity(&eid("center_hole"))
        .expect("cut for center_hole");
    let expected_center = match &geometry
        .entities
        .get(&eid("center_hole"))
        .unwrap()
        .primitive
    {
        GeometryPrimitive::Circle { center, .. } => *center,
        other => panic!("expected solved circle, got {other:?}"),
    };
    assert_eq!(cut.center, expected_center);
    assert_eq!(cut.center, point(20.0, 30.0));
    assert_eq!(cut.radius, 5.0);
}

// ---------------------------------------------------------------------------
// patterns
// ---------------------------------------------------------------------------

/// 14: A circular pattern with 8 holes lowers to exactly 8 cuts.
#[test]
fn circular_pattern_eight_cuts() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_circular_pattern(&mut model, "bolt_circle", (0.0, 0.0), 126.0, 8, 0.0, 10.0);
    set_extent(&mut model, "bolt_circle", HoleExtent::ThroughAll);
    let spatial = lower(&model).unwrap();
    assert_eq!(spatial.cuts.len(), 8);
    assert_eq!(spatial.cuts_for_feature(&fid("bolt_circle")).len(), 8);
}

/// 15: A 2×3 grid pattern lowers to exactly 6 cuts (positions from geometry).
#[test]
fn grid_pattern_six_cuts() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_grid_pattern(&mut model, "grid_holes", 2, 3, 20.0, 15.0, 6.0);
    set_extent(&mut model, "grid_holes", HoleExtent::ThroughAll);
    let spatial = lower(&model).unwrap();
    assert_eq!(spatial.cuts.len(), 6);
    assert_eq!(spatial.cuts_for_feature(&fid("grid_holes")).len(), 6);
}

/// 16: The canonical 4-corner pattern lowers to exactly 4 cuts.
#[test]
fn corner_pattern_four_cuts() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_corner_pattern(&mut model, "corner_holes", 10.0);
    set_extent(&mut model, "corner_holes", HoleExtent::ThroughAll);
    let spatial = lower(&model).unwrap();
    assert_eq!(spatial.cuts.len(), 4);
    assert_eq!(spatial.cuts_for_feature(&fid("corner_holes")).len(), 4);
}

/// 17: Every pattern cut shares the pattern FeatureId and keeps its own
/// distinct solved EntityId.
#[test]
fn pattern_shared_feature_distinct_entities() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_circular_pattern(&mut model, "bolt_circle", (0.0, 0.0), 126.0, 8, 0.0, 10.0);
    set_extent(&mut model, "bolt_circle", HoleExtent::ThroughAll);
    let (source_map, geometry) = solve(&model);
    let spatial = run(&model, &geometry, &source_map).unwrap();

    let cuts = spatial.cuts_for_feature(&fid("bolt_circle"));
    assert_eq!(cuts.len(), 8);
    let mut entities = std::collections::BTreeSet::new();
    for (id, cut) in &cuts {
        assert_eq!(cut.source_feature, fid("bolt_circle"));
        assert!(source_map
            .entities_for(&fid("bolt_circle"))
            .contains(&cut.source_entity));
        assert!(geometry.entities.contains_key(&cut.source_entity));
        entities.insert(cut.source_entity.clone());
        assert!(id.0.starts_with("bolt_circle:cut:"));
    }
    assert_eq!(entities.len(), 8, "each cut must witness a distinct entity");
}

/// 18: A pattern missing one solved instance fails closed (direct).
#[test]
fn pattern_missing_entity_fails() {
    // Direct construction: 8 solved circles, only 7 mapped.
    let mut circles = vec![("plate", plate_rectangle())];
    for index in 0..8 {
        circles.push((
            Box::leak(format!("bolt_circle_array[{index}]").into_boxed_str()) as &str,
            circle((10.0 * index as f64, 0.0), 5.0),
        ));
    }
    let geometry = geom_model(circles);
    let mut pairs = vec![("plate", "plate")];
    for index in 0..7 {
        pairs.push((
            "bolt_circle",
            Box::leak(format!("bolt_circle_array[{index}]").into_boxed_str()) as &str,
        ));
    }
    let source_map = map_of(pairs);
    let mut pattern_model = plate(SourcedLength::Literal { value: 10.0 });
    add_circular_pattern(
        &mut pattern_model,
        "bolt_circle",
        (0.0, 0.0),
        126.0,
        8,
        0.0,
        10.0,
    );
    set_extent(&mut pattern_model, "bolt_circle", HoleExtent::ThroughAll);
    let error = run(&pattern_model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::PatternCardinalityMismatch {
            feature: fid("bolt_circle"),
            expected: 8,
            found: 7,
        }
    );
}

/// 19: An extra solved instance fails closed too (direct).
#[test]
fn pattern_extra_entity_fails() {
    let mut circles = vec![("plate", plate_rectangle())];
    for index in 0..9 {
        circles.push((
            Box::leak(format!("bolt_circle_array[{index}]").into_boxed_str()) as &str,
            circle((10.0 * index as f64, 0.0), 5.0),
        ));
    }
    let geometry = geom_model(circles);
    let mut pairs = vec![("plate", "plate")];
    for index in 0..9 {
        pairs.push((
            "bolt_circle",
            Box::leak(format!("bolt_circle_array[{index}]").into_boxed_str()) as &str,
        ));
    }
    let source_map = map_of(pairs);
    let mut pattern_model = plate(SourcedLength::Literal { value: 10.0 });
    add_circular_pattern(
        &mut pattern_model,
        "bolt_circle",
        (0.0, 0.0),
        126.0,
        8,
        0.0,
        10.0,
    );
    set_extent(&mut pattern_model, "bolt_circle", HoleExtent::ThroughAll);
    let error = run(&pattern_model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::PatternCardinalityMismatch {
            feature: fid("bolt_circle"),
            expected: 8,
            found: 9,
        }
    );
}

/// 20: Engineering Ø10 vs solved radius 6 fails the diameter witness (direct).
#[test]
fn diameter_witness_fails() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("center_hole", circle((0.0, 0.0), 6.0)),
    ]);
    let source_map = map_of(vec![("plate", "plate"), ("center_hole", "center_hole")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::SpatialWitnessMismatch {
            feature: fid("center_hole"),
            entity: eid("center_hole"),
            engineering_value: 10.0,
            geometry_value: 12.0,
        }
    );
}

/// 21: A solved circle claimed by no spatial role fails closed (direct).
#[test]
fn unmapped_entity_fails() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    // The hole itself is mapped correctly; a stray solved circle is not.
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("center_hole", circle((0.0, 0.0), 5.0)),
        ("orphan_circle", circle((40.0, 40.0), 1.0)),
    ]);
    let source_map = map_of(vec![("plate", "plate"), ("center_hole", "center_hole")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::UnmappedGeometryEntity {
            entity: eid("orphan_circle"),
        }
    );
}

/// 22: A hole circle mapped under a foreign (construction) feature id fails
/// closed: the hole has no provenance of its own.
#[test]
fn wrong_provenance_fails() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    add_construction(&mut model, "cc", 20.0);
    // Direct construction: the hole's circle is mapped under `cc`.
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("center_hole", circle((0.0, 0.0), 5.0)),
        ("cc", circle((60.0, 40.0), 10.0)),
    ]);
    let source_map = map_of(vec![
        ("plate", "plate"),
        ("cc", "center_hole"),
        ("cc", "cc"),
    ]);
    // The amendment (§4-A) rule: the hole finds no solved entity of its own
    // ⇒ MissingSpatialSource (the circle is claimed by the construction
    // feature, so the model never silently reinterprets it as the hole).
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::MissingSpatialSource {
            feature: fid("center_hole"),
        }
    );
}

// ---------------------------------------------------------------------------
// construction circles (reference-only, policy B)
// ---------------------------------------------------------------------------

/// 23: A giant construction circle changes neither body, cuts, nor bounds.
#[test]
fn construction_circle_bounds_unchanged() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let without = lower(&model).unwrap();

    add_construction(&mut model, "big_cc", 500.0);
    let with = lower(&model).unwrap();

    assert_eq!(without.body, with.body);
    assert_eq!(without.bounds, with.bounds);
    assert_eq!(without.cuts, with.cuts);
    assert!(with.construction_circles.contains_key(&eid("big_cc")));
}

/// 24: Construction circles are recorded reference-only (entity → feature).
#[test]
fn construction_circle_recorded() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_construction(&mut model, "cc", 30.0);
    let spatial = lower(&model).unwrap();
    assert!(spatial.cuts.is_empty());
    assert_eq!(
        spatial.construction_circles.get(&eid("cc")),
        Some(&fid("cc"))
    );
}

/// 25: A cut far outside the body never expands the outer bounds (direct).
#[test]
fn cuts_do_not_expand_bounds() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "far_hole", 10.0);
    set_extent(&mut model, "far_hole", HoleExtent::ThroughAll);
    // Engineering has no position invariant for holes in Phase 9.2, so the
    // solved circle sits far outside the footprint by direct construction.
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("far_hole", circle((500.0, 500.0), 5.0)),
    ]);
    let source_map = map_of(vec![("plate", "plate"), ("far_hole", "far_hole")]);
    let spatial = run(&model, &geometry, &source_map).unwrap();
    assert_eq!(
        spatial.bounds,
        Bounds3D {
            min_x: 0.0,
            max_x: 120.0,
            min_y: 0.0,
            max_y: 80.0,
            min_z: 0.0,
            max_z: 10.0,
        }
    );
}

// ---------------------------------------------------------------------------
// fail-closed integrity (direct construction)
// ---------------------------------------------------------------------------

/// 26: A BaseProfile solved as a circle is not the spatial body (direct).
#[test]
fn base_primitive_mismatch_fails() {
    let model = plate(SourcedLength::Literal { value: 10.0 });
    let geometry = geom_model(vec![("plate", circle((0.0, 0.0), 60.0))]);
    let source_map = map_of(vec![("plate", "plate")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidBaseRepresentation {
            feature: fid("plate"),
            reason: InvalidBaseReason::NotRectangle,
        }
    );
}

/// 27: A Hole solved as a rectangle is not a cut circle (direct).
#[test]
fn hole_primitive_mismatch_fails() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        (
            "center_hole",
            GeometryPrimitive::Rectangle {
                origin: point(0.0, 0.0),
                width: 10.0,
                height: 10.0,
            },
        ),
    ]);
    let source_map = map_of(vec![("plate", "plate"), ("center_hole", "center_hole")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidSpatialGeometry {
            entity: eid("center_hole"),
            reason: InvalidGeometryReason::NotCircle {
                found: scc_spatial::PrimitiveKind::Rectangle,
            },
        }
    );
}

/// 28: Non-finite solved geometry is rejected (direct).
#[test]
fn non_finite_geometry_fails() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("center_hole", circle((0.0, 0.0), f64::NAN)),
    ]);
    let source_map = map_of(vec![("plate", "plate"), ("center_hole", "center_hole")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidSpatialGeometry {
            entity: eid("center_hole"),
            reason: InvalidGeometryReason::NonFiniteRadius,
        }
    );
}

/// 29: A spatial-ready model with empty geometry fails closed (direct).
#[test]
fn empty_geometry_fails() {
    let model = plate(SourcedLength::Literal { value: 10.0 });
    let geometry = GeometryModel::new(1, 0, 0);
    let source_map = EngineeringSourceMap::default();
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidBaseRepresentation {
            feature: fid("plate"),
            reason: InvalidBaseReason::NoSolvedEntity,
        }
    );
}

/// 30: Missing base representation, both variants.
#[test]
fn base_missing_cases() {
    // (a) Base feature exists but nothing is mapped/solved for it.
    let model = plate(SourcedLength::Literal { value: 10.0 });
    let geometry = geom_model(vec![("unrelated", circle((0.0, 0.0), 1.0))]);
    let source_map = map_of(vec![("unrelated_feature", "unrelated")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidBaseRepresentation {
            feature: fid("plate"),
            reason: InvalidBaseReason::NoSolvedEntity,
        }
    );

    // (b) No BaseProfile feature at all (thickness + declared hole extent
    // keep the model spatially ready).
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("orphan".into()),
            name: "orphan".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let error = lower(&model).unwrap_err();
    assert_eq!(error, SpatialError::MissingBaseRepresentation);
}

// ---------------------------------------------------------------------------
// serialization + determinism
// ---------------------------------------------------------------------------

/// 31: SpatialModel round-trips through JSON.
#[test]
fn serde_roundtrip() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    add_construction(&mut model, "cc", 30.0);
    let spatial = lower(&model).unwrap();
    let json = serde_json::to_string(&spatial).unwrap();
    let back: SpatialModel = serde_json::from_str(&json).unwrap();
    assert_eq!(back, spatial);
}

/// 32: Identical inputs produce identical (byte-stable) JSON.
#[test]
fn deterministic_json_ordering() {
    let build = || {
        let mut model = plate(SourcedLength::Literal { value: 10.0 });
        add_hole(&mut model, "a_hole", 10.0);
        set_extent(&mut model, "a_hole", HoleExtent::ThroughAll);
        add_circular_pattern(&mut model, "b_pattern", (0.0, 0.0), 60.0, 4, 0.0, 10.0);
        set_extent(&mut model, "b_pattern", HoleExtent::ThroughAll);
        lower(&model).unwrap()
    };
    let first = build();
    let second = build();
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
    // Cuts serialize in ascending SpatialCutId order (BTreeMap).
    let json = serde_json::to_string(&first).unwrap();
    let mut last_id = String::new();
    let mut count = 0;
    for key in first.cuts.keys() {
        assert!(key.0.as_str() > last_id.as_str(), "cuts out of order");
        last_id = key.0.clone();
        count += 1;
    }
    assert_eq!(count, 5);
    assert!(json.contains("\"cuts\":{"));
}

/// 33: The full pipeline is deterministic over 100 runs (composite fixture).
#[test]
fn end_to_end_determinism_100_runs() {
    let build = || {
        let mut model = plate_with(
            SourcedLength::Parameter { id: pid("t") },
            &[("t", ParameterValue::Scalar(10.0))],
        );
        add_hole(&mut model, "center_hole", 10.0);
        set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
        add_circular_pattern(&mut model, "bolt_circle", (0.0, 0.0), 126.0, 8, 0.0, 10.0);
        set_extent(&mut model, "bolt_circle", HoleExtent::ThroughAll);
        add_construction(&mut model, "cc", 40.0);
        let (source_map, geometry) = solve(&model);
        let spatial = run(&model, &geometry, &source_map).unwrap();
        (
            serde_json::to_string(&spatial).unwrap(),
            serde_json::to_string(&geometry).unwrap(),
        )
    };
    let (spatial_json, geometry_json) = build();
    for _ in 0..100 {
        let (next_spatial, next_geometry) = build();
        assert_eq!(next_spatial, spatial_json, "SpatialModel diverged");
        assert_eq!(next_geometry, geometry_json, "GeometryModel diverged");
    }
}

// ---------------------------------------------------------------------------
// parameter / geometry propagation
// ---------------------------------------------------------------------------

/// Composite fixture for propagation tests: parameter thickness + one hole
/// of each extent semantics.
fn propagation_model() -> EngineeringModel {
    let mut model = plate_with(
        SourcedLength::Parameter { id: pid("t") },
        &[("t", ParameterValue::Scalar(10.0))],
    );
    add_hole(&mut model, "through_hole", 10.0);
    set_extent(&mut model, "through_hole", HoleExtent::ThroughAll);
    add_hole(&mut model, "blind_base_hole", 10.0);
    set_extent(
        &mut model,
        "blind_base_hole",
        HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Literal { value: 3.0 },
        },
    );
    add_hole(&mut model, "blind_opposite_hole", 10.0);
    set_extent(
        &mut model,
        "blind_opposite_hole",
        HoleExtent::Blind {
            start_face: HoleStartFace::Opposite,
            depth: SourcedLength::Literal { value: 4.0 },
        },
    );
    model
}

fn cut_of<'a>(spatial: &'a SpatialModel, feature: &str) -> &'a scc_spatial::CylindricalCut {
    spatial
        .cuts
        .get(&scc_spatial::SpatialCutId(format!("{feature}:cut:0000")))
        .unwrap_or_else(|| panic!("missing cut for {feature}"))
}

/// 34: T 10→12 propagates: body z and ThroughAll cuts follow, blind base
/// stays fixed, blind opposite z_min = T−D moves.
#[test]
fn thickness_propagation() {
    let mut model = propagation_model();
    let before = lower(&model).unwrap();
    assert_eq!(
        before.body.z,
        ZInterval {
            min: 0.0,
            max: 10.0
        }
    );
    assert_eq!(cut_of(&before, "through_hole").z.max, 10.0);
    assert_eq!(
        cut_of(&before, "blind_base_hole").z,
        ZInterval { min: 0.0, max: 3.0 }
    );
    assert_eq!(
        cut_of(&before, "blind_opposite_hole").z,
        ZInterval {
            min: 6.0,
            max: 10.0
        }
    );

    set_parameter(&mut model, "t", 12.0);
    let after = lower(&model).unwrap();
    assert_eq!(
        after.body.z,
        ZInterval {
            min: 0.0,
            max: 12.0
        }
    );
    assert_eq!(
        cut_of(&after, "through_hole").z,
        ZInterval {
            min: 0.0,
            max: 12.0
        }
    );
    assert_eq!(
        cut_of(&after, "blind_base_hole").z,
        ZInterval { min: 0.0, max: 3.0 }
    );
    assert_eq!(
        cut_of(&after, "blind_opposite_hole").z,
        ZInterval {
            min: 8.0,
            max: 12.0
        }
    );
}

/// 35: A derived depth (D = T/2) re-resolves: [0,5] → [0,6] when T goes
/// 10 → 12. No cached interval anywhere.
#[test]
fn derived_depth_propagation() {
    let mut model = plate_with(
        SourcedLength::Parameter { id: pid("t") },
        &[("t", ParameterValue::Scalar(10.0))],
    );
    add_hole(&mut model, "center_hole", 10.0);
    declare(&mut model, "d", ParameterValue::Unresolved);
    apply(
        &mut model,
        EngineeringCommand::SetExpression {
            target: pid("d"),
            expression: Expression::divide_parameter_by_constant(pid("t"), 2.0),
        },
    );
    set_extent(
        &mut model,
        "center_hole",
        HoleExtent::Blind {
            start_face: HoleStartFace::Base,
            depth: SourcedLength::Parameter { id: pid("d") },
        },
    );
    let before = lower(&model).unwrap();
    assert_eq!(
        cut_of(&before, "center_hole").z,
        ZInterval { min: 0.0, max: 5.0 }
    );

    set_parameter(&mut model, "t", 12.0);
    let after = lower(&model).unwrap();
    assert_eq!(
        cut_of(&after, "center_hole").z,
        ZInterval { min: 0.0, max: 6.0 }
    );
}

/// 36: Moving a hole (bound center parameters) propagates through solve into
/// the cut center — spatial never reads a stale position.
#[test]
fn hole_position_propagation() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    declare(&mut model, "hx", ParameterValue::Scalar(20.0));
    declare(&mut model, "hy", ParameterValue::Scalar(30.0));
    add_hole(&mut model, "center_hole", 10.0);
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: fid("center_hole"),
            slot: FeatureParameterSlot::CenterX,
            parameter: pid("hx"),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: fid("center_hole"),
            slot: FeatureParameterSlot::CenterY,
            parameter: pid("hy"),
        },
    );
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);

    let before = lower(&model).unwrap();
    assert_eq!(cut_of(&before, "center_hole").center, point(20.0, 30.0));

    set_parameter(&mut model, "hx", 30.0);
    let after = lower(&model).unwrap();
    assert_eq!(cut_of(&after, "center_hole").center, point(30.0, 30.0));
}

/// 37: Changing the PCD re-solves the circle positions; the spatial cuts
/// match the new solved circles exactly (never recomputed from PCD here).
#[test]
fn pcd_propagation_matches_geometry() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    declare(&mut model, "pcd", ParameterValue::Scalar(100.0));
    add_circular_pattern(&mut model, "bolt_circle", (0.0, 0.0), 100.0, 8, 0.0, 10.0);
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: fid("bolt_circle"),
            slot: FeatureParameterSlot::PitchCircleDiameter,
            parameter: pid("pcd"),
        },
    );
    set_extent(&mut model, "bolt_circle", HoleExtent::ThroughAll);

    let (source_map, geometry) = solve(&model);
    let before = run(&model, &geometry, &source_map).unwrap();
    assert_eq!(before.cuts.len(), 8);

    set_parameter(&mut model, "pcd", 126.0);
    let (source_map, geometry) = solve(&model);
    let after = run(&model, &geometry, &source_map).unwrap();
    assert_eq!(after.cuts.len(), 8);

    // Every cut center is EXACTLY its solved circle's center.
    for cut in after.cuts.values() {
        let solved = &geometry.entities.get(&cut.source_entity).unwrap().primitive;
        match solved {
            GeometryPrimitive::Circle { center, radius } => {
                assert_eq!(cut.center, *center);
                assert_eq!(cut.radius, *radius);
            }
            other => panic!("expected circle, got {other:?}"),
        }
    }
    // The circles really moved (the propagation is observable).
    let before_centers: Vec<Point2> = before.cuts.values().map(|c| c.center).collect();
    let after_centers: Vec<Point2> = after.cuts.values().map(|c| c.center).collect();
    assert_ne!(before_centers, after_centers);
}

/// 38: Removing a hole feature removes its cut — no orphan spatial
/// primitives survive re-lowering.
#[test]
fn feature_removal_no_orphan() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    add_hole(&mut model, "second_hole", 8.0);
    set_extent(&mut model, "second_hole", HoleExtent::ThroughAll);
    let before = lower(&model).unwrap();
    assert_eq!(before.cuts.len(), 2);

    apply(
        &mut model,
        EngineeringCommand::RemoveFeature {
            target: fid("center_hole"),
        },
    );
    let after = lower(&model).unwrap();
    assert_eq!(after.cuts.len(), 1);
    assert!(after.cut_for_entity(&eid("center_hole")).is_none());
    assert!(after.cut_for_entity(&eid("second_hole")).is_some());
}

// ---------------------------------------------------------------------------
// removal of spatial intent blocks lowering (2D drawing still works)
// ---------------------------------------------------------------------------

/// 39: Clearing the thickness fails readiness; spatial lowering refuses.
#[test]
fn unset_thickness_blocks() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness { source: None },
    );
    let error = lower(&model).unwrap_err();
    assert_issues(&error, &[SpatialIssue::MissingThickness]);
}

/// 40: Clearing a hole extent fails readiness; spatial lowering refuses.
#[test]
fn unset_extent_blocks() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    apply(
        &mut model,
        EngineeringCommand::SetHoleExtent {
            target: fid("center_hole"),
            extent: None,
        },
    );
    let error = lower(&model).unwrap_err();
    assert_issues(
        &error,
        &[SpatialIssue::MissingHoleExtent {
            feature: fid("center_hole"),
        }],
    );
}

/// 41: Fillet features make the model not spatially ready at all —
/// the gate fires before any lowering work. Slot is a supported through cut.
#[test]
fn unsupported_feature_gate() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Fillet {
                id: fid("fillet_1"),
                radius: 3.0,
            },
        },
    );
    // No geometry/source map is even needed: the gate fires first.
    let geometry = GeometryModel::new(1, 0, 0);
    let source_map = EngineeringSourceMap::default();
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_issues(
        &error,
        &[SpatialIssue::UnsupportedForSpatial {
            feature: fid("fillet_1"),
        }],
    );
}

// ---------------------------------------------------------------------------
// more source-map integrity (direct construction)
// ---------------------------------------------------------------------------

/// 42: A mapped entity absent from the solved geometry fails closed.
#[test]
fn entity_not_in_geometry_fails() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let geometry = geom_model(vec![("plate", plate_rectangle())]);
    let source_map = map_of(vec![("plate", "plate"), ("center_hole", "center_hole")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidSpatialSource {
            feature: fid("center_hole"),
            entity: eid("center_hole"),
            reason: scc_spatial::InvalidSpatialSourceReason::NotInGeometry,
        }
    );
}

/// 43: A solved circle claimed by two features fails closed as foreign
/// provenance: the inverse map resolves the doubly-claimed entity to the
/// last-recording feature, so lowering the earlier feature sees a foreign
/// owner (EngineeringSourceMap derives the inverse from the same forward
/// storage — a double claim is the strongest representable tamper).
#[test]
fn foreign_feature_fails() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "a_hole", 10.0);
    set_extent(&mut model, "a_hole", HoleExtent::ThroughAll);
    add_hole(&mut model, "b_hole", 10.0);
    set_extent(&mut model, "b_hole", HoleExtent::ThroughAll);
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("a_hole", circle((-25.0, 0.0), 5.0)),
        ("b_hole", circle((25.0, 0.0), 5.0)),
    ]);
    // Both holes claim a_hole's solved circle: the inverse provenance of
    // that entity resolves to `b_hole`, so lowering `a_hole` fails closed.
    let source_map = map_of(vec![
        ("plate", "plate"),
        ("a_hole", "a_hole"),
        ("b_hole", "a_hole"),
    ]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidSpatialSource {
            feature: fid("a_hole"),
            entity: eid("a_hole"),
            reason: scc_spatial::InvalidSpatialSourceReason::ForeignFeature,
        }
    );
}

/// 44: More than one mapped base entity is rejected (direct).
#[test]
fn duplicate_base_entity_fails() {
    let model = plate(SourcedLength::Literal { value: 10.0 });
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("plate_copy", plate_rectangle()),
    ]);
    let source_map = map_of(vec![("plate", "plate"), ("plate", "plate_copy")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidBaseRepresentation {
            feature: fid("plate"),
            reason: InvalidBaseReason::NotUnique,
        }
    );
}

/// 45: The array template entity recorded in the source map is a lowering
/// artifact: it is absent from GeometryModel (removed by array expansion),
/// carries no claim, and the cut count equals the solved instance count.
#[test]
fn pattern_template_entity_is_lowering_artifact() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_circular_pattern(&mut model, "bolt_circle", (0.0, 0.0), 126.0, 4, 0.0, 10.0);
    set_extent(&mut model, "bolt_circle", HoleExtent::ThroughAll);
    let (source_map, geometry) = solve(&model);

    // Template + 4 instances are recorded as the pattern's provenance…
    let mapped = source_map.entities_for(&fid("bolt_circle"));
    assert_eq!(mapped.len(), 5);
    assert!(mapped.contains(&eid("bolt_circle")), "template recorded");

    // …but the template entity does not exist in the solved geometry…
    assert!(!geometry.entities.contains_key(&eid("bolt_circle")));
    assert_eq!(
        geometry
            .entities
            .keys()
            .filter(|entity| entity.0.starts_with("bolt_circle_array"))
            .count(),
        4
    );

    // …and the spatial model contains exactly the 4 solved cuts.
    let spatial = run(&model, &geometry, &source_map).unwrap();
    assert_eq!(spatial.cuts.len(), 4);
    assert_eq!(spatial.cuts_for_feature(&fid("bolt_circle")).len(), 4);
}

// ---------------------------------------------------------------------------
// defensive diameter gates (readiness does not check diameters)
// ---------------------------------------------------------------------------

/// 46: A hole diameter bound to an unresolved parameter fails closed with a
/// structured defensive error (SCC-SPAT-016) — never a panic. Direct
/// construction: the full pipeline already fails earlier in the engineering
/// lowering, but the public `lower_spatial` API must be fail-closed too.
#[test]
fn unresolved_hole_diameter_fails_closed() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_hole(&mut model, "center_hole", 10.0);
    declare(&mut model, "d", ParameterValue::Unresolved);
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: fid("center_hole"),
            slot: FeatureParameterSlot::Diameter,
            parameter: pid("d"),
        },
    );
    set_extent(&mut model, "center_hole", HoleExtent::ThroughAll);
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("center_hole", circle((0.0, 0.0), 5.0)),
    ]);
    let source_map = map_of(vec![("plate", "plate"), ("center_hole", "center_hole")]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidHoleDiameterDefensive {
            feature: fid("center_hole"),
        }
    );
}

/// 47: Same defensive gate for a pattern's hole diameter (both call sites
/// of the diameter resolution are fail-closed).
#[test]
fn unresolved_pattern_diameter_fails_closed() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_circular_pattern(&mut model, "bolt_circle", (0.0, 0.0), 60.0, 2, 0.0, 10.0);
    declare(&mut model, "d", ParameterValue::Unresolved);
    apply(
        &mut model,
        EngineeringCommand::BindFeatureParameter {
            feature: fid("bolt_circle"),
            slot: FeatureParameterSlot::Diameter,
            parameter: pid("d"),
        },
    );
    set_extent(&mut model, "bolt_circle", HoleExtent::ThroughAll);
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("bolt_circle_array[0]", circle((-15.0, 0.0), 5.0)),
        ("bolt_circle_array[1]", circle((15.0, 0.0), 5.0)),
    ]);
    let source_map = map_of(vec![
        ("plate", "plate"),
        ("bolt_circle", "bolt_circle_array[0]"),
        ("bolt_circle", "bolt_circle_array[1]"),
    ]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidHoleDiameterDefensive {
            feature: fid("bolt_circle"),
        }
    );
}

#[test]
fn rounded_rectangle_spatial_is_exact_composite_not_bbox_rectangle() {
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
                    center: scc_engineering::Point2D::new(30.0, -20.0),
                    width: 120.0,
                    height: 80.0,
                    corner_radius: 10.0,
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 8.0 }),
        },
    );
    let spatial = lower(&model).unwrap();
    assert_eq!(
        spatial.body.profile,
        scc_spatial::SpatialBaseProfile::RoundedRectangle {
            center: point(30.0, -20.0),
            width: 120.0,
            height: 80.0,
            corner_radius: 10.0,
        }
    );
    match &spatial.body.footprint {
        scc_spatial::SpatialFootprint::Composite { segments } => {
            assert_eq!(segments.len(), 8);
            assert!(segments.iter().any(|segment| {
                matches!(segment, scc_spatial::SpatialProfileSegment::Arc { .. })
            }));
        }
        other => panic!("rounded rectangle must not lower to {other:?}"),
    }
    assert_eq!(spatial.bounds.min_x, -30.0);
    assert_eq!(spatial.bounds.max_x, 90.0);
    assert_eq!(spatial.bounds.min_y, -60.0);
    assert_eq!(spatial.bounds.max_y, 20.0);
    assert_ne!(
        spatial.body.footprint,
        scc_spatial::SpatialFootprint::Rectangle {
            origin: point(-30.0, -60.0),
            width: 120.0,
            height: 80.0,
        }
    );
}

/// 48: A solved circle claimed by two construction features fails closed:
/// the construction branch cross-checks inverse provenance exactly like the
/// cut branches, so a double claim can never be silently absorbed as a
/// reference-only record.
#[test]
fn construction_foreign_claim_fails() {
    let mut model = plate(SourcedLength::Literal { value: 10.0 });
    add_construction(&mut model, "a_cc", 30.0);
    add_construction(&mut model, "b_cc", 30.0);
    let geometry = geom_model(vec![
        ("plate", plate_rectangle()),
        ("shared_cc", circle((60.0, 40.0), 15.0)),
    ]);
    let source_map = map_of(vec![
        ("plate", "plate"),
        ("a_cc", "shared_cc"),
        ("b_cc", "shared_cc"),
    ]);
    let error = run(&model, &geometry, &source_map).unwrap_err();
    assert_eq!(
        error,
        SpatialError::InvalidSpatialSource {
            feature: fid("a_cc"),
            entity: eid("shared_cc"),
            reason: scc_spatial::InvalidSpatialSourceReason::ForeignFeature,
        }
    );
}

#[test]
fn polygon_spatial_edges_follow_typed_canonical_indices_past_lexical_id_order() {
    let vertices = vec![
        (-50.0, 0.0),
        (-43.0, -25.0),
        (-25.0, -43.0),
        (0.0, -50.0),
        (25.0, -43.0),
        (43.0, -25.0),
        (50.0, 0.0),
        (43.0, 25.0),
        (25.0, 43.0),
        (0.0, 50.0),
        (-25.0, 43.0),
        (-43.0, 25.0),
    ];
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("polygon_order".into()),
            name: "polygon order".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("polygon_base"),
                name: "polygon_base".into(),
                profile: scc_engineering::BaseProfile::Polygon {
                    vertices: vertices
                        .iter()
                        .map(|(x, y)| scc_engineering::Point2D::new(*x, *y))
                        .collect(),
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

    let (source_map, geometry) = solve(&model);
    let spatial = run(&model, &geometry, &source_map).unwrap();
    assert_eq!(
        spatial.body.profile,
        scc_spatial::SpatialBaseProfile::Polygon
    );
    let canonical = scc_engineering::profile::canonicalize_polygon(
        &vertices
            .iter()
            .map(|(x, y)| scc_engineering::Point2D::new(*x, *y))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let scc_spatial::SpatialFootprint::Composite { segments } = &spatial.body.footprint else {
        panic!("Polygon must retain the exact composite footprint");
    };
    assert_eq!(segments.len(), canonical.len());
    for (index, segment) in segments.iter().enumerate() {
        let (start, end, source_entity, segment_index) = match segment {
            scc_spatial::SpatialProfileSegment::Line {
                start,
                end,
                source_entity,
                segment_index,
            } => (*start, *end, source_entity, *segment_index),
            scc_spatial::SpatialProfileSegment::Arc { .. } => {
                panic!("Engineering Polygon must lower to Lines only")
            }
        };
        let expected_start = &canonical[index];
        let expected_end = &canonical[(index + 1) % canonical.len()];
        assert_eq!(start.x, expected_start.x);
        assert_eq!(start.y, expected_start.y);
        assert_eq!(end.x, expected_end.x);
        assert_eq!(end.y, expected_end.y);
        assert_eq!(segment_index, index as u32);
        assert_eq!(
            source_entity,
            &eid(&format!("polygon_base_segment_{index}"))
        );
        let provenance = source_map.profile_segment(source_entity).unwrap();
        assert_eq!(provenance.segment_index, index as u32);
        assert_eq!(
            provenance.semantic,
            scc_engineering::source_map::ProfileSegmentSemantic::PolygonEdge {
                index: index as u32
            }
        );
    }
}

#[test]
fn composite_spatial_segments_follow_typed_source_indices_past_lexical_id_order() {
    let vertices = [
        (-50.0, 0.0),
        (-43.0, -25.0),
        (-25.0, -43.0),
        (0.0, -50.0),
        (25.0, -43.0),
        (43.0, -25.0),
        (50.0, 0.0),
        (43.0, 25.0),
        (25.0, 43.0),
        (0.0, 50.0),
        (-25.0, 43.0),
        (-43.0, 25.0),
    ];
    let source_segments = vertices
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = vertices[(index + 1) % vertices.len()];
            scc_engineering::ProfileSegment::Line {
                start: scc_engineering::Point2D::new(start.0, start.1),
                end: scc_engineering::Point2D::new(end.0, end.1),
            }
        })
        .collect();

    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("composite_order".into()),
            name: "composite order".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: fid("composite_base"),
                name: "composite_base".into(),
                profile: scc_engineering::BaseProfile::Composite {
                    segments: source_segments,
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

    let (source_map, geometry) = solve(&model);
    let spatial = run(&model, &geometry, &source_map).unwrap();
    assert_eq!(
        spatial.body.profile,
        scc_spatial::SpatialBaseProfile::Composite
    );
    let scc_spatial::SpatialFootprint::Composite { segments } = &spatial.body.footprint else {
        panic!("Composite must retain its exact line contour");
    };
    assert_eq!(segments.len(), vertices.len());
    for (index, segment) in segments.iter().enumerate() {
        let (start, end, source_entity, segment_index) = match segment {
            scc_spatial::SpatialProfileSegment::Line {
                start,
                end,
                source_entity,
                segment_index,
            } => (*start, *end, source_entity, *segment_index),
            scc_spatial::SpatialProfileSegment::Arc { .. } => {
                panic!("line-only Composite fixture must remain line-only")
            }
        };
        assert_eq!(start.x, vertices[index].0);
        assert_eq!(start.y, vertices[index].1);
        assert_eq!(end.x, vertices[(index + 1) % vertices.len()].0);
        assert_eq!(end.y, vertices[(index + 1) % vertices.len()].1);
        assert_eq!(segment_index, index as u32);
        assert_eq!(
            source_entity,
            &eid(&format!("composite_base_segment_{index}"))
        );
        let provenance = source_map.profile_segment(source_entity).unwrap();
        assert_eq!(provenance.feature, fid("composite_base"));
        assert_eq!(provenance.segment_index, index as u32);
        assert_eq!(
            provenance.semantic,
            scc_engineering::source_map::ProfileSegmentSemantic::CompositeSegment
        );
    }
}
