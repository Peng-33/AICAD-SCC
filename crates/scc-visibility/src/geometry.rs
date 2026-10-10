//! Direction-independent, non-mutating geometry predicates for visibility.

use scc_projection::{ProjectedGeometry, ProjectedPoint2};
use scc_spatial::{SpatialArcDirection, SpatialProfileSegment};

use std::f64::consts::{PI, TAU};

/// One shared absolute tolerance for all Phase 9.5 comparisons, in mm.
/// Projection and spatial layers remain semantically untouched; this value is
/// local to the visibility support-domain and comparison contract.
pub const VISIBILITY_TOLERANCE_MM: f64 = 1.0e-9;

pub(crate) fn approximately_equal(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let scale = a.abs().max(b.abs()).max(1.0);
    if scale == 1.0 {
        return (a - b).abs() <= VISIBILITY_TOLERANCE_MM;
    }
    let normalized_difference = (a / scale - b / scale).abs();
    normalized_difference <= VISIBILITY_TOLERANCE_MM / scale
}

/// Compare finite millimetre values using the Phase 9.5 shared policy.
/// Large opposite-sign values are normalized before subtraction.
pub fn approximately_equal_mm(a: f64, b: f64) -> bool {
    approximately_equal(a, b)
}

pub(crate) fn finite_add(a: f64, b: f64) -> Option<f64> {
    let value = a + b;
    value.is_finite().then_some(value)
}

pub(crate) fn finite_sub(a: f64, b: f64) -> Option<f64> {
    let value = a - b;
    value.is_finite().then_some(value)
}

pub(crate) fn circle_strictly_inside(
    center: (f64, f64),
    radius: f64,
    rectangle: (f64, f64, f64, f64),
) -> Option<bool> {
    let left = finite_sub(center.0, radius)?;
    let right = finite_add(center.0, radius)?;
    let bottom = finite_sub(center.1, radius)?;
    let top = finite_add(center.1, radius)?;
    let left_margin = finite_sub(left, rectangle.0)?;
    let right_margin = finite_sub(rectangle.1, right)?;
    let bottom_margin = finite_sub(bottom, rectangle.2)?;
    let top_margin = finite_sub(rectangle.3, top)?;
    Some(
        left_margin > VISIBILITY_TOLERANCE_MM
            && right_margin > VISIBILITY_TOLERANCE_MM
            && bottom_margin > VISIBILITY_TOLERANCE_MM
            && top_margin > VISIBILITY_TOLERANCE_MM,
    )
}

pub(crate) fn circle_strictly_inside_circle(
    inner_center: (f64, f64),
    inner_radius: f64,
    outer_center: (f64, f64),
    outer_radius: f64,
) -> Option<bool> {
    let distance = center_distance(inner_center, outer_center)?;
    let sum = finite_add(distance, inner_radius)?;
    let margin = finite_sub(outer_radius, sum)?;
    Some(margin > VISIBILITY_TOLERANCE_MM)
}

pub(crate) fn center_distance(a: (f64, f64), b: (f64, f64)) -> Option<f64> {
    let dx = finite_sub(a.0, b.0)?;
    let dy = finite_sub(a.1, b.1)?;
    let distance = dx.hypot(dy);
    distance.is_finite().then_some(distance)
}

pub(crate) fn radii_strictly_separated(distance: f64, radius_sum: f64) -> Option<bool> {
    let margin = finite_sub(distance, radius_sum)?;
    Some(margin > VISIBILITY_TOLERANCE_MM)
}

fn normalized_angle(angle: f64) -> f64 {
    angle.rem_euclid(TAU)
}

fn ccw_angle_distance(start: f64, end: f64) -> f64 {
    (normalized_angle(end) - normalized_angle(start)).rem_euclid(TAU)
}

fn angle_equal(a: f64, b: f64) -> bool {
    let delta = (normalized_angle(a) - normalized_angle(b)).abs();
    delta <= VISIBILITY_TOLERANCE_MM || (TAU - delta) <= VISIBILITY_TOLERANCE_MM
}

