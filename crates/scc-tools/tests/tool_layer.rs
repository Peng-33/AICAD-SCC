//! Nine-layer tests for the provider-neutral tool layer (task §75/§76).

use std::collections::BTreeMap;

use scc_engineering::command::EngineeringCommand;
use scc_engineering::lowering::lower_and_solve;
use scc_engineering::model::EngineeringModel;
use scc_geometry::model::{GeometryModel, GeometryPrimitive};
use scc_tools::executor::{apply_tool_call, apply_tool_calls_atomically};
use scc_tools::registry::ToolRegistry;
use scc_tools::validator::validate_arguments;
use scc_tools::{adapt_tool_call, array, num, str_value, ToolCall, ToolError, ToolValue};

fn call(name: &str, pairs: Vec<(&str, ToolValue)>) -> ToolCall {
    ToolCall::from_pairs(
        name.to_string(),
        pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    )
    .unwrap()
}

fn args(pairs: Vec<(&str, ToolValue)>) -> BTreeMap<String, ToolValue> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
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

fn solve_calls(calls: &[ToolCall]) -> (EngineeringModel, GeometryModel) {
    let mut model = EngineeringModel::new();
    apply_tool_calls_atomically(&mut model, calls).unwrap();
    let (_result, geometry) = lower_and_solve(&model).unwrap();
    (model, geometry)
}

// ---- Layer 1: ToolDefinition / schema / registry ----

