//! Phase 12.3 through-slot fixtures, mutations, and goldens.

use std::collections::BTreeMap;
use std::path::PathBuf;

use scc_drawing::dimension::{format_dimension, DimensionValue};
use scc_drawing::{
    build_three_view_drawing, drawing_to_dxf, drawing_to_svg, DrawingPrimitive,
    DrawingPrimitiveRole, ThreeViewDrawingInputs,
};
use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::feature::{DrawingProperty, EngineeringFeature, ProjectionMethod};
use scc_engineering::lowering::lower_and_solve;
use scc_engineering::spatial::SourcedLength;
use scc_engineering::{EngineeringModel, FeatureId, PartId, Point2D, ProfileSegment};
use scc_projection::{
    project_principal_views, PrincipalViewKind, ProjectedGeometry, SpatialElementRef,
};
use scc_projection_consistency::validate_projection_consistency;
use scc_spatial::{
    lower_spatial, SpatialCutId, SpatialFootprint, SpatialInputs, SpatialProfileSegment,
};
use scc_visibility::{classify_visibility, VisibilityClass};

fn apply(model: &mut EngineeringModel, command: EngineeringCommand) {
    CommandExecutor::apply(model, &command).unwrap();
}

fn slot_model(
    name: &str,
    profile: scc_engineering::BaseProfile,
    thickness: f64,
    center: Point2D,
    length: f64,
    width: f64,
    orientation_degrees: f64,
) -> EngineeringModel {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId(name.into()),
            name: name.into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("base".into()),
                name: "base".into(),
                profile,
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Slot {
                id: FeatureId("slot".into()),
                center,
                length,
                width,
                orientation_degrees,
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: thickness }),
        },
    );
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn fixture_a() -> EngineeringModel {
    slot_model(
        "phase123_slot_horizontal",
        scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
        10.0,
        Point2D::new(0.0, 0.0),
        40.0,
        12.0,
        0.0,
    )
}

fn fixture_b() -> EngineeringModel {
    slot_model(
        "phase123_slot_vertical",
        scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
        10.0,
        Point2D::new(0.0, 0.0),
        40.0,
        12.0,
        90.0,
    )
}

fn fixture_c() -> EngineeringModel {
    slot_model(
        "phase123_slot_rotated",
        scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
        10.0,
        Point2D::new(0.0, 0.0),
        40.0,
        12.0,
        45.0,
    )
}

fn fixture_d() -> EngineeringModel {
    slot_model(
        "phase123_slot_circle",
        scc_engineering::BaseProfile::Circle {
            center: Point2D::new(0.0, 0.0),
            diameter: 160.0,
        },
        20.0,
        Point2D::new(0.0, 0.0),
        40.0,
        12.0,
        0.0,
    )
}

fn fixture_e() -> EngineeringModel {
    slot_model(
        "phase123_slot_rounded",
        scc_engineering::BaseProfile::RoundedRectangle {
            center: Point2D::new(0.0, 0.0),
            width: 120.0,
            height: 80.0,
            corner_radius: 10.0,
        },
        10.0,
        Point2D::new(0.0, 0.0),
        40.0,
        12.0,
        0.0,
    )
}

fn fixture_f() -> EngineeringModel {
    slot_model(
        "phase123_slot_composite",
        composite_profile(),
        10.0,
        Point2D::new(0.0, 0.0),
        40.0,
        12.0,
        0.0,
    )
}

fn composite_profile() -> scc_engineering::BaseProfile {
    scc_engineering::BaseProfile::Composite {
        segments: vec![
            ProfileSegment::Line {
                start: Point2D::new(-60.0, -40.0),
                end: Point2D::new(60.0, -40.0),
            },
            ProfileSegment::Line {
                start: Point2D::new(60.0, -40.0),
                end: Point2D::new(50.0, 40.0),
            },
            ProfileSegment::Line {
                start: Point2D::new(50.0, 40.0),
                end: Point2D::new(-50.0, 40.0),
            },
            ProfileSegment::Line {
                start: Point2D::new(-50.0, 40.0),
                end: Point2D::new(-60.0, -40.0),
            },
        ],
    }
}

