//! Pure, deterministic builder from solved geometry to a drawing model.
//!
//! [`build_drawing`] is a pure function of its inputs: it never re-solves
//! constraints, never recomputes pattern geometry (patterns arrive already
//! expanded as solved entities), never re-evaluates parameter expressions, and
//! never re-runs engineering lowering. It consumes the **already-solved**
//! [`scc_geometry::GeometryModel`] and derives a presentation-only
//! [`DrawingModel`].
//!
//! Pipeline order (ADR-024):
//!
//! ```text
//! GeometryModel
//!   → validate non-empty
//!   → resolve DrawingScale / construction visibility (DrawingState intent)
//!   → per entity: provenance → role → presentation filter → convert
//!   → accumulate presentation model bounds
//!   → compute DrawingViewTransform (scaled minimum → sheet margin)
//!   → assemble view + sheet
//! ```
//!
//! Determinism: all iteration is over `BTreeMap`s and ids are derived
//! deterministically from the solved entity ids, so identical inputs produce
//! identical drawing ids, models, source maps, roles, transforms, bounds, and
//! serialization — with no network, LLM, timestamp, random uuid, global
//! mutable state, or input mutation.
//!
//! Provenance is fail-closed: every solved geometry entity must map through
//! [`scc_engineering::source_map::EngineeringSourceMap`] to an engineering
//! feature, or the build fails (`SCC-DRW-003`). [`DrawingSource::Synthetic`]
//! is **never** a fallback for missing provenance (ADR-023).

use std::collections::BTreeMap;

use scc_core::ids::EntityId;
use scc_engineering::feature::{DrawingProperty, DrawingPropertyKind, EngineeringFeature};
use scc_engineering::model::{DrawingState, EngineeringModel, PartDefinition};
use scc_engineering::source_map::EngineeringSourceMap;
use scc_engineering::FeatureId;
use scc_geometry::{GeometryModel, GeometryPrimitive};

use crate::error::DrawingError;
use crate::model::{
    DrawingBounds, DrawingModel, DrawingPrimitive, DrawingPrimitiveRole, DrawingSheet, DrawingView,
    DrawingViewTransform,
};
use crate::source_map::DrawingSourceMap;
use crate::{
    DrawingDimensionTarget, DrawingPoint, DrawingPrimitiveId, DrawingScale, DrawingSheetId,
    DrawingViewId, DrawingViewKind,
};

/// Whether any drawing primitive in `drawing_map` traces back (through the
/// engineering source map) to `feature` — i.e. whether the feature is
/// represented in the current presentation.
fn drawing_source_map_has_feature(
    drawing_map: &DrawingSourceMap,
    eng_map: &EngineeringSourceMap,
    feature: &FeatureId,
) -> bool {
    !drawing_map
        .primitives_for_feature(feature, eng_map)
        .is_empty()
}

/// The narrow, explicit set of inputs the drawing builder consumes.
///
/// Deliberately **not** the whole `EngineeringModel`: the drawing layer only
/// needs the solved geometry, the provenance map, the engineering **feature
/// kinds** (for role classification), and the drawing intent / part metadata.
/// This keeps the drawing engine from gaining an unnecessarily wide access
/// path into the engineering state.
pub struct DrawingInputs<'a> {
    /// Already-solved geometry (the single source of drawing positions).
    pub geometry: &'a GeometryModel,
    /// Engineering → SCC provenance (feature ↔ entity).
    pub engineering_source_map: &'a EngineeringSourceMap,
    /// Drawing intent stored on the authoritative model (scale, centerlines,
    /// construction visibility, projection method).
    pub drawing_state: &'a DrawingState,
    /// The engineering features map (feature id → feature). Used only to
    /// classify each primitive's [`DrawingPrimitiveRole`] (e.g.
    /// `ConstructionCircle` → `Construction`) and to verify feature existence.
    pub features: &'a BTreeMap<FeatureId, EngineeringFeature>,
    /// The authoritative engineering model, read-only, used **only** to derive
    /// dimensions (Phase 8.5): bindings (`feature_parameters`), parameter
    /// resolution (`evaluate_parameter`), and feature literals. Immutable
    /// borrow only — no Drawing→Engineering write path exists. `None` skips
    /// dimension generation entirely (geometry-only drawings stay legal).
    ///
    /// Why the whole model instead of narrower maps: dimension resolution must
    /// follow *exactly* the Phase 3 binding→literal order; duplicating that
    /// resolution in the drawing crate would risk the exact divergence this
    /// phase forbids (stale-literal dimensions after a re-binding).
    pub engineering_model: Option<&'a EngineeringModel>,
    /// Optional part metadata (used only for deterministic sheet naming).
    pub part: Option<&'a PartDefinition>,
}