#[test]
fn standard_registry_preserves_existing_and_adds_g7a_tool_surface() {
    let registry = ToolRegistry::standard();
    let names: Vec<String> = registry.list().iter().map(|d| d.name.0.clone()).collect();
    assert_eq!(names.len(), 37);
    for expected in [
        "add_feature_pattern",
        "add_parametric_hole",
        "add_parametric_rectangular_add",
        "add_parametric_rectangular_cut",
        "create_parametric_plate",
        "create_sketch",
        "create_datum_axis",
        "create_datum_plane",
        "create_revolve",
        "create_sweep",
        "create_loft",
        "create_fillet",
        "create_chamfer",
        "add_circular_hole_pattern",
        "add_constraint",
        "add_construction_circle",
        "add_corner_hole_pattern",
        "add_grid_hole_pattern",
        "add_hole",
        "add_slot",
        "bind_parameter",
        "create_base_rectangle",
        "create_base_circle",
        "create_base_rounded_rectangle",
        "create_base_polygon",
        "create_part",
        "create_path",
        "declare_parameter",
        "move_feature",
        "remove_constraint",
        "remove_feature",
        "set_drawing_property",
        "set_hole_extent",
        "set_part_thickness",
        "set_expression",
        "set_feature_parameter",
        "set_parameter",
    ] {
        assert!(
            registry.contains(&scc_tools::ToolName(expected.into())),
            "missing {expected}"
        );
    }
    // Unsupported capabilities are NOT exposed as tools.
    for forbidden in ["add_fillet", "add_concentric_constraint"] {
        assert!(!registry.contains(&scc_tools::ToolName(forbidden.into())));
    }
    // Stable order = sorted by name.
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn set_feature_parameter_schema_exposes_only_supported_v2_hole_slots() {
    let registry = ToolRegistry::standard();
    let definition = registry
        .get(&scc_tools::ToolName("set_feature_parameter".into()))
        .unwrap();
    let slots = definition
        .parameter("slot")
        .and_then(|parameter| parameter.constraints.as_ref())
        .and_then(|constraints| constraints.enum_values.as_ref())
        .unwrap();
    assert_eq!(
        slots,
        &[
            "diameter",
            "blind_depth",
            "counterbore_diameter",
            "counterbore_depth",
            "countersink_mouth_diameter",
            "countersink_included_angle_degrees",
        ]
    );
    let expected_type = definition
        .parameter("expected_type")
        .and_then(|parameter| parameter.constraints.as_ref())
        .and_then(|constraints| constraints.enum_values.as_ref())
        .unwrap();
    assert_eq!(expected_type, &["length_mm", "angle_degrees"]);
}

#[test]
fn set_feature_parameter_adapts_a_stable_typed_hole_reference() {
    let commands = adapt_tool_call(&call(
        "set_feature_parameter",
        vec![
            ("part_id", str_value("mounting_plate")),
            ("feature", str_value("center_hole")),
            ("slot", str_value("blind_depth")),
            ("parameter", str_value("fp_0123456789abcdef")),
            ("expected_type", str_value("length_mm")),
            ("value", num(8.0)),
        ],
    ))
    .unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        EngineeringCommand::SetFeatureParameter { target, value } => {
            assert_eq!(target.part_id.0, "mounting_plate");
            assert_eq!(target.feature_id.0, "center_hole");
            assert_eq!(target.slot.to_string(), "blind_depth");
            assert_eq!(target.parameter_id.0, "fp_0123456789abcdef");
            assert_eq!(
                target.expected_type,
                scc_engineering::binding::ExpectedParameterType::LengthMm
            );
            assert_eq!(
                *value,
                scc_engineering::parameter::ParameterValue::Scalar(8.0)
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn g2_tools_create_typed_parametric_base_hole_and_reusable_pattern_history() {
    let calls = [
        call(
            "create_part",
            vec![("id", str_value("part")), ("name", str_value("Plate"))],
        ),
        call(
            "create_parametric_plate",
            vec![
                ("id", str_value("plate")),
                ("width", num(140.0)),
                ("height", num(100.0)),
                ("thickness", num(20.0)),
            ],
        ),
        call(
            "add_parametric_rectangular_cut",
            vec![
                ("id", str_value("slot_seed")),
                ("plane", str_value("x_y")),
                ("support_offset", num(20.0)),
                ("center_u", num(-35.0)),
                ("center_v", num(-20.0)),
                ("width", num(8.0)),
                ("height", num(5.0)),
                ("reversed", scc_tools::bool_value(true)),
                ("termination", str_value("blind")),
                ("depth", num(3.0)),
            ],
        ),
        call(
            "add_feature_pattern",
            vec![
                ("id", str_value("slot_pattern")),
                ("seed_feature_id", str_value("slot_seed")),
                ("pattern_kind", str_value("linear")),
                ("count", scc_tools::int(3)),
                ("direction_x", num(0.0)),
                ("direction_y", num(1.0)),
                ("direction_z", num(0.0)),
                ("spacing", num(20.0)),
            ],
        ),
        call(
            "add_parametric_hole",
            vec![
                ("id", str_value("side_hole")),
                ("support_mode", str_value("planar_face")),
                ("support_feature_id", str_value("plate_extrude")),
                ("face_selector", str_value("positive_x")),
                ("face_offset", num(70.0)),
                ("center_u", num(0.0)),
                ("center_v", num(10.0)),
                ("diameter", num(6.0)),
                ("reversed", scc_tools::bool_value(true)),
                ("termination", str_value("blind")),
                ("depth", num(15.0)),
                ("hole_kind", str_value("simple")),
            ],
        ),
    ];
    let mut model = EngineeringModel::new();
    apply_tool_calls_atomically(&mut model, &calls).unwrap();
    model.validate_model().unwrap();
    assert_eq!(model.part.as_ref().unwrap().name, "Plate");
    assert!(matches!(
        model.features[&scc_engineering::FeatureId("side_hole".into())],
        scc_engineering::feature::EngineeringFeature::Parametric {
            feature: scc_engineering::ParametricFeature::Hole {
                support: scc_engineering::PlaneReference::PlanarFace {
                    selector: scc_engineering::PlanarFaceSelector::PositiveX,
                    ..
                },
                ..
            },
            ..
        }
    ));
    assert!(
        matches!(&model.features[&scc_engineering::FeatureId("slot_pattern".into())], scc_engineering::feature::EngineeringFeature::Parametric { feature: scc_engineering::ParametricFeature::Pattern { seed_feature_id, .. }, .. } if seed_feature_id.0 == "slot_seed")
    );
    assert_eq!(model.feature_order.len(), 7);
}

#[test]
fn g3_tools_create_typed_edge_treatment_and_reject_raw_indices() {
    let base = call(
        "create_parametric_plate",
        vec![
            ("id", str_value("plate")),
            ("width", num(100.0)),
            ("height", num(80.0)),
            ("thickness", num(10.0)),
        ],
    );
    let reference: ToolValue = serde_json::from_str(
        r#"{"source_feature_id":"plate_extrude","selector":{"kind":"profile_boundary","entity_id":"plate-edge-0000","side":"top"},"qualifier":{"curve_kind":"line","radius_mm":null,"length_mm":null,"midpoint":null,"tolerance_mm":0.000001}}"#,
    ).unwrap();
    let fillet = call(
        "create_fillet",
        vec![
            ("id", str_value("edge_round")),
            ("radius", num(2.0)),
            ("edge_refs", array(vec![reference.clone()])),
        ],
    );
    let chamfer = call(
        "create_chamfer",
        vec![
            ("id", str_value("edge_break")),
            ("distance", num(0.5)),
            ("edge_refs", array(vec![reference])),
        ],
    );
    let mut model = EngineeringModel::new();
    apply_tool_calls_atomically(
        &mut model,
        &[
            call("create_part", vec![("id", str_value("part"))]),
            base,
            fillet,
            chamfer,
        ],
    )
    .unwrap();
    model.validate_model().unwrap();
    assert!(matches!(
        model.features[&scc_engineering::FeatureId("edge_round".into())],
        scc_engineering::feature::EngineeringFeature::Parametric {
            feature: scc_engineering::ParametricFeature::Fillet { radius: 2.0, .. },
            ..
        }
    ));
    assert!(matches!(
        model.features[&scc_engineering::FeatureId("edge_break".into())],
        scc_engineering::feature::EngineeringFeature::Parametric {
            feature: scc_engineering::ParametricFeature::Chamfer { distance: 0.5, .. },
            ..
        }
    ));

    let raw_index: ToolValue = serde_json::from_str(
        r#"{"source_feature_id":"plate_extrude","edge_index":7,"selector":{"kind":"hole_entrance"},"qualifier":{"curve_kind":null,"radius_mm":null,"length_mm":null,"midpoint":null,"tolerance_mm":0.000001}}"#,
    ).unwrap();
    let error = adapt_tool_call(&call(
        "create_fillet",
        vec![
            ("id", str_value("bad")),
            ("radius", num(1.0)),
            ("edge_refs", array(vec![raw_index])),
        ],
    ))
    .unwrap_err();
    assert!(matches!(error, ToolError::InvalidArgumentValue { .. }));
}

#[test]
fn g4_tools_add_generic_sketch_datum_axis_and_revolve_nodes() {
    let profile: ToolValue = serde_json::from_str(
        r#"{"outer":{"entities":[{"id":"edge-0","geometry":{"kind":"circle","center":{"x":0.0,"y":0.0},"radius":5.0}}]}}"#,
    )
    .unwrap();
    let axis_definition: ToolValue =
        serde_json::from_str(r#"{"kind":"principal","axis":"x"}"#).unwrap();
    let calls = [
        call("create_part", vec![("id", str_value("shaft"))]),
        call(
            "create_sketch",
            vec![
                ("id", str_value("profile")),
                ("plane", str_value("xz")),
                ("profile", profile),
            ],
        ),
        call(
            "create_datum_axis",
            vec![("id", str_value("axis")), ("definition", axis_definition)],
        ),
        call(
            "create_revolve",
            vec![
                ("id", str_value("body")),
                ("profile_id", str_value("profile")),
                ("axis_feature_id", str_value("axis")),
                ("operation", str_value("add")),
                ("angle_mode", str_value("partial")),
                ("degrees", num(180.0)),
                ("orientation", str_value("forward")),
            ],
        ),
    ];
    let mut model = EngineeringModel::new();
    apply_tool_calls_atomically(&mut model, &calls).unwrap();
    model.validate_model().unwrap();
    assert!(matches!(
        model.features[&scc_engineering::FeatureId("axis".into())],
        scc_engineering::feature::EngineeringFeature::Parametric {
            feature: scc_engineering::ParametricFeature::DatumAxis { .. },
            ..
        }
    ));
    assert!(matches!(
        model.features[&scc_engineering::FeatureId("body".into())],
        scc_engineering::feature::EngineeringFeature::Parametric {
            feature: scc_engineering::ParametricFeature::Revolve {
                operation: scc_engineering::RevolveOperation::Add,
                angle: scc_engineering::RevolveAngle::Partial { degrees: 180.0 },
                ..
            },
            ..
        }
    ));
}

#[test]
fn g4_tool_rejects_unknown_profile_fields_and_degenerate_datum_axis() {
    let bad_profile: ToolValue = serde_json::from_str(
        r#"{"outer":{"entities":[{"id":"edge-0","unexpected":1,"geometry":{"kind":"circle","center":{"x":0.0,"y":0.0},"radius":5.0}}]}}"#,
    )
    .unwrap();
    let error = adapt_tool_call(&call(
        "create_sketch",
        vec![
            ("id", str_value("profile")),
            ("plane", str_value("xz")),
            ("profile", bad_profile),
        ],
    ))
    .unwrap_err();
    assert!(matches!(error, ToolError::InvalidArgumentValue { .. }));

    let degenerate: ToolValue = serde_json::from_str(
        r#"{"kind":"through_two_points","first":{"x":1.0,"y":2.0,"z":3.0},"second":{"x":1.0,"y":2.0,"z":3.0}}"#,
    )
    .unwrap();
    let commands = adapt_tool_call(&call(
        "create_datum_axis",
        vec![("id", str_value("axis")), ("definition", degenerate)],
    ))
    .unwrap();
    let mut model = EngineeringModel::new();
    scc_engineering::executor::CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: scc_engineering::PartId("part".into()),
            name: "part".into(),
        },
    )
    .unwrap();
    assert!(scc_engineering::executor::CommandExecutor::apply(&mut model, &commands[0]).is_err());
}

#[test]
fn duplicate_registration_is_rejected() {
    let mut registry = ToolRegistry::new();
    let definition = scc_tools::definition::ToolDefinition {
        name: scc_tools::ToolName("create_part".into()),
        description: "d".into(),
        parameters: vec![scc_tools::definition::ToolParameter::string(
            "id", true, "id",
        )],
    };
    registry.register(definition.clone()).unwrap();
    assert!(matches!(
        registry.register(definition).unwrap_err(),
        ToolError::UnsupportedTool { .. }
    ));
}

#[test]
fn schema_serializes_deterministically() {
    let registry = ToolRegistry::standard();
    let first = serde_json::to_string_pretty(&registry.list()).unwrap();
    for _ in 0..10 {
        let again = serde_json::to_string_pretty(&ToolRegistry::standard().list()).unwrap();
        assert_eq!(first, again);
    }
}

// ---- Layer 2: argument validator ----

#[test]
fn unknown_argument_is_rejected() {
    let registry = ToolRegistry::standard();
    let definition = registry
        .get(&scc_tools::ToolName("add_hole".into()))
        .unwrap();
    let mut bad = args(vec![
        ("id", str_value("h1")),
        ("center_x", num(0.0)),
        ("center_y", num(0.0)),
        ("diameter", num(10.0)),
        ("color", str_value("red")),
    ]);
    assert!(matches!(
        validate_arguments(definition, &bad).unwrap_err(),
        ToolError::UnexpectedArgument { argument, .. } if argument == "color"
    ));
    bad.remove("color");
    assert!(validate_arguments(definition, &bad).is_ok());
}

#[test]
fn missing_required_argument_is_rejected() {
    let registry = ToolRegistry::standard();
    let definition = registry
        .get(&scc_tools::ToolName("add_hole".into()))
        .unwrap();
    let missing = args(vec![
        ("id", str_value("h1")),
        ("center_x", num(0.0)),
        ("center_y", num(0.0)),
    ]);
    assert!(matches!(
        validate_arguments(definition, &missing).unwrap_err(),
        ToolError::MissingArgument { argument, .. } if argument == "diameter"
    ));
}

#[test]
fn wrong_type_is_rejected() {
    let registry = ToolRegistry::standard();
    let definition = registry
        .get(&scc_tools::ToolName("add_hole".into()))
        .unwrap();
    let bad = args(vec![
        ("id", str_value("h1")),
        ("center_x", num(0.0)),
        ("center_y", num(0.0)),
        ("diameter", str_value("ten")),
    ]);
    assert!(matches!(
        validate_arguments(definition, &bad).unwrap_err(),
        ToolError::InvalidArgumentType { .. }
    ));
}

#[test]
fn value_constraints_are_enforced() {
    let registry = ToolRegistry::standard();
    let definition = registry
        .get(&scc_tools::ToolName("add_hole".into()))
        .unwrap();
    let bad_diameter = args(vec![
        ("id", str_value("h1")),
        ("center_x", num(0.0)),
        ("center_y", num(0.0)),
        ("diameter", num(-10.0)),
    ]);
    assert!(matches!(
        validate_arguments(definition, &bad_diameter).unwrap_err(),
        ToolError::InvalidArgumentValue { .. }
    ));

    let circular_registry = ToolRegistry::standard();
    let circular = circular_registry
        .get(&scc_tools::ToolName("add_circular_hole_pattern".into()))
        .unwrap();
    let mut base = args(vec![
        ("id", str_value("b")),
        ("center_x", num(0.0)),
        ("center_y", num(0.0)),
        ("pcd", num(126.0)),
        ("count", scc_tools::int(8)),
        ("diameter", num(10.0)),
        ("start_angle", num(0.0)),
    ]);
    // count = 8.5 (Number, not Integer) → type error; count = 0 → min error.
    base.insert("count".into(), num(8.5));
    assert!(matches!(
        validate_arguments(circular, &base).unwrap_err(),
        ToolError::InvalidArgumentType { .. }
    ));
    base.insert("count".into(), scc_tools::int(0));
    assert!(matches!(
        validate_arguments(circular, &base).unwrap_err(),
        ToolError::InvalidArgumentValue { .. }
    ));
}

#[test]
fn nan_and_infinity_are_rejected() {
    let registry = ToolRegistry::standard();
    let definition = registry
        .get(&scc_tools::ToolName("add_hole".into()))
        .unwrap();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let bad = args(vec![
            ("id", str_value("h1")),
            ("center_x", num(0.0)),
            ("center_y", num(0.0)),
            ("diameter", num(value)),
        ]);
        assert!(matches!(
            validate_arguments(definition, &bad).unwrap_err(),
            ToolError::InvalidArgumentValue { .. }
        ));
    }
}

#[test]
fn duplicate_arguments_are_rejected_by_constructor() {
    let err = ToolCall::from_pairs(
        "add_hole".to_string(),
        vec![
            ("diameter".to_string(), num(10.0)),
            ("diameter".to_string(), num(12.0)),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, ToolError::DuplicateArgument { .. }));
}

// ---- Layer 3/4: ToolCall → EngineeringCommand ----

#[test]
fn create_part_adapts_to_create_part_command() {
    let commands = adapt_tool_call(&call("create_part", vec![("id", str_value("p"))])).unwrap();
    assert!(matches!(
        &commands[0],
        EngineeringCommand::CreatePart { .. }
    ));
}

#[test]
fn create_base_circle_adapts_to_typed_circle_profile() {
    let commands = adapt_tool_call(&call(
        "create_base_circle",
        vec![
            ("id", str_value("disk")),
            ("center_x", num(30.0)),
            ("center_y", num(-20.0)),
            ("diameter", num(160.0)),
        ],
    ))
    .unwrap();
    assert_eq!(commands.len(), 1);
    let EngineeringCommand::AddFeature { feature } = &commands[0] else {
        panic!("circle tool must emit one AddFeature command");
    };
    assert!(matches!(
        feature,
        scc_engineering::feature::EngineeringFeature::BaseProfile {
            profile: scc_engineering::BaseProfile::Circle { center, diameter },
            ..
        } if center.x == 30.0 && center.y == -20.0 && *diameter == 160.0
    ));
}

#[test]
fn create_base_rounded_rectangle_adapts_to_typed_profile() {
    let commands = adapt_tool_call(&call(
        "create_base_rounded_rectangle",
        vec![
            ("id", str_value("plate")),
            ("center_x", num(10.0)),
            ("center_y", num(-5.0)),
            ("width", num(120.0)),
            ("height", num(80.0)),
            ("corner_radius", num(10.0)),
        ],
    ))
    .unwrap();
    assert!(matches!(
        &commands[0],
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
    ));
}

#[test]
fn create_base_polygon_adapts_to_canonical_typed_profile() {
    let commands = adapt_tool_call(&call(
        "create_base_polygon",
        vec![
            ("id", str_value("plate")),
            (
                "points",
                array(vec![
                    array(vec![num(100.0), num(0.0)]),
                    array(vec![num(50.0), num(80.0)]),
                    array(vec![num(0.0), num(0.0)]),
                ]),
            ),
        ],
    ))
    .unwrap();
    let EngineeringCommand::AddFeature { feature } = &commands[0] else {
        panic!("polygon tool must emit one AddFeature command");
    };
    let scc_engineering::feature::EngineeringFeature::BaseProfile {
        profile: scc_engineering::BaseProfile::Polygon { vertices },
        ..
    } = feature
    else {
        panic!("polygon tool must emit a polygon profile");
    };
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[0], scc_engineering::Point2D::new(0.0, 0.0));
    assert_eq!(vertices[1], scc_engineering::Point2D::new(100.0, 0.0));
    assert_eq!(vertices[2], scc_engineering::Point2D::new(50.0, 80.0));
}

#[test]
fn add_slot_adapts_to_typed_slot_feature() {
    let commands = adapt_tool_call(&call(
        "add_slot",
        vec![
            ("id", str_value("s1")),
            ("center_x", num(10.0)),
            ("center_y", num(-5.0)),
            ("overall_length", num(40.0)),
            ("width", num(12.0)),
            ("orientation_degrees", num(45.0)),
        ],
    ))
    .unwrap();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        EngineeringCommand::AddFeature {
            feature: scc_engineering::feature::EngineeringFeature::Slot {
                center,
                length,
                width,
                orientation_degrees,
                ..
            }
        } if center.x == 10.0
            && center.y == -5.0
            && *length == 40.0
            && *width == 12.0
            && *orientation_degrees == 45.0
    ));
}

