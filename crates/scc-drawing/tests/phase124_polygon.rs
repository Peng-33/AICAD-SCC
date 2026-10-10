//! Phase 12.4 Polygon engineering closure fixtures and renderer goldens.

use std::collections::BTreeSet;
use std::path::PathBuf;

use scc_core::authority::AuthorityLevel;
use scc_drawing::{
    build_three_view_drawing, drawing_to_dxf, drawing_to_svg, ThreeViewDrawingInputs,
};
use scc_engineering::binding::FeatureParameterSlot;
use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::feature::{DrawingProperty, EngineeringFeature, ProjectionMethod};
use scc_engineering::lowering::lower_and_solve;
use scc_engineering::parameter::ParameterValue;
use scc_engineering::spatial::SourcedLength;
use scc_engineering::{EngineeringModel, FeatureId, ParameterId, ParameterOrigin, PartId, Point2D};
use scc_projection::{project_principal_views, PrincipalViewKind, SpatialElementRef};
use scc_projection_consistency::validate_projection_consistency;
use scc_spatial::{lower_spatial, SpatialFootprint, SpatialInputs};
use scc_visibility::classify_visibility;

fn apply(model: &mut EngineeringModel, command: EngineeringCommand) {
    CommandExecutor::apply(model, &command).unwrap();
}

fn polygon(vertices: &[(f64, f64)]) -> scc_engineering::BaseProfile {
    let vertices = vertices
        .iter()
        .map(|(x, y)| Point2D::new(*x, *y))
        .collect::<Vec<_>>();
    scc_engineering::BaseProfile::Polygon {
        vertices: scc_engineering::profile::canonicalize_polygon(&vertices).unwrap(),
    }
}

fn base_model(name: &str, profile: scc_engineering::BaseProfile) -> EngineeringModel {
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
        EngineeringCommand::SetPartThickness {
            source: Some(SourcedLength::Literal { value: 8.0 }),
        },
    );
    model.drawing.set(DrawingProperty::ProjectionMethod {
        method: ProjectionMethod::ThirdAngle,
    });
    model.drawing.set(DrawingProperty::Scale { value: 1.0 });
    model
}

fn add_hole_at(model: &mut EngineeringModel, id: &str, center: (f64, f64), diameter: f64) {
    apply(
        model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId(id.into()),
                diameter,
            },
        },
    );
    for (slot, suffix, value) in [
        (FeatureParameterSlot::CenterX, "x", center.0),
        (FeatureParameterSlot::CenterY, "y", center.1),
    ] {
        let parameter = ParameterId(format!("{id}_{suffix}"));
        apply(
            model,
            EngineeringCommand::DeclareParameter {
                id: parameter.clone(),
                value: ParameterValue::Scalar(value),
                origin: ParameterOrigin::User,
                authority: AuthorityLevel::L0,
            },
        );
        apply(
            model,
            EngineeringCommand::BindFeatureParameter {
                feature: FeatureId(id.into()),
                slot,
                parameter,
            },
        );
    }
    apply(
        model,
        EngineeringCommand::SetHoleExtent {
            target: FeatureId(id.into()),
            extent: Some(scc_engineering::HoleExtent::ThroughAll),
        },
    );
}

fn add_slot_at(model: &mut EngineeringModel, center: (f64, f64), angle: f64) {
    apply(
        model,
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Slot {
                id: FeatureId("slot".into()),
                center: Point2D::new(center.0, center.1),
                length: 30.0,
                width: 8.0,
                orientation_degrees: angle,
            },
        },
    );
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

fn assert_polygon_views(
    spatial: &scc_spatial::SpatialModel,
    projection: &scc_projection::ProjectionModel,
    visibility: &scc_visibility::VisibilityModel,
    edge_count: usize,
) {
    assert!(matches!(
        spatial.body.footprint,
        SpatialFootprint::Composite { .. }
    ));
    for kind in [
        PrincipalViewKind::Front,
        PrincipalViewKind::Top,
        PrincipalViewKind::Left,
    ] {
        let view = &projection.views[&kind];
        let mut indexes = BTreeSet::new();
        for id in view.primitives.keys() {
            assert!(visibility.class_for(kind, id).is_some());
            if let Some(SpatialElementRef::BodySegment { index }) =
                projection.element_for_primitive(id)
            {
                indexes.insert(*index);
            }
        }
        if kind == PrincipalViewKind::Front {
            assert_eq!(
                indexes.len(),
                edge_count,
                "missing Polygon edges in {kind:?}"
            );
        } else {
            assert!(
                !view.primitives.is_empty(),
                "empty derived silhouette in {kind:?}"
            );
        }
    }
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

fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name);
    std::fs::read_to_string(path).unwrap_or_default()
}