fn pipeline(
    model: EngineeringModel,
) -> (
    EngineeringModel,
    scc_spatial::SpatialModel,
    scc_projection::ProjectionModel,
    scc_visibility::VisibilityModel,
    scc_drawing::DrawingModel,
) {
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let spatial = lower_spatial(SpatialInputs {
        engineering: &model,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .unwrap();
    let projection = project_principal_views(&spatial).unwrap();
    let visibility = classify_visibility(&spatial, &projection).unwrap();
    validate_projection_consistency(&spatial, &projection, &visibility).unwrap();
    let drawing = build_three_view_drawing(&ThreeViewDrawingInputs {
        engineering: &model,
        spatial: &spatial,
        projection: &projection,
        visibility: &visibility,
    })
    .unwrap();
    (model, spatial, projection, visibility, drawing)
}

fn view(
    drawing: &scc_drawing::DrawingModel,
    kind: scc_drawing::DrawingViewKind,
) -> &scc_drawing::DrawingView {
    drawing
        .principal_sheet()
        .unwrap()
        .views
        .values()
        .find(|view| view.kind == kind)
        .unwrap()
}

fn slot_id() -> SpatialCutId {
    SpatialCutId("slot:cut:0000".into())
}

fn assert_front_slot_geometry(
    projection: &scc_projection::ProjectionModel,
    spatial: &scc_spatial::SpatialModel,
) {
    let front = &projection.views[&PrincipalViewKind::Front];
    let mut lines = 0;
    let mut arcs = 0;
    let mut indexes = BTreeMap::<u32, u32>::new();
    for (id, primitive) in &front.primitives {
        match projection.element_for_primitive(id) {
            Some(SpatialElementRef::SlotSegment { id: cut, index }) => {
                assert_eq!(cut, &slot_id());
                *indexes.entry(*index).or_default() += 1;
                match primitive.geometry {
                    ProjectedGeometry::Segment { .. } => lines += 1,
                    ProjectedGeometry::Arc { .. } => arcs += 1,
                    _ => panic!("front slot primitive is not a line or arc"),
                }
                assert!(
                    !id.0.contains("slot")
                        || id.0.contains("slot-cut")
                        || id.0.contains("slot-segment"),
                    "typed provenance must not parse ad-hoc ids: {id}"
                );
            }
            Some(
                SpatialElementRef::Body
                | SpatialElementRef::BodySegment { .. }
                | SpatialElementRef::Cut { .. }
                | SpatialElementRef::SlotCut { .. },
            ) => {}
            None => panic!("front primitive {id} has no source"),
        }
    }
    assert_eq!(lines, 4);
    assert_eq!(arcs, 4);
    assert_eq!(indexes.len(), 4);
    assert!(indexes.values().all(|count| *count == 2));
    let slot = spatial.slot_cuts.get(&slot_id()).unwrap();
    assert_eq!(slot.segments.len(), 4);
    assert!(matches!(
        slot.segments[0],
        SpatialProfileSegment::Line { .. }
    ));
    assert!(matches!(
        slot.segments[1],
        SpatialProfileSegment::Arc { .. }
    ));
    assert!(matches!(
        slot.segments[2],
        SpatialProfileSegment::Line { .. }
    ));
    assert!(matches!(
        slot.segments[3],
        SpatialProfileSegment::Arc { .. }
    ));
}

fn assert_side_slot_hidden(
    projection: &scc_projection::ProjectionModel,
    visibility: &scc_visibility::VisibilityModel,
    kind: PrincipalViewKind,
) {
    let view = &projection.views[&kind];
    let mut count = 0;
    for id in view.primitives.keys() {
        if matches!(
            projection.element_for_primitive(id),
            Some(SpatialElementRef::SlotCut { .. })
        ) {
            count += 1;
            assert_eq!(
                visibility.class_for(kind, id),
                Some(VisibilityClass::Hidden)
            );
        }
        assert!(!matches!(
            projection.element_for_primitive(id),
            Some(SpatialElementRef::SlotSegment { .. })
        ));
    }
    assert_eq!(count, 4);
}

fn assert_callout(drawing: &scc_drawing::DrawingModel) {
    let front = view(drawing, scc_drawing::DrawingViewKind::Front);
    let dimension = front
        .dimensions
        .get(&scc_drawing::DrawingDimensionId(
            "three_view.slot:slot".into(),
        ))
        .expect("typed slot callout");
    assert!(matches!(
        dimension.value,
        DimensionValue::SlotCallout { .. }
    ));
    assert_eq!(format_dimension(dimension), "SLOT 40 × 12 THRU");
    assert!(front.dimension_layouts.contains_key(&dimension.id));
}

fn assert_not_bbox_cut(spatial: &scc_spatial::SpatialModel) {
    let slot = spatial.slot_cuts.values().next().unwrap();
    assert!(
        slot.segments
            .iter()
            .any(|segment| matches!(segment, SpatialProfileSegment::Arc { .. })),
        "slot spatial truth must keep exact arcs, not a bounding rectangle"
    );
    assert_eq!(slot.overall_length, 40.0);
    assert_eq!(slot.width, 12.0);
}

fn write_golden_if_requested(name: &str, contents: &str) {
    if std::env::var("SCC_WRITE_GOLDENS").ok().as_deref() != Some("1") {
        return;
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name);
    std::fs::write(path, contents).unwrap();
}

#[test]
fn fixture_a_horizontal_through_slot() {
    let (model, spatial, projection, visibility, drawing) = pipeline(fixture_a());
    assert!(matches!(
        model.features.get(&FeatureId("slot".into())),
        Some(EngineeringFeature::Slot {
            length,
            width,
            orientation_degrees,
            ..
        }) if *length == 40.0 && *width == 12.0 && *orientation_degrees == 0.0
    ));
    assert_not_bbox_cut(&spatial);
    assert_front_slot_geometry(&projection, &spatial);
    assert_side_slot_hidden(&projection, &visibility, PrincipalViewKind::Top);
    assert_side_slot_hidden(&projection, &visibility, PrincipalViewKind::Left);
    assert_callout(&drawing);
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    let mut lines = 0;
    let mut arcs = 0;
    for (id, primitive) in &front.primitives {
        let Some(projected) = drawing.source_map.projected_for_primitive(id) else {
            continue;
        };
        if !matches!(
            projection.element_for_primitive(projected),
            Some(SpatialElementRef::SlotSegment { .. })
        ) {
            continue;
        }
        match primitive {
            DrawingPrimitive::Line {
                role: DrawingPrimitiveRole::Object,
                ..
            } => lines += 1,
            DrawingPrimitive::Arc {
                role: DrawingPrimitiveRole::Object,
                ..
            } => arcs += 1,
            other => panic!("front slot drawing primitive is not an object line/arc: {other:?}"),
        }
    }
    assert_eq!(lines, 2);
    assert_eq!(arcs, 2);
    let top = view(&drawing, scc_drawing::DrawingViewKind::Top);
    assert!(top.primitives.values().any(|primitive| matches!(
        primitive,
        DrawingPrimitive::Line {
            role: DrawingPrimitiveRole::Hidden,
            ..
        }
    )));
    let top_proj = &projection.views[&PrincipalViewKind::Top];
    let width = top_proj.bounds.max_u - top_proj.bounds.min_u;
    assert_eq!(width, 120.0);
    let slot = spatial.slot_cuts.get(&slot_id()).unwrap();
    let (x0, x1, _, _) = {
        let mut x0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        for segment in &slot.segments {
            let (min_x, max_x, _, _) = segment.bounds().unwrap();
            x0 = x0.min(min_x);
            x1 = x1.max(max_x);
        }
        (x0, x1, 0.0, 0.0)
    };
    assert!((x1 - x0 - 40.0).abs() < 1.0e-9);
    assert!((x1 - x0 - 28.0).abs() > 1.0);
}

#[test]
fn fixture_b_vertical_slot() {
    let (_, spatial, projection, visibility, drawing) = pipeline(fixture_b());
    assert_eq!(
        spatial
            .slot_cuts
            .values()
            .next()
            .unwrap()
            .orientation_degrees,
        90.0
    );
    assert_front_slot_geometry(&projection, &spatial);
    assert_side_slot_hidden(&projection, &visibility, PrincipalViewKind::Top);
    assert_callout(&drawing);
}

#[test]
fn fixture_c_rotated_slot_is_native_and_deterministic() {
    let model = fixture_c();
    let (_, spatial, projection, visibility, drawing) = pipeline(model.clone());
    assert_front_slot_geometry(&projection, &spatial);
    assert_side_slot_hidden(&projection, &visibility, PrincipalViewKind::Left);
    assert_callout(&drawing);
    let expected_svg = drawing_to_svg(&drawing).unwrap();
    let expected_dxf = drawing_to_dxf(&drawing).unwrap();
    assert!(expected_svg.contains("<path"));
    assert!(expected_dxf.contains("\nARC\n"));
    assert!(expected_dxf.contains("\nLINE\n"));
    assert!(expected_svg.contains("SLOT 40"));
    for _ in 0..100 {
        let next = pipeline(model.clone()).4;
        assert_eq!(drawing_to_svg(&next).unwrap(), expected_svg);
        assert_eq!(drawing_to_dxf(&next).unwrap(), expected_dxf);
    }
}

#[test]
fn fixture_g_135_degree_slot_is_native_and_contained() {
    let (_, spatial, projection, visibility, drawing) = pipeline(slot_model(
        "phase123_slot_135",
        scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
        10.0,
        Point2D::new(0.0, 0.0),
        40.0,
        12.0,
        135.0,
    ));
    assert_eq!(spatial.slot_cuts.len(), 1);
    assert_front_slot_geometry(&projection, &spatial);
    assert_side_slot_hidden(&projection, &visibility, PrincipalViewKind::Top);
    assert_callout(&drawing);
}

#[test]
fn fixture_d_circular_base_and_slot_coexist() {
    let (_, spatial, projection, visibility, drawing) = pipeline(fixture_d());
    assert!(matches!(
        spatial.body.footprint,
        SpatialFootprint::Circle { .. }
    ));
    assert_eq!(spatial.slot_cuts.len(), 1);
    assert_front_slot_geometry(&projection, &spatial);
    assert_side_slot_hidden(&projection, &visibility, PrincipalViewKind::Top);
    assert_callout(&drawing);
}

#[test]
fn fixture_e_rounded_base_and_slot_coexist() {
    let (_, spatial, projection, visibility, drawing) = pipeline(fixture_e());
    assert!(matches!(
        spatial.body.footprint,
        SpatialFootprint::Composite { .. }
    ));
    assert_eq!(spatial.slot_cuts.len(), 1);
    assert_front_slot_geometry(&projection, &spatial);
    assert_side_slot_hidden(&projection, &visibility, PrincipalViewKind::Left);
    assert_callout(&drawing);
}

#[test]
fn fixture_f_composite_base_and_slot_coexist() {
    let (_, spatial, projection, visibility, drawing) = pipeline(fixture_f());
    assert!(matches!(
        spatial.body.footprint,
        SpatialFootprint::Composite { .. }
    ));
    assert_eq!(spatial.slot_cuts.len(), 1);
    assert_front_slot_geometry(&projection, &spatial);
    assert_side_slot_hidden(&projection, &visibility, PrincipalViewKind::Top);
    assert_callout(&drawing);
}

#[test]
fn orientation_180_is_the_same_engineering_identity_as_zero() {
    let zero = pipeline(fixture_a());
    let flipped = pipeline(slot_model(
        "phase123_slot_horizontal",
        scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
        10.0,
        Point2D::new(0.0, 0.0),
        40.0,
        12.0,
        180.0,
    ));
    let EngineeringFeature::Slot {
        orientation_degrees,
        ..
    } = flipped.0.features.get(&FeatureId("slot".into())).unwrap()
    else {
        panic!("expected slot");
    };
    assert_eq!(*orientation_degrees, 0.0);
    assert_eq!(
        serde_json::to_string(&zero.1.slot_cuts).unwrap(),
        serde_json::to_string(&flipped.1.slot_cuts).unwrap()
    );
}

#[test]
fn invalid_slots_fail_closed() {
    for (length, width, angle) in [
        (0.0, 12.0, 0.0),
        (40.0, 0.0, 0.0),
        (10.0, 12.0, 0.0),
        (12.0, 12.0, 0.0),
        (40.0, 12.0, f64::NAN),
        (f64::INFINITY, 12.0, 0.0),
        (40.0, f64::NAN, 0.0),
    ] {
        let mut model = EngineeringModel::new();
        apply(
            &mut model,
            EngineeringCommand::CreatePart {
                id: PartId("p".into()),
                name: "p".into(),
            },
        );
        let err = CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::Slot {
                    id: FeatureId("slot".into()),
                    center: Point2D::new(0.0, 0.0),
                    length,
                    width,
                    orientation_degrees: angle,
                },
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                scc_engineering::error::EngineeringError::InvalidProfile { .. }
                    | scc_engineering::error::EngineeringError::NonPositiveLength { .. }
                    | scc_engineering::error::EngineeringError::NonFiniteValue { .. }
            ),
            "expected fail-closed for L={length} W={width} angle={angle}, got {err:?}"
        );
    }
}