#[test]
fn add_hole_is_a_composite_tool_with_five_commands() {
    let commands = adapt_tool_call(&call(
        "add_hole",
        vec![
            ("id", str_value("h")),
            ("center_x", num(40.0)),
            ("center_y", num(30.0)),
            ("diameter", num(10.0)),
        ],
    ))
    .unwrap();
    assert_eq!(commands.len(), 5);
    assert!(
        commands
            .iter()
            .filter(|c| matches!(c, EngineeringCommand::DeclareParameter { .. }))
            .count()
            == 2
    );
    assert!(
        commands
            .iter()
            .filter(|c| matches!(c, EngineeringCommand::BindFeatureParameter { .. }))
            .count()
            == 2
    );
    assert!(commands
        .iter()
        .any(|c| matches!(c, EngineeringCommand::AddFeature { .. })));
}

#[test]
fn patterns_are_not_expanded_by_the_adapter() {
    let commands = adapt_tool_call(&call(
        "add_circular_hole_pattern",
        vec![
            ("id", str_value("bolts")),
            ("center_x", num(0.0)),
            ("center_y", num(0.0)),
            ("pcd", num(126.0)),
            ("count", scc_tools::int(8)),
            ("diameter", num(10.0)),
            ("start_angle", num(0.0)),
        ],
    ))
    .unwrap();
    // ONE AddFeature(HolePattern) — never eight holes.
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        EngineeringCommand::AddFeature { .. }
    ));
}

