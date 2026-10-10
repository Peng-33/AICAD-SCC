use std::collections::BTreeMap;

use scc_core::ids::EntityId;
use scc_geometry::{
    export_dxf, export_svg, GeometryEntity, GeometryModel, GeometryPrimitive, Point2,
};

fn sample_model() -> GeometryModel {
    let mut model = GeometryModel::new(1, 2, 0);
    let primitives = [
        (
            "arc",
            GeometryPrimitive::Arc {
                center: Point2 { x: 0.0, y: 0.0 },
                radius: 1.0,
                start_angle: 0.0,
                end_angle: std::f64::consts::FRAC_PI_2,
            },
        ),
        (
            "circle&hole",
            GeometryPrimitive::Circle {
                center: Point2 { x: 2.0, y: 3.0 },
                radius: 1.5,
            },
        ),
        (
            "line",
            GeometryPrimitive::Line {
                start: Point2 { x: -2.0, y: -1.0 },
                end: Point2 { x: 4.0, y: 5.0 },
            },
        ),
        (
            "marker",
            GeometryPrimitive::Point {
                position: Point2 { x: 0.5, y: 0.5 },
            },
        ),
        (
            "plate",
            GeometryPrimitive::Rectangle {
                origin: Point2 { x: -1.0, y: -2.0 },
                width: 2.0,
                height: 4.0,
            },
        ),
        (
            "profile",
            GeometryPrimitive::Polygon {
                vertices: vec![
                    Point2 { x: 0.0, y: 0.0 },
                    Point2 { x: 2.0, y: 0.0 },
                    Point2 { x: 1.0, y: 2.0 },
                ],
            },
        ),
    ];
    model.entities = primitives
        .into_iter()
        .map(|(id, primitive)| {
            let id = EntityId(id.into());
            (id.clone(), GeometryEntity { id, primitive })
        })
        .collect::<BTreeMap<_, _>>();
    model
}

#[test]
fn svg_matches_byte_for_byte_golden_fixture() {
    let svg = export_svg(&sample_model()).unwrap();
    assert_eq!(svg, include_str!("golden/model.svg"));
}

#[test]
fn svg_is_deterministic() {
    let model = sample_model();
    assert_eq!(export_svg(&model).unwrap(), export_svg(&model).unwrap());
}

#[test]
fn dxf_matches_byte_for_byte_golden_fixture() {
    let dxf = export_dxf(&sample_model()).unwrap();
    let golden = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/model.dxf");
    if std::env::var("SCC_WRITE_GOLDENS").ok().as_deref() == Some("1") {
        std::fs::write(golden, &dxf).unwrap();
        return;
    }
    assert_eq!(dxf, include_str!("golden/model.dxf"));
}

#[test]
fn dxf_is_deterministic() {
    let model = sample_model();
    assert_eq!(export_dxf(&model).unwrap(), export_dxf(&model).unwrap());
}