impl<'a> DrawingInputs<'a> {
    /// Construct the drawing inputs from the narrow sources.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        geometry: &'a GeometryModel,
        engineering_source_map: &'a EngineeringSourceMap,
        drawing_state: &'a DrawingState,
        features: &'a BTreeMap<FeatureId, EngineeringFeature>,
        engineering_model: Option<&'a EngineeringModel>,
        part: Option<&'a PartDefinition>,
    ) -> Self {
        Self {
            geometry,
            engineering_source_map,
            drawing_state,
            features,
            engineering_model,
            part,
        }
    }
}

/// Build a derived, read-only drawing model from solved geometry.
///
/// Pure and deterministic. Returns fail-closed structured errors instead of
/// silently dropping any geometry or silently guessing provenance.
pub fn build_drawing(inputs: &DrawingInputs<'_>) -> Result<DrawingModel, DrawingError> {
    if inputs.geometry.entities.is_empty() {
        return Err(DrawingError::EmptyDrawing);
    }

    let scale = resolve_scale(inputs.drawing_state)?;
    let show_construction = resolve_construction_visibility(inputs.drawing_state)?;
    let sheet_id = DrawingSheetId(
        inputs
            .part
            .map(|p| p.id.0.clone())
            .unwrap_or_else(|| "drawing".into()),
    );
    let view_id = DrawingViewId("principal".into());

    // Provenance inverse: solved entity → engineering feature id.
    let entity_to_feature = inputs.engineering_source_map.entity_to_feature();

    let mut source_map = DrawingSourceMap::new();
    let mut primitives = BTreeMap::new();
    let mut bounds: Option<DrawingBounds> = None;

    // BTreeMap<EntityId, _> iterates in deterministic (sorted) id order.
    for (entity_id, entity) in &inputs.geometry.entities {
        // Fail-closed provenance: every solved entity must map to a feature.
        let feature_id =
            entity_to_feature
                .get(entity_id)
                .ok_or_else(|| DrawingError::MissingSourceMapping {
                    entity_id: entity_id.clone(),
                })?;
        // The feature must exist (its kind drives the role).
        let feature = inputs.features.get(feature_id).ok_or_else(|| {
            DrawingError::InvalidSourceIntegrity {
                reason: format!(
                    "entity {} maps to feature {} which is not present in the engineering model",
                    entity_id, feature_id
                ),
            }
        })?;
        let role = feature_role(feature);

        // Presentation decision: hidden construction geometry is omitted from
        // the drawing (ADR-023). Provenance is already resolved above, so
        // hiding never erases or corrupts it — it simply is not part of this
        // presentation, and re-running with visibility on restores it.
        if role == DrawingPrimitiveRole::Construction && !show_construction {
            continue;
        }

        let primitive = geometry_to_drawing(entity_id, &entity.primitive, role)?;
        let prim_id = DrawingPrimitiveId(entity_id.to_string());
        // Record provenance: primitive → solved geometry entity.
        source_map.record(prim_id.clone(), entity_id.clone());
        // Expand per-primitive bounds into the view aggregate.
        let prim_bounds = primitive_bounds(&primitive)?;
        match &mut bounds {
            Some(b) => b.include(&prim_bounds),
            None => bounds = Some(prim_bounds),
        }
        primitives.insert(prim_id, primitive);
    }

    // If every solved primitive was filtered (e.g. a construction-only part
    // with `ShowConstructionGeometry=false`), the view is legitimately empty —
    // that is a presentation decision, not "no geometry". `EmptyDrawing` above
    // already rejects the truly-empty-geometry case.
    let model_bounds = bounds.unwrap_or_else(DrawingBounds::zero);

    // Deterministic principal-view placement (ADR-024): the scaled
    // presentation minimum lands exactly on the sheet margin, so the content
    // always sits inside `margin..(extent - margin)` in positive sheet space.
    //
    //   translation = margin − model_min × scale
    //
    // For an empty presentation the degenerate zero bounds place the (absent)
    // content origin at the margin corner, yielding the minimal 2·margin
    // envelope — no NaN, no negative extent.
    let margin = DrawingSheet::DEFAULT_SHEET_MARGIN_MM;
    let s = scale.0;
    let translation = DrawingPoint::new(
        margin - model_bounds.min_x * s,
        margin - model_bounds.min_y * s,
    );
    let transform = DrawingViewTransform::new(scale, translation)?;

    // Engineering dimensions (Phase 8.5, ADR-026): derived per *feature* —
    // never per primitive — with binding-first value sources. Each target must
    // actually be represented in this presentation (fail-closed), and the
    // finished set is integrity-checked against the model.
    let mut dimensions = BTreeMap::new();
    if let Some(model) = inputs.engineering_model {
        for dimension in crate::dimension::generate_dimensions(model)? {
            let DrawingDimensionTarget::Feature { feature } = &dimension.target else {
                return Err(DrawingError::InvalidDimensionTarget {
                    reason: format!(
                        "dimension {} body target is unsupported in legacy drawing",
                        dimension.id
                    ),
                });
            };
            if drawing_source_map_has_feature(&source_map, inputs.engineering_source_map, feature) {
                dimensions.insert(dimension.id.clone(), dimension);
            } else {
                return Err(DrawingError::InvalidDimensionTarget {
                    reason: format!(
                        "dimension {} targets feature {} which has no represented primitive in the drawing presentation",
                        dimension.id, feature
                    ),
                });
            }
        }
    }

    // Deterministic dimension layouts (Phase 8.6, ADR-027): witness geometry
    // from the presented primitives, cross-checked against the engineering
    // values before anything is placed. Geometry placement is untouched; the
    // sheet envelope grows toward +X/+Y via the layouts' extents.
    let dimension_layouts = if dimensions.is_empty() {
        BTreeMap::new()
    } else {
        // Reference edge for side placement: the placed geometry's own
        // sheet-space content bounds (scale + translation).
        let geometry_sheet_bounds = transform.transform_bounds(model_bounds);
        crate::dimension_layout::generate_dimension_layouts(
            &primitives,
            &dimensions,
            &source_map,
            inputs.engineering_source_map,
            &transform,
            &geometry_sheet_bounds,
        )?
    };

    // Drawing-derived center presentation (Phase 8.7, ADR-028): marks and
    // axial centerlines for circular object features (Hole / HolePattern
    // elements), gated by ShowCenterlines and independent of construction
    // visibility. Sheet-space output; paper-mm constants; no primitive or
    // source-map pollution.
    let center_geometry = if resolve_centerlines_visible(inputs.drawing_state)? {
        crate::center_geometry::generate_center_geometry(
            &primitives,
            &source_map,
            inputs.engineering_source_map,
            inputs.features,
            &transform,
        )?
    } else {
        BTreeMap::new()
    };

    let view = DrawingView {
        id: view_id.clone(),
        kind: DrawingViewKind::Principal,
        display_label: None,
        transform,
        primitives,
        dimensions,
        dimension_layouts,
        center_geometry,
        model_bounds,
    };

    let mut views = BTreeMap::new();
    views.insert(view_id, view);

    // Sheet-level annotation: the part title (ADR-025). Derived from
    // `PartDefinition.name`, placed deterministically inside the existing
    // bottom margin — it never moves geometry and never changes the sheet
    // envelope. Absent when no part metadata exists or the name is blank.
    let mut annotations = BTreeMap::new();
    if let Some(title) = crate::annotation::resolve_part_title(inputs.part)? {
        annotations.insert(title.id.clone(), title);
    }

    let mut sheets = BTreeMap::new();
    sheets.insert(
        sheet_id.clone(),
        DrawingSheet {
            id: sheet_id,
            views,
            annotations,
        },
    );

    let drawing = DrawingModel {
        sheets,
        source_map,
        associative_dimensions: BTreeMap::new(),
    };
    // Fail-closed integrity check over the assembled annotations (cheap; the
    // model must never rely on the builder "probably being right").
    crate::annotation::validate_annotation_integrity(&drawing.sheets, inputs.part)?;
    // And over the derived dimensions (Phase 8.5): sources must re-resolve to
    // exactly the cached values.
    if let Some(model) = inputs.engineering_model {
        for sheet in drawing.sheets.values() {
            for view in sheet.views.values() {
                crate::dimension::validate_dimension_integrity(view, model)?;
            }
        }
    }
    // Layout pairing/geometry integrity (Phase 8.6): every dimension laid out
    // exactly once, no orphans, all coordinates finite, no degenerate lines.
    for sheet in drawing.sheets.values() {
        for view in sheet.views.values() {
            crate::dimension_layout::validate_dimension_layout_integrity(view)?;
            // Center geometry integrity (Phase 8.7): sources resolvable,
            // coordinates finite, marks proper crosses, features represented.
            crate::center_geometry::validate_center_geometry_integrity(
                view,
                &drawing.source_map,
                inputs.engineering_source_map,
                inputs.features,
            )?;
        }
    }
    Ok(drawing)
}

