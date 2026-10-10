//! AICAD engineering drawing presentation layer (Phase 8).
//!
//! `scc-drawing` converts an already-solved
//! [`scc_geometry::GeometryModel`] into a derived, read-only
//! [`DrawingModel`] that references that solved geometry. It never re-solves
//! constraints, never recomputes pattern geometry, and never stores a second
//! editable engineering state: `EngineeringModel` stays the single
//! authoritative engineering truth (ADR-016), `GeometryModel` stays the single
//! solved-geometry truth, and `DrawingModel` is a pure, deterministic
//! presentation model built on demand (ADR-022).
//!
//! Dependency direction is strictly one-way and leaf-shaped:
//!
//! ```text
//! scc-drawing
//!    ├── scc-geometry     (solved geometry — authoritative positions)
//!    └── scc-engineering  (source map, drawing intent, part metadata)
//!    └── scc-core         (ErrorStage / StructuredError / EntityId)
//! ```
//!
//! `scc-engineering` must never depend on `scc-drawing`; the engineering core
//! has no knowledge of the drawing layer.
//!
//! Coordinate spaces (ADR-024): primitives are stored in **model space** (mm);
//! one deterministic [`DrawingViewTransform`] per view derives **sheet space**
//! — millimetres on paper, `+X` right, `+Y` up, origin bottom-left, with a
//! fixed margin envelope:
//!
//! ```text
//! sheet = model × scale + translation
//! ```
//!
//! Renderers consume these values as-is and never re-fit / re-center /
//! re-scale. No renderer (SVG/DXF/PDF), dimension, centerline, annotation,
//! construction styling, or projection logic lives in this crate yet.

pub mod annotation;
pub mod annotation_layout;
pub mod associative_dimension;
pub mod builder;
pub mod center_geometry;
pub mod dimension;
pub mod dimension_layout;
pub mod dxf;
pub mod error;
pub mod model;
pub mod section_view;
pub mod source_map;
pub mod svg;
pub mod three_view;
pub mod three_view_annotations;

pub use annotation::{
    validate_annotation_integrity, DrawingAnnotation, DrawingAnnotationId, DrawingAnnotationKind,
    DrawingAnnotationSource, DrawingTextAnchor, DEFAULT_ANNOTATION_TEXT_HEIGHT_MM,
};
pub use annotation_layout::{
    optimize_dimension_layouts, optimize_three_view_layouts, validate_dimension_layout_collisions,
    AnnotationLayoutCollision, AnnotationLayoutReport, APPROXIMATE_DIMENSION_GLYPH_ADVANCE,
    DEFAULT_ANNOTATION_COLLISION_CLEARANCE_MM, MAX_ANNOTATION_REFLOW_ATTEMPTS,
};
pub use associative_dimension::{
    AssociativeDatumKind, AssociativeDimension, AssociativeDimensionReference,
    AssociativeDimensionStatus, AssociativeDimensionUnit, DimensionGeometryWitness,
    DimensionObservability, DimensionValueProvenance,
};
pub use center_geometry::{
    validate_center_geometry_integrity, CenterMark, Centerline, CenterlineAxis,
    DrawingCenterGeometry, DrawingCenterGeometryId, DEFAULT_CENTERLINE_OVERHANG_MM,
    DEFAULT_CENTER_MARK_HALF_LENGTH_MM,
};
pub use dimension::{
    format_dimension, format_dimension_with_profile, format_hole_callout,
    validate_dimension_integrity, DimensionAxis3D, DimensionComponent, DimensionValue,
    DimensionValueSource, DrawingDimension, DrawingDimensionId, DrawingDimensionKind,
    DrawingDimensionSpecification, DrawingDimensionTarget, DrawingFormattingProfile,
    DrawingFormattingProfileId, HoleCalloutExtent, HoleCalloutProvenance, HoleCalloutSemantic,
    HoleCalloutWitness, HoleDatum, LinearAxis, ParametricCounterboreCallout,
    ParametricDimensionParameter, ParametricHoleCalloutSemantic, ParametricHoleExtent,
    ParametricHolePatternCallout, PatternSemanticField, PatternShape, ResolvedDimensionValue,
    SlotCalloutSemantic, ToleranceStyle, AICAD_INTERNAL_V1,
};
pub use dimension_layout::{
    validate_dimension_layout_integrity, ArrowAnchor, DimensionLayoutGeometry,
    DrawingDimensionLayout, LayoutSegment, TextOrientation, DEFAULT_ARROW_SIZE_MM,
    DEFAULT_DIMENSION_OFFSET_MM, DEFAULT_DIMENSION_STACK_PITCH_MM, DEFAULT_DIMENSION_TEXT_GAP_MM,
    DEFAULT_EXTENSION_GAP_MM, DEFAULT_EXTENSION_OVERHANG_MM, DIAMETER_AXIS_ANGLE_DEG,
    WITNESS_EPSILON_MM,
};
pub use dxf::{
    drawing_to_dxf, LAYER_ANNOTATION, LAYER_CENTER, LAYER_CONSTRUCTION, LAYER_DIMENSION,
    LAYER_HIDDEN, LAYER_OBJECT, LTYPE_CENTERLINE, LTYPE_CONTINUOUS, LTYPE_DASHED,
};
pub use error::DrawingError;
pub use model::{
    DrawingBounds, DrawingModel, DrawingPrimitive, DrawingPrimitiveRole, DrawingSheet,
    DrawingStrokeKind, DrawingView, DrawingViewTransform,
};
pub use section_view::{
    append_section_aa, SectionAxis, SectionViewGeometry, SectionViewReport, SectionViewSide,
};
pub use source_map::{
    DrawingSource, DrawingSourceMap, SectionPrimitiveOrigin, SectionPrimitiveSource,
    SyntheticDrawingKind,
};
pub use svg::{
    drawing_to_svg, DEFAULT_DIMENSION_TEXT_HEIGHT_MM, OBJECT_STROKE_WIDTH_MM,
    SVG_CENTER_DASH_PATTERN, SVG_CONSTRUCTION_DASH_PATTERN, SVG_HIDDEN_DASH_PATTERN,
    THIN_STROKE_WIDTH_MM,
};
pub use three_view::{
    build_three_view_drawing, ThreeViewDrawingError, ThreeViewDrawingInputs, VIEW_GAP_MM,
};
pub use three_view_annotations::{
    build_three_view_annotations, normalize_annotation_bounds, validate_three_view_annotations,
};

