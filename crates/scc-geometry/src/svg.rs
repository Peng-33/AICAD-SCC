use std::fmt::Write;

use crate::{GeometryError, GeometryModel, GeometryPrimitive, Point2};

const VIEWBOX_PADDING: f64 = 1.0;

#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn from_model(model: &GeometryModel) -> Result<Self, GeometryError> {
        let mut bounds: Option<Self> = None;
        for entity in model.entities.values() {
            for point in primitive_bounds(&entity.primitive)? {
                bounds = Some(match bounds {
                    Some(current) => Self {
                        min_x: current.min_x.min(point.x),
                        min_y: current.min_y.min(point.y),
                        max_x: current.max_x.max(point.x),
                        max_y: current.max_y.max(point.y),
                    },
                    None => Self {
                        min_x: point.x,
                        min_y: point.y,
                        max_x: point.x,
                        max_y: point.y,
                    },
                });
            }
        }
        Ok(bounds.unwrap_or(Self {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        }))
    }
}

pub(crate) fn render_svg(model: &GeometryModel) -> Result<String, GeometryError> {
    let bounds = Bounds::from_model(model)?;
    let min_x = bounds.min_x - VIEWBOX_PADDING;
    let min_y = -bounds.max_y - VIEWBOX_PADDING;
    let width = (bounds.max_x - bounds.min_x) + VIEWBOX_PADDING * 2.0;
    let height = (bounds.max_y - bounds.min_y) + VIEWBOX_PADDING * 2.0;

    let mut svg = String::new();
    writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\">",
        number(min_x),
        number(min_y),
        number(width),
        number(height)
    )
    .unwrap();
    svg.push_str("  <g fill=\"none\" stroke=\"black\" stroke-width=\"0.5\">\n");
    for entity in model.entities.values() {
        write_svg_entity(&mut svg, &entity.id.0, &entity.primitive)?;
    }
    svg.push_str("  </g>\n</svg>\n");
    Ok(svg)
}

fn write_svg_entity(
    output: &mut String,
    id: &str,
    primitive: &GeometryPrimitive,
) -> Result<(), GeometryError> {
    let id = escape_xml(id);
    match primitive {
        GeometryPrimitive::Point { position } => writeln!(
            output,
            "    <circle id=\"{}\" cx=\"{}\" cy=\"{}\" r=\"0.75\" fill=\"black\" />",
            id,
            number(position.x),
            number(-position.y)
        ),
        GeometryPrimitive::Line { start, end } => writeln!(
            output,
            "    <line id=\"{}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" />",
            id,
            number(start.x),
            number(-start.y),
            number(end.x),
            number(-end.y)
        ),
        GeometryPrimitive::Circle { center, radius } => writeln!(
            output,
            "    <circle id=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" />",
            id,
            number(center.x),
            number(-center.y),
            number(*radius)
        ),
        GeometryPrimitive::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let start = arc_point(*center, *radius, *start_angle);
            let end = arc_point(*center, *radius, *end_angle);
            let delta = (*end_angle - *start_angle).rem_euclid(std::f64::consts::TAU);
            writeln!(
                output,
                "    <path id=\"{}\" d=\"M {} {} A {} {} 0 {} 0 {} {}\" />",
                id,
                number(start.x),
                number(-start.y),
                number(*radius),
                number(*radius),
                u8::from(delta > std::f64::consts::PI),
                number(end.x),
                number(-end.y)
            )
        }
        GeometryPrimitive::Rectangle {
            origin,
            width,
            height,
        } => writeln!(
            output,
            "    <rect id=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" />",
            id,
            number(origin.x),
            number(-(origin.y + height)),
            number(*width),
            number(*height)
        ),
        GeometryPrimitive::Polygon { vertices } => {
            let points = vertices
                .iter()
                .map(|point| format!("{},{}", number(point.x), number(-point.y)))
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(
                output,
                "    <polygon id=\"{}\" points=\"{}\" />",
                id, points
            )
        }
    }
    .map_err(|error| GeometryError::InvalidGraph {
        message: format!("failed to format SVG output: {error}"),
    })
}

fn primitive_bounds(primitive: &GeometryPrimitive) -> Result<Vec<Point2>, GeometryError> {
    let points = match primitive {
        GeometryPrimitive::Point { position } => vec![*position],
        GeometryPrimitive::Line { start, end } => vec![*start, *end],
        GeometryPrimitive::Circle { center, radius }
        | GeometryPrimitive::Arc { center, radius, .. } => vec![
            Point2 {
                x: center.x - radius,
                y: center.y - radius,
            },
            Point2 {
                x: center.x + radius,
                y: center.y + radius,
            },
        ],
        GeometryPrimitive::Rectangle {
            origin,
            width,
            height,
        } => vec![
            *origin,
            Point2 {
                x: origin.x + width,
                y: origin.y + height,
            },
        ],
        GeometryPrimitive::Polygon { vertices } => vertices.clone(),
    };
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err(GeometryError::InvalidGraph {
            message: "geometry model contains non-finite coordinates".into(),
        });
    }
    Ok(points)
}

fn arc_point(center: Point2, radius: f64, angle: f64) -> Point2 {
    Point2 {
        x: center.x + radius * angle.cos(),
        y: center.y + radius * angle.sin(),
    }
}

pub fn number(value: f64) -> String {
    let mut rendered = format!("{value:.12}");
    while rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.pop();
    }
    if rendered == "-0" {
        "0".into()
    } else {
        rendered
    }
}

pub fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