#[test]
fn set_expression_parses_through_the_dsl_parser() {
    let commands = adapt_tool_call(&call(
        "set_expression",
        vec![
            ("target", str_value("hole_diameter")),
            ("expression", str_value("plate_width / 8")),
        ],
    ))
    .unwrap();
    assert!(matches!(
        &commands[0],
        EngineeringCommand::SetExpression { .. }
    ));
}

#[test]
fn spatial_tools_set_thickness_and_hole_extent() {
    let mut model = EngineeringModel::new();
    apply_tool_calls_atomically(
        &mut model,
        &[
            call("create_part", vec![("id", str_value("p"))]),
            call(
                "create_base_rectangle",
                vec![
                    ("id", str_value("main")),
                    ("width", num(120.0)),
                    ("height", num(80.0)),
                ],
            ),
            call(
                "add_hole",
                vec![
                    ("id", str_value("center_hole")),
                    ("center_x", num(0.0)),
                    ("center_y", num(0.0)),
                    ("diameter", num(20.0)),
                ],
            ),
            call("set_part_thickness", vec![("thickness", num(10.0))]),
            call(
                "set_hole_extent",
                vec![
                    ("target", str_value("center_hole")),
                    ("extent", str_value("through_all")),
                ],
            ),
        ],
    )
    .unwrap();
    assert!(model.validate_spatial_readiness().ready);
}

