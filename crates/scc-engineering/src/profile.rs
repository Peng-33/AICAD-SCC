//! Exact engineering profile primitives and validation.

use std::f64::consts::TAU;

use serde::de::Error as DeError;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::error::EngineeringError;
use crate::Point2D;

/// Shared tolerance for source profile topology and analytic curve checks.
pub const PROFILE_TOLERANCE: f64 = 1.0e-9;

/// Direction of a profile arc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcDirection {
    Clockwise,
    CounterClockwise,
}

/// One exact segment in an ordered composite outer loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProfileSegment {
    Line {
        start: Point2D,
        end: Point2D,
    },
    Arc {
        start: Point2D,
        end: Point2D,
        center: Point2D,
        radius: f64,
        direction: ArcDirection,
    },
}

/// The typed planar material profile.
///
/// Rectangle and Circle retain their Phase 10/12.1 wire forms. New variants
/// carry an explicit `profile_kind` discriminator in persisted JSON.
#[derive(Debug, Clone, PartialEq)]
pub enum BaseProfile {
    Rectangle {
        width: f64,
        height: f64,
    },
    Circle {
        center: Point2D,
        diameter: f64,
    },
    RoundedRectangle {
        center: Point2D,
        width: f64,
        height: f64,
        corner_radius: f64,
    },
    Composite {
        segments: Vec<ProfileSegment>,
    },
    /// Ordered polygon vertices. Closure is implicit; the first vertex is not
    /// repeated. Stored form is canonical CCW starting at the lexicographic
    /// minimum vertex.
    Polygon {
        vertices: Vec<Point2D>,
    },
}

impl Serialize for BaseProfile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Rectangle { width, height } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("width", width)?;
                map.serialize_entry("height", height)?;
                map.end()
            }
            Self::Circle { center, diameter } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("center", center)?;
                map.serialize_entry("diameter", diameter)?;
                map.end()
            }
            Self::RoundedRectangle {
                center,
                width,
                height,
                corner_radius,
            } => {
                let mut map = serializer.serialize_map(Some(5))?;
                map.serialize_entry("profile_kind", "rounded_rectangle")?;
                map.serialize_entry("center", center)?;
                map.serialize_entry("width", width)?;
                map.serialize_entry("height", height)?;
                map.serialize_entry("corner_radius", corner_radius)?;
                map.end()
            }
            Self::Composite { segments } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("profile_kind", "composite")?;
                map.serialize_entry("segments", segments)?;
                map.end()
            }
            Self::Polygon { vertices } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("profile_kind", "polygon")?;
                map.serialize_entry("vertices", vertices)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for BaseProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("base profile must be a JSON object"))?;

        match object.get("profile_kind").and_then(Value::as_str) {
            Some("rounded_rectangle") => {
                let center = field::<Point2D, D::Error>(object, "center")?;
                let width = field::<f64, D::Error>(object, "width")?;
                let height = field::<f64, D::Error>(object, "height")?;
                let corner_radius = field::<f64, D::Error>(object, "corner_radius")?;
                Ok(Self::RoundedRectangle {
                    center,
                    width,
                    height,
                    corner_radius,
                })
            }
            Some("composite") => Ok(Self::Composite {
                segments: field(object, "segments")?,
            }),
            Some("polygon") => {
                let vertices: Vec<Point2D> = field(object, "vertices")?;
                let vertices = canonicalize_polygon(&vertices).map_err(D::Error::custom)?;
                Ok(Self::Polygon { vertices })
            }
            Some(kind) => Err(D::Error::custom(format!(
                "unsupported base profile_kind `{kind}`"
            ))),
            None if object.contains_key("width") && object.contains_key("height") => {
                Ok(Self::Rectangle {
                    width: field(object, "width")?,
                    height: field(object, "height")?,
                })
            }
            None if object.contains_key("center") && object.contains_key("diameter") => {
                Ok(Self::Circle {
                    center: field(object, "center")?,
                    diameter: field(object, "diameter")?,
                })
            }
            None => Err(D::Error::custom(
                "base profile requires legacy rectangle/circle fields or profile_kind",
            )),
        }
    }
}

