//! End-to-end vertical slices: DSL → commands → model → lowering → SCC →
//! GeometryModel (task §48/§49/§50, plus determinism).

use scc_dsl::command_builder::apply_document;
use scc_dsl::{build_commands, compile, parse_document};
use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::lowering::{lower_and_solve, lower_engineering_model};
use scc_engineering::model::EngineeringModel;
use scc_geometry::model::{GeometryModel, GeometryPrimitive};

const SLICE_CORNER: &str = r#"
AICAD 1

PART mounting_plate "Mounting Plate"

UNITS mm
ORIGIN CENTER

BASE_RECT main
  WIDTH 120
  HEIGHT 80

HOLE_PATTERN mounting CORNER
  BASE main
  OFFSET_X 15
  OFFSET_Y 15
  DIA 10
"#;

const SLICE_CIRCULAR: &str = r#"
AICAD 1

PART circular_plate

BASE_RECT main
  WIDTH 180
  HEIGHT 180

HOLE center_hole
  CENTER 0 0
  DIA 64

HOLE_PATTERN bolts CIRCULAR
  CENTER 0 0
  PCD 126
  COUNT 8
  DIA 10
  START_ANGLE 0
"#;

const SLICE_PARAM: &str = r#"
AICAD 1

PART param_plate

PARAM plate_width = 120
PARAM plate_height = 80
PARAM edge_offset = 15
PARAM hole_diameter := plate_height / 8

BASE_RECT main
  WIDTH 120
  HEIGHT 80

BIND main WIDTH <- plate_width
BIND main HEIGHT <- plate_height

HOLE_PATTERN mounting CORNER
  BASE main
  OFFSET_X 15
  OFFSET_Y 15
  DIA 10

BIND mounting OFFSET_X <- edge_offset
BIND mounting OFFSET_Y <- edge_offset
BIND mounting DIAMETER <- hole_diameter
"#;

const SLICE_CIRCULAR_BASE: &str = r#"
AICAD 1

PART circular_flange

PARAM pcd = 120
PARAM outer_diameter := pcd + 40

BASE_CIRCLE main
  CENTER 30 -20
  DIA 160

BIND main DIAMETER <- outer_diameter
"#;

fn solve(source: &str) -> (EngineeringModel, GeometryModel) {
    let mut model = EngineeringModel::new();
    apply_document(&mut model, source).unwrap();
    let (_result, geometry) = lower_and_solve(&model).unwrap();
    (model, geometry)
}

fn circle_centers(geometry: &GeometryModel) -> Vec<(f64, f64)> {
    let mut centers = Vec::new();
    for entity in geometry.entities.values() {
        if let GeometryPrimitive::Circle { center, .. } = &entity.primitive {
            centers.push((center.x, center.y));
        }
    }
    centers.sort_by(|a, b| a.partial_cmp(b).unwrap());
    centers
}

// ---- Level 4: DSL → EngineeringCommand ----

#[test]
fn slot_lowers_to_typed_slot_feature() {
    let source = r#"
AICAD 1
PART plate
BASE_RECT main
  WIDTH 120
  HEIGHT 80
SLOT s1
  CENTER 0 0
  LENGTH 40
  WIDTH 12
  ANGLE 45
  THRU
"#;
    let commands = compile(source).unwrap();
    assert!(commands.iter().any(|command| matches!(
        command,
        EngineeringCommand::AddFeature {
            feature: scc_engineering::feature::EngineeringFeature::Slot {
                length: 40.0,
                width: 12.0,
                orientation_degrees: 45.0,
                ..
            }
        }
    )));
    let mut model = EngineeringModel::new();
    apply_document(&mut model, source).unwrap();
    assert!(matches!(
        model.features.get(&scc_engineering::FeatureId("s1".into())),
        Some(scc_engineering::feature::EngineeringFeature::Slot { .. })
    ));
}

