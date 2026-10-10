//! Drawing annotations (Phase 8.4): non-geometric drawing information.
//!
//! An annotation is **derived presentation data** inside a
//! [`DrawingModel`](crate::DrawingModel) — never a second authoritative state
//! and never written back to the engineering model. The engineering truth for
//! a part title is [`scc_engineering::model::PartDefinition::name`]; the
//! annotation stores the *resolved presentation text* and is re-derived on
//! every build.
//!
//! Annotations are deliberately separate from
//! [`DrawingPrimitive`](crate::DrawingPrimitive): primitives present solved
//! geometry, annotations carry non-geometric drawing information. Dimension
//! text ("Ø64"), centerlines, leaders, GD&T etc. are **not** annotations —
//! they get their own strong-typed semantics in later phases.
//!
//! Positions are in **sheet space** (mm on paper, `+X` right, `+Y` up,
//! origin bottom-left — ADR-024). Text height is paper millimetres and is
//! **never** scaled by the engineering [`DrawingScale`](crate::DrawingScale).
//! Fonts, escaping (SVG/DXF), text measurement, wrapping, and collision
//! handling are renderer / later-phase concerns.

use std::collections::BTreeMap;

use scc_engineering::model::PartDefinition;
use serde::{Deserialize, Serialize};

use crate::{DrawingError, DrawingPoint, DrawingSheet};
use scc_engineering::PartId;

/// The deterministic MVP text height for annotations, in paper millimetres.
///
/// Fixed by ADR-025; finite, strictly positive, renderer-independent. It is
/// never multiplied by the engineering drawing scale: at 1:2 the geometry
/// halves but a 3.5 mm title stays 3.5 mm on the paper.
pub const DEFAULT_ANNOTATION_TEXT_HEIGHT_MM: f64 = 3.5;

/// Strong-typed identifier of a drawing annotation.
///
/// Semantic and stable (e.g. `"part_title"`) — derived from the input, never a
/// random UUID or timestamp. Same inputs ⇒ same annotation ids.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DrawingAnnotationId(pub String);

impl std::fmt::Display for DrawingAnnotationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// What an annotation is.
///
/// Phase 8.4 supports exactly one kind. Note/tolerance/datum/GD&T kinds are
/// deliberately absent until they have a real authoritative source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingAnnotationKind {
    /// The part's display name, sourced from `PartDefinition.name`.
    PartTitle,
}

/// Where an annotation's content comes from — strong-typed provenance.
///
/// A `PartTitle` traces back to its [`PartId`]; it is **never** synthetic and
/// never carries a fabricated entity/feature id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DrawingAnnotationSource {
    /// Derived from the named engineering part (`PartDefinition`).
    PartTitle { part: PartId },
}

/// Horizontal anchoring of annotation text relative to its position.
///
/// Domain-level semantics only (no CSS/SVG attribute names): `Start` extends
/// toward +X from the position, `Middle` centers on it, `End` extends toward
/// −X. Vertically the position is the text **baseline** (ADR-025).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingTextAnchor {
    /// Text begins at the position and extends toward +X.
    Start,
    /// Text is centered horizontally on the position.
    Middle,
    /// Text ends at the position, extending from −X.
    End,
}

/// One non-geometric drawing annotation.
///
/// `text` is the resolved presentation string (kept raw — Unicode, `&`, `<`,
/// quotes stay as-is; escaping is a renderer concern). `position` is in sheet
/// space; `text_height_mm` is paper millimetres, unaffected by the drawing
/// scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingAnnotation {
    pub id: DrawingAnnotationId,
    pub kind: DrawingAnnotationKind,
    /// Resolved presentation text (raw UTF-8; never pre-escaped).
    pub text: String,
    /// Anchor point in sheet space (mm; +Y up, origin bottom-left).
    pub position: DrawingPoint,
    /// Text height in paper millimetres (never scaled by DrawingScale).
    pub text_height_mm: f64,
    /// Horizontal anchor; vertically `position.y` is the baseline.
    pub anchor: DrawingTextAnchor,
    /// Strong-typed provenance back to the engineering source.
    pub source: DrawingAnnotationSource,
}

impl DrawingAnnotation {
    /// Whether this annotation is fully valid (finite position, positive
    /// finite height, non-empty text without control characters).
    pub fn is_valid(&self) -> bool {
        self.position.x.is_finite()
            && self.position.y.is_finite()
            && self.text_height_mm.is_finite()
            && self.text_height_mm > 0.0
            && !self.text.is_empty()
            && !self.text.chars().any(char::is_control)
    }
}