#[test]
fn blind_extent_tool_requires_a_positive_depth() {
    let error = adapt_tool_call(&call(
        "set_hole_extent",
        vec![
            ("target", str_value("h")),
            ("extent", str_value("blind_base")),
        ],
    ))
    .unwrap_err();
    assert!(matches!(error, ToolError::MissingArgument { argument, .. } if argument == "depth"));
}

// ---- Layer 5: ToolCall → EngineeringModel ----

#[test]
fn tool_calls_build_a_model() {
    let mut model = EngineeringModel::new();
    apply_tool_calls_atomically(
        &mut model,
        &[
            call("create_part", vec![("id", str_value("p"))]),
            call(
                "create_base_rectangle",
                vec![
                    ("id", str_value("main")),
                    ("width", num(120.0)),
                    ("height", num(80.0)),
                ],
            ),
            call(
                "declare_parameter",
                vec![("id", str_value("pw")), ("value", num(120.0))],
            ),
            call(
                "bind_parameter",
                vec![
                    ("feature", str_value("main")),
                    ("slot", str_value("width")),
                    ("parameter", str_value("pw")),
                ],
            ),
        ],
    )
    .unwrap();
    assert!(model
        .features
        .contains_key(&scc_engineering::FeatureId("main".into())));
    assert!(model
        .parameters
        .contains_key(&scc_engineering::ParameterId("pw".into())));
    model.validate_model().unwrap();
}

