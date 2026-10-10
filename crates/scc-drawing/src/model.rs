//! The derived, read-only drawing model (Phases 8.1–8.3).
//!
//! `DrawingModel` is a **presentation** of already-solved geometry: it says how
//! geometry from a [`scc_geometry::GeometryModel`] is referenced inside a
//! [`DrawingView`]. It is deliberately *not* a second engineering state — it
//! has no command surface, no write API, and is never persisted as
//! authoritative truth (see ADR-022).
//!
//! Coordinate spaces (formal definitions in ADR-024):
//!
//! - **Model space** — the solved geometry's own millimetre coordinates
//!   (SCC engineering convention; may be negative / centered at the origin).
//!   [`DrawingPrimitive`]s store model-space geometry only.
//! - **View space** — model geometry after the engineering drawing scale
//!   (`model × DrawingScale`), local to a view.
//! - **Sheet space** — millimetres on paper, `+X` right, `+Y` **up**, origin
//!   at the bottom-left. Derived deterministically from model space through
//!   [`DrawingViewTransform`] (`sheet = model × scale + translation`).
//!
//! Sheet space is renderer-independent: SVG Y-down pixels and DXF details are
//! renderer concerns derived from these values, never drawing truth.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::source_map::DrawingSourceMap;
use crate::{
    DrawingError, DrawingPoint, DrawingPrimitiveId, DrawingScale, DrawingSheetId, DrawingViewId,
    DrawingViewKind,
};

/// The full derived drawing document.
///
/// All collections are `BTreeMap`/`BTreeSet` backed for deterministic
/// iteration and serialization (no `HashMap`, no random UUIDs, no timestamps).
/// Identical inputs to [`crate::builder::build_drawing`] produce identical
/// `DrawingModel`, `DrawingSourceMap`, and serialization.
///
/// Coordinate spaces (ADR-024): primitives carry **model space** geometry; the
/// per-view [`DrawingViewTransform`] derives **sheet space** placement
/// deterministically. There is no second stored transformed copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingModel {
    /// One sheet per drawing (Phases 8.1–8.3 build a single principal sheet).
    pub sheets: BTreeMap<DrawingSheetId, DrawingSheet>,
    /// Strong-typed provenance from each drawing primitive back to its solved
    /// geometry entity.
    pub source_map: DrawingSourceMap,
    /// Formal associations for derived parametric dimensions. The rendered
    /// DrawingDimension and layout stay on their view and are validated 1:1.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub associative_dimensions:
        BTreeMap<crate::DrawingDimensionId, crate::associative_dimension::AssociativeDimension>,
}

impl DrawingModel {
    /// The principal sheet — Phase 8.3 builds exactly one; the first sheet in
    /// deterministic key order.
    pub fn principal_sheet(&self) -> Option<&DrawingSheet> {
        self.sheets.values().next()
    }

    /// The principal view of this drawing, if present.
    pub fn principal_view(&self) -> Option<&DrawingView> {
        self.principal_sheet()?
            .views
            .values()
            .find(|view| view.kind == DrawingViewKind::Principal)
    }

    /// Mutable access to the principal sheet — validator/test plumbing only;
    /// production drawings are built once by [`crate::builder::build_drawing`].
    pub fn principal_sheet_mut(&mut self) -> Option<&mut DrawingSheet> {
        self.sheets.values_mut().next()
    }

    /// Mutable access to the principal view (see [`Self::principal_sheet_mut`]).
    pub fn principal_view_mut(&mut self) -> Option<&mut DrawingView> {
        self.principal_sheet_mut()?
            .views
            .values_mut()
            .find(|view| view.kind == DrawingViewKind::Principal)
    }

    /// Locate the `(sheet id, view id)` owning a primitive.
    ///
    /// Derived by traversal of the typed hierarchy (`sheet → view →
    /// primitive`) — never by string scanning of ids. Deterministic.
    pub fn locate_primitive(
        &self,
        primitive: &DrawingPrimitiveId,
    ) -> Option<(&DrawingSheetId, &DrawingViewId)> {
        for (sheet_id, sheet) in &self.sheets {
            for (view_id, view) in &sheet.views {
                if view.primitives.contains_key(primitive) {
                    return Some((sheet_id, view_id));
                }
            }
        }
        None
    }
}

