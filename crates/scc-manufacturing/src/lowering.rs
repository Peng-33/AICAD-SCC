use std::collections::BTreeSet;
use std::f64::consts::TAU;

use scc_spatial::{SpatialBaseProfile, SpatialModel};

use crate::error::SolidBuildError;
use crate::model::{
    ClosedProfileSpec, SolidArcDirection, SolidAxis, SolidBodySource, SolidBodySpec, SolidBounds3D,
    SolidBuildSourceMap, SolidBuildSpec, SolidFootprint, SolidOperation, SolidOperationId,
    SolidPartSpec, SolidPoint2, SolidProfileSegment, SolidZInterval,
    SOLID_BUILD_SPEC_SCHEMA_VERSION,
};

/// Lower a resolved [`SpatialModel`] into a backend-neutral manufacturing
/// snapshot. The input is never mutated and no engineering value is read.
pub fn lower_solid_build_spec(spatial: &SpatialModel) -> Result<SolidBuildSpec, SolidBuildError> {
    let part = lower_part(spatial)?;
    let mut operations = std::collections::BTreeMap::new();
    let mut operation_sources = std::collections::BTreeMap::new();
    let mut source_entities = BTreeSet::new();
    validate_source_id("body feature", &spatial.body.source_feature.0)?;
    validate_source_id("body entity", &spatial.body.source_entity.0)?;
    enum CutRef<'a> {
        Cylindrical(&'a scc_spatial::CylindricalCut),
        Slot(&'a scc_spatial::SlotCut),
    }
    let mut resolved_cuts: Vec<_> = spatial
        .cuts
        .iter()
        .map(|(id, cut)| (id, CutRef::Cylindrical(cut)))
        .chain(
            spatial
                .slot_cuts
                .iter()
                .map(|(id, cut)| (id, CutRef::Slot(cut))),
        )
        .collect();
    resolved_cuts.sort_by(|left, right| left.0.cmp(right.0));
    for pair in resolved_cuts.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: format!(
                    "spatial cut {} is present in more than one cut map",
                    pair[0].0
                ),
            });
        }
    }
    for (ordinal, (spatial_id, cut)) in resolved_cuts.into_iter().enumerate() {
        let ordinal =
            u32::try_from(ordinal).map_err(|_| SolidBuildError::InvalidManufacturingInput {
                detail: "cut count exceeds the operation id range".into(),
            })?;
        let operation_id = SolidOperationId::from_ordinal(ordinal);
        let operation = match cut {
            CutRef::Cylindrical(cut) => {
                validate_cut(spatial_id, cut, &mut source_entities)?;
                SolidOperation::CylindricalCut {
                    center: SolidPoint2 {
                        x: cut.center.x,
                        y: cut.center.y,
                    },
                    radius: cut.radius,
                    z: SolidZInterval {
                        min: cut.z.min,
                        max: cut.z.max,
                    },
                    axis: SolidAxis::PositiveZ,
                }
            }
            CutRef::Slot(cut) => SolidOperation::SlotCut {
                profile: lower_slot_profile(spatial_id, cut, &mut source_entities)?,
                z: SolidZInterval {
                    min: cut.z.min,
                    max: cut.z.max,
                },
                axis: SolidAxis::PositiveZ,
            },
        };
        operations.insert(operation_id, operation);
        operation_sources.insert(operation_id, spatial_id.clone());
    }
    if source_entities.contains(&spatial.body.source_entity) {
        return Err(SolidBuildError::InvalidManufacturingSource {
            detail: format!(
                "body source entity {} is also claimed by a cut",
                spatial.body.source_entity
            ),
        });
    }
    let spec = SolidBuildSpec {
        schema_version: SOLID_BUILD_SPEC_SCHEMA_VERSION,
        part,
        operations,
        source_map: SolidBuildSourceMap {
            body: SolidBodySource {
                feature: spatial.body.source_feature.clone(),
                entity: spatial.body.source_entity.clone(),
            },
            operations: operation_sources,
        },
    };
    validate_spec(&spec)?;
    Ok(spec)
}