// ---- Layer 6: Tool → SCC → GeometryModel ----

#[test]
fn vertical_slice_rectangle_plus_four_corner_holes() {
    let (_model, geometry) = solve_calls(&[
        call(
            "create_part",
            vec![
                ("id", str_value("mounting_plate")),
                ("name", str_value("Mounting Plate")),
            ],
        ),
        call(
            "create_base_rectangle",
            vec![
                ("id", str_value("main")),
                ("width", num(120.0)),
                ("height", num(80.0)),
            ],
        ),
        call(
            "add_corner_hole_pattern",
            vec![
                ("id", str_value("mounting")),
                ("base", str_value("main")),
                ("offset_x", num(15.0)),
                ("offset_y", num(15.0)),
                ("diameter", num(10.0)),
            ],
        ),
    ]);
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
        _ => panic!("expected rectangle"),
    }
    let expected = [(-45.0, -25.0), (-45.0, 25.0), (45.0, -25.0), (45.0, 25.0)];
    let centers = circle_centers(&geometry);
    assert_eq!(centers.len(), 4);
    for (center, (ex, ey)) in centers.iter().zip(expected) {
        assert!((center.0 - ex).abs() < 1.0e-9);
        assert!((center.1 - ey).abs() < 1.0e-9);
    }
}

#[test]
fn vertical_slice_center_hole_plus_circular_pattern() {
    let (_model, geometry) = solve_calls(&[
        call("create_part", vec![("id", str_value("circular_plate"))]),
        call(
            "create_base_rectangle",
            vec![
                ("id", str_value("main")),
                ("width", num(180.0)),
                ("height", num(180.0)),
            ],
        ),
        call(
            "add_hole",
            vec![
                ("id", str_value("center_hole")),
                ("center_x", num(0.0)),
                ("center_y", num(0.0)),
                ("diameter", num(64.0)),
            ],
        ),
        call(
            "add_circular_hole_pattern",
            vec![
                ("id", str_value("bolts")),
                ("center_x", num(0.0)),
                ("center_y", num(0.0)),
                ("pcd", num(126.0)),
                ("count", scc_tools::int(8)),
                ("diameter", num(10.0)),
                ("start_angle", num(0.0)),
            ],
        ),
    ]);
    let centers = circle_centers(&geometry);
    assert_eq!(centers.len(), 9); // 1 center + 8 on ring
    let on_ring = centers.iter().filter(|(x, y)| x.hypot(*y) > 1.0e-6).count();
    assert_eq!(on_ring, 8);
    for (x, y) in &centers {
        if x.hypot(*y) > 1.0e-6 {
            assert!((x.hypot(*y) - 63.0).abs() < 1.0e-6);
        }
    }
}

#[test]
fn vertical_slice_parameter_expression_update() {
    let mut model = EngineeringModel::new();
    apply_tool_calls_atomically(
        &mut model,
        &[
            call("create_part", vec![("id", str_value("param_plate"))]),
            call(
                "declare_parameter",
                vec![("id", str_value("plate_width")), ("value", num(120.0))],
            ),
            call(
                "declare_parameter",
                vec![("id", str_value("plate_height")), ("value", num(80.0))],
            ),
            call(
                "declare_parameter",
                vec![("id", str_value("edge_offset")), ("value", num(15.0))],
            ),
            call(
                "declare_parameter",
                vec![("id", str_value("hole_diameter")), ("value", num(10.0))],
            ),
            call(
                "create_base_rectangle",
                vec![
                    ("id", str_value("main")),
                    ("width", num(120.0)),
                    ("height", num(80.0)),
                ],
            ),
            call(
                "bind_parameter",
                vec![
                    ("feature", str_value("main")),
                    ("slot", str_value("width")),
                    ("parameter", str_value("plate_width")),
                ],
            ),
            call(
                "bind_parameter",
                vec![
                    ("feature", str_value("main")),
                    ("slot", str_value("height")),
                    ("parameter", str_value("plate_height")),
                ],
            ),
            call(
                "add_corner_hole_pattern",
                vec![
                    ("id", str_value("mounting")),
                    ("base", str_value("main")),
                    ("offset_x", num(15.0)),
                    ("offset_y", num(15.0)),
                    ("diameter", num(10.0)),
                ],
            ),
            call(
                "bind_parameter",
                vec![
                    ("feature", str_value("mounting")),
                    ("slot", str_value("offset_x")),
                    ("parameter", str_value("edge_offset")),
                ],
            ),
            call(
                "bind_parameter",
                vec![
                    ("feature", str_value("mounting")),
                    ("slot", str_value("offset_y")),
                    ("parameter", str_value("edge_offset")),
                ],
            ),
            call(
                "bind_parameter",
                vec![
                    ("feature", str_value("mounting")),
                    ("slot", str_value("diameter")),
                    ("parameter", str_value("hole_diameter")),
                ],
            ),
            call(
                "set_expression",
                vec![
                    ("target", str_value("hole_diameter")),
                    ("expression", str_value("plate_width / 8")),
                ],
            ),
        ],
    )
    .unwrap();
    // hole_diameter = 120 / 8 = 15.
    assert_eq!(
        model
            .evaluate_parameter(&scc_engineering::ParameterId("hole_diameter".into()))
            .unwrap(),
        scc_engineering::ResolvedValue::Scalar(15.0)
    );
    let (_result, geometry_before) = lower_and_solve(&model).unwrap();
    let before = circle_centers(&geometry_before);

    apply_tool_call(
        &mut model,
        &call(
            "set_parameter",
            vec![("target", str_value("plate_width")), ("value", num(96.0))],
        ),
    )
    .unwrap();
    assert_eq!(
        model
            .evaluate_parameter(&scc_engineering::ParameterId("hole_diameter".into()))
            .unwrap(),
        scc_engineering::ResolvedValue::Scalar(12.0)
    );
    let (_result, geometry_after) = lower_and_solve(&model).unwrap();
    assert_ne!(before, circle_centers(&geometry_after));
}