/// One drawing sheet, owning its orthographic views.
///
/// A sheet lives in **sheet space**: millimetres on paper, `+X` right, `+Y`
/// **up**, origin at the bottom-left corner. Sheet space is renderer-independent
/// — SVG's Y-down pixel space and DXF's implementation details are produced by
/// renderers from these values, never stored here (ADR-024).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingSheet {
    pub id: DrawingSheetId,
    /// Views on this sheet, keyed in deterministic order. Phases 8.1–8.3 hold
    /// a single [`.views`](DrawingViewKind::Principal) view.
    pub views: BTreeMap<DrawingViewId, DrawingView>,
    /// Non-geometric drawing annotations (Phase 8.4), keyed by strong-typed
    /// id. Sheet-level: a part title describes the whole part / sheet, not any
    /// single view's geometry. Annotations never affect [`bounds`](DrawingSheet::bounds)
    /// or view transforms.
    pub annotations: BTreeMap<crate::DrawingAnnotationId, crate::DrawingAnnotation>,
}

impl DrawingSheet {
    /// The deterministic drawing margin (millimetres on paper) applied on
    /// every side when deriving the sheet envelope.
    ///
    /// This is a drawing *presentation* default fixed by ADR-024 — it is not
    /// engineering truth and not a standard paper size (A4/A3 selection is
    /// explicitly deferred).
    pub const DEFAULT_SHEET_MARGIN_MM: f64 = 10.0;

    /// Union of all views' sheet-space content bounds **plus** their
    /// dimension-layout extents (ADR-027).
    ///
    /// One envelope algorithm: geometry presentation and dimension layouts are
    /// unioned once here; a single outer margin is then applied by
    /// [`DrawingSheet::bounds`] — never per dimension. Derived
    /// deterministically; no copied state. The union is seeded from the first
    /// contributing bounds (never a zero box); with no views this is the
    /// degenerate zero bounds.
    pub fn content_bounds(&self) -> DrawingBounds {
        let mut acc: Option<DrawingBounds> = None;
        let include = |b: DrawingBounds, acc: &mut Option<DrawingBounds>| match acc {
            Some(a) => a.include(&b),
            None => *acc = Some(b),
        };
        for view in self.views.values() {
            include(view.sheet_bounds(), &mut acc);
            if let Some(layouts) = view.layout_extents() {
                include(layouts, &mut acc);
            }
            if let Some(centers) = view.center_geometry_extents() {
                include(centers, &mut acc);
            }
        }
        acc.unwrap_or_else(DrawingBounds::zero)
    }

    /// The paper envelope of this sheet in sheet space: origin `(0, 0)` at the
    /// bottom-left, extent `content size + margin on every side`.
    ///
    /// `sheet.width  = content_width  + 2 × DEFAULT_SHEET_MARGIN_MM`
    /// `sheet.height = content_height + 2 × DEFAULT_SHEET_MARGIN_MM`
    ///
    /// An empty presentation therefore yields the minimal deterministic
    /// envelope `20 mm × 20 mm`. Derived only — never stored, never NaN.
    pub fn bounds(&self) -> DrawingBounds {
        let margin = Self::DEFAULT_SHEET_MARGIN_MM;
        let content = self.content_bounds();
        let width = (content.max_x - content.min_x).max(0.0);
        let height = (content.max_y - content.min_y).max(0.0);
        DrawingBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: width + 2.0 * margin,
            max_y: height + 2.0 * margin,
        }
    }
}