fn field<T, E>(object: &serde_json::Map<String, Value>, name: &str) -> Result<T, E>
where
    T: for<'de> Deserialize<'de>,
    E: DeError,
{
    let value = object
        .get(name)
        .ok_or_else(|| E::custom(format!("base profile is missing `{name}`")))?;
    T::deserialize(value.clone()).map_err(|error| E::custom(format!("invalid `{name}`: {error}")))
}

impl BaseProfile {
    /// Validate topology and all analytic parameters.
    pub fn validate(&self) -> Result<(), EngineeringError> {
        match self {
            Self::Rectangle { width, height } => {
                finite_positive(*width, "rectangle width")?;
                finite_positive(*height, "rectangle height")?;
            }
            Self::Circle { center, diameter } => {
                finite_point(*center, "circle center")?;
                finite_positive(*diameter, "circle diameter")?;
            }
            Self::RoundedRectangle {
                center,
                width,
                height,
                corner_radius,
            } => {
                finite_point(*center, "rounded rectangle center")?;
                finite_positive(*width, "rounded rectangle width")?;
                finite_positive(*height, "rounded rectangle height")?;
                finite_nonnegative(*corner_radius, "rounded rectangle corner radius")?;
                if *corner_radius > width.min(*height) / 2.0 + PROFILE_TOLERANCE {
                    return invalid(
                        "rounded rectangle corner radius must not exceed half the minimum extent",
                    );
                }
            }
            Self::Composite { segments } => validate_composite(segments)?,
            Self::Polygon { vertices } => {
                canonicalize_polygon(vertices)?;
            }
        }
        Ok(())
    }

    /// Return the exact ordered segments used by the geometry lowering.
    pub fn segments(&self) -> Result<Vec<ProfileSegment>, EngineeringError> {
        self.validate()?;
        match self {
            Self::Rectangle { width, height } => {
                let (x, y) = (-width / 2.0, -height / 2.0);
                Ok(vec![
                    line((x, y), (x + width, y)),
                    line((x + width, y), (x + width, y + height)),
                    line((x + width, y + height), (x, y + height)),
                    line((x, y + height), (x, y)),
                ])
            }
            Self::Circle { .. } => Ok(Vec::new()),
            Self::RoundedRectangle {
                center,
                width,
                height,
                corner_radius,
            } => Ok(rounded_rectangle_segments(
                *center,
                *width,
                *height,
                *corner_radius,
            )),
            Self::Composite { segments } => Ok(segments.clone()),
            Self::Polygon { vertices } => Ok(polygon_segments(vertices)),
        }
    }
}

/// Validate a polygon and return its canonical CCW / lex-start vertex list.
pub fn canonicalize_polygon(vertices: &[Point2D]) -> Result<Vec<Point2D>, EngineeringError> {
    if vertices.len() < 3 {
        return Err(EngineeringError::InvalidProfile {
            reason: "polygon must contain at least three vertices".into(),
        });
    }
    for (index, vertex) in vertices.iter().enumerate() {
        finite_point(*vertex, "polygon vertex")?;
        for (other_index, other) in vertices.iter().enumerate() {
            if other_index != index && close(*vertex, *other) {
                return Err(EngineeringError::InvalidProfile {
                    reason: format!("polygon vertex {index} duplicates vertex {other_index}"),
                });
            }
        }
    }
    let segments = polygon_segments(vertices);
    validate_composite(&segments)?;
    let mut canonical = vertices.to_vec();
    if signed_area(&segments) < 0.0 {
        canonical.reverse();
    }
    let start = canonical
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.x.total_cmp(&right.x).then(left.y.total_cmp(&right.y)))
        .map(|(index, _)| index)
        .expect("polygon has at least three vertices");
    canonical.rotate_left(start);
    Ok(canonical)
}

fn polygon_segments(vertices: &[Point2D]) -> Vec<ProfileSegment> {
    vertices
        .iter()
        .enumerate()
        .map(|(index, start)| ProfileSegment::Line {
            start: *start,
            end: vertices[(index + 1) % vertices.len()],
        })
        .collect()
}