#[test]
fn base_rect_lowers_to_add_feature() {
    let commands = compile("AICAD 1\nPART p\nBASE_RECT main\n  WIDTH 120\n  HEIGHT 80\n").unwrap();
    assert!(matches!(commands[1], EngineeringCommand::AddFeature { .. }));
}

#[test]
fn base_circle_lowers_to_typed_circle_with_center_bindings() {
    let commands =
        compile("AICAD 1\nPART p\nBASE_CIRCLE main\n  CENTER 30 -20\n  DIA 160\n").unwrap();
    assert!(commands.iter().any(|command| matches!(
        command,
        EngineeringCommand::AddFeature {
            feature: scc_engineering::feature::EngineeringFeature::BaseProfile {
                profile: scc_engineering::BaseProfile::Circle { center, diameter, .. },
                ..
            }
        } if center.x == 30.0 && center.y == -20.0 && *diameter == 160.0
    )));
}

#[test]
fn base_rounded_rect_lowers_to_typed_profile_with_center_bindings() {
    let commands = compile(
        "AICAD 1\nPART p\nBASE_ROUNDED_RECT main\n  CENTER 10 -5\n  WIDTH 120\n  HEIGHT 80\n  RADIUS 10\n",
    )
    .unwrap();
    assert!(commands.iter().any(|command| matches!(
        command,
        EngineeringCommand::AddFeature {
            feature: scc_engineering::feature::EngineeringFeature::BaseProfile {
                profile: scc_engineering::BaseProfile::RoundedRectangle {
                    center, width, height, corner_radius
                },
                ..
            }
        } if center.x == 10.0
            && center.y == -5.0
            && *width == 120.0
            && *height == 80.0
            && *corner_radius == 10.0
    )));
}

#[test]
fn base_polygon_lowers_to_typed_profile() {
    let source = "AICAD 1\nPART p\nBASE_POLYGON main\n  POINT 0 0\n  POINT 100 0\n  POINT 120 40\n  POINT 80 90\n  POINT 0 60\n";
    let commands = compile(source).unwrap();
    assert!(commands.iter().any(|command| matches!(
        command,
        EngineeringCommand::AddFeature {
            feature: scc_engineering::feature::EngineeringFeature::BaseProfile {
                profile: scc_engineering::BaseProfile::Polygon { vertices },
                ..
            }
        } if vertices.len() == 5
    )));

    let mut model = EngineeringModel::new();
    apply_document(&mut model, source).unwrap();
    assert!(matches!(
        model
            .features
            .get(&scc_engineering::FeatureId("main".into())),
        Some(scc_engineering::feature::EngineeringFeature::BaseProfile {
            profile: scc_engineering::BaseProfile::Polygon { .. },
            ..
        })
    ));
}

#[test]
fn param_lowers_to_declare_parameter() {
    let commands = compile("AICAD 1\nPARAM plate_width = 120\n").unwrap();
    assert!(matches!(
        commands[0],
        EngineeringCommand::DeclareParameter { .. }
    ));
}

#[test]
fn bind_lowers_to_bind_feature_parameter() {
    let commands = compile(
        "AICAD 1\nPART p\nPARAM plate_width = 120\nBASE_RECT main\n  WIDTH 120\n  HEIGHT 80\nBIND main WIDTH <- plate_width\n",
    )
    .unwrap();
    assert!(commands
        .iter()
        .any(|c| { matches!(c, EngineeringCommand::BindFeatureParameter { .. }) }));
}

#[test]
fn thickness_and_extent_enter_the_engineering_model() {
    let source = r#"
AICAD 1
PART plate
BASE_RECT main
  WIDTH 120
  HEIGHT 80
THICKNESS 10
HOLE center_hole
  CENTER 0 0
  DIA 20
EXTENT center_hole THRU
"#;
    let commands = compile(source).unwrap();
    assert!(commands.iter().any(|command| matches!(
        command,
        EngineeringCommand::SetPartThickness {
            source: Some(scc_engineering::SourcedLength::Literal { value: 10.0 })
        }
    )));
    assert!(commands.iter().any(|command| matches!(
        command,
        EngineeringCommand::SetHoleExtent {
            target,
            extent: Some(scc_engineering::HoleExtent::ThroughAll)
        } if target.0 == "center_hole"
    )));

    let mut model = EngineeringModel::new();
    apply_document(&mut model, source).unwrap();
    assert!(model.validate_spatial_readiness().ready);
}