fn lower_slot_profile(
    spatial_id: &scc_spatial::SpatialCutId,
    cut: &scc_spatial::SlotCut,
    source_entities: &mut BTreeSet<scc_core::ids::EntityId>,
) -> Result<ClosedProfileSpec, SolidBuildError> {
    validate_source_id("slot cut id", &spatial_id.0)?;
    validate_source_id("slot cut feature", &cut.source_feature.0)?;
    if cut.kind != scc_spatial::SpatialCutKind::ThroughAll {
        return Err(SolidBuildError::UnsupportedManufacturingFeature {
            detail: format!("slot cut {spatial_id} must be resolved as ThroughAll"),
        });
    }
    let z = SolidZInterval {
        min: cut.z.min,
        max: cut.z.max,
    };
    validate_interval(&format!("slot cut {spatial_id}.z"), z, false)?;
    let mut profile = Vec::with_capacity(cut.segments.len());
    for (index, segment) in cut.segments.iter().enumerate() {
        if u32::try_from(index).ok() != Some(segment.segment_index()) {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: format!("slot cut {spatial_id} segment indices are not in canonical order"),
            });
        }
        validate_source_id("slot segment entity", &segment.source_entity().0)?;
        if !source_entities.insert(segment.source_entity().clone()) {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: format!(
                    "slot segment source entity {} is claimed more than once",
                    segment.source_entity()
                ),
            });
        }
        profile.push(match segment {
            scc_spatial::SpatialProfileSegment::Line { start, end, .. } => {
                SolidProfileSegment::Line {
                    start: SolidPoint2 {
                        x: start.x,
                        y: start.y,
                    },
                    end: SolidPoint2 { x: end.x, y: end.y },
                }
            }
            scc_spatial::SpatialProfileSegment::Arc {
                start,
                end,
                center,
                radius,
                direction,
                ..
            } => SolidProfileSegment::Arc {
                start: SolidPoint2 {
                    x: start.x,
                    y: start.y,
                },
                end: SolidPoint2 { x: end.x, y: end.y },
                center: SolidPoint2 {
                    x: center.x,
                    y: center.y,
                },
                radius: *radius,
                direction: match direction {
                    scc_spatial::SpatialArcDirection::Clockwise => SolidArcDirection::Clockwise,
                    scc_spatial::SpatialArcDirection::CounterClockwise => {
                        SolidArcDirection::CounterClockwise
                    }
                },
            },
        });
    }
    let profile = ClosedProfileSpec { segments: profile };
    closed_profile_bounds(&profile.segments, z)?;
    Ok(profile)
}