fn projected_arc_components(
    geometry: &ProjectedGeometry,
) -> Option<(ProjectedPoint2, f64, f64, f64)> {
    let ProjectedGeometry::Arc {
        center,
        radius,
        start_angle,
        end_angle,
    } = geometry
    else {
        return None;
    };
    if !center.u.is_finite()
        || !center.v.is_finite()
        || !radius.is_finite()
        || *radius <= 0.0
        || !start_angle.is_finite()
        || !end_angle.is_finite()
    {
        return None;
    }
    let sweep = ccw_angle_distance(*start_angle, *end_angle);
    (sweep > VISIBILITY_TOLERANCE_MM && sweep < TAU - VISIBILITY_TOLERANCE_MM).then_some((
        *center,
        *radius,
        *start_angle,
        *end_angle,
    ))
}

fn projected_arc_point(center: ProjectedPoint2, radius: f64, angle: f64) -> ProjectedPoint2 {
    ProjectedPoint2 {
        u: center.u + radius * angle.cos(),
        v: center.v + radius * angle.sin(),
    }
}

fn point_on_projected_arc(arc: &ProjectedGeometry, point: ProjectedPoint2) -> Option<bool> {
    let (center, radius, start, end) = projected_arc_components(arc)?;
    let distance = (point.u - center.u).hypot(point.v - center.v);
    if !distance.is_finite() || !approximately_equal(distance, radius) {
        return Some(false);
    }
    let angle = (point.v - center.v).atan2(point.u - center.u);
    let sweep = ccw_angle_distance(start, end);
    Some(ccw_angle_distance(start, angle) <= sweep + VISIBILITY_TOLERANCE_MM)
}

fn line_arc_intersects(line: &ProjectedGeometry, arc: &ProjectedGeometry) -> Option<bool> {
    let (ProjectedGeometry::Segment { start, end }, Some((center, radius, _, _))) =
        (line, projected_arc_components(arc))
    else {
        return Some(false);
    };
    let dx = finite_sub(end.u, start.u)?;
    let dy = finite_sub(end.v, start.v)?;
    let fx = finite_sub(start.u, center.u)?;
    let fy = finite_sub(start.v, center.v)?;
    let a = dx * dx + dy * dy;
    if !a.is_finite() || a <= 0.0 {
        return None;
    }
    let b = 2.0 * (fx * dx + fy * dy);
    let c = fx * fx + fy * fy - radius * radius;
    let mut discriminant = b * b - 4.0 * a * c;
    if !b.is_finite() || !c.is_finite() || !discriminant.is_finite() {
        return None;
    }
    let scale = b.abs().max((4.0 * a * c).abs()).max(1.0);
    let limit = VISIBILITY_TOLERANCE_MM * scale;
    if discriminant < -limit {
        return Some(false);
    }
    if discriminant < 0.0 {
        discriminant = 0.0;
    }
    let root = discriminant.sqrt();
    for t in [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)] {
        if !t.is_finite()
            || !(-VISIBILITY_TOLERANCE_MM..=1.0 + VISIBILITY_TOLERANCE_MM).contains(&t)
        {
            continue;
        }
        let point = ProjectedPoint2 {
            u: start.u + t * dx,
            v: start.v + t * dy,
        };
        if point_on_projected_arc(arc, point)? {
            return Some(true);
        }
    }
    Some(false)
}