#[test]
fn missing_thickness_and_blind_extent_fail_closed() {
    let mut model = EngineeringModel::new();
    apply(
        &mut model,
        EngineeringCommand::CreatePart {
            id: PartId("p".into()),
            name: "p".into(),
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("base".into()),
                name: "base".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    );
    apply(
        &mut model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Slot {
                id: FeatureId("slot".into()),
                center: Point2D::new(0.0, 0.0),
                length: 40.0,
                width: 12.0,
                orientation_degrees: 0.0,
            },
        },
    );
    assert!(!model.validate_spatial_readiness().ready);
    apply(
        &mut model,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    let err = CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetHoleExtent {
            target: FeatureId("slot".into()),
            extent: Some(scc_engineering::HoleExtent::Blind {
                start_face: scc_engineering::HoleStartFace::Base,
                depth: SourcedLength::Literal { value: 4.0 },
            }),
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        scc_engineering::error::EngineeringError::InvalidSpatialTarget { .. }
    ));
}

#[test]
fn slot_outside_or_on_boundary_fails_closed() {
    for center in [Point2D::new(50.0, 0.0), Point2D::new(0.0, 40.0)] {
        let model = slot_model(
            "out",
            scc_engineering::BaseProfile::Rectangle {
                width: 120.0,
                height: 80.0,
            },
            10.0,
            center,
            40.0,
            12.0,
            0.0,
        );
        let (lowering, geometry) = lower_and_solve(&model).unwrap();
        assert!(lower_spatial(SpatialInputs {
            engineering: &model,
            geometry: &geometry,
            source_map: &lowering.source_map,
        })
        .is_err());
    }
    let touching = slot_model(
        "touch",
        scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
        10.0,
        Point2D::new(0.0, 0.0),
        120.0,
        12.0,
        0.0,
    );
    let (lowering, geometry) = lower_and_solve(&touching).unwrap();
    assert!(lower_spatial(SpatialInputs {
        engineering: &touching,
        geometry: &geometry,
        source_map: &lowering.source_map,
    })
    .is_err());
}