fn triangle() -> EngineeringModel {
    base_model(
        "phase124_triangle",
        polygon(&[(0.0, 0.0), (100.0, 0.0), (50.0, 80.0)]),
    )
}

fn pentagon() -> EngineeringModel {
    base_model(
        "phase124_pentagon",
        polygon(&[
            (0.0, 0.0),
            (100.0, 0.0),
            (120.0, 40.0),
            (80.0, 90.0),
            (0.0, 60.0),
        ]),
    )
}

#[test]
fn triangle_and_pentagon_close_all_derived_view_layers() {
    for (model, edge_count) in [(triangle(), 3), (pentagon(), 5)] {
        let (_, spatial, projection, visibility, drawing) = pipeline(model.clone());
        assert_polygon_views(&spatial, &projection, &visibility, edge_count);
        let svg = drawing_to_svg(&drawing).unwrap();
        let dxf = drawing_to_dxf(&drawing).unwrap();
        assert!(svg.contains("<svg"));
        assert!(dxf.contains("SECTION"));
        for _ in 0..20 {
            let next = pipeline(model.clone()).4;
            assert_eq!(drawing_to_svg(&next).unwrap(), svg);
            assert_eq!(drawing_to_dxf(&next).unwrap(), dxf);
        }
    }
}

#[test]
fn polygon_goldens_match_svg_and_dxf_renderers() {
    let cases = [
        (
            triangle(),
            "phase124_polygon_triangle.svg",
            "phase124_polygon_triangle.dxf",
        ),
        (
            pentagon(),
            "phase124_polygon_pentagon.svg",
            "phase124_polygon_pentagon.dxf",
        ),
    ];
    for (model, svg_name, dxf_name) in cases {
        let drawing = pipeline(model).4;
        let svg = drawing_to_svg(&drawing).unwrap();
        let dxf = drawing_to_dxf(&drawing).unwrap();
        write_golden_if_requested(svg_name, &svg);
        write_golden_if_requested(dxf_name, &dxf);
        assert_eq!(svg, golden(svg_name));
        assert_eq!(dxf, golden(dxf_name));
    }
}

#[test]
fn polygon_hole_inside_is_supported_and_boundary_cases_fail_closed() {
    for (center, expected_ok) in [
        ((30.0, 20.0), true),
        ((5.0, 5.0), false),
        ((2.0, 20.0), false),
        ((-10.0, 20.0), false),
    ] {
        let mut model = pentagon();
        add_hole_at(&mut model, "hole", center, 10.0);
        let result = {
            let (lowering, geometry) = lower_and_solve(&model).unwrap();
            lower_spatial(SpatialInputs {
                engineering: &model,
                geometry: &geometry,
                source_map: &lowering.source_map,
            })
        };
        assert_eq!(result.is_ok(), expected_ok, "hole center {center:?}");
    }
}

#[test]
fn polygon_slot_inside_and_rotated_are_supported_and_crossing_fails_closed() {
    for (center, angle, expected_ok) in [
        ((50.0, 30.0), 0.0, true),
        ((50.0, 30.0), 45.0, true),
        ((3.0, 20.0), 0.0, false),
        ((-15.0, 20.0), 0.0, false),
    ] {
        let mut model = pentagon();
        add_slot_at(&mut model, center, angle);
        let result = {
            let (lowering, geometry) = lower_and_solve(&model).unwrap();
            lower_spatial(SpatialInputs {
                engineering: &model,
                geometry: &geometry,
                source_map: &lowering.source_map,
            })
        };
        assert_eq!(result.is_ok(), expected_ok, "slot {center:?} at {angle}°");
    }
}