// ---- Layer 7: DSL ↔ Tool semantic equivalence ----

const DSL_SLICE: &str = r#"
AICAD 1
PART mounting_plate "Mounting Plate"
BASE_RECT main
  WIDTH 120
  HEIGHT 80
HOLE center_hole
  CENTER 0 0
  DIA 64
HOLE_PATTERN bolts CIRCULAR
  CENTER 0 0
  PCD 126
  COUNT 8
  DIA 10
  START_ANGLE 0
HOLE_PATTERN mounting CORNER
  BASE main
  OFFSET_X 15
  OFFSET_Y 15
  DIA 10
"#;

#[test]
fn dsl_and_tools_produce_the_same_model_and_geometry() {
    // DSL front end.
    let mut dsl_model = EngineeringModel::new();
    scc_dsl::command_builder::apply_document(&mut dsl_model, DSL_SLICE).unwrap();
    let (_dsl_lowered, dsl_geometry) = lower_and_solve(&dsl_model).unwrap();

    // Tool front end (same intent).
    let tool_calls = [
        call(
            "create_part",
            vec![
                ("id", str_value("mounting_plate")),
                ("name", str_value("Mounting Plate")),
            ],
        ),
        call(
            "create_base_rectangle",
            vec![
                ("id", str_value("main")),
                ("width", num(120.0)),
                ("height", num(80.0)),
            ],
        ),
        call(
            "add_hole",
            vec![
                ("id", str_value("center_hole")),
                ("center_x", num(0.0)),
                ("center_y", num(0.0)),
                ("diameter", num(64.0)),
            ],
        ),
        call(
            "add_circular_hole_pattern",
            vec![
                ("id", str_value("bolts")),
                ("center_x", num(0.0)),
                ("center_y", num(0.0)),
                ("pcd", num(126.0)),
                ("count", scc_tools::int(8)),
                ("diameter", num(10.0)),
                ("start_angle", num(0.0)),
            ],
        ),
        call(
            "add_corner_hole_pattern",
            vec![
                ("id", str_value("mounting")),
                ("base", str_value("main")),
                ("offset_x", num(15.0)),
                ("offset_y", num(15.0)),
                ("diameter", num(10.0)),
            ],
        ),
    ];
    let mut tool_model = EngineeringModel::new();
    apply_tool_calls_atomically(&mut tool_model, &tool_calls).unwrap();
    let (_tool_lowered, tool_geometry) = lower_and_solve(&tool_model).unwrap();

    assert_eq!(
        dsl_model, tool_model,
        "front ends must produce the same model"
    );
    assert_eq!(
        dsl_geometry, tool_geometry,
        "front ends must produce the same geometry"
    );
}

// ---- Layer 8: atomicity ----

#[test]
fn composite_tool_failure_leaves_model_unchanged() {
    // add_hole on an empty model: AddFeature fails (no part) → whole composite
    // is rejected, model unchanged.
    let mut model = EngineeringModel::new();
    let err = apply_tool_call(
        &mut model,
        &call(
            "add_hole",
            vec![
                ("id", str_value("h1")),
                ("center_x", num(0.0)),
                ("center_y", num(0.0)),
                ("diameter", num(10.0)),
            ],
        ),
    )
    .unwrap_err();
    assert!(matches!(err, ToolError::CommandApplicationFailed { .. }));
    assert!(model.part.is_none());
    assert!(model.parameters.is_empty());
    assert!(model.features.is_empty());
}

#[test]
fn batch_of_tool_calls_is_all_or_nothing() {
    let mut model = EngineeringModel::new();
    let err = apply_tool_calls_atomically(
        &mut model,
        &[
            call("create_part", vec![("id", str_value("p"))]),
            // Unknown argument on the second call rejects the whole batch.
            ToolCall::new(
                "add_hole",
                args(vec![
                    ("id", str_value("h")),
                    ("center_x", num(0.0)),
                    ("center_y", num(0.0)),
                    ("diameter", num(10.0)),
                    ("extra", num(1.0)),
                ]),
            ),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, ToolError::UnexpectedArgument { .. }));
    assert!(
        model.part.is_none(),
        "batch must not be partially committed"
    );
}

// ---- Layer 9: determinism ----