fn lower_part(spatial: &SpatialModel) -> Result<SolidPartSpec, SolidBuildError> {
    let footprint = match &spatial.body.profile {
        SpatialBaseProfile::Rectangle {
            origin,
            width,
            height,
        } => {
            if !origin.x.is_finite()
                || !origin.y.is_finite()
                || !width.is_finite()
                || !height.is_finite()
                || *width <= 0.0
                || *height <= 0.0
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "body rectangle must have finite positive dimensions".into(),
                });
            }
            SolidFootprint::Rectangle {
                origin: SolidPoint2 { x: origin.x, y: origin.y },
                width: *width,
                height: *height,
            }
        }
        SpatialBaseProfile::Circle { center, radius } => {
            if !center.x.is_finite()
                || !center.y.is_finite()
                || !radius.is_finite()
                || *radius <= 0.0
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "body circle must have finite positive radius".into(),
                });
            }
            SolidFootprint::Circle {
                center: SolidPoint2 { x: center.x, y: center.y },
                radius: *radius,
            }
        }
        SpatialBaseProfile::RoundedRectangle {
            center,
            width,
            height,
            corner_radius,
        } => {
            if !center.x.is_finite()
                || !center.y.is_finite()
                || !width.is_finite()
                || !height.is_finite()
                || !corner_radius.is_finite()
                || *width <= 0.0
                || *height <= 0.0
                || *corner_radius <= 0.0
                || *corner_radius > width.min(*height) / 2.0 + 1.0e-9
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "body rounded rectangle must have finite positive dimensions and a valid corner radius".into(),
                });
            }
            SolidFootprint::RoundedRectangle {
                center: SolidPoint2 { x: center.x, y: center.y },
                width: *width,
                height: *height,
                corner_radius: *corner_radius,
            }
        }
        SpatialBaseProfile::Parametric => {
            return Err(SolidBuildError::UnsupportedManufacturingFeature {
                detail: "generic parametric Extrude history is intentionally deferred from SolidBuildSpec v1 and the canonical STEP path".into(),
            });
        }
        SpatialBaseProfile::Polygon => polygon_footprint(
            &spatial.body.footprint,
            &spatial.body.source_entity,
        )?,
        SpatialBaseProfile::Composite => composite_footprint(
            &spatial.body.footprint,
            &spatial.body.source_entity,
        )?,
        SpatialBaseProfile::Legacy => match &spatial.body.footprint {
            scc_spatial::SpatialFootprint::Rectangle {
                origin,
                width,
                height,
            } => SolidFootprint::Rectangle {
                origin: SolidPoint2 { x: origin.x, y: origin.y },
                width: *width,
                height: *height,
            },
            scc_spatial::SpatialFootprint::Circle { center, radius } => {
                SolidFootprint::Circle {
                    center: SolidPoint2 { x: center.x, y: center.y },
                    radius: *radius,
                }
            }
            scc_spatial::SpatialFootprint::Parametric { .. } => {
                return Err(SolidBuildError::UnsupportedManufacturingFeature {
                    detail: "parametric history is not a SolidBuildSpec v1 input".into(),
                })
            }
            scc_spatial::SpatialFootprint::Composite { .. } => {
                return Err(SolidBuildError::UnsupportedManufacturingFeature {
                    detail: "legacy composite base manufacturing is deferred in SolidBuildSpec v1; no envelope approximation is allowed".into(),
                })
            }
        },
    };
    let body_z = SolidZInterval {
        min: spatial.body.z.min,
        max: spatial.body.z.max,
    };
    validate_interval("body.z", body_z, true)?;
    if body_z.min != 0.0 {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: format!("body.z.min must be 0 mm, got {}", body_z.min),
        });
    }
    let expected_bounds = match footprint.clone() {
        SolidFootprint::Rectangle {
            origin,
            width,
            height,
        } => {
            let max_x = origin.x + width;
            let max_y = origin.y + height;
            if !max_x.is_finite() || !max_y.is_finite() {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "body rectangle arithmetic overflowed to a non-finite bound".into(),
                });
            }
            SolidBounds3D {
                min_x: origin.x,
                max_x,
                min_y: origin.y,
                max_y,
                min_z: body_z.min,
                max_z: body_z.max,
            }
        }
        SolidFootprint::Circle { center, radius } => {
            let min_x = center.x - radius;
            let max_x = center.x + radius;
            let min_y = center.y - radius;
            let max_y = center.y + radius;
            if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite()
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "body circle arithmetic overflowed to a non-finite bound".into(),
                });
            }
            SolidBounds3D {
                min_x,
                max_x,
                min_y,
                max_y,
                min_z: body_z.min,
                max_z: body_z.max,
            }
        }
        SolidFootprint::RoundedRectangle {
            center,
            width,
            height,
            corner_radius: _,
        } => {
            let half_width = width / 2.0;
            let half_height = height / 2.0;
            let min_x = center.x - half_width;
            let max_x = center.x + half_width;
            let min_y = center.y - half_height;
            let max_y = center.y + half_height;
            if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite()
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "body rounded rectangle arithmetic overflowed".into(),
                });
            }
            SolidBounds3D {
                min_x,
                max_x,
                min_y,
                max_y,
                min_z: body_z.min,
                max_z: body_z.max,
            }
        }
        SolidFootprint::ClosedProfile { segments } => closed_profile_bounds(&segments, body_z)?,
    };
    let actual_bounds = SolidBounds3D {
        min_x: spatial.bounds.min_x,
        max_x: spatial.bounds.max_x,
        min_y: spatial.bounds.min_y,
        max_y: spatial.bounds.max_y,
        min_z: spatial.bounds.min_z,
        max_z: spatial.bounds.max_z,
    };
    validate_bounds(actual_bounds)?;
    if actual_bounds != expected_bounds {
        return Err(SolidBuildError::InvalidManufacturingInput {
            detail: "SpatialModel bounds do not match the resolved body".into(),
        });
    }
    Ok(SolidPartSpec {
        body: SolidBodySpec {
            footprint,
            z: body_z,
        },
        bounds: actual_bounds,
    })
}
fn polygon_footprint(
    footprint: &scc_spatial::SpatialFootprint,
    body_entity: &scc_core::ids::EntityId,
) -> Result<SolidFootprint, SolidBuildError> {
    let scc_spatial::SpatialFootprint::Composite { segments } = footprint else {
        return Err(SolidBuildError::InvalidManufacturingInput {
            detail: "typed Polygon requires an exact Composite spatial contour".into(),
        });
    };
    if let Some(first) = segments.first() {
        if first.source_entity() != body_entity {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: "polygon body source entity does not match the first canonical edge".into(),
            });
        }
    }
    let mut source_entities = BTreeSet::new();
    let mut solid_segments = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        if u32::try_from(index).ok() != Some(segment.segment_index()) {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: format!(
                    "polygon edge index {} is out of canonical order at position {index}",
                    segment.segment_index()
                ),
            });
        }
        validate_source_id("polygon edge entity", &segment.source_entity().0)?;
        if !source_entities.insert(segment.source_entity().clone()) {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: format!(
                    "polygon edge source entity {} is repeated",
                    segment.source_entity()
                ),
            });
        }
        let scc_spatial::SpatialProfileSegment::Line { start, end, .. } = segment else {
            return Err(SolidBuildError::UnsupportedManufacturingFeature {
                detail: "typed Polygon contour contains an unexpected arc; polygon manufacturing accepts line edges only".into(),
            });
        };
        solid_segments.push(SolidProfileSegment::Line {
            start: SolidPoint2 {
                x: start.x,
                y: start.y,
            },
            end: SolidPoint2 { x: end.x, y: end.y },
        });
    }
    Ok(SolidFootprint::ClosedProfile {
        segments: solid_segments,
    })
}
fn composite_footprint(
    footprint: &scc_spatial::SpatialFootprint,
    body_entity: &scc_core::ids::EntityId,
) -> Result<SolidFootprint, SolidBuildError> {
    let scc_spatial::SpatialFootprint::Composite { segments } = footprint else {
        return Err(SolidBuildError::InvalidManufacturingInput {
            detail: "typed Composite requires an exact Composite spatial contour".into(),
        });
    };
    match segments.first() {
        Some(first) if first.source_entity() == body_entity => {}
        Some(_) => {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: "composite body source entity does not match the first canonical segment"
                    .into(),
            });
        }
        None => {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: "typed Composite has no source segments".into(),
            });
        }
    }

    let mut source_entities = BTreeSet::new();
    let mut solid_segments = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        if u32::try_from(index).ok() != Some(segment.segment_index()) {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: format!(
                    "composite segment index {} is out of source order at position {index}",
                    segment.segment_index()
                ),
            });
        }
        validate_source_id("composite segment entity", &segment.source_entity().0)?;
        if !source_entities.insert(segment.source_entity().clone()) {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: format!(
                    "composite segment source entity {} is repeated",
                    segment.source_entity()
                ),
            });
        }
        solid_segments.push(match segment {
            scc_spatial::SpatialProfileSegment::Line { start, end, .. } => {
                SolidProfileSegment::Line {
                    start: SolidPoint2 {
                        x: start.x,
                        y: start.y,
                    },
                    end: SolidPoint2 { x: end.x, y: end.y },
                }
            }
            scc_spatial::SpatialProfileSegment::Arc {
                start,
                end,
                center,
                radius,
                direction,
                ..
            } => SolidProfileSegment::Arc {
                start: SolidPoint2 {
                    x: start.x,
                    y: start.y,
                },
                end: SolidPoint2 { x: end.x, y: end.y },
                center: SolidPoint2 {
                    x: center.x,
                    y: center.y,
                },
                radius: *radius,
                direction: match direction {
                    scc_spatial::SpatialArcDirection::Clockwise => SolidArcDirection::Clockwise,
                    scc_spatial::SpatialArcDirection::CounterClockwise => {
                        SolidArcDirection::CounterClockwise
                    }
                },
            },
        });
    }
    Ok(SolidFootprint::ClosedProfile {
        segments: solid_segments,
    })
}