/// Resolve the drawing scale from the authoritative drawing intent.
///
/// `DrawingPropertyKind::Scale` drives the scale; if it is unspecified the
/// deterministic default `1.0` (one-to-one) is used — this default is defined
/// in ADR-022, never inferred from the UI.
fn resolve_scale(drawing_state: &DrawingState) -> Result<DrawingScale, DrawingError> {
    match drawing_state.get(DrawingPropertyKind::Scale) {
        Some(DrawingProperty::Scale { value }) => {
            let scale = DrawingScale(*value);
            if !scale.is_valid() {
                return Err(DrawingError::InvalidDrawingScale { scale: *value });
            }
            Ok(scale)
        }
        // A property stored under the Scale kind is always the Scale variant.
        Some(_) | None => Ok(DrawingScale::default()),
    }
}

/// Resolve whether construction geometry is visible in the drawing.
///
/// `ShowConstructionGeometry = true` (or unspecified — construction geometry
/// is reference geometry and is visible by default) keeps `Construction`-role
/// primitives in the drawing; `false` omits them. Deterministic default
/// defined in ADR-023, never inferred from the UI.
fn resolve_construction_visibility(drawing_state: &DrawingState) -> Result<bool, DrawingError> {
    match drawing_state.get(DrawingPropertyKind::ShowConstructionGeometry) {
        Some(DrawingProperty::ShowConstructionGeometry { enabled }) => Ok(*enabled),
        // A property stored under the ShowConstructionGeometry kind is always
        // that variant.
        Some(_) | None => Ok(true),
    }
}