#[test]
fn adapter_and_pipeline_are_deterministic() {
    let calls = [
        call("create_part", vec![("id", str_value("p"))]),
        call(
            "create_base_rectangle",
            vec![
                ("id", str_value("main")),
                ("width", num(120.0)),
                ("height", num(80.0)),
            ],
        ),
        call(
            "add_corner_hole_pattern",
            vec![
                ("id", str_value("mounting")),
                ("base", str_value("main")),
                ("offset_x", num(15.0)),
                ("offset_y", num(15.0)),
                ("diameter", num(10.0)),
            ],
        ),
    ];
    // Adapter determinism.
    let reference_commands = adapt_tool_call(&calls[0]).unwrap();
    for _ in 0..100 {
        assert_eq!(adapt_tool_call(&calls[0]).unwrap(), reference_commands);
        assert_eq!(
            serde_json::to_string(&adapt_tool_call(&calls[0]).unwrap()).unwrap(),
            serde_json::to_string(&reference_commands).unwrap()
        );
    }
    // Full chain determinism.
    let build_all = || {
        let mut model = EngineeringModel::new();
        apply_tool_calls_atomically(&mut model, &calls).unwrap();
        let (_result, geometry) = lower_and_solve(&model).unwrap();
        (model, geometry)
    };
    let reference = build_all();
    for _ in 0..100 {
        let candidate = build_all();
        assert_eq!(candidate.0, reference.0);
        assert_eq!(candidate.1, reference.1);
    }
}

// ---- Error surface (§76): no panic, model unchanged, structured errors ----

#[test]
fn unknown_tool_is_rejected() {
    let mut model = EngineeringModel::new();
    let err = apply_tool_call(&mut model, &call("add_magic_feature", vec![])).unwrap_err();
    assert!(matches!(&err, ToolError::UnknownTool { .. }));
    let structured: scc_core::error::StructuredError = err.into();
    assert_eq!(structured.code, "SCC-TOOL-001");
    structured.validate_contract().unwrap();
    assert!(model.part.is_none());
}

#[test]
fn engineering_errors_are_preserved_not_flattened() {
    let mut model = EngineeringModel::new();
    apply_tool_call(
        &mut model,
        &call("create_part", vec![("id", str_value("p"))]),
    )
    .unwrap();
    apply_tool_call(
        &mut model,
        &call(
            "create_base_rectangle",
            vec![
                ("id", str_value("main")),
                ("width", num(10.0)),
                ("height", num(10.0)),
            ],
        ),
    )
    .unwrap();
    // remove_feature on a feature referenced by a constraint → SCC-ENG-013.
    apply_tool_call(
        &mut model,
        &call(
            "add_constraint",
            vec![
                ("id", str_value("c1")),
                ("type", str_value("fixed")),
                ("target", str_value("main")),
            ],
        ),
    )
    .unwrap();
    let err = apply_tool_call(
        &mut model,
        &call("remove_feature", vec![("target", str_value("main"))]),
    )
    .unwrap_err();
    match &err {
        ToolError::CommandApplicationFailed { source, .. } => {
            assert!(matches!(
                source,
                scc_engineering::error::EngineeringError::FeatureInUse { .. }
            ));
        }
        other => panic!("expected CommandApplicationFailed, got {other:?}"),
    }
    // The structured error keeps the engineering stage.
    let structured: scc_core::error::StructuredError = err.into();
    assert_eq!(structured.code, "SCC-TOOL-010");
    let source = structured.source.as_ref().unwrap();
    assert_eq!(source.code, "SCC-ENG-013");
    assert!(model
        .features
        .contains_key(&scc_engineering::FeatureId("main".into())));
}

#[test]
fn expression_parse_failure_is_rejected() {
    let mut model = EngineeringModel::new();
    apply_tool_call(
        &mut model,
        &call("create_part", vec![("id", str_value("p"))]),
    )
    .unwrap();
    apply_tool_call(
        &mut model,
        &call(
            "declare_parameter",
            vec![("id", str_value("pw")), ("value", num(120.0))],
        ),
    )
    .unwrap();
    let err = apply_tool_call(
        &mut model,
        &call(
            "set_expression",
            vec![
                ("target", str_value("pw")),
                ("expression", str_value("(1 + ")),
            ],
        ),
    )
    .unwrap_err();
    assert!(matches!(err, ToolError::ExpressionParseFailed { .. }));
}

#[test]
fn invalid_slot_is_rejected() {
    let mut model = EngineeringModel::new();
    apply_tool_call(
        &mut model,
        &call("create_part", vec![("id", str_value("p"))]),
    )
    .unwrap();
    apply_tool_call(
        &mut model,
        &call(
            "create_base_rectangle",
            vec![
                ("id", str_value("main")),
                ("width", num(10.0)),
                ("height", num(10.0)),
            ],
        ),
    )
    .unwrap();
    apply_tool_call(
        &mut model,
        &call(
            "declare_parameter",
            vec![("id", str_value("x")), ("value", num(1.0))],
        ),
    )
    .unwrap();
    let err = apply_tool_call(
        &mut model,
        &call(
            "bind_parameter",
            vec![
                ("feature", str_value("main")),
                ("slot", str_value("pcd")),
                ("parameter", str_value("x")),
            ],
        ),
    )
    .unwrap_err();
    // "pcd" IS a valid slot name, so validation passes; the slot is invalid
    // FOR a rectangle → engineering layer rejects (SCC-ENG-028), preserved.
    assert!(matches!(err, ToolError::CommandApplicationFailed { .. }));
}