fn validate_cut(
    spatial_id: &scc_spatial::SpatialCutId,
    cut: &scc_spatial::CylindricalCut,
    source_entities: &mut BTreeSet<scc_core::ids::EntityId>,
) -> Result<(), SolidBuildError> {
    validate_source_id("cut id", &spatial_id.0)?;
    validate_source_id("cut feature", &cut.source_feature.0)?;
    validate_source_id("cut entity", &cut.source_entity.0)?;
    if !source_entities.insert(cut.source_entity.clone()) {
        return Err(SolidBuildError::InvalidManufacturingSource {
            detail: format!(
                "cut source entity {} is claimed more than once",
                cut.source_entity
            ),
        });
    }
    if !cut.center.x.is_finite() || !cut.center.y.is_finite() {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: format!("cut {} center is non-finite", spatial_id),
        });
    }
    if !cut.radius.is_finite() || cut.radius <= 0.0 {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: format!("cut {} radius must be finite and positive", spatial_id),
        });
    }
    validate_interval(
        &format!("cut {}.z", spatial_id),
        SolidZInterval {
            min: cut.z.min,
            max: cut.z.max,
        },
        false,
    )
}

fn validate_source_id(label: &str, value: &str) -> Result<(), SolidBuildError> {
    if value.trim().is_empty() {
        return Err(SolidBuildError::InvalidManufacturingSource {
            detail: format!("{label} is empty"),
        });
    }
    Ok(())
}

