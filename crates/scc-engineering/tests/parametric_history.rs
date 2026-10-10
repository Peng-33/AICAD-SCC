use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::feature::{
    AxisReference, BaseProfile, DatumAxisDefinition, DatumPlaneDefinition, EngineeringFeature,
    ExtrudeDirection, ExtrudeExtent, ExtrudeOperation, ParametricFeature, PlaneReference,
    PrincipalPlane, RevolveAngle, RevolveOperation, RevolveOrientation, SketchLoop, SketchProfile,
    Vector3D,
};
use scc_engineering::{EngineeringModel, FeatureId, PartId, Point2D};

fn empty_part() -> EngineeringModel {
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

fn sketch(profile: SketchProfile, support: PlaneReference) -> ParametricFeature {
    ParametricFeature::Sketch { support, profile }
}

fn extrude(profile: &str, operation: ExtrudeOperation, extent: ExtrudeExtent) -> ParametricFeature {
    ParametricFeature::Extrude {
        profile: FeatureId(profile.into()),
        operation,
        direction: ExtrudeDirection::Normal { reversed: false },
        extent,
    }
}

fn principal_xy() -> PlaneReference {
    PlaneReference::Principal {
        plane: PrincipalPlane::XY,
    }
}

fn rectangle_profile(id: &str, center: Point2D, width: f64, height: f64) -> SketchProfile {
    SketchProfile {
        outer: SketchLoop::rectangle(id, center, width, height).unwrap(),
        inner_loops: vec![],
    }
}

#[test]
fn generic_sketch_and_extrude_expresses_mixed_composition_matrix_cases_1_to_8() {
    let mut model = empty_part();

    // Case 1: Rectangle -> Extrude ADD.
    add(
        &mut model,
        "base_sketch",
        sketch(
            rectangle_profile("base-edge", Point2D::new(0.0, 0.0), 100.0, 60.0),
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "base_add",
        extrude(
            "base_sketch",
            ExtrudeOperation::Add,
            ExtrudeExtent::Distance { length: 10.0 },
        ),
    );

    // Case 2: Circle -> Extrude ADD on a datum plane offset to the plate top.
    add(
        &mut model,
        "top_plane",
        ParametricFeature::DatumPlane {
            definition: DatumPlaneDefinition::Offset {
                base: principal_xy(),
                distance: 10.0,
            },
        },
    );
    add(
        &mut model,
        "boss_sketch",
        sketch(
            SketchProfile {
                outer: SketchLoop::circle("boss-circle", Point2D::new(-24.0, 0.0), 12.0),
                inner_loops: vec![],
            },
            PlaneReference::Datum {
                feature: FeatureId("top_plane".into()),
            },
        ),
    );
    add(
        &mut model,
        "boss_add",
        extrude(
            "boss_sketch",
            ExtrudeOperation::Add,
            ExtrudeExtent::Distance { length: 7.0 },
        ),
    );

    // Case 3: an off-origin rectangular additive profile.
    add(
        &mut model,
        "offset_sketch",
        sketch(
            rectangle_profile("offset-edge", Point2D::new(27.0, 13.0), 18.0, 12.0),
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "offset_add",
        extrude(
            "offset_sketch",
            ExtrudeOperation::Add,
            ExtrudeExtent::Distance { length: 3.0 },
        ),
    );

    // Case 4: Circle -> Extrude CUT.
    add(
        &mut model,
        "circle_cut_sketch",
        sketch(
            SketchProfile {
                outer: SketchLoop::circle("cut-circle", Point2D::new(0.0, 0.0), 4.0),
                inner_loops: vec![],
            },
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "circle_cut",
        extrude(
            "circle_cut_sketch",
            ExtrudeOperation::Cut,
            ExtrudeExtent::ThroughAll,
        ),
    );

    // Case 5: Slot -> Extrude CUT through all.
    let slot_segments =
        scc_engineering::slot::slot_profile_segments(Point2D::new(12.0, 0.0), 30.0, 8.0, 0.0)
            .unwrap();
    add(
        &mut model,
        "slot_sketch",
        sketch(
            SketchProfile {
                outer: SketchLoop::from_segments("slot-edge", &slot_segments).unwrap(),
                inner_loops: vec![],
            },
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "slot_cut",
        extrude(
            "slot_sketch",
            ExtrudeOperation::Cut,
            ExtrudeExtent::ThroughAll,
        ),
    );

    // Case 6: RoundedRectangle -> blind Extrude CUT.
    let rounded = BaseProfile::RoundedRectangle {
        center: Point2D::new(0.0, 0.0),
        width: 28.0,
        height: 20.0,
        corner_radius: 3.0,
    };
    add(
        &mut model,
        "pocket_sketch",
        sketch(
            SketchProfile {
                outer: SketchLoop::from_profile("pocket-edge", &rounded).unwrap(),
                inner_loops: vec![],
            },
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "pocket_cut",
        extrude(
            "pocket_sketch",
            ExtrudeOperation::Cut,
            ExtrudeExtent::Distance { length: 2.0 },
        ),
    );

    // Case 7: a second consecutive ADD followed by Case 8 ADD-CUT-ADD-CUT.
    add(
        &mut model,
        "second_add_sketch",
        sketch(
            rectangle_profile("second-edge", Point2D::new(0.0, 0.0), 8.0, 8.0),
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "second_add",
        extrude(
            "second_add_sketch",
            ExtrudeOperation::Add,
            ExtrudeExtent::Symmetric { length: 4.0 },
        ),
    );
    add(
        &mut model,
        "final_cut_sketch",
        sketch(
            SketchProfile {
                outer: SketchLoop::circle("final-circle", Point2D::new(2.0, 2.0), 1.5),
                inner_loops: vec![],
            },
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "final_cut",
        extrude(
            "final_cut_sketch",
            ExtrudeOperation::Cut,
            ExtrudeExtent::ThroughAll,
        ),
    );

    model.validate_model().unwrap();
    assert_eq!(model.feature_order.len(), model.features.len());
    assert_eq!(
        model.ordered_feature_ids().last(),
        Some(&FeatureId("final_cut".into()))
    );
    assert_eq!(
        model
            .context_view()
            .features
            .iter()
            .map(|feature| FeatureId(feature.id.clone()))
            .collect::<Vec<_>>(),
        model.feature_order,
    );
    assert_eq!(
        model
            .workbench_view()
            .features
            .iter()
            .map(|feature| FeatureId(feature.id.clone()))
            .collect::<Vec<_>>(),
        model.feature_order,
    );
    let serialized = serde_json::to_string(&model).unwrap();
    let roundtrip: EngineeringModel = serde_json::from_str(&serialized).unwrap();
    assert_eq!(roundtrip, model);
}

#[test]
fn duplicate_ids_and_forward_or_wrong_type_references_fail_closed() {
    let mut model = empty_part();
    add(
        &mut model,
        "profile",
        sketch(
            rectangle_profile("rect", Point2D::new(0.0, 0.0), 20.0, 10.0),
            principal_xy(),
        ),
    );
    let duplicate = CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Parametric {
                id: FeatureId("profile".into()),
                name: "duplicate".into(),
                feature: extrude(
                    "profile",
                    ExtrudeOperation::Add,
                    ExtrudeExtent::Distance { length: 1.0 },
                ),
                suppressed: false,
            },
        },
    );
    assert!(duplicate.is_err());
    assert_eq!(model.feature_order, vec![FeatureId("profile".into())]);

    let bad_reference = CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Parametric {
                id: FeatureId("cut".into()),
                name: "cut".into(),
                feature: extrude(
                    "missing_sketch",
                    ExtrudeOperation::Cut,
                    ExtrudeExtent::ThroughAll,
                ),
                suppressed: false,
            },
        },
    );
    assert!(bad_reference.is_err());
    assert!(!model.features.contains_key(&FeatureId("cut".into())));
}

#[test]
fn upstream_sketch_replacement_is_stable_and_history_reorder_is_atomic() {
    let mut model = empty_part();
    add(
        &mut model,
        "sketch",
        sketch(
            rectangle_profile("edge", Point2D::new(0.0, 0.0), 20.0, 10.0),
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "add",
        extrude(
            "sketch",
            ExtrudeOperation::Add,
            ExtrudeExtent::Distance { length: 5.0 },
        ),
    );
    let before = model.clone();
    let replacement = EngineeringFeature::Parametric {
        id: FeatureId("sketch".into()),
        name: "sketch".into(),
        feature: sketch(
            rectangle_profile("edge", Point2D::new(4.0, -3.0), 24.0, 12.0),
            principal_xy(),
        ),
        suppressed: false,
    };
    let command = EngineeringCommand::ReplaceFeature {
        feature: replacement,
    };
    CommandExecutor::apply(&mut model, &command).unwrap();
    let mut repeated = before;
    CommandExecutor::apply(&mut repeated, &command).unwrap();
    assert_eq!(model, repeated);
    assert_eq!(
        model.feature_order,
        vec![FeatureId("sketch".into()), FeatureId("add".into())]
    );

    let unchanged = model.clone();
    let reorder = CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::ReorderFeature {
            target: FeatureId("add".into()),
            before: Some(FeatureId("sketch".into())),
        },
    );
    assert!(
        reorder.is_err(),
        "a profile cannot be moved behind its dependent extrude"
    );
    assert_eq!(model, unchanged);
}

#[test]
fn suppressing_an_upstream_node_requires_dependants_to_be_suppressed_first() {
    let mut model = empty_part();
    add(
        &mut model,
        "sketch",
        sketch(
            rectangle_profile("edge", Point2D::new(0.0, 0.0), 10.0, 10.0),
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "add",
        extrude(
            "sketch",
            ExtrudeOperation::Add,
            ExtrudeExtent::Distance { length: 5.0 },
        ),
    );
    let rejected = CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetFeatureSuppressed {
            target: FeatureId("sketch".into()),
            suppressed: true,
        },
    );
    assert!(rejected.is_err());
    assert!(matches!(
        model.features.get(&FeatureId("sketch".into())),
        Some(EngineeringFeature::Parametric {
            suppressed: false,
            ..
        })
    ));

    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetFeatureSuppressed {
            target: FeatureId("add".into()),
            suppressed: true,
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetFeatureSuppressed {
            target: FeatureId("sketch".into()),
            suppressed: true,
        },
    )
    .unwrap();
    model.validate_model().unwrap();
}

#[test]
fn legacy_spatial_thickness_cannot_be_added_to_parametric_history() {
    let mut model = empty_part();
    add(
        &mut model,
        "sketch",
        sketch(
            rectangle_profile("edge", Point2D::new(0.0, 0.0), 20.0, 10.0),
            principal_xy(),
        ),
    );
    let result = CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetPartThickness {
            source: Some(scc_engineering::SourcedLength::Literal { value: 5.0 }),
        },
    );
    assert!(result.is_err());
    assert!(model.spatial.is_empty());
    model.validate_model().unwrap();
}

#[test]
fn generic_history_does_not_claim_existing_geometry_or_spatial_support() {
    let mut model = empty_part();
    add(
        &mut model,
        "sketch",
        sketch(
            rectangle_profile("edge", Point2D::new(0.0, 0.0), 20.0, 10.0),
            principal_xy(),
        ),
    );
    add(
        &mut model,
        "add",
        extrude(
            "sketch",
            ExtrudeOperation::Add,
            ExtrudeExtent::Distance { length: 5.0 },
        ),
    );
    let readiness = model.validate_spatial_readiness();
    assert!(!readiness.ready);
    assert!(readiness.issues.iter().any(|issue| matches!(
        issue,
        scc_engineering::SpatialIssue::UnsupportedForSpatial { feature }
            if feature == &FeatureId("sketch".into()) || feature == &FeatureId("add".into())
    )));
    let error = scc_engineering::lowering::lower_and_solve(&model).unwrap_err();
    assert!(format!("{error:?}").contains("Parametric"));
}

#[test]
fn datum_axis_and_revolve_join_the_same_history_with_dependency_guards() {
    let mut model = empty_part();
    add(
        &mut model,
        "profile",
        sketch(
            rectangle_profile("revolve-edge", Point2D::new(10.0, 5.0), 20.0, 10.0),
            PlaneReference::Principal {
                plane: PrincipalPlane::XZ,
            },
        ),
    );
    add(
        &mut model,
        "axis",
        ParametricFeature::DatumAxis {
            definition: DatumAxisDefinition::ThroughTwoPoints {
                first: Vector3D {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                second: Vector3D {
                    x: 2.0,
                    y: 0.0,
                    z: 0.0,
                },
            },
        },
    );
    add(
        &mut model,
        "revolve",
        ParametricFeature::Revolve {
            profile: FeatureId("profile".into()),
            axis: AxisReference::DatumAxis {
                feature: FeatureId("axis".into()),
            },
            operation: RevolveOperation::Add,
            angle: RevolveAngle::Partial { degrees: 180.0 },
            orientation: RevolveOrientation::Reverse,
        },
    );
    model.validate_model().unwrap();

    let snapshot = model.clone();
    assert!(CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetFeatureSuppressed {
            target: FeatureId("axis".into()),
            suppressed: true,
        },
    )
    .is_err());
    assert_eq!(model, snapshot);

    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetFeatureSuppressed {
            target: FeatureId("revolve".into()),
            suppressed: true,
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetFeatureSuppressed {
            target: FeatureId("axis".into()),
            suppressed: true,
        },
    )
    .unwrap();
    model.validate_model().unwrap();

    let reorder = CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::ReorderFeature {
            target: FeatureId("axis".into()),
            before: Some(FeatureId("profile".into())),
        },
    );
    assert!(reorder.is_ok());
    let reorder_axis_after_revolve = CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::ReorderFeature {
            target: FeatureId("axis".into()),
            before: None,
        },
    );
    assert!(reorder_axis_after_revolve.is_err());
}

#[test]
fn datum_axis_rejects_zero_length_and_revolve_rejects_invalid_angle() {
    let zero_axis = ParametricFeature::DatumAxis {
        definition: DatumAxisDefinition::ThroughTwoPoints {
            first: Vector3D {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            second: Vector3D {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
        },
    };
    assert!(zero_axis.validate().is_err());

    let overflow_axis = ParametricFeature::DatumAxis {
        definition: DatumAxisDefinition::ThroughTwoPoints {
            first: Vector3D {
                x: -1.0e308,
                y: 0.0,
                z: 0.0,
            },
            second: Vector3D {
                x: 1.0e308,
                y: 0.0,
                z: 0.0,
            },
        },
    };
    let error = overflow_axis.validate().unwrap_err();
    assert!(format!("{error:?}").contains("INVALID_DATUM_AXIS"));

    let invalid = ParametricFeature::Revolve {
        profile: FeatureId("profile".into()),
        axis: AxisReference::DatumAxis {
            feature: FeatureId("axis".into()),
        },
        operation: RevolveOperation::Add,
        angle: RevolveAngle::Partial { degrees: f64::NAN },
        orientation: RevolveOrientation::Forward,
    };
    assert!(invalid.validate().is_err());
}