fn arc_arc_intersects(a: &ProjectedGeometry, b: &ProjectedGeometry) -> Option<bool> {
    let (a_center, a_radius, a_start, a_end) = projected_arc_components(a)?;
    let (b_center, b_radius, b_start, b_end) = projected_arc_components(b)?;
    let dx = finite_sub(b_center.u, a_center.u)?;
    let dy = finite_sub(b_center.v, a_center.v)?;
    let distance = dx.hypot(dy);
    if !distance.is_finite() {
        return None;
    }
    if approximately_equal(distance, 0.0) && approximately_equal(a_radius, b_radius) {
        let a_start_point = projected_arc_point(a_center, a_radius, a_start);
        let a_end_point = projected_arc_point(a_center, a_radius, a_end);
        let b_start_point = projected_arc_point(b_center, b_radius, b_start);
        let b_end_point = projected_arc_point(b_center, b_radius, b_end);
        return Some(
            (angle_equal(a_start, b_start) && angle_equal(a_end, b_end))
                || point_on_projected_arc(b, a_start_point)?
                || point_on_projected_arc(b, a_end_point)?
                || point_on_projected_arc(a, b_start_point)?
                || point_on_projected_arc(a, b_end_point)?,
        );
    }
    if distance > a_radius + b_radius + VISIBILITY_TOLERANCE_MM
        || distance < (a_radius - b_radius).abs() - VISIBILITY_TOLERANCE_MM
        || approximately_equal(distance, 0.0)
    {
        return Some(false);
    }
    let a_projection =
        (a_radius * a_radius - b_radius * b_radius + distance * distance) / (2.0 * distance);
    let height_squared = a_radius * a_radius - a_projection * a_projection;
    if !a_projection.is_finite() || !height_squared.is_finite() {
        return None;
    }
    let height = height_squared.max(0.0).sqrt();
    let ux = dx / distance;
    let uy = dy / distance;
    let base = ProjectedPoint2 {
        u: a_center.u + a_projection * ux,
        v: a_center.v + a_projection * uy,
    };
    let intersections = [
        ProjectedPoint2 {
            u: base.u - height * uy,
            v: base.v + height * ux,
        },
        ProjectedPoint2 {
            u: base.u + height * uy,
            v: base.v - height * ux,
        },
    ];
    Some(intersections.iter().any(|point| {
        point_on_projected_arc(a, *point).unwrap_or(false)
            && point_on_projected_arc(b, *point).unwrap_or(false)
    }))
}

/// Compare two segments as unoriented line segments. The implementation never
/// mutates either geometry and uses no rounded/serialized key.
pub(crate) fn segments_intersect_or_touch(
    a: &ProjectedGeometry,
    b: &ProjectedGeometry,
) -> Option<bool> {
    if matches!(a, ProjectedGeometry::Segment { .. }) && matches!(b, ProjectedGeometry::Arc { .. })
    {
        return line_arc_intersects(a, b);
    }
    if matches!(a, ProjectedGeometry::Arc { .. }) && matches!(b, ProjectedGeometry::Segment { .. })
    {
        return line_arc_intersects(b, a);
    }
    if matches!(a, ProjectedGeometry::Arc { .. }) && matches!(b, ProjectedGeometry::Arc { .. }) {
        return arc_arc_intersects(a, b);
    }
    let (a_start, a_end, b_start, b_end) = match (a, b) {
        (
            ProjectedGeometry::Segment {
                start: a_start,
                end: a_end,
            },
            ProjectedGeometry::Segment {
                start: b_start,
                end: b_end,
            },
        ) => (a_start, a_end, b_start, b_end),
        _ => return Some(false),
    };
    let a_orientation_b_start = orientation(a_start, a_end, b_start)?;
    let a_orientation_b_end = orientation(a_start, a_end, b_end)?;
    let b_orientation_a_start = orientation(b_start, b_end, a_start)?;
    let b_orientation_a_end = orientation(b_start, b_end, a_end)?;

    if is_near_zero(a_orientation_b_start) && point_on_segment(b_start, a_start, a_end)?
        || is_near_zero(a_orientation_b_end) && point_on_segment(b_end, a_start, a_end)?
        || is_near_zero(b_orientation_a_start) && point_on_segment(a_start, b_start, b_end)?
        || is_near_zero(b_orientation_a_end) && point_on_segment(a_end, b_start, b_end)?
    {
        return Some(true);
    }

    Some(
        is_strictly_positive(a_orientation_b_start) != is_strictly_positive(a_orientation_b_end)
            && is_strictly_positive(b_orientation_a_start)
                != is_strictly_positive(b_orientation_a_end),
    )
}

#[derive(Clone, Copy)]
struct Orientation {
    value: f64,
    tolerance: f64,
}

