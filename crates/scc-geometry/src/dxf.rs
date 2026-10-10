use std::fmt::Write;

use crate::svg::number;
use crate::{GeometryError, GeometryModel, GeometryPrimitive, Point2};

pub(crate) fn render_dxf(model: &GeometryModel) -> Result<String, GeometryError> {
    let mut dxf = String::from(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1027\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n",
    );
    for entity in model.entities.values() {
        match &entity.primitive {
            GeometryPrimitive::Point { position } => {
                point_entity(&mut dxf, position.x, position.y, "0")?;
            }
            GeometryPrimitive::Line { start, end } => {
                line_entity(&mut dxf, start.x, start.y, end.x, end.y, "0")?;
            }
            GeometryPrimitive::Circle { center, radius } => {
                circle_entity(&mut dxf, center.x, center.y, *radius, "0")?;
            }
            GeometryPrimitive::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                arc_entity(
                    &mut dxf,
                    center.x,
                    center.y,
                    *radius,
                    start_angle.to_degrees(),
                    end_angle.to_degrees(),
                    "0",
                )?;
            }
            GeometryPrimitive::Rectangle {
                origin,
                width,
                height,
            } => {
                closed_polyline_entity(
                    &mut dxf,
                    &[
                        *origin,
                        Point2 {
                            x: origin.x + width,
                            y: origin.y,
                        },
                        Point2 {
                            x: origin.x + width,
                            y: origin.y + height,
                        },
                        Point2 {
                            x: origin.x,
                            y: origin.y + height,
                        },
                    ],
                    "0",
                )?;
            }
            GeometryPrimitive::Polygon { vertices } => {
                closed_polyline_entity(&mut dxf, vertices, "0")?;
            }
        }
    }
    dxf.push_str("0\nENDSEC\n0\nEOF\n");
    Ok(dxf)
}

/// Write one `code/value` pair.
pub fn group(output: &mut String, code: u16, value: &str) -> Result<(), GeometryError> {
    writeln!(output, "{code}\n{value}").map_err(|error| GeometryError::InvalidGraph {
        message: format!("failed to format DXF output: {error}"),
    })
}

/// Entity prologue: entity type followed by its layer.
pub fn entity_header(output: &mut String, kind: &str, layer: &str) -> Result<(), GeometryError> {
    let subclass = match kind {
        "POINT" => "AcDbPoint",
        "LINE" => "AcDbLine",
        "CIRCLE" | "ARC" => "AcDbCircle",
        "LWPOLYLINE" => "AcDbPolyline",
        _ => {
            return Err(GeometryError::InvalidGraph {
                message: format!("unsupported DXF entity type: {kind}"),
            })
        }
    };
    group(output, 0, kind)?;
    group(output, 100, "AcDbEntity")?;
    group(output, 8, layer)?;
    group(output, 100, subclass)
}

/// A single coordinate triple (group codes 10/20/30, z forced to 0).
pub fn point_groups(output: &mut String, x: f64, y: f64) -> Result<(), GeometryError> {
    group(output, 10, &number(x))?;
    group(output, 20, &number(y))?;
    group(output, 30, "0")
}

/// `POINT` entity.
pub fn point_entity(output: &mut String, x: f64, y: f64, layer: &str) -> Result<(), GeometryError> {
    entity_header(output, "POINT", layer)?;
    point_groups(output, x, y)
}

/// `LINE` entity from (x1,y1) to (x2,y2).
#[allow(clippy::too_many_arguments)]
pub fn line_entity(
    output: &mut String,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    layer: &str,
) -> Result<(), GeometryError> {
    entity_header(output, "LINE", layer)?;
    point_groups(output, x1, y1)?;
    group(output, 11, &number(x2))?;
    group(output, 21, &number(y2))?;
    group(output, 31, "0")
}

/// `CIRCLE` entity.
pub fn circle_entity(
    output: &mut String,
    cx: f64,
    cy: f64,
    radius: f64,
    layer: &str,
) -> Result<(), GeometryError> {
    entity_header(output, "CIRCLE", layer)?;
    point_groups(output, cx, cy)?;
    group(output, 40, &number(radius))
}

/// `ARC` entity; angles are degrees, counter-clockwise in the DXF Y-up plane.
#[allow(clippy::too_many_arguments)]
pub fn arc_entity(
    output: &mut String,
    cx: f64,
    cy: f64,
    radius: f64,
    start_angle_deg: f64,
    end_angle_deg: f64,
    layer: &str,
) -> Result<(), GeometryError> {
    entity_header(output, "ARC", layer)?;
    point_groups(output, cx, cy)?;
    group(output, 40, &number(radius))?;
    group(output, 50, &number(start_angle_deg))?;
    group(output, 51, &number(end_angle_deg))
}

/// Closed `LWPOLYLINE` through `vertices` in order.
pub fn closed_polyline_entity(
    output: &mut String,
    vertices: &[Point2],
    layer: &str,
) -> Result<(), GeometryError> {
    entity_header(output, "LWPOLYLINE", layer)?;
    group(output, 90, &vertices.len().to_string())?;
    group(output, 70, "1")?;
    for vertex in vertices {
        group(output, 10, &number(vertex.x))?;
        group(output, 20, &number(vertex.y))?;
    }
    Ok(())
}