#[test]
fn blind_extent_preserves_face_and_depth() {
    let source = r#"
AICAD 1
PART plate
BASE_RECT main
  WIDTH 120
  HEIGHT 80
THICKNESS 10
HOLE h
  CENTER 0 0
  DIA 8
EXTENT h BLIND OPPOSITE 4
"#;
    let mut model = EngineeringModel::new();
    apply_document(&mut model, source).unwrap();
    assert!(matches!(
        model
            .spatial
            .hole_extents
            .get(&scc_engineering::FeatureId("h".into())),
        Some(scc_engineering::HoleExtent::Blind {
            start_face: scc_engineering::HoleStartFace::Opposite,
            depth: scc_engineering::SourcedLength::Literal { value: 4.0 }
        })
    ));
}

// ---- Level 5: DSL → model ----

#[test]
fn slice_corner_builds_expected_model() {
    let mut model = EngineeringModel::new();
    apply_document(&mut model, SLICE_CORNER).unwrap();
    assert_eq!(model.part.as_ref().unwrap().name, "Mounting Plate");
    assert!(model
        .features
        .contains_key(&scc_engineering::FeatureId("main".into())));
    assert!(model
        .features
        .contains_key(&scc_engineering::FeatureId("mounting".into())));
    // Corner pattern stays ONE pattern feature.
    assert_eq!(model.features.len(), 2);
    model.validate_model().unwrap();
}

#[test]
fn slice_circular_has_one_hole_and_one_pattern() {
    let mut model = EngineeringModel::new();
    apply_document(&mut model, SLICE_CIRCULAR).unwrap();
    // The DSL must not expand the pattern into 8 holes.
    assert_eq!(model.features.len(), 3); // base + 1 hole + 1 pattern
    assert_eq!(model.parameters.len(), 2); // desugared center params only
}

#[test]
fn circular_base_expression_and_center_survive_full_dsl_lowering() {
    let (model, geometry) = solve(SLICE_CIRCULAR_BASE);
    let base = model
        .features
        .get(&scc_engineering::FeatureId("main".into()))
        .unwrap();
    assert!(matches!(
        base,
        scc_engineering::feature::EngineeringFeature::BaseProfile {
            profile: scc_engineering::BaseProfile::Circle { center, diameter, .. },
            ..
        } if center.x == 30.0 && center.y == -20.0 && *diameter == 160.0
    ));
    let circle = geometry
        .entities
        .values()
        .find_map(|entity| match &entity.primitive {
            GeometryPrimitive::Circle { center, radius } => Some((*center, *radius)),
            _ => None,
        })
        .unwrap();
    assert_eq!(circle.0.x, 30.0);
    assert_eq!(circle.0.y, -20.0);
    assert_eq!(circle.1, 80.0);
}

// ---- Level 6: DSL → SCC → GeometryModel ----

#[test]
fn fixture1_rectangle_plus_four_corner_holes() {
    let (_model, geometry) = solve(SLICE_CORNER);
    // Plate rectangle 120 × 80 centered at origin.
    match &geometry.entities[&scc_core::ids::EntityId("main".into())].primitive {
        GeometryPrimitive::Rectangle {
            origin,
            width,
            height,
            ..
        } => {
            assert_eq!(*width, 120.0);
            assert_eq!(*height, 80.0);
            assert!((origin.x + 60.0).abs() < 1.0e-9);
            assert!((origin.y + 40.0).abs() < 1.0e-9);
        }
        _ => panic!("expected rectangle plate"),
    }
    let centers = circle_centers(&geometry);
    assert_eq!(centers.len(), 4);
    let expected = [(-45.0, -25.0), (-45.0, 25.0), (45.0, -25.0), (45.0, 25.0)];
    for (center, (ex, ey)) in centers.iter().zip(expected) {
        assert!((center.0 - ex).abs() < 1.0e-9, "x {} != {ex}", center.0);
        assert!((center.1 - ey).abs() < 1.0e-9, "y {} != {ey}", center.1);
    }
}