use serde::{Deserialize, Serialize};

/// Stable identifier for a drawing document (the root of `DrawingModel`).
///
/// IDs are deterministic, derived from the input (part id) and never
/// auto-generated random UUIDs or timestamps. The `String`-newtype pattern
/// mirrors [`scc_engineering::FeatureId`] / [`scc_core::ids::EntityId`] so that
/// drawing ids are strong-typed and distinct from every other id family.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DrawingId(pub String);

/// Stable identifier for a drawing sheet.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DrawingSheetId(pub String);

/// Stable identifier for a drawing view.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DrawingViewId(pub String);

/// Stable identifier for a drawing primitive.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DrawingPrimitiveId(pub String);

impl std::fmt::Display for DrawingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Display for DrawingSheetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Display for DrawingViewId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Display for DrawingPrimitiveId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A 2-D coordinate in **model space**, expressed in millimetres.
///
/// Kept as a dedicated drawing value type (rather than reusing
/// `scc_geometry::Point2`) so that the drawing model owns its coordinate
/// vocabulary as a stable contract, after the same precedent by which
/// `scc_engineering` keeps its own `Point2D`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DrawingPoint {
    pub x: f64,
    pub y: f64,
}

impl DrawingPoint {
    /// Construct a point.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// The scale of a drawing, as a positive, finite ratio.
///
/// `1.0` means one drawing-unit per one model-unit (millimetre). Phase 8.1
/// only stores and validates the scale; it does not fit/lay out the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DrawingScale(pub f64);

impl DrawingScale {
    /// The default scale used when `DrawingProperty::Scale` is unspecified.
    ///
    /// The model's dimensions are expressed in millimetres (doc comments in
    /// `scc-engineering`), so the deterministic default is `1.0` model-unit
    /// per drawing-unit. This default is defined in ADR-022, not inferred from
    /// the UI.
    pub const DEFAULT_ONE_TO_ONE: Self = Self(1.0);

    /// Whether the scale is finite and strictly positive (a scale of zero or
    /// less cannot produce a valid drawing).
    pub fn is_valid(self) -> bool {
        self.0.is_finite() && self.0 > 0.0
    }
}

impl Default for DrawingScale {
    /// The deterministic default scale: one-to-one.
    fn default() -> Self {
        Self::DEFAULT_ONE_TO_ONE
    }
}

/// What kind of orthographic view a [`DrawingView`] holds.
///
/// Phase 8.1 deliberately supports only the single **principal** 2-D view of
/// the solved geometry. Front / Top / Left projection views are strictly a
/// Phase 9 concern and are not added here; this enum stays a plain additive
/// enum so future kinds extend it without touching existing code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingViewKind {
    /// The principal (plan) view of the solved 2-D geometry.
    Principal,
    /// The formal Front principal projection view.
    Front,
    /// The formal Top principal projection view.
    Top,
    /// The formal Left principal projection view.
    Left,
    /// A generated principal-plane section identified as A-A.
    #[serde(rename = "section_aa")]
    SectionAA,
}