/// A single orthographic view referencing presentation primitives.
///
/// Primitives are stored in **model space** (mm) — never duplicated into a
/// second authoritative transformed copy. Sheet positions are derived through
/// [`transform`](DrawingView::transform), the single placement rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingView {
    pub id: DrawingViewId,
    /// The view kind. Only [`DrawingViewKind::Principal`] in Phase 8.3.
    pub kind: DrawingViewKind,
    /// Optional renderer-owned display title derived from formal view semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_label: Option<String>,
    /// The single deterministic model→sheet placement rule (scale +
    /// translation). Renderers consume it; they never re-fit/re-center/re-scale.
    pub transform: DrawingViewTransform,
    /// The presentation primitives, keyed by strong-typed id, in model space.
    pub primitives: BTreeMap<DrawingPrimitiveId, DrawingPrimitive>,
    /// Engineering dimensions attached to this view (Phase 8.5), keyed by
    /// strong-typed id. Dimensions describe what is measured and where the
    /// number comes from — never layout coordinates. They belong to the view
    /// because they measure *this view's* engineering geometry.
    pub dimensions: BTreeMap<crate::DrawingDimensionId, crate::DrawingDimension>,
    /// Deterministic sheet-space layouts for this view's dimensions
    /// (Phase 8.6), keyed 1:1 by the same dimension id. Layouts hold the
    /// presentation geometry (dimension line, extension lines, arrow anchors,
    /// text anchor) — never engineering values or sources.
    pub dimension_layouts: BTreeMap<crate::DrawingDimensionId, crate::DrawingDimensionLayout>,
    /// Drawing-derived center marks and centerlines (Phase 8.7), keyed by
    /// strong-typed id. Pure presentation for circular object features — no
    /// EntityId fabrication, nothing in the primitive source map. Controlled
    /// by `ShowCenterlines`; independent of construction visibility.
    pub center_geometry: BTreeMap<crate::DrawingCenterGeometryId, crate::DrawingCenterGeometry>,
    /// Axis-aligned bounds of this view's **final presentation** primitives in
    /// **model space**. Sheet-space bounds derive via
    /// [`DrawingView::sheet_bounds`].
    pub model_bounds: DrawingBounds,
}

impl DrawingView {
    /// The sheet-space bounds of this view's presentation content:
    /// [`model_bounds`](DrawingView::model_bounds) mapped through
    /// [`transform`](DrawingView::transform). Derived; never stored twice.
    ///
    /// This stays **geometry-only** (ADR-027): dimension layouts extend the
    /// *sheet* envelope, not the geometry view bounds.
    pub fn sheet_bounds(&self) -> DrawingBounds {
        let mut bounds = self.transform.transform_bounds(self.model_bounds);
        if let Some(label) = &self.display_label {
            let anchor = self
                .display_label_position()
                .expect("a display label has an anchor");
            bounds.max_x = bounds
                .max_x
                .max(anchor.x + label.chars().count() as f64 * 2.4);
            bounds.max_y = bounds.max_y.max(anchor.y + 1.0);
        }
        bounds
    }

    /// Stable sheet-space title anchor used by both renderers.
    pub fn display_label_position(&self) -> Option<DrawingPoint> {
        self.display_label.as_ref().map(|_| {
            self.transform.model_to_sheet(DrawingPoint::new(
                self.model_bounds.min_x,
                self.model_bounds.max_y + 4.0 / self.transform.scale.0,
            ))
        })
    }

    /// Union of all dimension-layout extents on this view, if any exist.
    pub fn layout_extents(&self) -> Option<DrawingBounds> {
        let mut acc: Option<DrawingBounds> = None;
        for layout in self.dimension_layouts.values() {
            let b = layout.extents();
            match &mut acc {
                Some(a) => a.include(&b),
                None => acc = Some(b),
            }
        }
        acc
    }

    /// Union of all center-geometry extents on this view, if any exist.
    pub fn center_geometry_extents(&self) -> Option<DrawingBounds> {
        let mut acc: Option<DrawingBounds> = None;
        for element in self.center_geometry.values() {
            let b = element.extents();
            match &mut acc {
                Some(a) => a.include(&b),
                None => acc = Some(b),
            }
        }
        acc
    }
}