fn validate_interval(
    label: &str,
    interval: SolidZInterval,
    require_base_plane: bool,
) -> Result<(), SolidBuildError> {
    if !interval.min.is_finite()
        || !interval.max.is_finite()
        || interval.max <= interval.min
        || (require_base_plane && interval.min != 0.0)
    {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: format!("{label} is not a finite non-empty interval"),
        });
    }
    Ok(())
}

fn validate_bounds(bounds: SolidBounds3D) -> Result<(), SolidBuildError> {
    let values = [
        bounds.min_x,
        bounds.max_x,
        bounds.min_y,
        bounds.max_y,
        bounds.min_z,
        bounds.max_z,
    ];
    if values.iter().any(|value| !value.is_finite())
        || bounds.max_x <= bounds.min_x
        || bounds.max_y <= bounds.min_y
        || bounds.max_z <= bounds.min_z
    {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: "bounds must be finite and strictly increasing".into(),
        });
    }
    Ok(())
}

const PROFILE_TOLERANCE: f64 = 1.0e-9;

fn closed_profile_bounds(
    segments: &[SolidProfileSegment],
    z: SolidZInterval,
) -> Result<SolidBounds3D, SolidBuildError> {
    if segments.is_empty() {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: "closed profile requires one non-empty outer loop".into(),
        });
    }
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut area = 0.0;
    let mut previous_end = None;

    for (index, segment) in segments.iter().enumerate() {
        let start = profile_segment_start(segment);
        let end = profile_segment_end(segment);
        if let Some(previous) = previous_end {
            if !points_close(previous, start) {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: format!("closed profile segment {index} is disconnected"),
                });
            }
        }
        match segment {
            SolidProfileSegment::Line { start, end } => {
                if !finite_point(*start) || !finite_point(*end) || points_close(*start, *end) {
                    return Err(SolidBuildError::InvalidManufacturingGeometry {
                        detail: format!("closed profile line segment {index} is invalid"),
                    });
                }
                area += cross(*start, *end) / 2.0;
                include_point(*start, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                include_point(*end, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
            }
            SolidProfileSegment::Arc {
                start,
                end,
                center,
                radius,
                direction,
            } => {
                if !finite_point(*start)
                    || !finite_point(*end)
                    || !finite_point(*center)
                    || !radius.is_finite()
                    || *radius <= 0.0
                    || points_close(*start, *end)
                    || !scale_close(distance(*center, *start), *radius)
                    || !scale_close(distance(*center, *end), *radius)
                {
                    return Err(SolidBuildError::InvalidManufacturingGeometry {
                        detail: format!("closed profile arc segment {index} is invalid"),
                    });
                }
                let start_angle = angle_of(*center, *start);
                let end_angle = angle_of(*center, *end);
                let sweep = directed_sweep(start_angle, end_angle, *direction);
                if sweep <= PROFILE_TOLERANCE || sweep >= TAU - PROFILE_TOLERANCE {
                    return Err(SolidBuildError::InvalidManufacturingGeometry {
                        detail: format!("closed profile arc segment {index} has invalid sweep"),
                    });
                }
                let signed_sweep = match direction {
                    SolidArcDirection::CounterClockwise => sweep,
                    SolidArcDirection::Clockwise => -sweep,
                };
                area += 0.5
                    * (center.x * (end.y - start.y) - center.y * (end.x - start.x)
                        + radius * radius * signed_sweep);
                include_point(*start, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                include_point(*end, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                for cardinal in [
                    0.0,
                    std::f64::consts::FRAC_PI_2,
                    std::f64::consts::PI,
                    3.0 * std::f64::consts::FRAC_PI_2,
                ] {
                    if angle_on_directed_arc(start_angle, end_angle, *direction, cardinal) {
                        include_point(
                            SolidPoint2 {
                                x: center.x + radius * cardinal.cos(),
                                y: center.y + radius * cardinal.sin(),
                            },
                            &mut min_x,
                            &mut max_x,
                            &mut min_y,
                            &mut max_y,
                        );
                    }
                }
            }
        }
        previous_end = Some(end);
    }
    if !points_close(
        profile_segment_end(segments.last().expect("non-empty checked")),
        profile_segment_start(&segments[0]),
    ) {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: "closed profile loop is not closed".into(),
        });
    }
    if !area.is_finite() || area.abs() <= PROFILE_TOLERANCE {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: "closed profile loop has zero signed area".into(),
        });
    }
    if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite() {
        return Err(SolidBuildError::InvalidManufacturingGeometry {
            detail: "closed profile bounds are non-finite".into(),
        });
    }
    Ok(SolidBounds3D {
        min_x,
        max_x,
        min_y,
        max_y,
        min_z: z.min,
        max_z: z.max,
    })
}