fn orientation(
    a: &ProjectedPoint2,
    b: &ProjectedPoint2,
    c: &ProjectedPoint2,
) -> Option<Orientation> {
    let ab_u = finite_sub(b.u, a.u)?;
    let ab_v = finite_sub(b.v, a.v)?;
    let ac_u = finite_sub(c.u, a.u)?;
    let ac_v = finite_sub(c.v, a.v)?;
    let scale = ab_u
        .abs()
        .max(ab_v.abs())
        .max(ac_u.abs())
        .max(ac_v.abs())
        .max(1.0);
    let value = (ab_u / scale) * (ac_v / scale) - (ab_v / scale) * (ac_u / scale);
    let tolerance = VISIBILITY_TOLERANCE_MM / scale / scale;
    (value.is_finite() && tolerance.is_finite()).then_some(Orientation { value, tolerance })
}

fn is_near_zero(orientation: Orientation) -> bool {
    orientation.value.abs() <= orientation.tolerance
}

fn is_strictly_positive(orientation: Orientation) -> bool {
    orientation.value > orientation.tolerance
}

fn point_on_segment(
    point: &ProjectedPoint2,
    start: &ProjectedPoint2,
    end: &ProjectedPoint2,
) -> Option<bool> {
    Some(
        coordinate_between(point.u, start.u, end.u)?
            && coordinate_between(point.v, start.v, end.v)?,
    )
}

fn coordinate_between(value: f64, first: f64, second: f64) -> Option<bool> {
    let min = first.min(second);
    let max = first.max(second);
    if value < min {
        return Some(finite_sub(min, value)? <= VISIBILITY_TOLERANCE_MM);
    }
    if value > max {
        return Some(finite_sub(value, max)? <= VISIBILITY_TOLERANCE_MM);
    }
    Some(true)
}

pub(crate) fn segments_coincident(a: &ProjectedGeometry, b: &ProjectedGeometry) -> Option<bool> {
    if let (
        Some((a_center, a_radius, a_start, a_end)),
        Some((b_center, b_radius, b_start, b_end)),
    ) = (projected_arc_components(a), projected_arc_components(b))
    {
        return Some(
            points_equal(a_center.u, a_center.v, b_center.u, b_center.v)
                && approximately_equal(a_radius, b_radius)
                && angle_equal(a_start, b_start)
                && angle_equal(a_end, b_end),
        );
    }
    let (a_start, a_end, b_start, b_end) = match (a, b) {
        (
            ProjectedGeometry::Segment {
                start: a_start,
                end: a_end,
            },
            ProjectedGeometry::Segment {
                start: b_start,
                end: b_end,
            },
        ) => (a_start, a_end, b_start, b_end),
        _ => return Some(false),
    };
    Some(
        points_equal(a_start.u, a_start.v, b_start.u, b_start.v)
            && points_equal(a_end.u, a_end.v, b_end.u, b_end.v)
            || points_equal(a_start.u, a_start.v, b_end.u, b_end.v)
                && points_equal(a_end.u, a_end.v, b_start.u, b_start.v),
    )
}

fn spatial_point_xy(segment: &SpatialProfileSegment, start: bool) -> (f64, f64) {
    let point = if start {
        segment.start()
    } else {
        segment.end()
    };
    (point.x, point.y)
}

type SpatialArcData = ((f64, f64), f64, f64, f64, SpatialArcDirection);

fn spatial_arc_data(segment: &SpatialProfileSegment) -> Option<SpatialArcData> {
    let SpatialProfileSegment::Arc {
        center,
        radius,
        start,
        end,
        direction,
        ..
    } = segment
    else {
        return None;
    };
    if !center.x.is_finite()
        || !center.y.is_finite()
        || !radius.is_finite()
        || *radius <= 0.0
        || !start.x.is_finite()
        || !start.y.is_finite()
        || !end.x.is_finite()
        || !end.y.is_finite()
    {
        return None;
    }
    let start_angle = (start.y - center.y).atan2(start.x - center.x);
    let end_angle = (end.y - center.y).atan2(end.x - center.x);
    let sweep = match direction {
        SpatialArcDirection::CounterClockwise => ccw_angle_distance(start_angle, end_angle),
        SpatialArcDirection::Clockwise => ccw_angle_distance(end_angle, start_angle),
    };
    (sweep > VISIBILITY_TOLERANCE_MM && sweep < TAU - VISIBILITY_TOLERANCE_MM).then_some((
        (center.x, center.y),
        *radius,
        start_angle,
        end_angle,
        *direction,
    ))
}