/// Resolve the deterministic `PartTitle` annotation for a part, if any.
///
/// Returns `Ok(None)` when no title should be generated:
/// - `part` is `None` (a legitimate geometry-only drawing);
/// - the part name is empty or blank (an empty display name must not fail the
///   geometry drawing).
///
/// Returns `Err(SCC-DRW-008)` when the name contains control characters
/// (e.g. an interior newline) — malformed input is surfaced, never silently
/// altered or hidden.
///
/// Placement rule (ADR-025): the title sits inside the existing bottom sheet
/// margin, at `(DEFAULT_SHEET_MARGIN_MM, DEFAULT_SHEET_MARGIN_MM / 2)`,
/// anchored `Start`, so it can never move the geometry or change the sheet
/// envelope.
pub fn resolve_part_title(
    part: Option<&PartDefinition>,
) -> Result<Option<DrawingAnnotation>, DrawingError> {
    let Some(part) = part else {
        return Ok(None);
    };
    if part.name.trim().is_empty() {
        return Ok(None);
    }
    if part.name.chars().any(char::is_control) {
        return Err(DrawingError::InvalidDrawingAnnotation {
            reason: format!(
                "part {} name contains control characters; single-line titles only",
                part.id
            ),
        });
    }
    let margin = DrawingSheet::DEFAULT_SHEET_MARGIN_MM;
    Ok(Some(DrawingAnnotation {
        id: DrawingAnnotationId("part_title".into()),
        kind: DrawingAnnotationKind::PartTitle,
        text: part.name.clone(),
        position: DrawingPoint::new(margin, margin / 2.0),
        text_height_mm: DEFAULT_ANNOTATION_TEXT_HEIGHT_MM,
        anchor: DrawingTextAnchor::Start,
        source: DrawingAnnotationSource::PartTitle {
            part: part.id.clone(),
        },
    }))
}

impl crate::DrawingSheet {
    /// The annotation with the given id on this sheet, if present.
    pub fn annotation(&self, id: &DrawingAnnotationId) -> Option<&DrawingAnnotation> {
        self.annotations.get(id)
    }
}

impl crate::DrawingModel {
    /// The first annotation with the given id across all sheets.
    pub fn annotation(&self, id: &DrawingAnnotationId) -> Option<&DrawingAnnotation> {
        self.sheets.values().find_map(|sheet| sheet.annotation(id))
    }

    /// All annotations sourced from the given part, in deterministic order.
    ///
    /// Strong-typed provenance lookup — no string scanning.
    pub fn annotations_for_part(
        &self,
        part: &PartId,
    ) -> Vec<(&crate::DrawingSheetId, &DrawingAnnotation)> {
        self.sheets
            .iter()
            .flat_map(|(sheet_id, sheet)| {
                sheet
                    .annotations
                    .values()
                    .filter(move |a| {
                        matches!(
                            &a.source,
                            DrawingAnnotationSource::PartTitle { part: p } if p == part
                        )
                    })
                    .map(move |a| (sheet_id, a))
            })
            .collect()
    }
}

/// Validate the annotation integrity of a whole drawing model, fail-closed.
///
/// Checks every annotation on every sheet:
/// - structural validity ([`DrawingAnnotation::is_valid`]);
/// - at most one `PartTitle` per sheet;
/// - a `PartTitle`'s source part id and text match the supplied
///   `PartDefinition`, when one is provided.
///
/// Duplicate/conflicting annotation ids are structurally prevented by the
/// `BTreeMap` key. Violations return `SCC-DRW-008`.
pub fn validate_annotation_integrity(
    sheets: &BTreeMap<crate::DrawingSheetId, crate::DrawingSheet>,
    part: Option<&PartDefinition>,
) -> Result<(), DrawingError> {
    for (sheet_id, sheet) in sheets {
        let mut part_titles = 0usize;
        for (annotation_id, annotation) in &sheet.annotations {
            if !annotation.is_valid() {
                return Err(DrawingError::InvalidDrawingAnnotation {
                    reason: format!(
                        "annotation {} on sheet {} has invalid text, position, or text height",
                        annotation_id, sheet_id
                    ),
                });
            }
            if annotation.kind == DrawingAnnotationKind::PartTitle {
                part_titles += 1;
                if part_titles > 1 {
                    return Err(DrawingError::InvalidDrawingAnnotation {
                        reason: format!(
                            "sheet {} has more than one part-title annotation",
                            sheet_id
                        ),
                    });
                }
                // The only current source variant is `PartTitle`; when a new
                // `DrawingAnnotationSource` variant appears this destructuring
                // becomes a compile-time reminder to extend this check.
                let DrawingAnnotationSource::PartTitle { part: source_part } = &annotation.source;
                if let Some(part) = part {
                    if *source_part != part.id {
                        return Err(DrawingError::InvalidDrawingAnnotation {
                            reason: format!(
                                "part title {} sources part {} but the model defines {}",
                                annotation_id, source_part, part.id
                            ),
                        });
                    }
                    if annotation.text != part.name {
                        return Err(DrawingError::InvalidDrawingAnnotation {
                            reason: format!(
                                "part title text {:?} does not match the part name {:?}",
                                annotation.text, part.name
                            ),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}