fn profile_segment_start(segment: &SolidProfileSegment) -> SolidPoint2 {
    match segment {
        SolidProfileSegment::Line { start, .. } | SolidProfileSegment::Arc { start, .. } => *start,
    }
}

fn profile_segment_end(segment: &SolidProfileSegment) -> SolidPoint2 {
    match segment {
        SolidProfileSegment::Line { end, .. } | SolidProfileSegment::Arc { end, .. } => *end,
    }
}

fn finite_point(point: SolidPoint2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn distance(left: SolidPoint2, right: SolidPoint2) -> f64 {
    (left.x - right.x).hypot(left.y - right.y)
}

fn points_close(left: SolidPoint2, right: SolidPoint2) -> bool {
    distance(left, right) <= PROFILE_TOLERANCE
}

fn scale_close(left: f64, right: f64) -> bool {
    (left - right).abs() <= PROFILE_TOLERANCE * 1.0_f64.max(left.abs()).max(right.abs())
}

fn cross(left: SolidPoint2, right: SolidPoint2) -> f64 {
    left.x * right.y - left.y * right.x
}

fn angle_of(center: SolidPoint2, point: SolidPoint2) -> f64 {
    (point.y - center.y).atan2(point.x - center.x)
}

fn directed_sweep(start: f64, end: f64, direction: SolidArcDirection) -> f64 {
    match direction {
        SolidArcDirection::CounterClockwise => (end - start).rem_euclid(TAU),
        SolidArcDirection::Clockwise => (start - end).rem_euclid(TAU),
    }
}

fn angle_on_directed_arc(start: f64, end: f64, direction: SolidArcDirection, angle: f64) -> bool {
    directed_sweep(start, angle, direction)
        <= directed_sweep(start, end, direction) + PROFILE_TOLERANCE
}

fn include_point(
    point: SolidPoint2,
    min_x: &mut f64,
    max_x: &mut f64,
    min_y: &mut f64,
    max_y: &mut f64,
) {
    *min_x = (*min_x).min(point.x);
    *max_x = (*max_x).max(point.x);
    *min_y = (*min_y).min(point.y);
    *max_y = (*max_y).max(point.y);
}

pub(crate) fn validate_spec(spec: &SolidBuildSpec) -> Result<(), SolidBuildError> {
    if spec.schema_version != SOLID_BUILD_SPEC_SCHEMA_VERSION {
        return Err(SolidBuildError::InvalidManufacturingInput {
            detail: format!("unsupported schema version {}", spec.schema_version),
        });
    }
    if spec.operations.len() != spec.source_map.operations.len()
        || spec.operations.keys().ne(spec.source_map.operations.keys())
    {
        return Err(SolidBuildError::InvalidManufacturingSource {
            detail: "operation and source-map keys are not identical".into(),
        });
    }
    validate_source_id("body feature", &spec.source_map.body.feature.0)?;
    validate_source_id("body entity", &spec.source_map.body.entity.0)?;
    let mut source_ids = BTreeSet::new();
    for spatial_id in spec.source_map.operations.values() {
        validate_source_id("operation spatial cut", &spatial_id.0)?;
        if !source_ids.insert(spatial_id) {
            return Err(SolidBuildError::InvalidManufacturingSource {
                detail: format!("spatial cut {} is mapped more than once", spatial_id),
            });
        }
    }
    let body = &spec.part.body;
    let expected_bounds = match body.footprint.clone() {
        SolidFootprint::Rectangle {
            origin,
            width,
            height,
        } => {
            if !origin.x.is_finite()
                || !origin.y.is_finite()
                || !width.is_finite()
                || !height.is_finite()
                || width <= 0.0
                || height <= 0.0
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "spec body rectangle is invalid".into(),
                });
            }
            let max_x = origin.x + width;
            let max_y = origin.y + height;
            if !max_x.is_finite() || !max_y.is_finite() {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "spec body rectangle arithmetic overflowed".into(),
                });
            }
            SolidBounds3D {
                min_x: origin.x,
                max_x,
                min_y: origin.y,
                max_y,
                min_z: body.z.min,
                max_z: body.z.max,
            }
        }
        SolidFootprint::Circle { center, radius } => {
            if !center.x.is_finite()
                || !center.y.is_finite()
                || !radius.is_finite()
                || radius <= 0.0
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "spec body circle is invalid".into(),
                });
            }
            let min_x = center.x - radius;
            let max_x = center.x + radius;
            let min_y = center.y - radius;
            let max_y = center.y + radius;
            if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite()
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "spec body circle arithmetic overflowed".into(),
                });
            }
            SolidBounds3D {
                min_x,
                max_x,
                min_y,
                max_y,
                min_z: body.z.min,
                max_z: body.z.max,
            }
        }
        SolidFootprint::RoundedRectangle {
            center,
            width,
            height,
            corner_radius,
        } => {
            if !center.x.is_finite()
                || !center.y.is_finite()
                || !width.is_finite()
                || !height.is_finite()
                || !corner_radius.is_finite()
                || width <= 0.0
                || height <= 0.0
                || corner_radius <= 0.0
                || corner_radius > width.min(height) / 2.0 + 1.0e-9
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "spec body rounded rectangle is invalid".into(),
                });
            }
            let min_x = center.x - width / 2.0;
            let max_x = center.x + width / 2.0;
            let min_y = center.y - height / 2.0;
            let max_y = center.y + height / 2.0;
            if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite()
            {
                return Err(SolidBuildError::InvalidManufacturingGeometry {
                    detail: "spec body rounded rectangle arithmetic overflowed".into(),
                });
            }
            SolidBounds3D {
                min_x,
                max_x,
                min_y,
                max_y,
                min_z: body.z.min,
                max_z: body.z.max,
            }
        }
        SolidFootprint::ClosedProfile { segments } => closed_profile_bounds(&segments, body.z)?,
    };
    validate_interval("spec body.z", body.z, true)?;
    validate_bounds(spec.part.bounds)?;
    if expected_bounds != spec.part.bounds {
        return Err(SolidBuildError::InvalidManufacturingInput {
            detail: "spec bounds do not match the resolved body".into(),
        });
    }
    for operation in spec.operations.values() {
        match operation {
            SolidOperation::CylindricalCut {
                center,
                radius,
                z,
                axis,
            } => {
                if !center.x.is_finite()
                    || !center.y.is_finite()
                    || !radius.is_finite()
                    || *radius <= 0.0
                {
                    return Err(SolidBuildError::InvalidManufacturingGeometry {
                        detail: "cylindrical cut has invalid center or radius".into(),
                    });
                }
                if *axis != SolidAxis::PositiveZ {
                    return Err(SolidBuildError::UnsupportedManufacturingFeature {
                        detail: "only canonical +Z cylindrical cuts are supported".into(),
                    });
                }
                validate_interval("spec cut.z", *z, false)?;
            }
            SolidOperation::SlotCut { profile, z, axis } => {
                if *axis != SolidAxis::PositiveZ {
                    return Err(SolidBuildError::UnsupportedManufacturingFeature {
                        detail: "only canonical +Z slot cuts are supported".into(),
                    });
                }
                validate_interval("spec slot cut.z", *z, false)?;
                if *z != body.z {
                    return Err(SolidBuildError::InvalidManufacturingGeometry {
                        detail: "through slot cut interval must match the resolved body Z interval"
                            .into(),
                    });
                }
                let bounds = closed_profile_bounds(&profile.segments, *z)?;
                if bounds.min_x <= spec.part.bounds.min_x
                    || bounds.max_x >= spec.part.bounds.max_x
                    || bounds.min_y <= spec.part.bounds.min_y
                    || bounds.max_y >= spec.part.bounds.max_y
                {
                    return Err(SolidBuildError::InvalidManufacturingGeometry {
                        detail: "slot profile must lie strictly inside the body bounds".into(),
                    });
                }
            }
        }
    }
    Ok(())
}
