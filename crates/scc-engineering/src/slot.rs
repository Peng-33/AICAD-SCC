//! Canonical through-slot geometry (Phase 12.3, ADR-053).
//!
//! `overall_length` is end-to-end length, never center-distance. Semicircular
//! ends are explicit directed 180° arcs; Composite source profiles still
//! reject ambiguous semicircles.

use crate::error::EngineeringError;
use crate::profile::{ArcDirection, ProfileSegment, PROFILE_TOLERANCE};
use crate::Point2D;

/// Map any finite orientation into the canonical half-turn `[0, 180)`.
pub fn canonicalize_orientation_degrees(degrees: f64) -> Result<f64, EngineeringError> {
    if !degrees.is_finite() {
        return Err(EngineeringError::NonFiniteValue {
            what: "slot orientation",
        });
    }
    Ok(degrees.rem_euclid(180.0))
}

/// Reject non-positive width and overall length that is not strictly longer
/// than the width (a round hole is `Hole`, not `Slot`).
pub fn validate_slot_dimensions(overall_length: f64, width: f64) -> Result<(), EngineeringError> {
    crate::positive("slot width", width)?;
    crate::positive("slot overall length", overall_length)?;
    if overall_length <= width + PROFILE_TOLERANCE {
        return Err(EngineeringError::InvalidProfile {
            reason:
                "slot overall length must be greater than width; use a Hole when they are equal"
                    .into(),
        });
    }
    Ok(())
}

/// Exact ordered outer loop: top line, right semicircle, bottom line, left
/// semicircle, then rotated by canonical orientation about `center`.
pub fn slot_profile_segments(
    center: Point2D,
    overall_length: f64,
    width: f64,
    orientation_degrees: f64,
) -> Result<Vec<ProfileSegment>, EngineeringError> {
    crate::finite_point(center, "slot center")?;
    validate_slot_dimensions(overall_length, width)?;
    let angle = canonicalize_orientation_degrees(orientation_degrees)?;
    let radius = width / 2.0;
    let half_span = (overall_length - width) / 2.0;
    let local = [
        ProfileSegment::Line {
            start: Point2D::new(-half_span, radius),
            end: Point2D::new(half_span, radius),
        },
        ProfileSegment::Arc {
            start: Point2D::new(half_span, radius),
            end: Point2D::new(half_span, -radius),
            center: Point2D::new(half_span, 0.0),
            radius,
            direction: ArcDirection::Clockwise,
        },
        ProfileSegment::Line {
            start: Point2D::new(half_span, -radius),
            end: Point2D::new(-half_span, -radius),
        },
        ProfileSegment::Arc {
            start: Point2D::new(-half_span, -radius),
            end: Point2D::new(-half_span, radius),
            center: Point2D::new(-half_span, 0.0),
            radius,
            direction: ArcDirection::Clockwise,
        },
    ];
    Ok(local
        .into_iter()
        .map(|segment| transform_segment(segment, center, angle))
        .collect())
}

fn transform_segment(segment: ProfileSegment, origin: Point2D, degrees: f64) -> ProfileSegment {
    match segment {
        ProfileSegment::Line { start, end } => ProfileSegment::Line {
            start: rotate_about(start, origin, degrees),
            end: rotate_about(end, origin, degrees),
        },
        ProfileSegment::Arc {
            start,
            end,
            center,
            radius,
            direction,
        } => ProfileSegment::Arc {
            start: rotate_about(start, origin, degrees),
            end: rotate_about(end, origin, degrees),
            center: rotate_about(center, origin, degrees),
            radius,
            direction,
        },
    }
}

fn rotate_about(point: Point2D, origin: Point2D, degrees: f64) -> Point2D {
    let radians = degrees.to_radians();
    let cos = radians.cos();
    let sin = radians.sin();
    Point2D::new(
        origin.x + point.x * cos - point.y * sin,
        origin.y + point.x * sin + point.y * cos,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_180_is_the_same_identity_as_zero() {
        assert_eq!(
            canonicalize_orientation_degrees(180.0).unwrap(),
            canonicalize_orientation_degrees(0.0).unwrap()
        );
        assert_eq!(canonicalize_orientation_degrees(360.0).unwrap(), 0.0);
        assert_eq!(canonicalize_orientation_degrees(-45.0).unwrap(), 135.0);
    }

    #[test]
    fn equal_length_and_width_is_rejected() {
        assert!(validate_slot_dimensions(12.0, 12.0).is_err());
        assert!(validate_slot_dimensions(10.0, 12.0).is_err());
        assert!(validate_slot_dimensions(40.0, 12.0).is_ok());
    }

    #[test]
    fn overall_length_is_end_to_end_not_center_distance() {
        let segments = slot_profile_segments(Point2D::new(0.0, 0.0), 40.0, 12.0, 0.0).unwrap();
        let ProfileSegment::Line { start, end } = &segments[0] else {
            panic!("top must be a line");
        };
        assert!((end.x - start.x - 28.0).abs() < 1.0e-9);
        let ProfileSegment::Arc {
            center,
            radius,
            direction,
            ..
        } = &segments[1]
        else {
            panic!("right end must be an arc");
        };
        assert!((center.x - 14.0).abs() < 1.0e-9);
        assert!((radius - 6.0).abs() < 1.0e-9);
        assert_eq!(*direction, ArcDirection::Clockwise);
        let right = crate::profile::geometry_angles(&segments[1]).unwrap();
        let left = crate::profile::geometry_angles(&segments[3]).unwrap();
        let right_outer = Point2D::new(
            14.0 + 6.0 * ((right.0 + right.1) / 2.0).cos(),
            6.0 * ((right.0 + right.1) / 2.0).sin(),
        );
        let left_outer = Point2D::new(
            -14.0 + 6.0 * ((left.0 + left.1) / 2.0).cos(),
            6.0 * ((left.0 + left.1) / 2.0).sin(),
        );
        assert!((right_outer.x - 20.0).abs() < 1.0e-9);
        assert!((left_outer.x + 20.0).abs() < 1.0e-9);
        assert!((right_outer.x - left_outer.x - 40.0).abs() < 1.0e-9);
        assert!((right_outer.x - left_outer.x - 28.0).abs() > 1.0);
    }

    #[test]
    fn non_finite_orientation_and_dimensions_fail_closed() {
        assert!(canonicalize_orientation_degrees(f64::NAN).is_err());
        assert!(canonicalize_orientation_degrees(f64::INFINITY).is_err());
        assert!(validate_slot_dimensions(f64::NAN, 12.0).is_err());
        assert!(validate_slot_dimensions(40.0, f64::INFINITY).is_err());
        assert!(validate_slot_dimensions(0.0, 12.0).is_err());
        assert!(validate_slot_dimensions(40.0, 0.0).is_err());
        assert!(slot_profile_segments(Point2D::new(f64::NAN, 0.0), 40.0, 12.0, 0.0).is_err());
    }
}