fn rounded_rectangle_segments(
    center: Point2D,
    width: f64,
    height: f64,
    radius: f64,
) -> Vec<ProfileSegment> {
    let left = center.x - width / 2.0;
    let right = center.x + width / 2.0;
    let bottom = center.y - height / 2.0;
    let top = center.y + height / 2.0;
    if radius <= PROFILE_TOLERANCE {
        return vec![
            line((left, bottom), (right, bottom)),
            line((right, bottom), (right, top)),
            line((right, top), (left, top)),
            line((left, top), (left, bottom)),
        ];
    }
    let p = |x: f64, y: f64| Point2D::new(x, y);
    vec![
        line((left + radius, bottom), (right - radius, bottom)),
        arc(
            p(right - radius, bottom),
            p(right, bottom + radius),
            p(right - radius, bottom + radius),
            radius,
            ArcDirection::CounterClockwise,
        ),
        line((right, bottom + radius), (right, top - radius)),
        arc(
            p(right, top - radius),
            p(right - radius, top),
            p(right - radius, top - radius),
            radius,
            ArcDirection::CounterClockwise,
        ),
        line((right - radius, top), (left + radius, top)),
        arc(
            p(left + radius, top),
            p(left, top - radius),
            p(left + radius, top - radius),
            radius,
            ArcDirection::CounterClockwise,
        ),
        line((left, top - radius), (left, bottom + radius)),
        arc(
            p(left, bottom + radius),
            p(left + radius, bottom),
            p(left + radius, bottom + radius),
            radius,
            ArcDirection::CounterClockwise,
        ),
    ]
}

fn line(start: (f64, f64), end: (f64, f64)) -> ProfileSegment {
    ProfileSegment::Line {
        start: Point2D::new(start.0, start.1),
        end: Point2D::new(end.0, end.1),
    }
}

fn arc(
    start: Point2D,
    end: Point2D,
    center: Point2D,
    radius: f64,
    direction: ArcDirection,
) -> ProfileSegment {
    ProfileSegment::Arc {
        start,
        end,
        center,
        radius,
        direction,
    }
}

fn validate_composite(segments: &[ProfileSegment]) -> Result<(), EngineeringError> {
    if segments.is_empty() {
        return invalid("composite profile must contain at least one segment");
    }
    for (index, segment) in segments.iter().enumerate() {
        validate_segment(segment, index)?;
        let next = &segments[(index + 1) % segments.len()];
        if !close(segment_end(segment), segment_start(next)) {
            return invalid(format!(
                "composite segment {index} is disconnected from its successor"
            ));
        }
    }

    for left in 0..segments.len() {
        for right in (left + 1)..segments.len() {
            let adjacent = right == left + 1 || (left == 0 && right + 1 == segments.len());
            if curves_intersect(&segments[left], &segments[right])
                && !(adjacent && only_shared_endpoint(&segments[left], &segments[right]))
            {
                return invalid(format!(
                    "composite profile self-intersects between segments {left} and {right}"
                ));
            }
        }
    }

    if signed_area(segments).abs() <= PROFILE_TOLERANCE {
        return invalid("composite profile must enclose nonzero area");
    }
    Ok(())
}

fn validate_segment(segment: &ProfileSegment, index: usize) -> Result<(), EngineeringError> {
    let start = segment_start(segment);
    let end = segment_end(segment);
    finite_point(start, "profile segment point")?;
    finite_point(end, "profile segment point")?;
    if close(start, end) {
        return invalid(format!("profile segment {index} is degenerate"));
    }
    if let ProfileSegment::Arc { center, radius, .. } = segment {
        finite_point(*center, "profile arc center")?;
        finite_positive(*radius, "profile arc radius")?;
        let start_radius = distance(*center, start);
        let end_radius = distance(*center, end);
        if !scale_close(start_radius, *radius) || !scale_close(end_radius, *radius) {
            return invalid(format!(
                "profile arc {index} endpoint radius does not match radius"
            ));
        }
        let sweep = directed_delta(
            angle(*center, start),
            angle(*center, end),
            segment_direction(segment),
        );
        if sweep <= PROFILE_TOLERANCE || (TAU - sweep).abs() <= PROFILE_TOLERANCE {
            return invalid(format!("profile arc {index} has zero or ambiguous sweep"));
        }
        if (sweep - std::f64::consts::PI).abs() <= PROFILE_TOLERANCE {
            return invalid(format!(
                "profile arc {index} has an ambiguous semicircle sweep"
            ));
        }
    }
    Ok(())
}