/// Resolve whether center marks / centerlines are drawn.
///
/// `ShowCenterlines = true` (or unspecified — mechanical drawings conventionally
/// show hole centers; the deterministic default is fixed in ADR-028 and never
/// inferred from the UI) enables center geometry; `false` suppresses it
/// entirely. Independent of [`resolve_construction_visibility`].
fn resolve_centerlines_visible(drawing_state: &DrawingState) -> Result<bool, DrawingError> {
    match drawing_state.get(DrawingPropertyKind::ShowCenterlines) {
        Some(DrawingProperty::ShowCenterlines { enabled }) => Ok(*enabled),
        Some(_) | None => Ok(true),
    }
}

/// Classify the engineering drawing role of a feature.
///
/// Strong-typed, based on the feature kind — never on id strings.
fn feature_role(feature: &EngineeringFeature) -> DrawingPrimitiveRole {
    match feature {
        // Reference-only geometry → construction role.
        EngineeringFeature::ConstructionCircle { .. } => DrawingPrimitiveRole::Construction,
        // Material part geometry → object role. Slot/Fillet never reach solved
        // geometry in the current lowering (fail-closed), but are material
        // features and would be Object if they did.
        EngineeringFeature::BaseProfile { .. }
        | EngineeringFeature::Hole { .. }
        | EngineeringFeature::HolePattern { .. }
        | EngineeringFeature::Slot { .. }
        | EngineeringFeature::Fillet { .. } => DrawingPrimitiveRole::Object,
        EngineeringFeature::Parametric { feature, .. } => match feature {
            scc_engineering::ParametricFeature::Extrude { .. }
            | scc_engineering::ParametricFeature::Revolve { .. }
            | scc_engineering::ParametricFeature::Sweep { .. }
            | scc_engineering::ParametricFeature::Loft { .. }
            | scc_engineering::ParametricFeature::Hole { .. }
            | scc_engineering::ParametricFeature::Pattern { .. }
            | scc_engineering::ParametricFeature::Fillet { .. }
            | scc_engineering::ParametricFeature::Chamfer { .. } => DrawingPrimitiveRole::Object,
            scc_engineering::ParametricFeature::DatumPlane { .. }
            | scc_engineering::ParametricFeature::DatumAxis { .. }
            | scc_engineering::ParametricFeature::Sketch { .. }
            | scc_engineering::ParametricFeature::Path { .. } => DrawingPrimitiveRole::Construction,
        },
    }
}