/// The single deterministic model→sheet placement rule.
///
/// Composition (ADR-024):
///
/// ```text
/// view_space  = model_space × scale            (engineering drawing scale)
/// sheet_space = view_space + translation       (placement on the paper)
/// ```
///
/// so for any model point `p`:
///
/// ```text
/// sheet_x = p.x × scale + translation.x
/// sheet_y = p.y × scale + translation.y
/// ```
///
/// Sheet space is millimetres on paper, `+X` right, `+Y` up, origin at the
/// bottom-left. No rotation, no arbitrary matrix, no camera/perspective, no 3-D
/// component. The builder computes `translation` once so that the scaled
/// presentation minimum lands exactly on
/// (`DrawingSheet::DEFAULT_SHEET_MARGIN_MM`, `DEFAULT_SHEET_MARGIN_MM`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DrawingViewTransform {
    /// Engineering drawing scale: `1.0` = 1:1, `0.5` = 1:2, `2.0` = 2:1
    /// (model mm → paper mm). Never a UI zoom.
    pub scale: DrawingScale,
    /// Translation applied after scaling, in sheet millimetres.
    pub translation: DrawingPoint,
}

impl DrawingViewTransform {
    /// Construct a validated transform, fail-closed.
    ///
    /// Rejects a non-finite / non-positive scale and non-finite translation
    /// (`SCC-DRW-007`) so an invalid transform can never enter a drawing.
    pub fn new(scale: DrawingScale, translation: DrawingPoint) -> Result<Self, DrawingError> {
        if !scale.is_valid() {
            return Err(DrawingError::InvalidDrawingTransform {
                reason: "scale must be finite and strictly positive".into(),
            });
        }
        if !translation.x.is_finite() || !translation.y.is_finite() {
            return Err(DrawingError::InvalidDrawingTransform {
                reason: "translation must be finite".into(),
            });
        }
        Ok(Self { scale, translation })
    }

    /// The identity transform: one-to-one scale, zero translation.
    pub fn identity() -> Self {
        Self {
            scale: DrawingScale::default(),
            translation: DrawingPoint::new(0.0, 0.0),
        }
    }

    /// The untranslated transform with an explicit scale.
    pub fn identity_with_scale(scale: DrawingScale) -> Self {
        Self {
            scale,
            translation: DrawingPoint::new(0.0, 0.0),
        }
    }

    /// Model space → view space: apply the engineering drawing scale only.
    pub fn model_to_view(&self, point: DrawingPoint) -> DrawingPoint {
        DrawingPoint::new(point.x * self.scale.0, point.y * self.scale.0)
    }

    /// View space → sheet space: apply the placement translation only.
    pub fn view_to_sheet(&self, point: DrawingPoint) -> DrawingPoint {
        DrawingPoint::new(point.x + self.translation.x, point.y + self.translation.y)
    }

    /// Model space → sheet space (the composed single-view pipeline):
    /// `sheet = model × scale + translation`.
    pub fn model_to_sheet(&self, point: DrawingPoint) -> DrawingPoint {
        self.view_to_sheet(self.model_to_view(point))
    }

    /// Map model-space bounds to sheet-space bounds.
    ///
    /// The scale is validated strictly positive, so min/max ordering is
    /// preserved; the min/max re-ordering here is defensive only.
    pub fn transform_bounds(&self, bounds: DrawingBounds) -> DrawingBounds {
        let a = self.model_to_sheet(DrawingPoint::new(bounds.min_x, bounds.min_y));
        let b = self.model_to_sheet(DrawingPoint::new(bounds.max_x, bounds.max_y));
        DrawingBounds {
            min_x: a.x.min(b.x),
            min_y: a.y.min(b.y),
            max_x: a.x.max(b.x),
            max_y: a.y.max(b.y),
        }
    }
}

/// The engineering drawing role of a primitive.
///
/// A role answers *"what is this primitive in the engineering drawing?"* —
/// real part geometry vs construction/reference geometry vs drawing-only
/// synthetic geometry. It lives in the drawing semantic layer: it is derived
/// from strong-typed provenance (feature kind), never from id strings, and it
/// is what a future line-style resolver will map to renderer-specific styling.
///
/// Phase 9.7 adds `Hidden` as a role assigned from the frozen visibility
/// sidecar; centerlines, dimensions, extension lines, and leaders remain
/// separate generated presentation elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingPrimitiveRole {
    /// Real material part geometry (outline, holes, pattern holes).
    Object,
    /// Reference-only construction geometry (e.g. `ConstructionCircle`).
    Construction,
    /// Reference/datum geometry that is not construction but not material.
    /// Reserved: no current engineering feature produces it.
    Reference,
    /// Drawing-only synthetic geometry created by the drawing engine itself
    /// (future: dimension extension lines, arrows, center marks, view/sheet
    /// borders). **Never** a fallback for missing provenance — the
    /// geometry→drawing builder fails closed instead.
    Synthetic,
    /// A projected part edge classified as hidden by the visibility sidecar.
    Hidden,
    /// A thin section line representing material cut by the section plane.
    SectionHatch,
}