fn curves_intersect(left: &ProfileSegment, right: &ProfileSegment) -> bool {
    match (left, right) {
        (ProfileSegment::Line { .. }, ProfileSegment::Line { .. }) => {
            line_line_intersects(left, right)
        }
        (ProfileSegment::Line { .. }, ProfileSegment::Arc { .. })
        | (ProfileSegment::Arc { .. }, ProfileSegment::Line { .. }) => {
            line_arc_intersects(left, right)
        }
        (ProfileSegment::Arc { .. }, ProfileSegment::Arc { .. }) => arc_arc_intersects(left, right),
    }
}

fn line_line_intersects(left: &ProfileSegment, right: &ProfileSegment) -> bool {
    let a = segment_start(left);
    let b = segment_end(left);
    let c = segment_start(right);
    let d = segment_end(right);
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);
    if sign(ab_c) * sign(ab_d) < 0.0 && sign(cd_a) * sign(cd_b) < 0.0 {
        return true;
    }
    (ab_c.abs() <= PROFILE_TOLERANCE && on_segment(a, b, c))
        || (ab_d.abs() <= PROFILE_TOLERANCE && on_segment(a, b, d))
        || (cd_a.abs() <= PROFILE_TOLERANCE && on_segment(c, d, a))
        || (cd_b.abs() <= PROFILE_TOLERANCE && on_segment(c, d, b))
}

fn line_arc_intersects(left: &ProfileSegment, right: &ProfileSegment) -> bool {
    let (line, arc) = match (left, right) {
        (ProfileSegment::Line { .. }, ProfileSegment::Arc { .. }) => (left, right),
        _ => (right, left),
    };
    let a = segment_start(line);
    let b = segment_end(line);
    let ProfileSegment::Arc { center, radius, .. } = arc else {
        return false;
    };
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let fx = a.x - center.x;
    let fy = a.y - center.y;
    let aa = dx * dx + dy * dy;
    let bb = 2.0 * (fx * dx + fy * dy);
    let cc = fx * fx + fy * fy - radius * radius;
    let discriminant = bb * bb - 4.0 * aa * cc;
    if discriminant < -PROFILE_TOLERANCE {
        return false;
    }
    let root = discriminant.max(0.0).sqrt();
    [(-bb - root) / (2.0 * aa), (-bb + root) / (2.0 * aa)]
        .into_iter()
        .any(|t| {
            if !(-PROFILE_TOLERANCE..=1.0 + PROFILE_TOLERANCE).contains(&t) {
                return false;
            }
            let point = Point2D::new(a.x + t * dx, a.y + t * dy);
            point_on_arc(arc, point)
        })
}