/// Convert a solved geometry primitive into a presentation primitive,
/// fail-closed, threading the real entity id and role through for accurate
/// results and errors.
fn geometry_to_drawing(
    entity_id: &EntityId,
    primitive: &GeometryPrimitive,
    role: DrawingPrimitiveRole,
) -> Result<DrawingPrimitive, DrawingError> {
    match primitive {
        GeometryPrimitive::Point { position } => Ok(DrawingPrimitive::Point {
            role,
            position: to_drawing_point(position),
        }),
        GeometryPrimitive::Line { start, end } => Ok(DrawingPrimitive::Line {
            role,
            start: to_drawing_point(start),
            end: to_drawing_point(end),
        }),
        GeometryPrimitive::Circle { center, radius } => {
            validate_geometric_length(entity_id, "circle radius", *radius)?;
            Ok(DrawingPrimitive::Circle {
                role,
                center: to_drawing_point(center),
                radius: *radius,
            })
        }
        GeometryPrimitive::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            validate_geometric_length(entity_id, "arc radius", *radius)?;
            if !start_angle.is_finite() || !end_angle.is_finite() {
                return Err(DrawingError::InvalidGeometryValue {
                    entity_id: entity_id.clone(),
                    reason: "non-finite arc angle".into(),
                });
            }
            Ok(DrawingPrimitive::Arc {
                role,
                center: to_drawing_point(center),
                radius: *radius,
                start_angle: *start_angle,
                end_angle: *end_angle,
            })
        }
        GeometryPrimitive::Rectangle {
            origin,
            width,
            height,
        } => {
            if !width.is_finite() || !height.is_finite() {
                return Err(DrawingError::InvalidGeometryValue {
                    entity_id: entity_id.clone(),
                    reason: "non-finite rectangle dimensions".into(),
                });
            }
            Ok(DrawingPrimitive::Rectangle {
                role,
                origin: to_drawing_point(origin),
                width: *width,
                height: *height,
            })
        }
        GeometryPrimitive::Polygon { vertices } => {
            if vertices.len() < 3 {
                return Err(DrawingError::InvalidGeometryValue {
                    entity_id: entity_id.clone(),
                    reason: "polygon needs at least three vertices".into(),
                });
            }
            Ok(DrawingPrimitive::Polygon {
                role,
                vertices: vertices.iter().map(to_drawing_point).collect(),
            })
        }
    }
}