#[test]
fn containment_matrix_covers_all_supported_base_profiles() {
    type ContainmentCase = (
        &'static str,
        fn() -> scc_engineering::BaseProfile,
        f64,
        f64,
        f64,
    );
    let cases: [ContainmentCase; 4] = [
        (
            "rectangle",
            || scc_engineering::BaseProfile::Rectangle {
                width: 120.0,
                height: 80.0,
            },
            40.0,
            45.0,
            90.0,
        ),
        (
            "circle",
            || scc_engineering::BaseProfile::Circle {
                center: Point2D::new(0.0, 0.0),
                diameter: 160.0,
            },
            60.0,
            70.0,
            100.0,
        ),
        (
            "rounded_rectangle",
            || scc_engineering::BaseProfile::RoundedRectangle {
                center: Point2D::new(0.0, 0.0),
                width: 120.0,
                height: 80.0,
                corner_radius: 10.0,
            },
            40.0,
            45.0,
            90.0,
        ),
        ("composite", composite_profile, 35.0, 40.0, 90.0),
    ];

    for (name, profile, touch, cross, outside) in cases {
        let inside = slot_model(
            &format!("phase123_inside_{name}"),
            profile(),
            10.0,
            Point2D::new(0.0, 0.0),
            40.0,
            12.0,
            0.0,
        );
        let (lowering, geometry) = lower_and_solve(&inside).unwrap();
        let spatial = lower_spatial(SpatialInputs {
            engineering: &inside,
            geometry: &geometry,
            source_map: &lowering.source_map,
        })
        .unwrap();
        assert_eq!(spatial.slot_cuts.len(), 1, "inside {name}");

        for (case, center_x) in [("touch", touch), ("cross", cross), ("outside", outside)] {
            let model = slot_model(
                &format!("phase123_{case}_{name}"),
                profile(),
                10.0,
                Point2D::new(center_x, 0.0),
                40.0,
                12.0,
                0.0,
            );
            let (lowering, geometry) = lower_and_solve(&model).unwrap();
            assert!(
                lower_spatial(SpatialInputs {
                    engineering: &model,
                    geometry: &geometry,
                    source_map: &lowering.source_map,
                })
                .is_err(),
                "{case} slot unexpectedly accepted for {name}"
            );
        }
    }
}