fn arc_arc_intersects(left: &ProfileSegment, right: &ProfileSegment) -> bool {
    let (c1, r1) = arc_center_radius(left).unwrap();
    let (c2, r2) = arc_center_radius(right).unwrap();
    let dx = c2.x - c1.x;
    let dy = c2.y - c1.y;
    let d = (dx * dx + dy * dy).sqrt();
    if d <= PROFILE_TOLERANCE && scale_close(r1, r2) {
        return true;
    }
    if d > r1 + r2 + PROFILE_TOLERANCE || d < (r1 - r2).abs() - PROFILE_TOLERANCE {
        return false;
    }
    if d <= PROFILE_TOLERANCE {
        return false;
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h2 = r1 * r1 - a * a;
    if h2 < -PROFILE_TOLERANCE {
        return false;
    }
    let h = h2.max(0.0).sqrt();
    let ux = dx / d;
    let uy = dy / d;
    let base = Point2D::new(c1.x + a * ux, c1.y + a * uy);
    [
        Point2D::new(base.x - h * uy, base.y + h * ux),
        Point2D::new(base.x + h * uy, base.y - h * ux),
    ]
    .into_iter()
    .any(|point| point_on_arc(left, point) && point_on_arc(right, point))
}

fn only_shared_endpoint(left: &ProfileSegment, right: &ProfileSegment) -> bool {
    let shared = [
        (segment_start(left), segment_start(right)),
        (segment_start(left), segment_end(right)),
        (segment_end(left), segment_start(right)),
        (segment_end(left), segment_end(right)),
    ]
    .into_iter()
    .any(|(a, b)| close(a, b));
    if !shared {
        return false;
    }
    match (left, right) {
        (ProfileSegment::Line { .. }, ProfileSegment::Line { .. }) => {
            !line_line_intersects_interior(left, right)
        }
        _ => true,
    }
}

fn line_line_intersects_interior(left: &ProfileSegment, right: &ProfileSegment) -> bool {
    let a = segment_start(left);
    let b = segment_end(left);
    let c = segment_start(right);
    let d = segment_end(right);
    let shared = [a, b]
        .into_iter()
        .find(|point| close(*point, c) || close(*point, d));
    let Some(shared) = shared else { return true };
    let samples = [
        Point2D::new((a.x + shared.x) / 2.0, (a.y + shared.y) / 2.0),
        Point2D::new((b.x + shared.x) / 2.0, (b.y + shared.y) / 2.0),
    ];
    samples
        .into_iter()
        .any(|point| on_segment(c, d, point) && !close(point, shared))
}

fn point_on_arc(arc: &ProfileSegment, point: Point2D) -> bool {
    let ProfileSegment::Arc {
        center,
        radius,
        start,
        end,
        direction,
    } = arc
    else {
        return false;
    };
    if !scale_close(distance(*center, point), *radius) {
        return false;
    }
    let total = directed_delta(angle(*center, *start), angle(*center, *end), *direction);
    let progress = directed_delta(angle(*center, *start), angle(*center, point), *direction);
    progress <= total + PROFILE_TOLERANCE
}

fn signed_area(segments: &[ProfileSegment]) -> f64 {
    segments
        .iter()
        .map(|segment| match segment {
            ProfileSegment::Line { start, end } => start.x * end.y - end.x * start.y,
            ProfileSegment::Arc {
                start,
                center,
                radius,
                direction,
                ..
            } => {
                let start_angle = angle(*center, *start);
                let delta = match direction {
                    ArcDirection::CounterClockwise => directed_delta(
                        start_angle,
                        angle(*center, segment_end(segment)),
                        *direction,
                    ),
                    ArcDirection::Clockwise => -directed_delta(
                        start_angle,
                        angle(*center, segment_end(segment)),
                        *direction,
                    ),
                };
                center.x * radius * (start_angle + delta).sin()
                    - center.x * radius * start_angle.sin()
                    + center.y * radius * (-(start_angle + delta).cos() + start_angle.cos())
                    + radius * radius * delta
            }
        })
        .sum::<f64>()
        / 2.0
}

fn arc_center_radius(segment: &ProfileSegment) -> Option<(Point2D, f64)> {
    match segment {
        ProfileSegment::Arc { center, radius, .. } => Some((*center, *radius)),
        _ => None,
    }
}

fn segment_start(segment: &ProfileSegment) -> Point2D {
    match segment {
        ProfileSegment::Line { start, .. } | ProfileSegment::Arc { start, .. } => *start,
    }
}

fn segment_end(segment: &ProfileSegment) -> Point2D {
    match segment {
        ProfileSegment::Line { end, .. } | ProfileSegment::Arc { end, .. } => *end,
    }
}

fn segment_direction(segment: &ProfileSegment) -> ArcDirection {
    match segment {
        ProfileSegment::Arc { direction, .. } => *direction,
        ProfileSegment::Line { .. } => ArcDirection::CounterClockwise,
    }
}

fn finite_point(point: Point2D, what: &'static str) -> Result<(), EngineeringError> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(EngineeringError::NonFiniteValue { what });
    }
    Ok(())
}

fn finite_positive(value: f64, what: &'static str) -> Result<(), EngineeringError> {
    if !value.is_finite() {
        return Err(EngineeringError::NonFiniteValue { what });
    }
    if value <= 0.0 {
        return Err(EngineeringError::NonPositiveLength { what, value });
    }
    Ok(())
}