/// A geometric length/radius must be finite and non-negative.
fn validate_geometric_length(
    entity_id: &EntityId,
    context: &str,
    value: f64,
) -> Result<(), DrawingError> {
    if !value.is_finite() || value < 0.0 {
        return Err(DrawingError::InvalidGeometryValue {
            entity_id: entity_id.clone(),
            reason: format!("{} must be finite and non-negative", context),
        });
    }
    Ok(())
}

/// Convert a geometry coordinate into a drawing (model-space) coordinate.
fn to_drawing_point(point: &scc_geometry::Point2) -> DrawingPoint {
    DrawingPoint::new(point.x, point.y)
}

/// Axis-aligned bounds of a single drawing primitive in model space.
///
/// Negative coordinates are fully supported. A non-finite coordinate or a
/// degenerate primitive is treated as an invalid geometry value (fail closed).
/// The primitive's role does not affect its bounds.
pub(crate) fn primitive_bounds(
    primitive: &DrawingPrimitive,
) -> Result<DrawingBounds, DrawingError> {
    let mut builder = BoundsBuilder::new();
    match primitive {
        DrawingPrimitive::Point { position, .. } => builder.push(*position, "point")?,
        DrawingPrimitive::Line { start, end, .. } => {
            builder.push(*start, "line")?;
            builder.push(*end, "line")?;
        }
        DrawingPrimitive::Circle { center, radius, .. } => {
            builder.push(*center, "circle")?;
            builder.push_extent(*center, *radius, "circle")?;
        }
        DrawingPrimitive::Arc {
            center,
            radius,
            start_angle,
            end_angle,
            ..
        } => return exact_arc_bounds(*center, *radius, *start_angle, *end_angle),
        DrawingPrimitive::Rectangle {
            origin,
            width,
            height,
            ..
        } => {
            builder.push(*origin, "rectangle")?;
            builder.push(
                DrawingPoint::new(origin.x + width, origin.y + height),
                "rectangle",
            )?;
        }
        DrawingPrimitive::Polygon { vertices, .. } => {
            if vertices.len() < 3 {
                return Err(DrawingError::InvalidGeometryValue {
                    entity_id: EntityId("polygon".into()),
                    reason: "polygon needs at least three vertices".into(),
                });
            }
            for v in vertices {
                builder.push(*v, "polygon")?;
            }
        }
    }
    builder.finish()
}

fn exact_arc_bounds(
    center: DrawingPoint,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
) -> Result<DrawingBounds, DrawingError> {
    if !radius.is_finite() || radius < 0.0 {
        return Err(DrawingError::InvalidGeometryValue {
            entity_id: EntityId("arc".into()),
            reason: "non-finite or negative radius".into(),
        });
    }
    if !start_angle.is_finite() || !end_angle.is_finite() {
        return Err(DrawingError::InvalidGeometryValue {
            entity_id: EntityId("arc".into()),
            reason: "non-finite arc angle".into(),
        });
    }
    let sweep = normalized_arc_sweep(start_angle, end_angle);
    if sweep <= ARC_SWEEP_EPSILON || sweep >= std::f64::consts::TAU - ARC_SWEEP_EPSILON {
        return Err(DrawingError::InvalidGeometryValue {
            entity_id: EntityId("arc".into()),
            reason: "arc sweep must be greater than zero and less than one full turn".into(),
        });
    }

    let mut builder = BoundsBuilder::new();
    builder.push(arc_point(center, radius, start_angle), "arc")?;
    builder.push(arc_point(center, radius, end_angle), "arc")?;
    for (angle, u_sign, v_sign) in [
        (0.0, 1.0, 0.0),
        (std::f64::consts::FRAC_PI_2, 0.0, 1.0),
        (std::f64::consts::PI, -1.0, 0.0),
        (std::f64::consts::PI * 1.5, 0.0, -1.0),
    ] {
        if ccw_angle_distance(start_angle, angle) <= sweep {
            builder.push(
                DrawingPoint::new(center.x + radius * u_sign, center.y + radius * v_sign),
                "arc",
            )?;
        }
    }
    builder.finish()
}

const ARC_SWEEP_EPSILON: f64 = 1.0e-9;