#[test]
fn mutation_bbox_cut_fails() {
    let (_, mut spatial, _, visibility, _) = pipeline(fixture_a());
    let slot = spatial.slot_cuts.get_mut(&slot_id()).unwrap();
    let (x0, x1, y0, y1) = {
        let mut x0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut y0 = f64::INFINITY;
        let mut y1 = f64::NEG_INFINITY;
        for segment in &slot.segments {
            let (min_x, max_x, min_y, max_y) = segment.bounds().unwrap();
            x0 = x0.min(min_x);
            x1 = x1.max(max_x);
            y0 = y0.min(min_y);
            y1 = y1.max(max_y);
        }
        (x0, x1, y0, y1)
    };
    slot.segments = vec![
        line_seg(x0, y1, x1, y1, 0),
        line_seg(x1, y1, x1, y0, 1),
        line_seg(x1, y0, x0, y0, 2),
        line_seg(x0, y0, x0, y1, 3),
    ];
    let projection = project_principal_views(&spatial).unwrap();
    assert!(validate_projection_consistency(&spatial, &projection, &visibility).is_err());
}

fn line_seg(x0: f64, y0: f64, x1: f64, y1: f64, index: u32) -> SpatialProfileSegment {
    SpatialProfileSegment::Line {
        start: scc_geometry::Point2 { x: x0, y: y0 },
        end: scc_geometry::Point2 { x: x1, y: y1 },
        source_entity: scc_core::ids::EntityId(format!("slot:{index}")),
        segment_index: index,
    }
}