/// Renderer-independent stroke semantics (ADR-028).
///
/// Domain-level only: how a renderer realizes each kind (solid / dashed /
/// dash-dot/hidden patterns, DXF linetype tables) is entirely a renderer
/// concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingStrokeKind {
    /// Real material geometry.
    Object,
    /// Reference/construction geometry.
    Construction,
    /// Derived center presentation (marks and centerlines).
    Center,
    /// A hidden projected edge, rendered with a distinct dashed style.
    Hidden,
    /// A section hatch line, rendered as a thin continuous line.
    Hatch,
}

impl DrawingPrimitiveRole {
    /// The stroke semantic implied by this role — no per-primitive duplicated
    /// state; renderers map role → stroke via this single function.
    pub fn stroke_kind(&self) -> DrawingStrokeKind {
        match self {
            DrawingPrimitiveRole::Object => DrawingStrokeKind::Object,
            // Reference geometry presents like construction geometry.
            DrawingPrimitiveRole::Construction | DrawingPrimitiveRole::Reference => {
                DrawingStrokeKind::Construction
            }
            // Synthetic drawing-engine primitives are object-styled today.
            DrawingPrimitiveRole::Synthetic => DrawingStrokeKind::Object,
            DrawingPrimitiveRole::Hidden => DrawingStrokeKind::Hidden,
            DrawingPrimitiveRole::SectionHatch => DrawingStrokeKind::Hatch,
        }
    }
}

/// A presentation primitive covering every
/// [`scc_geometry::GeometryPrimitive`] variant losslessly, annotated with its
/// engineering drawing role.
///
/// Geometry is stored in model-space coordinates (mm). Rendering styles
/// (line type, color, hidden/construction distinction) do **not** belong here
/// in Phase 8.1 — those arrive with the renderer phases. The `role` is
/// assigned deterministically by the builder from strong-typed provenance
/// (feature kind), never by parsing id strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DrawingPrimitive {
    Point {
        role: DrawingPrimitiveRole,
        position: DrawingPoint,
    },
    Line {
        role: DrawingPrimitiveRole,
        start: DrawingPoint,
        end: DrawingPoint,
    },
    Circle {
        role: DrawingPrimitiveRole,
        center: DrawingPoint,
        radius: f64,
    },
    Arc {
        role: DrawingPrimitiveRole,
        center: DrawingPoint,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    Rectangle {
        role: DrawingPrimitiveRole,
        origin: DrawingPoint,
        width: f64,
        height: f64,
    },
    Polygon {
        role: DrawingPrimitiveRole,
        vertices: Vec<DrawingPoint>,
    },
}

/// An axis-aligned bounding box, used in **both** model space and sheet space.
///
/// The space is named at each use site ([`DrawingView::model_bounds`] vs
/// [`DrawingSheet::bounds`]); the box itself is unit-agnostic millimetres.
/// Negative coordinates are fully supported (nothing assumes a positive
/// origin).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DrawingBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl DrawingBounds {
    /// The degenerate zero-size bounds at the origin (an empty view).
    pub fn zero() -> Self {
        Self {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        }
    }

    /// Expand these bounds to include `other`.
    pub(crate) fn include(&mut self, other: &DrawingBounds) {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
    }

    /// Whether every coordinate is finite (no NaN / infinities leaked).
    pub fn is_finite(&self) -> bool {
        [self.min_x, self.min_y, self.max_x, self.max_y]
            .iter()
            .all(|v| v.is_finite())
    }
}