fn normalized_arc_sweep(start_angle: f64, end_angle: f64) -> f64 {
    (end_angle - start_angle).rem_euclid(std::f64::consts::TAU)
}

fn ccw_angle_distance(start_angle: f64, angle: f64) -> f64 {
    (angle - start_angle).rem_euclid(std::f64::consts::TAU)
}

fn arc_point(center: DrawingPoint, radius: f64, angle: f64) -> DrawingPoint {
    DrawingPoint::new(
        center.x + radius * angle.cos(),
        center.y + radius * angle.sin(),
    )
}

/// Incremental axis-aligned bounds accumulator for primitives.
struct BoundsBuilder {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    set: bool,
}

impl BoundsBuilder {
    fn new() -> Self {
        Self {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
            set: false,
        }
    }

    fn push(&mut self, p: DrawingPoint, what: &str) -> Result<(), DrawingError> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(DrawingError::InvalidGeometryValue {
                entity_id: EntityId(what.into()),
                reason: "non-finite coordinate in drawing primitive".into(),
            });
        }
        if !self.set {
            self.min_x = p.x;
            self.min_y = p.y;
            self.max_x = p.x;
            self.max_y = p.y;
            self.set = true;
        } else {
            self.min_x = self.min_x.min(p.x);
            self.min_y = self.min_y.min(p.y);
            self.max_x = self.max_x.max(p.x);
            self.max_y = self.max_y.max(p.y);
        }
        Ok(())
    }

    /// Push a center plus a radial extent (circle/arc), covering ±radius on
    /// both axes.
    fn push_extent(
        &mut self,
        center: DrawingPoint,
        radius: f64,
        what: &str,
    ) -> Result<(), DrawingError> {
        if !radius.is_finite() || radius < 0.0 {
            return Err(DrawingError::InvalidGeometryValue {
                entity_id: EntityId(what.into()),
                reason: "non-finite or negative radius".into(),
            });
        }
        self.push(
            DrawingPoint::new(center.x - radius, center.y - radius),
            what,
        )?;
        self.push(
            DrawingPoint::new(center.x + radius, center.y + radius),
            what,
        )
    }

    fn finish(self) -> Result<DrawingBounds, DrawingError> {
        if !self.set {
            return Err(DrawingError::EmptyDrawing);
        }
        Ok(DrawingBounds {
            min_x: self.min_x,
            min_y: self.min_y,
            max_x: self.max_x,
            max_y: self.max_y,
        })
    }
}

/// Compute the aggregate axis-aligned bounds of all solved geometry entities.
///
/// This is a pure, local convenience for establishing a principal view's
/// extents; it deliberately does **not** change any `scc-geometry` API in
/// Phase 8.1. Negative coordinates are supported. Provenance is not required
/// here (bounds are geometry-only); the primitive role is irrelevant to bounds
/// and is set to `Object` as a neutral placeholder.
pub fn compute_geometry_bounds(geometry: &GeometryModel) -> Result<DrawingBounds, DrawingError> {
    let mut bounds: Option<DrawingBounds> = None;
    for entity in geometry.entities.values() {
        let prim_id = EntityId(entity.id.to_string());
        let primitive =
            geometry_to_drawing(&prim_id, &entity.primitive, DrawingPrimitiveRole::Object)?;
        let b = primitive_bounds(&primitive)?;
        match &mut bounds {
            Some(acc) => acc.include(&b),
            None => bounds = Some(b),
        }
    }
    bounds.ok_or(DrawingError::EmptyDrawing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_bounds_are_exact_for_a_quarter_arc() {
        let bounds = primitive_bounds(&DrawingPrimitive::Arc {
            role: DrawingPrimitiveRole::Object,
            center: DrawingPoint::new(10.0, 20.0),
            radius: 5.0,
            start_angle: 0.0,
            end_angle: std::f64::consts::FRAC_PI_2,
        })
        .unwrap();
        assert_eq!(
            (bounds.min_x, bounds.max_x, bounds.min_y, bounds.max_y),
            (10.0, 15.0, 20.0, 25.0)
        );
    }
}