#[test]
fn mutation_geometry_inference_is_rejected() {
    let (model, _, _, _, _) = pipeline(fixture_e());
    assert_eq!(
        model
            .features
            .values()
            .filter(|feature| matches!(feature, EngineeringFeature::Slot { .. }))
            .count(),
        1
    );
    let mut body_only = EngineeringModel::new();
    apply(
        &mut body_only,
        EngineeringCommand::CreatePart {
            id: PartId("body".into()),
            name: "body".into(),
        },
    );
    apply(
        &mut body_only,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("base".into()),
                name: "base".into(),
                profile: scc_engineering::BaseProfile::RoundedRectangle {
                    center: Point2D::new(0.0, 0.0),
                    width: 120.0,
                    height: 80.0,
                    corner_radius: 10.0,
                },
            },
        },
    );
    apply(
        &mut body_only,
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 10.0 }),
        },
    );
    body_only.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    body_only.drawing.set(DrawingProperty::Scale { value: 1.0 });
    let drawing = pipeline(body_only).4;
    let front = view(&drawing, scc_drawing::DrawingViewKind::Front);
    assert!(!front.dimensions.keys().any(|id| id.0.contains("slot")));
    assert!(!front
        .dimensions
        .values()
        .any(|dimension| matches!(dimension.value, DimensionValue::SlotCallout { .. })));
}

