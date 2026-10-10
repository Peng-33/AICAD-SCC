//! The AICAD Engineering DSL abstract syntax tree.
//!
//! The AST is the syntax structure a user (or LLM) writes down. It is **not**
//! the engineering truth — [`scc_engineering::model::EngineeringModel`] is.
//! The DSL pipeline never passes the AST to a model directly; the
//! [`crate::command_builder`] converts it into `EngineeringCommand` values
//! first (ADR-018 Decision 1/2).

use serde::{Deserialize, Serialize};

use crate::token::SourceSpan;

/// A parsed DSL document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslDocument {
    pub version: u32,
    pub unit: DslUnit,
    pub origin: DslOrigin,
    pub statements: Vec<DslStatement>,
}

/// Supported unit. v1 only supports millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DslUnit {
    Mm,
}

/// Supported origin. v1 only supports part-center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DslOrigin {
    Center,
}

/// One top-level DSL statement, with its source span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslStatement {
    pub kind: DslStatementKind,
    pub span: SourceSpan,
}

/// The kind of a top-level statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DslStatementKind {
    Part(DslPart),
    Parameter(DslParameter),
    BaseRect(DslBaseRect),
    BaseCircle(DslBaseCircle),
    BaseRoundedRect(DslBaseRoundedRect),
    BasePolygon(DslBasePolygon),
    Hole(DslHole),
    Slot(DslSlotFeature),
    HolePattern(DslHolePattern),
    Thickness(DslThickness),
    HoleExtent(DslHoleExtent),
    ConstructionCircle(DslConstructionCircle),
    Binding(DslBinding),
    Constraint(DslConstraint),
    Drawing(DslDrawing),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslPart {
    pub id: String,
    pub display_name: Option<String>,
}

/// A parameter value: a direct number, `UNRESOLVED`, or a derived expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DslParameterValue {
    Direct(f64),
    Unresolved,
    Expression(DslExpression),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslParameter {
    pub id: String,
    pub value: DslParameterValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslBaseRect {
    pub id: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslBaseCircle {
    pub id: String,
    pub center: (f64, f64),
    pub diameter: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslBaseRoundedRect {
    pub id: String,
    pub center: (f64, f64),
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslBasePolygon {
    pub id: String,
    pub vertices: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslHole {
    pub id: String,
    pub center: Option<(f64, f64)>,
    pub diameter: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslSlotFeature {
    pub id: String,
    pub center: (f64, f64),
    pub length: f64,
    pub width: f64,
    pub angle: f64,
}

/// Which hole-pattern subtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DslPatternKind {
    Circular,
    Grid,
    Corner,
}

/// All fields accepted across pattern subtypes; a subtype's parser only fills
/// the fields that are valid for it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DslPatternFields {
    pub center: Option<(f64, f64)>,
    pub diameter: Option<f64>,
    pub pcd: Option<f64>,
    pub count: Option<f64>,
    pub start_angle: Option<f64>,
    pub rows: Option<f64>,
    pub columns: Option<f64>,
    pub spacing_x: Option<f64>,
    pub spacing_y: Option<f64>,
    pub offset_x: Option<f64>,
    pub offset_y: Option<f64>,
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslHolePattern {
    pub id: String,
    pub kind: DslPatternKind,
    pub fields: DslPatternFields,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslThickness {
    pub value: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslHoleExtent {
    pub target: String,
    pub extent: DslHoleExtentKind,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DslHoleExtentKind {
    ThroughAll,
    BlindBase(f64),
    BlindOpposite(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslConstructionCircle {
    pub id: String,
    pub center: Option<(f64, f64)>,
    pub diameter: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslBinding {
    pub feature: String,
    pub slot: DslSlot,
    pub parameter: String,
}

/// The typed feature field a binding drives. Mirrors
/// [`scc_engineering::binding::FeatureParameterSlot`] one-to-one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DslSlot {
    Width,
    Height,
    CenterX,
    CenterY,
    Diameter,
    Radius,
    Length,
    Orientation,
    Count,
    PitchCircleDiameter,
    StartAngle,
    Rows,
    Columns,
    SpacingX,
    SpacingY,
    OffsetX,
    OffsetY,
}

impl DslSlot {
    /// Map to the strong-typed engineering slot (never a string lookup).
    pub fn to_engineering(self) -> scc_engineering::binding::FeatureParameterSlot {
        use scc_engineering::binding::FeatureParameterSlot as S;
        match self {
            DslSlot::Width => S::Width,
            DslSlot::Height => S::Height,
            DslSlot::CenterX => S::CenterX,
            DslSlot::CenterY => S::CenterY,
            DslSlot::Diameter => S::Diameter,
            DslSlot::Radius => S::Radius,
            DslSlot::Length => S::Length,
            DslSlot::Orientation => S::Orientation,
            DslSlot::Count => S::Count,
            DslSlot::PitchCircleDiameter => S::PitchCircleDiameter,
            DslSlot::StartAngle => S::StartAngle,
            DslSlot::Rows => S::Rows,
            DslSlot::Columns => S::Columns,
            DslSlot::SpacingX => S::SpacingX,
            DslSlot::SpacingY => S::SpacingY,
            DslSlot::OffsetX => S::OffsetX,
            DslSlot::OffsetY => S::OffsetY,
        }
    }
}

/// Constraint kind in the DSL surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DslConstraintKind {
    Symmetric,
    Aligned,
    Distance,
    Tangent,
    Fixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DslConstraint {
    pub id: String,
    pub kind: DslConstraintKind,
    pub subject: String,
    pub reference: String,
    pub value: Option<f64>,
}

/// Drawing property in the DSL surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DslDrawing {
    Scale(f64),
    ShowCenterlines(bool),
    ShowConstructionGeometry(bool),
    ProjectionMethod(DslProjectionMethod),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DslProjectionMethod {
    FirstAngle,
    ThirdAngle,
}

/// Binary expression operator (Phase 2 AST subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DslBinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// A DSL expression tree. Converted to the Phase 2
/// [`scc_engineering::expression::Expression`] by the command builder; never
/// eval'd from a string (ADR-018 Decision 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DslExpression {
    Number(f64),
    Reference(String),
    Binary {
        left: Box<DslExpression>,
        op: DslBinaryOp,
        right: Box<DslExpression>,
    },
}

impl DslExpression {
    /// The parameter ids referenced by this expression.
    pub fn referenced_parameters(&self) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut std::collections::BTreeSet<String>) {
        match self {
            DslExpression::Number(_) => {}
            DslExpression::Reference(id) => {
                out.insert(id.clone());
            }
            DslExpression::Binary { left, right, .. } => {
                left.collect(out);
                right.collect(out);
            }
        }
    }
}