#[test]
fn fixture2_eight_hole_circular_pattern() {
    let (_model, geometry) = solve(SLICE_CIRCULAR);
    let centers = circle_centers(&geometry);
    // 8 × Ø10 on radius 63, plus the Ø64 center hole = 9 circles.
    assert_eq!(centers.len(), 9);
    for (x, y) in &centers {
        let r = x.hypot(*y);
        if r < 1.0e-6 {
            // center hole
            continue;
        }
        assert!((r - 63.0).abs() < 1.0e-6, "hole on radius {r}");
    }
    let on_ring = centers.iter().filter(|(x, y)| x.hypot(*y) > 1.0e-6).count();
    assert_eq!(on_ring, 8);
}

#[test]
fn fixture3_parameter_relation_slice() {
    let mut model = EngineeringModel::new();
    apply_document(&mut model, SLICE_PARAM).unwrap();
    // hole_diameter = plate_height / 8 = 80 / 8 = 10.
    assert_eq!(
        model
            .evaluate_parameter(&scc_engineering::ParameterId("hole_diameter".into()))
            .unwrap(),
        scc_engineering::ResolvedValue::Scalar(10.0)
    );
    let (_result, geometry) = lower_and_solve(&model).unwrap();
    let before = circle_centers(&geometry);

    // Change plate_height through the existing command protocol; the DSL is
    // not involved in the recomputation.
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetParameter {
            target: scc_engineering::ParameterId("plate_height".into()),
            value: scc_engineering::parameter::ParameterValue::Scalar(96.0),
        },
    )
    .unwrap();
    assert_eq!(
        model
            .evaluate_parameter(&scc_engineering::ParameterId("hole_diameter".into()))
            .unwrap(),
        scc_engineering::ResolvedValue::Scalar(12.0)
    );
    let (_result, geometry) = lower_and_solve(&model).unwrap();
    let after = circle_centers(&geometry);
    assert_ne!(before, after, "holes must move after plate_height changes");
    // Hole Y now ±(96/2 − 15) = ±33.
    assert!(after.iter().any(|(_, y)| (y.abs() - 33.0).abs() < 1.0e-9));
}

// ---- Level 7: determinism ----

#[test]
fn full_chain_is_deterministic_over_many_runs() {
    let build_all = || {
        let document = parse_document(SLICE_CORNER).unwrap();
        let commands = build_commands(&document).unwrap();
        let mut model = EngineeringModel::new();
        apply_document(&mut model, SLICE_CORNER).unwrap();
        let lowered = lower_engineering_model(&model).unwrap();
        let (_result, geometry) = lower_and_solve(&model).unwrap();
        (document, commands, model, lowered, geometry)
    };

    let reference = build_all();
    for _ in 0..100 {
        let candidate = build_all();
        assert_eq!(candidate.0, reference.0, "AST equal");
        assert_eq!(candidate.1, reference.1, "commands equal");
        assert_eq!(
            serde_json::to_string(&candidate.1).unwrap(),
            serde_json::to_string(&reference.1).unwrap(),
            "command serialization equal"
        );
        assert_eq!(candidate.2, reference.2, "model equal");
        assert_eq!(candidate.3.proposal, reference.3.proposal, "proposal equal");
        assert_eq!(
            candidate.3.source_map, reference.3.source_map,
            "source map equal"
        );
        assert_eq!(candidate.4, reference.4, "geometry equal");
        assert_eq!(
            serde_json::to_string(&candidate.4).unwrap(),
            serde_json::to_string(&reference.4).unwrap(),
            "geometry serialization equal"
        );
    }
}