#[test]
fn mutation_formatted_callout_is_not_engineering_truth() {
    assert!(serde_json::from_str::<EngineeringFeature>("\"SLOT 40 × 12 THRU\"").is_err());
    let json = serde_json::to_string(fixture_a().features.get(&FeatureId("slot".into())).unwrap())
        .unwrap();
    assert!(!json.contains("SLOT 40"));
    assert!(json.contains("\"type\":\"slot\""));
    assert!(json.contains("\"length\":40.0") || json.contains("\"length\":40"));
}

#[test]
fn mutation_drawing_does_not_invent_hidden_geometry() {
    let (_, _, projection, visibility, drawing) = pipeline(fixture_a());
    for kind in [
        scc_drawing::DrawingViewKind::Top,
        scc_drawing::DrawingViewKind::Left,
    ] {
        let drawing_view = view(&drawing, kind);
        let proj_kind = match kind {
            scc_drawing::DrawingViewKind::Top => PrincipalViewKind::Top,
            scc_drawing::DrawingViewKind::Left => PrincipalViewKind::Left,
            _ => unreachable!(),
        };
        let hidden_drawing = drawing_view
            .primitives
            .values()
            .filter(|primitive| {
                matches!(
                    primitive,
                    DrawingPrimitive::Line {
                        role: DrawingPrimitiveRole::Hidden,
                        ..
                    } | DrawingPrimitive::Arc {
                        role: DrawingPrimitiveRole::Hidden,
                        ..
                    }
                )
            })
            .count();
        let hidden_visibility = projection.views[&proj_kind]
            .primitives
            .keys()
            .filter(|id| visibility.class_for(proj_kind, id) == Some(VisibilityClass::Hidden))
            .count();
        assert_eq!(hidden_drawing, hidden_visibility);
    }
}

#[test]
fn phase123_goldens_match_renderer_output() {
    let cases = [
        (
            fixture_a(),
            "phase123_slot_horizontal.svg",
            "phase123_slot_horizontal.dxf",
        ),
        (
            fixture_c(),
            "phase123_slot_rotated.svg",
            "phase123_slot_rotated.dxf",
        ),
    ];
    for (model, svg_name, dxf_name) in cases {
        let drawing = pipeline(model).4;
        let svg = drawing_to_svg(&drawing).unwrap();
        let dxf = drawing_to_dxf(&drawing).unwrap();
        write_golden_if_requested(svg_name, &svg);
        write_golden_if_requested(dxf_name, &dxf);
        let expected_svg = golden(svg_name);
        let expected_dxf = golden(dxf_name);
        assert_eq!(svg, expected_svg);
        assert_eq!(dxf, expected_dxf);
    }
}

fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name);
    std::fs::read_to_string(path).unwrap_or_default()
}