fn spatial_arc_parameter(start_angle: f64, direction: SpatialArcDirection, angle: f64) -> f64 {
    match direction {
        SpatialArcDirection::CounterClockwise => ccw_angle_distance(start_angle, angle),
        SpatialArcDirection::Clockwise => ccw_angle_distance(angle, start_angle),
    }
}

fn spatial_angle_on_arc(
    start_angle: f64,
    end_angle: f64,
    direction: SpatialArcDirection,
    angle: f64,
) -> bool {
    let sweep = match direction {
        SpatialArcDirection::CounterClockwise => ccw_angle_distance(start_angle, end_angle),
        SpatialArcDirection::Clockwise => ccw_angle_distance(end_angle, start_angle),
    };
    spatial_arc_parameter(start_angle, direction, angle) <= sweep + VISIBILITY_TOLERANCE_MM
}

fn point_to_line_distance(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> Option<f64> {
    let dx = finite_sub(end.0, start.0)?;
    let dy = finite_sub(end.1, start.1)?;
    let length_squared = dx * dx + dy * dy;
    if !length_squared.is_finite() || length_squared <= 0.0 {
        return None;
    }
    let t = ((point.0 - start.0) * dx + (point.1 - start.1) * dy) / length_squared;
    let t = t.clamp(0.0, 1.0);
    let nearest = (start.0 + t * dx, start.1 + t * dy);
    center_distance(point, nearest)
}

fn point_to_profile_segment_distance(
    point: (f64, f64),
    segment: &SpatialProfileSegment,
) -> Option<f64> {
    match segment {
        SpatialProfileSegment::Line { .. } => point_to_line_distance(
            point,
            spatial_point_xy(segment, true),
            spatial_point_xy(segment, false),
        ),
        SpatialProfileSegment::Arc { .. } => {
            let (center, radius, start_angle, end_angle, direction) = spatial_arc_data(segment)?;
            let distance_to_center = center_distance(point, center)?;
            let radial_angle = (point.1 - center.1).atan2(point.0 - center.0);
            if spatial_angle_on_arc(start_angle, end_angle, direction, radial_angle) {
                Some((distance_to_center - radius).abs())
            } else {
                let start = spatial_point_xy(segment, true);
                let end = spatial_point_xy(segment, false);
                Some(center_distance(point, start)?.min(center_distance(point, end)?))
            }
        }
    }
}

fn composite_point_inside(point: (f64, f64), segments: &[SpatialProfileSegment]) -> Option<bool> {
    if segments.is_empty() {
        return Some(false);
    }
    let mut winding = 0i32;
    for segment in segments {
        match segment {
            SpatialProfileSegment::Line { .. } => {
                let (start_x, start_y) = spatial_point_xy(segment, true);
                let (end_x, end_y) = spatial_point_xy(segment, false);
                if ![start_x, start_y, end_x, end_y]
                    .iter()
                    .all(|value| value.is_finite())
                {
                    return None;
                }
                let upward = start_y <= point.1 && end_y > point.1;
                let downward = start_y > point.1 && end_y <= point.1;
                if upward || downward {
                    let x = start_x + (point.1 - start_y) * (end_x - start_x) / (end_y - start_y);
                    if x > point.0 + VISIBILITY_TOLERANCE_MM {
                        winding += if upward { 1 } else { -1 };
                    }
                }
            }
            SpatialProfileSegment::Arc { .. } => {
                let (center, radius, start_angle, end_angle, direction) =
                    spatial_arc_data(segment)?;
                let sine = (point.1 - center.1) / radius;
                if !sine.is_finite()
                    || !(-1.0 - VISIBILITY_TOLERANCE_MM..=1.0 + VISIBILITY_TOLERANCE_MM)
                        .contains(&sine)
                {
                    continue;
                }
                let sine = sine.clamp(-1.0, 1.0);
                let candidates = [sine.asin(), PI - sine.asin()];
                for angle in candidates {
                    let point_on_circle = (
                        center.0 + radius * angle.cos(),
                        center.1 + radius * angle.sin(),
                    );
                    if point_on_circle.0 <= point.0 + VISIBILITY_TOLERANCE_MM
                        || !spatial_angle_on_arc(start_angle, end_angle, direction, angle)
                    {
                        continue;
                    }
                    let parameter = spatial_arc_parameter(start_angle, direction, angle);
                    let sweep = match direction {
                        SpatialArcDirection::CounterClockwise => {
                            ccw_angle_distance(start_angle, end_angle)
                        }
                        SpatialArcDirection::Clockwise => {
                            ccw_angle_distance(end_angle, start_angle)
                        }
                    };
                    if parameter >= sweep || approximately_equal(parameter, sweep) {
                        continue;
                    }
                    let derivative = match direction {
                        SpatialArcDirection::CounterClockwise => radius * angle.cos(),
                        SpatialArcDirection::Clockwise => -radius * angle.cos(),
                    };
                    if derivative.abs() <= VISIBILITY_TOLERANCE_MM {
                        continue;
                    }
                    winding += if derivative > 0.0 { 1 } else { -1 };
                }
            }
        }
    }
    Some(winding != 0)
}

/// Exact simple-loop containment for a circular cut. The returned pair is
/// `(strictly_inside, tangent_or_boundary)` so callers can preserve the
/// fail-closed tangent diagnostic without using the envelope as truth.
pub(crate) fn composite_circle_containment(
    center: (f64, f64),
    radius: f64,
    segments: &[SpatialProfileSegment],
) -> Option<(bool, bool)> {
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let inside = composite_point_inside(center, segments)?;
    let minimum_distance = segments
        .iter()
        .map(|segment| point_to_profile_segment_distance(center, segment))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .fold(f64::INFINITY, f64::min);
    if !minimum_distance.is_finite() {
        return None;
    }
    let clearance = minimum_distance - radius;
    let tangent = inside && clearance.abs() <= VISIBILITY_TOLERANCE_MM;
    Some((inside && clearance > VISIBILITY_TOLERANCE_MM, tangent))
}

/// Whether a whole segment is contained in another, orientation-independent
/// segment. Surface rims are allowed to be proper subsegments of body edges.
pub(crate) fn segment_contained_in_segment(
    inner: &ProjectedGeometry,
    outer: &ProjectedGeometry,
) -> Option<bool> {
    let (inner_start, inner_end, outer_start, outer_end) = match (inner, outer) {
        (
            ProjectedGeometry::Segment {
                start: inner_start,
                end: inner_end,
            },
            ProjectedGeometry::Segment {
                start: outer_start,
                end: outer_end,
            },
        ) => (inner_start, inner_end, outer_start, outer_end),
        _ => return Some(false),
    };

    let dx = finite_sub(outer_end.u, outer_start.u)?;
    let dy = finite_sub(outer_end.v, outer_start.v)?;
    let length = dx.hypot(dy);
    if !length.is_finite() || length == 0.0 {
        return Some(false);
    }

    let start_dx = finite_sub(inner_start.u, outer_start.u)?;
    let start_dy = finite_sub(inner_start.v, outer_start.v)?;
    let end_dx = finite_sub(inner_end.u, outer_start.u)?;
    let end_dy = finite_sub(inner_end.v, outer_start.v)?;
    let cross_start = start_dx * dy - start_dy * dx;
    let cross_end = end_dx * dy - end_dy * dx;
    if !cross_start.is_finite() || !cross_end.is_finite() {
        return None;
    }
    let cross_limit = VISIBILITY_TOLERANCE_MM * length.max(1.0);
    if !cross_limit.is_finite() || cross_start.abs() > cross_limit || cross_end.abs() > cross_limit
    {
        return Some(false);
    }

    let outer_projection = dx * dx + dy * dy;
    if !outer_projection.is_finite() || outer_projection == 0.0 {
        return None;
    }
    let start_projection = start_dx * dx + start_dy * dy;
    let end_projection = end_dx * dx + end_dy * dy;
    if !start_projection.is_finite() || !end_projection.is_finite() {
        return None;
    }
    let tolerance_projection = VISIBILITY_TOLERANCE_MM * length;
    if !tolerance_projection.is_finite() {
        return None;
    }
    Some(
        start_projection >= -tolerance_projection
            && start_projection <= outer_projection + tolerance_projection
            && end_projection >= -tolerance_projection
            && end_projection <= outer_projection + tolerance_projection,
    )
}

fn points_equal(a_u: f64, a_v: f64, b_u: f64, b_v: f64) -> bool {
    approximately_equal(a_u, b_u) && approximately_equal(a_v, b_v)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use scc_projection::{ProjectedGeometry, ProjectedPoint2};
    use scc_spatial::SpatialProfileSegment;

    use super::{
        approximately_equal, composite_circle_containment, segments_coincident,
        segments_intersect_or_touch,
    };

    fn point(u: f64, v: f64) -> ProjectedPoint2 {
        ProjectedPoint2 { u, v }
    }

    fn arc(start_angle: f64, end_angle: f64) -> ProjectedGeometry {
        ProjectedGeometry::Arc {
            center: point(0.0, 0.0),
            radius: 10.0,
            start_angle,
            end_angle,
        }
    }

    fn line(start: (f64, f64), end: (f64, f64)) -> ProjectedGeometry {
        ProjectedGeometry::Segment {
            start: point(start.0, start.1),
            end: point(end.0, end.1),
        }
    }

    fn spatial_line(
        start: (f64, f64),
        end: (f64, f64),
        source_entity: &str,
        segment_index: u32,
    ) -> SpatialProfileSegment {
        serde_json::from_value(serde_json::json!({
            "kind": "line",
            "start": {"x": start.0, "y": start.1},
            "end": {"x": end.0, "y": end.1},
            "source_entity": source_entity,
            "segment_index": segment_index,
        }))
        .unwrap()
    }

    #[test]
    fn approximate_comparison_handles_large_opposite_sign_values() {
        assert!(!approximately_equal(f64::MAX, -f64::MAX));
        assert!(approximately_equal(f64::MAX, f64::MAX));
    }

    #[test]
    fn line_arc_intersection_is_analytic() {
        assert_eq!(
            segments_intersect_or_touch(&line((-12.0, 0.0), (12.0, 0.0)), &arc(0.0, PI)),
            Some(true)
        );
        assert_eq!(
            segments_intersect_or_touch(&line((-12.0, -12.0), (-11.0, -11.0)), &arc(0.0, PI)),
            Some(false)
        );
    }

    #[test]
    fn arc_arc_intersection_and_normalized_coincidence_are_supported() {
        assert_eq!(
            segments_intersect_or_touch(&arc(0.0, std::f64::consts::FRAC_PI_2), &arc(PI, 1.5 * PI)),
            Some(false)
        );
        assert_eq!(
            segments_intersect_or_touch(&arc(0.0, std::f64::consts::FRAC_PI_2), &arc(0.5 * PI, PI)),
            Some(true)
        );
        assert_eq!(
            segments_coincident(
                &arc(0.0, std::f64::consts::FRAC_PI_2),
                &arc(2.0 * PI, 2.5 * PI)
            ),
            Some(true)
        );
    }

    #[test]
    fn composite_circle_containment_rejects_tangent_and_outside_cuts() {
        let segments = vec![
            spatial_line((0.0, 0.0), (10.0, 0.0), "a", 0),
            spatial_line((10.0, 0.0), (10.0, 10.0), "b", 1),
            spatial_line((10.0, 10.0), (0.0, 10.0), "c", 2),
            spatial_line((0.0, 10.0), (0.0, 0.0), "d", 3),
        ];
        assert_eq!(
            composite_circle_containment((5.0, 5.0), 1.0, &segments),
            Some((true, false))
        );
        assert_eq!(
            composite_circle_containment((1.0, 5.0), 1.0, &segments),
            Some((false, true))
        );
        assert_eq!(
            composite_circle_containment((-1.0, 5.0), 1.0, &segments),
            Some((false, false))
        );
    }
}