fn finite_nonnegative(value: f64, what: &'static str) -> Result<(), EngineeringError> {
    if !value.is_finite() {
        return Err(EngineeringError::NonFiniteValue { what });
    }
    if value < 0.0 {
        return invalid(format!("{what} must be nonnegative"));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> Result<(), EngineeringError> {
    Err(EngineeringError::InvalidProfile {
        reason: message.into(),
    })
}

fn close(left: Point2D, right: Point2D) -> bool {
    scale_close(left.x, right.x) && scale_close(left.y, right.y)
}

fn scale_close(left: f64, right: f64) -> bool {
    (left - right).abs() <= PROFILE_TOLERANCE * 1.0_f64.max(left.abs()).max(right.abs())
}

fn distance(left: Point2D, right: Point2D) -> f64 {
    ((left.x - right.x).powi(2) + (left.y - right.y).powi(2)).sqrt()
}

fn cross(a: Point2D, b: Point2D, c: Point2D) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn sign(value: f64) -> f64 {
    if value > PROFILE_TOLERANCE {
        1.0
    } else if value < -PROFILE_TOLERANCE {
        -1.0
    } else {
        0.0
    }
}

fn on_segment(a: Point2D, b: Point2D, point: Point2D) -> bool {
    cross(a, b, point).abs() <= PROFILE_TOLERANCE
        && point.x >= a.x.min(b.x) - PROFILE_TOLERANCE
        && point.x <= a.x.max(b.x) + PROFILE_TOLERANCE
        && point.y >= a.y.min(b.y) - PROFILE_TOLERANCE
        && point.y <= a.y.max(b.y) + PROFILE_TOLERANCE
}

fn angle(center: Point2D, point: Point2D) -> f64 {
    (point.y - center.y).atan2(point.x - center.x)
}

fn directed_delta(start: f64, end: f64, direction: ArcDirection) -> f64 {
    let raw = match direction {
        ArcDirection::CounterClockwise => end - start,
        ArcDirection::Clockwise => start - end,
    };
    raw.rem_euclid(TAU)
}

/// Convert a typed arc to the existing CCW-only GeometryPrimitive angle pair.
pub fn geometry_angles(segment: &ProfileSegment) -> Option<(f64, f64)> {
    let ProfileSegment::Arc {
        start,
        end,
        center,
        direction,
        ..
    } = segment
    else {
        return None;
    };
    let start_angle = angle(*center, *start);
    let sweep = directed_delta(start_angle, angle(*center, *end), *direction);
    match direction {
        ArcDirection::CounterClockwise => Some((start_angle, start_angle + sweep)),
        ArcDirection::Clockwise => {
            let end_angle = angle(*center, *end);
            Some((end_angle, end_angle + sweep))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_zero_radius_is_four_lines() {
        let profile = BaseProfile::RoundedRectangle {
            center: Point2D::new(0.0, 0.0),
            width: 20.0,
            height: 10.0,
            corner_radius: 0.0,
        };
        assert_eq!(profile.segments().unwrap().len(), 4);
        assert!(profile
            .segments()
            .unwrap()
            .iter()
            .all(|s| matches!(s, ProfileSegment::Line { .. })));
    }

    #[test]
    fn rounded_profile_roundtrips_with_discriminator() {
        let profile = BaseProfile::RoundedRectangle {
            center: Point2D::new(1.0, 2.0),
            width: 20.0,
            height: 10.0,
            corner_radius: 2.0,
        };
        let json = serde_json::to_value(&profile).unwrap();
        assert_eq!(json["profile_kind"], "rounded_rectangle");
        assert_eq!(
            serde_json::from_value::<BaseProfile>(json).unwrap(),
            profile
        );
    }

    #[test]
    fn composite_accepts_closed_connected_line_arc_loop() {
        let profile = BaseProfile::Composite {
            segments: vec![
                line((0.0, 0.0), (20.0, 0.0)),
                arc(
                    Point2D::new(20.0, 0.0),
                    Point2D::new(0.0, 20.0),
                    Point2D::new(0.0, 0.0),
                    20.0,
                    ArcDirection::CounterClockwise,
                ),
                line((0.0, 20.0), (0.0, 0.0)),
            ],
        };
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn composite_rejects_disconnected_or_open_loop() {
        let disconnected = BaseProfile::Composite {
            segments: vec![
                line((0.0, 0.0), (2.0, 0.0)),
                line((3.0, 0.0), (3.0, 2.0)),
                line((3.0, 2.0), (0.0, 2.0)),
                line((0.0, 2.0), (0.0, 0.0)),
            ],
        };
        assert!(disconnected.validate().is_err());
        let open = BaseProfile::Composite {
            segments: vec![
                line((0.0, 0.0), (2.0, 0.0)),
                line((2.0, 0.0), (2.0, 2.0)),
                line((2.0, 2.0), (0.0, 2.0)),
            ],
        };
        assert!(open.validate().is_err());
        let degenerate = BaseProfile::Composite {
            segments: vec![
                line((0.0, 0.0), (0.0, 0.0)),
                line((0.0, 0.0), (2.0, 0.0)),
                line((2.0, 0.0), (0.0, 0.0)),
            ],
        };
        assert!(degenerate.validate().is_err());
    }

    #[test]
    fn composite_rejects_self_intersection() {
        let line_line = BaseProfile::Composite {
            segments: vec![
                line((0.0, 0.0), (2.0, 2.0)),
                line((2.0, 2.0), (0.0, 2.0)),
                line((0.0, 2.0), (2.0, 0.0)),
                line((2.0, 0.0), (0.0, 0.0)),
            ],
        };
        assert!(line_line.validate().is_err());

        let line_arc = BaseProfile::Composite {
            segments: vec![
                line((0.0, 0.0), (3.0, 0.0)),
                line((3.0, 0.0), (3.0, 3.0)),
                line((3.0, 3.0), (0.0, 3.0)),
                arc(
                    Point2D::new(0.0, 3.0),
                    Point2D::new(0.0, 0.0),
                    Point2D::new(1.5, 1.5),
                    (1.5_f64).hypot(1.5),
                    ArcDirection::Clockwise,
                ),
            ],
        };
        assert!(line_arc.validate().is_err());

        let opposite_arcs = BaseProfile::Composite {
            segments: vec![
                line((-20.0, -10.0), (20.0, -10.0)),
                arc(
                    Point2D::new(20.0, -10.0),
                    Point2D::new(30.0, 0.0),
                    Point2D::new(20.0, 0.0),
                    10.0,
                    ArcDirection::CounterClockwise,
                ),
                arc(
                    Point2D::new(30.0, 0.0),
                    Point2D::new(20.0, 10.0),
                    Point2D::new(20.0, 0.0),
                    10.0,
                    ArcDirection::CounterClockwise,
                ),
                line((20.0, 10.0), (-20.0, 10.0)),
                arc(
                    Point2D::new(-20.0, 10.0),
                    Point2D::new(-30.0, 0.0),
                    Point2D::new(-20.0, 0.0),
                    10.0,
                    ArcDirection::CounterClockwise,
                ),
                arc(
                    Point2D::new(-30.0, 0.0),
                    Point2D::new(-20.0, -10.0),
                    Point2D::new(-20.0, 0.0),
                    10.0,
                    ArcDirection::CounterClockwise,
                ),
            ],
        };
        assert!(opposite_arcs.validate().is_ok());

        let crossing_arcs = BaseProfile::Composite {
            segments: vec![
                arc(
                    Point2D::new(0.0, 2.0),
                    Point2D::new(2.0, 0.0),
                    Point2D::new(0.0, 0.0),
                    2.0,
                    ArcDirection::Clockwise,
                ),
                line((2.0, 0.0), (4.0, 0.0)),
                line((4.0, 0.0), (4.0, 2.0)),
                line((4.0, 2.0), (2.0, 2.0)),
                arc(
                    Point2D::new(2.0, 2.0),
                    Point2D::new(0.0, 0.0),
                    Point2D::new(2.0, 0.0),
                    2.0,
                    ArcDirection::CounterClockwise,
                ),
                line((0.0, 0.0), (0.0, 2.0)),
            ],
        };
        assert!(crossing_arcs.validate().is_err());
    }

    #[test]
    fn composite_arc_fails_closed_on_invalid_contracts() {
        let start = Point2D::new(1.0, 0.0);
        let end = Point2D::new(0.0, 1.0);
        let center = Point2D::new(0.0, 0.0);
        let cases = [
            ProfileSegment::Arc {
                start,
                end,
                center,
                radius: 0.0,
                direction: ArcDirection::CounterClockwise,
            },
            ProfileSegment::Arc {
                start,
                end: start,
                center,
                radius: 1.0,
                direction: ArcDirection::CounterClockwise,
            },
            ProfileSegment::Arc {
                start,
                end,
                center: Point2D::new(f64::NAN, 0.0),
                radius: 1.0,
                direction: ArcDirection::CounterClockwise,
            },
            ProfileSegment::Arc {
                start,
                end,
                center,
                radius: 2.0,
                direction: ArcDirection::CounterClockwise,
            },
            ProfileSegment::Arc {
                start,
                end: start,
                center,
                radius: 1.0,
                direction: ArcDirection::Clockwise,
            },
            ProfileSegment::Arc {
                start: Point2D::new(1.0, 0.0),
                end: Point2D::new(-1.0, 0.0),
                center,
                radius: 1.0,
                direction: ArcDirection::CounterClockwise,
            },
            ProfileSegment::Arc {
                start,
                end,
                center,
                radius: f64::INFINITY,
                direction: ArcDirection::CounterClockwise,
            },
        ];
        for segment in cases {
            let profile = BaseProfile::Composite {
                segments: vec![segment],
            };
            assert!(profile.validate().is_err());
        }
    }

    #[test]
    fn polygon_canonicalizes_winding_and_start_vertex() {
        let cw = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(50.0, 80.0),
            Point2D::new(100.0, 0.0),
        ];
        let rotated = vec![
            Point2D::new(100.0, 0.0),
            Point2D::new(50.0, 80.0),
            Point2D::new(0.0, 0.0),
        ];
        let canonical = canonicalize_polygon(&[
            Point2D::new(0.0, 0.0),
            Point2D::new(100.0, 0.0),
            Point2D::new(50.0, 80.0),
        ])
        .unwrap();
        assert_eq!(canonicalize_polygon(&cw).unwrap(), canonical);
        assert_eq!(canonicalize_polygon(&rotated).unwrap(), canonical);
        assert_eq!(canonical[0], Point2D::new(0.0, 0.0));
        let area = signed_area(&polygon_segments(&canonical));
        assert!(area > 0.0);
    }

    #[test]
    fn polygon_rejects_degenerate_and_bow_tie_inputs() {
        assert!(canonicalize_polygon(&[]).is_err());
        assert!(canonicalize_polygon(&[Point2D::new(0.0, 0.0)]).is_err());
        assert!(canonicalize_polygon(&[Point2D::new(0.0, 0.0), Point2D::new(1.0, 0.0)]).is_err());
        assert!(canonicalize_polygon(&[
            Point2D::new(0.0, 0.0),
            Point2D::new(1.0, 0.0),
            Point2D::new(0.0, 0.0),
        ])
        .is_err());
        assert!(canonicalize_polygon(&[
            Point2D::new(0.0, 0.0),
            Point2D::new(1.0, 0.0),
            Point2D::new(2.0, 0.0),
        ])
        .is_err());
        assert!(canonicalize_polygon(&[
            Point2D::new(0.0, 0.0),
            Point2D::new(2.0, 0.0),
            Point2D::new(1.0, 1.0),
            Point2D::new(1.0, -1.0),
        ])
        .is_err());
        assert!(canonicalize_polygon(&[
            Point2D::new(0.0, 0.0),
            Point2D::new(1.0, 0.0),
            Point2D::new(f64::NAN, 1.0),
        ])
        .is_err());
        assert!(canonicalize_polygon(&[
            Point2D::new(0.0, 0.0),
            Point2D::new(1.0, 0.0),
            Point2D::new(0.0, f64::INFINITY),
        ])
        .is_err());
    }
}
