//! Tokens and source spans for the AICAD Engineering DSL lexer.

use serde::{Deserialize, Serialize};

/// A 1-based line/column position in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

impl Position {
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// A half-open source span `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: Position,
    pub end: Position,
}

impl SourceSpan {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

/// Canonical uppercase DSL keyword.
///
/// Keywords are written in **uppercase**; identifiers must be **lowercase**
/// `[a-z][a-z0-9_-]{0,63}`. This is the single, testable case policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Keyword {
    // document header / configuration
    Aicad,
    Version,
    Units,
    Origin,
    // statements
    Part,
    Param,
    BaseRect,
    BaseCircle,
    BaseRoundedRect,
    BasePolygon,
    Hole,
    HolePattern,
    Thickness,
    Extent,
    ConstructionCircle,
    Bind,
    Constraint,
    Drawing,
    // known-but-disabled v1 statements
    Fillet,
    // through slot cut (Phase 12.3)
    Slot,
    Angle,
    Thru,
    Blind,
    Opposite,
    // pattern subtypes
    Circular,
    Grid,
    Corner,
    // fields
    Width,
    Height,
    Center,
    Point,
    Dia,
    Pcd,
    Count,
    StartAngle,
    Rows,
    Columns,
    SpacingX,
    SpacingY,
    OffsetX,
    OffsetY,
    Base,
    // binding slot names
    CenterX,
    CenterY,
    Diameter,
    Radius,
    Length,
    // constraint kinds
    Symmetric,
    Aligned,
    Distance,
    Tangent,
    Fixed,
    Concentric,
    // drawing properties
    Scale,
    ShowCenterlines,
    ShowConstructionGeometry,
    ProjectionMethod,
    FirstAngle,
    ThirdAngle,
    // values
    Unresolved,
    True,
    False,
}

/// A lexical token kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TokenKind {
    Keyword(Keyword),
    Identifier(String),
    Number(f64),
    Str(String),
    /// `<-`
    Arrow,
    /// `=`
    Assign,
    /// `:=`
    ColonAssign,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Newline,
    Eof,
}

/// A token with its source span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SourceSpan,
}

impl Token {
    pub fn new(kind: TokenKind, span: SourceSpan) -> Self {
        Self { kind, span }
    }
}

/// The keyword table for the canonical uppercase keyword set.
pub(crate) fn keyword_from_str(word: &str) -> Option<Keyword> {
    Some(match word {
        "AICAD" => Keyword::Aicad,
        "VERSION" => Keyword::Version,
        "UNITS" => Keyword::Units,
        "ORIGIN" => Keyword::Origin,
        "PART" => Keyword::Part,
        "PARAM" => Keyword::Param,
        "BASE_RECT" => Keyword::BaseRect,
        "BASE_CIRCLE" => Keyword::BaseCircle,
        "BASE_ROUNDED_RECT" => Keyword::BaseRoundedRect,
        "BASE_POLYGON" => Keyword::BasePolygon,
        "HOLE" => Keyword::Hole,
        "HOLE_PATTERN" => Keyword::HolePattern,
        "THICKNESS" => Keyword::Thickness,
        "EXTENT" => Keyword::Extent,
        "CONSTRUCTION_CIRCLE" => Keyword::ConstructionCircle,
        "BIND" => Keyword::Bind,
        "CONSTRAINT" => Keyword::Constraint,
        "DRAWING" => Keyword::Drawing,
        "FILLET" => Keyword::Fillet,
        "SLOT" => Keyword::Slot,
        "ANGLE" => Keyword::Angle,
        "THRU" => Keyword::Thru,
        "BLIND" => Keyword::Blind,
        "OPPOSITE" => Keyword::Opposite,
        "CIRCULAR" => Keyword::Circular,
        "GRID" => Keyword::Grid,
        "CORNER" => Keyword::Corner,
        "WIDTH" => Keyword::Width,
        "HEIGHT" => Keyword::Height,
        "CENTER" => Keyword::Center,
        "POINT" => Keyword::Point,
        "DIA" => Keyword::Dia,
        "PCD" => Keyword::Pcd,
        "COUNT" => Keyword::Count,
        "START_ANGLE" => Keyword::StartAngle,
        "ROWS" => Keyword::Rows,
        "COLUMNS" => Keyword::Columns,
        "SPACING_X" => Keyword::SpacingX,
        "SPACING_Y" => Keyword::SpacingY,
        "OFFSET_X" => Keyword::OffsetX,
        "OFFSET_Y" => Keyword::OffsetY,
        "BASE" => Keyword::Base,
        "CENTER_X" => Keyword::CenterX,
        "CENTER_Y" => Keyword::CenterY,
        "DIAMETER" => Keyword::Diameter,
        "RADIUS" => Keyword::Radius,
        "LENGTH" => Keyword::Length,
        "SYMMETRIC" => Keyword::Symmetric,
        "ALIGNED" => Keyword::Aligned,
        "DISTANCE" => Keyword::Distance,
        "TANGENT" => Keyword::Tangent,
        "FIXED" => Keyword::Fixed,
        "CONCENTRIC" => Keyword::Concentric,
        "SCALE" => Keyword::Scale,
        "SHOW_CENTERLINES" => Keyword::ShowCenterlines,
        "SHOW_CONSTRUCTION_GEOMETRY" => Keyword::ShowConstructionGeometry,
        "PROJECTION_METHOD" => Keyword::ProjectionMethod,
        "FIRST_ANGLE" => Keyword::FirstAngle,
        "THIRD_ANGLE" => Keyword::ThirdAngle,
        "UNRESOLVED" => Keyword::Unresolved,
        "TRUE" => Keyword::True,
        "FALSE" => Keyword::False,
        _ => return None,
    })
}
