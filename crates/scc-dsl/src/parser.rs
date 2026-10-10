//! Hand-written recursive-descent parser for the AICAD Engineering DSL.
//!
//! The parser is pure and deterministic: it produces a [`DslDocument`] and
//! never touches an [`scc_engineering::model::EngineeringModel`]
//! (ADR-018 Decision 2). Blocks are keyword-delimited — the next top-level
//! keyword ends the current block; there is no `{`/`}` or `END`.

use std::collections::BTreeSet;

use crate::ast::*;
use crate::error::DslError;
use crate::lexer::lex;
use crate::token::{Keyword, SourceSpan, Token, TokenKind};

/// Parse a DSL source document into its AST.
pub fn parse_document(source: &str) -> Result<DslDocument, DslError> {
    let tokens = lex(source)?;
    Parser {
        tokens: &tokens,
        pos: 0,
    }
    .parse()
}

/// Parse a standalone expression (`"plate_width / 8"`) into a DSL expression
/// AST, reusing the same grammar as `PARAM x := ...` (Phase 5 tool calling
/// reuses this parser rather than eval'ing text).
pub fn parse_expression(source: &str) -> Result<DslExpression, DslError> {
    let tokens = lex(source)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
    };
    parser.skip_newlines();
    let expression = parser.parse_expression()?;
    parser.skip_newlines();
    if !parser.at_eof() {
        return Err(parser.unexpected("unexpected tokens after the expression"));
    }
    Ok(expression)
}

/// Whether a keyword starts a new top-level statement (and therefore ends the
/// current block).
fn is_top_level(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::Aicad
            | Keyword::Version
            | Keyword::Units
            | Keyword::Origin
            | Keyword::Part
            | Keyword::Param
            | Keyword::BaseRect
            | Keyword::BaseCircle
            | Keyword::BaseRoundedRect
            | Keyword::BasePolygon
            | Keyword::Hole
            | Keyword::HolePattern
            | Keyword::Thickness
            | Keyword::Extent
            | Keyword::ConstructionCircle
            | Keyword::Bind
            | Keyword::Constraint
            | Keyword::Drawing
            | Keyword::Fillet
            | Keyword::Slot
    )
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        Some(&self.peek().kind)
    }

    fn peek_keyword(&self) -> Option<Keyword> {
        match self.peek_kind() {
            Some(TokenKind::Keyword(kw)) => Some(*kw),
            _ => None,
        }
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        token
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Eof))
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), Some(TokenKind::Newline)) {
            self.advance();
        }
    }

    fn unexpected(&self, message: &str) -> DslError {
        let token = self.peek();
        match &token.kind {
            TokenKind::Eof => DslError::UnexpectedEof {
                message: message.into(),
            },
            _ => DslError::UnexpectedToken {
                message: format!("{} (found {})", message, describe(&token.kind)),
                line: token.span.start.line,
                column: token.span.start.column,
            },
        }
    }

    fn expect_keyword(&mut self, kw: Keyword) -> Result<SourceSpan, DslError> {
        let token = self.peek();
        if let TokenKind::Keyword(actual) = &token.kind {
            if *actual == kw {
                let span = token.span;
                self.advance();
                return Ok(span);
            }
        }
        Err(self.unexpected(&format!("expected keyword {:?}", kw)))
    }

    fn expect_identifier(&mut self) -> Result<(String, SourceSpan), DslError> {
        let token = self.peek();
        if let TokenKind::Identifier(id) = &token.kind {
            let result = (id.clone(), token.span);
            self.advance();
            return Ok(result);
        }
        Err(self.unexpected("expected an identifier"))
    }

    /// A field/expression number: an optional leading `-` followed by a number.
    fn expect_number(&mut self) -> Result<(f64, SourceSpan), DslError> {
        let token = self.peek();
        if let TokenKind::Minus = token.kind {
            let span = token.span;
            self.advance();
            let (value, _) = self.expect_plain_number()?;
            return Ok((-value, span));
        }
        self.expect_plain_number()
    }

    fn expect_plain_number(&mut self) -> Result<(f64, SourceSpan), DslError> {
        let token = self.peek();
        if let TokenKind::Number(value) = token.kind {
            let span = token.span;
            self.advance();
            return Ok((value, span));
        }
        Err(self.unexpected("expected a number"))
    }

    fn expect_arrow(&mut self) -> Result<(), DslError> {
        match self.peek_kind() {
            Some(TokenKind::Arrow) => {
                self.advance();
                Ok(())
            }
            _ => Err(self.unexpected("expected `<-`")),
        }
    }

    /// After a single-line statement, require a line break or EOF.
    fn expect_line_end(&self) -> Result<(), DslError> {
        match self.peek_kind() {
            Some(TokenKind::Newline) | Some(TokenKind::Eof) => Ok(()),
            _ => Err(self.unexpected("expected the end of the line")),
        }
    }

    fn parse(&mut self) -> Result<DslDocument, DslError> {
        self.skip_newlines();
        let (version, header_span) = self.parse_header()?;
        if version != 1 {
            return Err(DslError::UnsupportedVersion {
                version,
                line: header_span.start.line,
                column: header_span.start.column,
            });
        }

        let unit = DslUnit::Mm;
        let origin = DslOrigin::Center;
        let mut statements = Vec::new();

        self.skip_newlines();
        while !self.at_eof() {
            let Some(kw) = self.peek_keyword() else {
                return Err(self.unexpected("expected a statement keyword"));
            };
            match kw {
                Keyword::Units => {
                    self.parse_units()?;
                }
                Keyword::Origin => {
                    self.parse_origin()?;
                }
                Keyword::Fillet => {
                    let token = self.advance();
                    return Err(DslError::UnsupportedDslFeature {
                        feature: format!("{:?}", kw),
                        line: token.span.start.line,
                        column: token.span.start.column,
                    });
                }
                _ => {
                    statements.push(self.parse_statement()?);
                }
            }
            self.skip_newlines();
        }

        Ok(DslDocument {
            version,
            unit,
            origin,
            statements,
        })
    }

    fn parse_header(&mut self) -> Result<(u32, SourceSpan), DslError> {
        let token = self.peek();
        let (version, span) = match &token.kind {
            TokenKind::Keyword(Keyword::Aicad) | TokenKind::Keyword(Keyword::Version) => {
                let kw_span = token.span;
                self.advance();
                let (value, value_span) = self.expect_plain_number()?;
                (value as u32, SourceSpan::new(kw_span.start, value_span.end))
            }
            _ => {
                return Err(DslError::MissingHeader {
                    line: token.span.start.line,
                    column: token.span.start.column,
                });
            }
        };
        self.expect_line_end()?;
        Ok((version, span))
    }

    fn parse_units(&mut self) -> Result<(), DslError> {
        self.expect_keyword(Keyword::Units)?;
        let token = self.peek();
        match &token.kind {
            TokenKind::Identifier(name) if name.eq_ignore_ascii_case("mm") => {}
            TokenKind::Keyword(kw) => {
                return Err(DslError::UnsupportedUnit {
                    unit: format!("{:?}", kw),
                    line: token.span.start.line,
                    column: token.span.start.column,
                });
            }
            TokenKind::Identifier(name) => {
                return Err(DslError::UnsupportedUnit {
                    unit: name.clone(),
                    line: token.span.start.line,
                    column: token.span.start.column,
                });
            }
            _ => return Err(self.unexpected("expected a unit")),
        }
        self.advance();
        self.expect_line_end()
    }

    fn parse_origin(&mut self) -> Result<(), DslError> {
        self.expect_keyword(Keyword::Origin)?;
        let token = self.peek();
        match &token.kind {
            TokenKind::Keyword(Keyword::Center) => {
                self.advance();
                self.expect_line_end()
            }
            _ => Err(self.unexpected("expected ORIGIN CENTER")),
        }
    }

    fn parse_statement(&mut self) -> Result<DslStatement, DslError> {
        let start = self.peek().span.start;
        let Some(kw) = self.peek_keyword() else {
            return Err(self.unexpected("expected a statement keyword"));
        };
        let kind = match kw {
            Keyword::Part => DslStatementKind::Part(self.parse_part()?),
            Keyword::Param => DslStatementKind::Parameter(self.parse_param()?),
            Keyword::BaseRect => DslStatementKind::BaseRect(self.parse_base_rect()?),
            Keyword::BaseCircle => DslStatementKind::BaseCircle(self.parse_base_circle()?),
            Keyword::BaseRoundedRect => {
                DslStatementKind::BaseRoundedRect(self.parse_base_rounded_rect()?)
            }
            Keyword::BasePolygon => DslStatementKind::BasePolygon(self.parse_base_polygon()?),
            Keyword::Hole => DslStatementKind::Hole(self.parse_hole()?),
            Keyword::Slot => DslStatementKind::Slot(self.parse_slot()?),
            Keyword::Thickness => DslStatementKind::Thickness(self.parse_thickness()?),
            Keyword::Extent => DslStatementKind::HoleExtent(self.parse_hole_extent()?),
            Keyword::HolePattern => DslStatementKind::HolePattern(self.parse_hole_pattern()?),
            Keyword::ConstructionCircle => {
                DslStatementKind::ConstructionCircle(self.parse_construction_circle()?)
            }
            Keyword::Bind => DslStatementKind::Binding(self.parse_bind()?),
            Keyword::Constraint => DslStatementKind::Constraint(self.parse_constraint()?),
            Keyword::Drawing => DslStatementKind::Drawing(self.parse_drawing()?),
            _ => return Err(self.unexpected("unknown statement keyword")),
        };
        let end = self.peek().span.end;
        Ok(DslStatement {
            kind,
            span: SourceSpan::new(start, end),
        })
    }

    fn parse_part(&mut self) -> Result<DslPart, DslError> {
        self.expect_keyword(Keyword::Part)?;
        let (id, _) = self.expect_identifier()?;
        let display_name = match self.peek_kind() {
            Some(TokenKind::Str(_)) => {
                let token = self.advance();
                match token.kind {
                    TokenKind::Str(s) => Some(s),
                    _ => unreachable!(),
                }
            }
            _ => None,
        };
        self.expect_line_end()?;
        Ok(DslPart { id, display_name })
    }

    fn parse_param(&mut self) -> Result<DslParameter, DslError> {
        self.expect_keyword(Keyword::Param)?;
        let (id, _) = self.expect_identifier()?;
        let value = match self.peek_kind() {
            Some(TokenKind::Assign) => {
                self.advance();
                if matches!(
                    self.peek_kind(),
                    Some(TokenKind::Keyword(Keyword::Unresolved))
                ) {
                    self.advance();
                    DslParameterValue::Unresolved
                } else {
                    let (number, _) = self.expect_number()?;
                    DslParameterValue::Direct(number)
                }
            }
            Some(TokenKind::ColonAssign) => {
                self.advance();
                let expression = self.parse_expression()?;
                DslParameterValue::Expression(expression)
            }
            _ => return Err(self.unexpected("expected `=` or `:=`")),
        };
        self.expect_line_end()?;
        Ok(DslParameter { id, value })
    }

    /// Parse the field block of a feature statement.
    ///
    /// Returns when the next top-level keyword or EOF is reached. `seen` tracks
    /// already-consumed field names for duplicate rejection. Each field keyword
    /// is consumed here; the handler only parses the field's values.
    fn parse_field_block(
        &mut self,
        seen: &mut BTreeSet<&'static str>,
        mut on_field: impl FnMut(
            &mut Self,
            Keyword,
            &mut BTreeSet<&'static str>,
            u32,
            u32,
        ) -> Result<(), DslError>,
    ) -> Result<(), DslError> {
        loop {
            self.skip_newlines();
            if self.at_eof() {
                return Ok(());
            }
            let Some(kw) = self.peek_keyword() else {
                return Err(self.unexpected("expected a field keyword or the next statement"));
            };
            if is_top_level(kw) {
                return Ok(());
            }
            let token = self.advance();
            let span = token.span;
            on_field(self, kw, seen, span.start.line, span.start.column)?;
        }
    }

    fn parse_base_rect(&mut self) -> Result<DslBaseRect, DslError> {
        self.expect_keyword(Keyword::BaseRect)?;
        let (id, _) = self.expect_identifier()?;
        let mut width: Option<f64> = None;
        let mut height: Option<f64> = None;
        let mut seen = BTreeSet::new();
        self.parse_field_block(&mut seen, |parser, kw, seen, line, column| {
            match kw {
                Keyword::Width => {
                    reject_duplicate(seen, "WIDTH", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    width = Some(v);
                }
                Keyword::Height => {
                    reject_duplicate(seen, "HEIGHT", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    height = Some(v);
                }
                _ => return Err(invalid_field("BASE_RECT", kw, line, column)),
            }
            Ok(())
        })?;
        let width = width.ok_or_else(|| missing_field("BASE_RECT", &id, "WIDTH", self.peek()))?;
        let height =
            height.ok_or_else(|| missing_field("BASE_RECT", &id, "HEIGHT", self.peek()))?;
        Ok(DslBaseRect { id, width, height })
    }

    fn parse_base_circle(&mut self) -> Result<DslBaseCircle, DslError> {
        self.expect_keyword(Keyword::BaseCircle)?;
        let (id, _) = self.expect_identifier()?;
        let mut center: Option<(f64, f64)> = None;
        let mut diameter: Option<f64> = None;
        let mut seen = BTreeSet::new();
        self.parse_field_block(&mut seen, |parser, kw, seen, line, column| {
            match kw {
                Keyword::Center => {
                    reject_duplicate(seen, "CENTER", line, column)?;
                    let (x, _) = parser.expect_number()?;
                    let (y, _) = parser.expect_number()?;
                    center = Some((x, y));
                }
                Keyword::Dia => {
                    reject_duplicate(seen, "DIA", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    diameter = Some(v);
                }
                _ => return Err(invalid_field("BASE_CIRCLE", kw, line, column)),
            }
            Ok(())
        })?;
        let center =
            center.ok_or_else(|| missing_field("BASE_CIRCLE", &id, "CENTER", self.peek()))?;
        let diameter =
            diameter.ok_or_else(|| missing_field("BASE_CIRCLE", &id, "DIA", self.peek()))?;
        Ok(DslBaseCircle {
            id,
            center,
            diameter,
        })
    }

    fn parse_base_rounded_rect(&mut self) -> Result<DslBaseRoundedRect, DslError> {
        self.expect_keyword(Keyword::BaseRoundedRect)?;
        let (id, _) = self.expect_identifier()?;
        let mut center = None;
        let mut width = None;
        let mut height = None;
        let mut radius = None;
        let mut seen = BTreeSet::new();
        self.parse_field_block(&mut seen, |parser, kw, seen, line, column| {
            match kw {
                Keyword::Center => {
                    reject_duplicate(seen, "CENTER", line, column)?;
                    let (x, _) = parser.expect_number()?;
                    let (y, _) = parser.expect_number()?;
                    center = Some((x, y));
                }
                Keyword::Width => {
                    reject_duplicate(seen, "WIDTH", line, column)?;
                    width = Some(parser.expect_number()?.0);
                }
                Keyword::Height => {
                    reject_duplicate(seen, "HEIGHT", line, column)?;
                    height = Some(parser.expect_number()?.0);
                }
                Keyword::Radius => {
                    reject_duplicate(seen, "RADIUS", line, column)?;
                    radius = Some(parser.expect_number()?.0);
                }
                _ => return Err(invalid_field("BASE_ROUNDED_RECT", kw, line, column)),
            }
            Ok(())
        })?;
        Ok(DslBaseRoundedRect {
            id: id.clone(),
            center: center
                .ok_or_else(|| missing_field("BASE_ROUNDED_RECT", &id, "CENTER", self.peek()))?,
            width: width
                .ok_or_else(|| missing_field("BASE_ROUNDED_RECT", &id, "WIDTH", self.peek()))?,
            height: height
                .ok_or_else(|| missing_field("BASE_ROUNDED_RECT", &id, "HEIGHT", self.peek()))?,
            radius: radius
                .ok_or_else(|| missing_field("BASE_ROUNDED_RECT", &id, "RADIUS", self.peek()))?,
        })
    }

    fn parse_base_polygon(&mut self) -> Result<DslBasePolygon, DslError> {
        self.expect_keyword(Keyword::BasePolygon)?;
        let (id, _) = self.expect_identifier()?;
        let mut vertices = Vec::new();
        let mut seen = BTreeSet::new();
        self.parse_field_block(&mut seen, |parser, kw, _seen, line, column| {
            match kw {
                Keyword::Point => {
                    let (x, _) = parser.expect_number()?;
                    let (y, _) = parser.expect_number()?;
                    vertices.push((x, y));
                }
                _ => return Err(invalid_field("BASE_POLYGON", kw, line, column)),
            }
            Ok(())
        })?;
        if vertices.len() < 3 {
            return Err(DslError::MissingRequiredField {
                feature: format!("BASE_POLYGON {id}"),
                missing: "at least three POINT fields".into(),
                line: self.peek().span.start.line,
                column: self.peek().span.start.column,
            });
        }
        Ok(DslBasePolygon { id, vertices })
    }

    fn parse_hole(&mut self) -> Result<DslHole, DslError> {
        self.expect_keyword(Keyword::Hole)?;
        let (id, _) = self.expect_identifier()?;
        let mut center: Option<(f64, f64)> = None;
        let mut diameter: Option<f64> = None;
        let mut seen = BTreeSet::new();
        self.parse_field_block(&mut seen, |parser, kw, seen, line, column| {
            match kw {
                Keyword::Dia => {
                    reject_duplicate(seen, "DIA", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    diameter = Some(v);
                }
                Keyword::Center => {
                    reject_duplicate(seen, "CENTER", line, column)?;
                    let (x, _) = parser.expect_number()?;
                    let (y, _) = parser.expect_number()?;
                    center = Some((x, y));
                }
                _ => return Err(invalid_field("HOLE", kw, line, column)),
            }
            Ok(())
        })?;
        let diameter = diameter.ok_or_else(|| missing_field("HOLE", &id, "DIA", self.peek()))?;
        Ok(DslHole {
            id,
            center,
            diameter,
        })
    }
    fn parse_thickness(&mut self) -> Result<DslThickness, DslError> {
        self.expect_keyword(Keyword::Thickness)?;
        let (value, _) = self.expect_number()?;
        self.expect_line_end()?;
        Ok(DslThickness { value })
    }
    fn parse_hole_extent(&mut self) -> Result<DslHoleExtent, DslError> {
        self.expect_keyword(Keyword::Extent)?;
        let (target, _) = self.expect_identifier()?;
        let extent = match self.peek_keyword() {
            Some(Keyword::Thru) => {
                self.advance();
                DslHoleExtentKind::ThroughAll
            }
            Some(Keyword::Blind) => {
                self.advance();
                match self.peek_keyword() {
                    Some(Keyword::Base) => {
                        self.advance();
                        DslHoleExtentKind::BlindBase(self.expect_number()?.0)
                    }
                    Some(Keyword::Opposite) => {
                        self.advance();
                        DslHoleExtentKind::BlindOpposite(self.expect_number()?.0)
                    }
                    _ => return Err(self.unexpected("expected BASE or OPPOSITE after BLIND")),
                }
            }
            _ => return Err(self.unexpected("expected THRU or BLIND after hole feature id")),
        };
        self.expect_line_end()?;
        Ok(DslHoleExtent { target, extent })
    }

    fn parse_slot(&mut self) -> Result<DslSlotFeature, DslError> {
        self.expect_keyword(Keyword::Slot)?;
        let (id, _) = self.expect_identifier()?;
        let mut center = None;
        let mut length = None;
        let mut width = None;
        let mut angle = None;
        let mut seen = BTreeSet::new();
        self.parse_field_block(&mut seen, |parser, kw, seen, line, column| {
            match kw {
                Keyword::Center => {
                    reject_duplicate(seen, "CENTER", line, column)?;
                    let (x, _) = parser.expect_number()?;
                    let (y, _) = parser.expect_number()?;
                    center = Some((x, y));
                }
                Keyword::Length => {
                    reject_duplicate(seen, "LENGTH", line, column)?;
                    length = Some(parser.expect_number()?.0);
                }
                Keyword::Width => {
                    reject_duplicate(seen, "WIDTH", line, column)?;
                    width = Some(parser.expect_number()?.0);
                }
                Keyword::Angle => {
                    reject_duplicate(seen, "ANGLE", line, column)?;
                    angle = Some(parser.expect_number()?.0);
                }
                Keyword::Thru => {
                    reject_duplicate(seen, "THRU", line, column)?;
                }
                _ => return Err(invalid_field("SLOT", kw, line, column)),
            }
            Ok(())
        })?;
        Ok(DslSlotFeature {
            id: id.clone(),
            center: center.ok_or_else(|| missing_field("SLOT", &id, "CENTER", self.peek()))?,
            length: length.ok_or_else(|| missing_field("SLOT", &id, "LENGTH", self.peek()))?,
            width: width.ok_or_else(|| missing_field("SLOT", &id, "WIDTH", self.peek()))?,
            angle: angle.unwrap_or(0.0),
        })
    }

    fn parse_hole_pattern(&mut self) -> Result<DslHolePattern, DslError> {
        self.expect_keyword(Keyword::HolePattern)?;
        let (id, _) = self.expect_identifier()?;
        let token = self.peek();
        let kind = match &token.kind {
            TokenKind::Keyword(Keyword::Circular) => {
                self.advance();
                DslPatternKind::Circular
            }
            TokenKind::Keyword(Keyword::Grid) => {
                self.advance();
                DslPatternKind::Grid
            }
            TokenKind::Keyword(Keyword::Corner) => {
                self.advance();
                DslPatternKind::Corner
            }
            _ => return Err(self.unexpected("expected CIRCULAR, GRID, or CORNER")),
        };
        let mut fields = DslPatternFields::default();
        let mut seen = BTreeSet::new();
        self.parse_field_block(&mut seen, |parser, kw, seen, line, column| {
            match kw {
                Keyword::Center => {
                    reject_duplicate(seen, "CENTER", line, column)?;
                    let (x, _) = parser.expect_number()?;
                    let (y, _) = parser.expect_number()?;
                    fields.center = Some((x, y));
                }
                Keyword::Dia => {
                    reject_duplicate(seen, "DIA", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.diameter = Some(v);
                }
                Keyword::Pcd => {
                    reject_duplicate(seen, "PCD", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.pcd = Some(v);
                }
                Keyword::Count => {
                    reject_duplicate(seen, "COUNT", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.count = Some(v);
                }
                Keyword::StartAngle => {
                    reject_duplicate(seen, "START_ANGLE", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.start_angle = Some(v);
                }
                Keyword::Rows => {
                    reject_duplicate(seen, "ROWS", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.rows = Some(v);
                }
                Keyword::Columns => {
                    reject_duplicate(seen, "COLUMNS", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.columns = Some(v);
                }
                Keyword::SpacingX => {
                    reject_duplicate(seen, "SPACING_X", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.spacing_x = Some(v);
                }
                Keyword::SpacingY => {
                    reject_duplicate(seen, "SPACING_Y", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.spacing_y = Some(v);
                }
                Keyword::OffsetX => {
                    reject_duplicate(seen, "OFFSET_X", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.offset_x = Some(v);
                }
                Keyword::OffsetY => {
                    reject_duplicate(seen, "OFFSET_Y", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    fields.offset_y = Some(v);
                }
                Keyword::Base => {
                    reject_duplicate(seen, "BASE", line, column)?;
                    let (base, _) = parser.expect_identifier()?;
                    fields.base = Some(base);
                }
                _ => return Err(invalid_field("HOLE_PATTERN", kw, line, column)),
            }
            Ok(())
        })?;

        // Per-subtype required/valid field checks.
        let type_name = match kind {
            DslPatternKind::Circular => "CIRCULAR",
            DslPatternKind::Grid => "GRID",
            DslPatternKind::Corner => "CORNER",
        };
        match kind {
            DslPatternKind::Circular => {
                for (name, present) in [
                    ("PCD", fields.pcd.is_some()),
                    ("COUNT", fields.count.is_some()),
                    ("DIA", fields.diameter.is_some()),
                ] {
                    if !present {
                        return Err(missing_field(
                            &format!("HOLE_PATTERN {} {}", id, type_name),
                            &id,
                            name,
                            self.peek(),
                        ));
                    }
                }
            }
            DslPatternKind::Grid => {
                for (name, present) in [
                    ("ROWS", fields.rows.is_some()),
                    ("COLUMNS", fields.columns.is_some()),
                    ("SPACING_X", fields.spacing_x.is_some()),
                    ("SPACING_Y", fields.spacing_y.is_some()),
                    ("DIA", fields.diameter.is_some()),
                ] {
                    if !present {
                        return Err(missing_field(
                            &format!("HOLE_PATTERN {} {}", id, type_name),
                            &id,
                            name,
                            self.peek(),
                        ));
                    }
                }
            }
            DslPatternKind::Corner => {
                for (name, present) in [
                    ("OFFSET_X", fields.offset_x.is_some()),
                    ("OFFSET_Y", fields.offset_y.is_some()),
                    ("DIA", fields.diameter.is_some()),
                ] {
                    if !present {
                        return Err(missing_field(
                            &format!("HOLE_PATTERN {} {}", id, type_name),
                            &id,
                            name,
                            self.peek(),
                        ));
                    }
                }
            }
        }
        Ok(DslHolePattern { id, kind, fields })
    }

    fn parse_construction_circle(&mut self) -> Result<DslConstructionCircle, DslError> {
        self.expect_keyword(Keyword::ConstructionCircle)?;
        let (id, _) = self.expect_identifier()?;
        let mut center: Option<(f64, f64)> = None;
        let mut diameter: Option<f64> = None;
        let mut seen = BTreeSet::new();
        self.parse_field_block(&mut seen, |parser, kw, seen, line, column| {
            match kw {
                Keyword::Dia => {
                    reject_duplicate(seen, "DIA", line, column)?;
                    let (v, _) = parser.expect_number()?;
                    diameter = Some(v);
                }
                Keyword::Center => {
                    reject_duplicate(seen, "CENTER", line, column)?;
                    let (x, _) = parser.expect_number()?;
                    let (y, _) = parser.expect_number()?;
                    center = Some((x, y));
                }
                _ => return Err(invalid_field("CONSTRUCTION_CIRCLE", kw, line, column)),
            }
            Ok(())
        })?;
        let diameter = diameter
            .ok_or_else(|| missing_field("CONSTRUCTION_CIRCLE", &id, "DIA", self.peek()))?;
        Ok(DslConstructionCircle {
            id,
            center,
            diameter,
        })
    }

    fn parse_bind(&mut self) -> Result<DslBinding, DslError> {
        self.expect_keyword(Keyword::Bind)?;
        let (feature, _) = self.expect_identifier()?;
        let token = self.peek();
        let slot = match &token.kind {
            TokenKind::Keyword(kw) => match slot_from_keyword(*kw) {
                Some(slot) => {
                    self.advance();
                    slot
                }
                None => return Err(self.unexpected("expected a binding slot keyword")),
            },
            _ => return Err(self.unexpected("expected a binding slot keyword")),
        };
        self.expect_arrow()?;
        let (parameter, _) = self.expect_identifier()?;
        self.expect_line_end()?;
        Ok(DslBinding {
            feature,
            slot,
            parameter,
        })
    }

    fn parse_constraint(&mut self) -> Result<DslConstraint, DslError> {
        self.expect_keyword(Keyword::Constraint)?;
        let (id, _) = self.expect_identifier()?;
        let token = self.peek();
        let (kind, arity) = match &token.kind {
            TokenKind::Keyword(Keyword::Symmetric) => {
                self.advance();
                (DslConstraintKind::Symmetric, 2)
            }
            TokenKind::Keyword(Keyword::Aligned) => {
                self.advance();
                (DslConstraintKind::Aligned, 2)
            }
            TokenKind::Keyword(Keyword::Distance) => {
                self.advance();
                (DslConstraintKind::Distance, 3)
            }
            TokenKind::Keyword(Keyword::Tangent) => {
                self.advance();
                (DslConstraintKind::Tangent, 2)
            }
            TokenKind::Keyword(Keyword::Fixed) => {
                self.advance();
                (DslConstraintKind::Fixed, 1)
            }
            TokenKind::Keyword(Keyword::Concentric) => {
                let t = self.advance();
                return Err(DslError::UnsupportedDslFeature {
                    feature: "CONCENTRIC".into(),
                    line: t.span.start.line,
                    column: t.span.start.column,
                });
            }
            _ => return Err(self.unexpected("expected a constraint kind")),
        };

        let (subject, _) = self.expect_identifier()?;
        let (reference, _) = if arity >= 2 {
            self.expect_identifier()?
        } else {
            (subject.clone(), self.peek().span)
        };
        let value = if arity >= 3 {
            let (v, _) = self.expect_number()?;
            Some(v)
        } else {
            None
        };
        self.expect_line_end()?;
        Ok(DslConstraint {
            id,
            kind,
            subject,
            reference,
            value,
        })
    }

    fn parse_drawing(&mut self) -> Result<DslDrawing, DslError> {
        self.expect_keyword(Keyword::Drawing)?;
        let token = self.peek();
        let result = match &token.kind {
            TokenKind::Keyword(Keyword::Scale) => {
                self.advance();
                let (v, _) = self.expect_number()?;
                DslDrawing::Scale(v)
            }
            TokenKind::Keyword(Keyword::ShowCenterlines) => {
                self.advance();
                DslDrawing::ShowCenterlines(self.expect_boolean()?)
            }
            TokenKind::Keyword(Keyword::ShowConstructionGeometry) => {
                self.advance();
                DslDrawing::ShowConstructionGeometry(self.expect_boolean()?)
            }
            TokenKind::Keyword(Keyword::ProjectionMethod) => {
                self.advance();
                let token = self.peek();
                match &token.kind {
                    TokenKind::Keyword(Keyword::FirstAngle) => {
                        self.advance();
                        DslDrawing::ProjectionMethod(DslProjectionMethod::FirstAngle)
                    }
                    TokenKind::Keyword(Keyword::ThirdAngle) => {
                        self.advance();
                        DslDrawing::ProjectionMethod(DslProjectionMethod::ThirdAngle)
                    }
                    _ => return Err(self.unexpected("expected FIRST_ANGLE or THIRD_ANGLE")),
                }
            }
            _ => return Err(self.unexpected("expected a drawing property")),
        };
        self.expect_line_end()?;
        Ok(result)
    }

    fn expect_boolean(&mut self) -> Result<bool, DslError> {
        let token = self.peek();
        let value = match &token.kind {
            TokenKind::Keyword(Keyword::True) => true,
            TokenKind::Keyword(Keyword::False) => false,
            TokenKind::Identifier(name) if name.eq_ignore_ascii_case("true") => true,
            TokenKind::Identifier(name) if name.eq_ignore_ascii_case("false") => false,
            _ => return Err(self.unexpected("expected TRUE or FALSE")),
        };
        self.advance();
        Ok(value)
    }

    // ---- expressions (precedence climbing, left-associative) ----

    fn parse_expression(&mut self) -> Result<DslExpression, DslError> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> Result<DslExpression, DslError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            match self.peek_kind() {
                Some(TokenKind::Plus) => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = DslExpression::Binary {
                        left: Box::new(left),
                        op: DslBinaryOp::Add,
                        right: Box::new(right),
                    };
                }
                Some(TokenKind::Minus) => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = DslExpression::Binary {
                        left: Box::new(left),
                        op: DslBinaryOp::Subtract,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<DslExpression, DslError> {
        let mut left = self.parse_primary()?;
        loop {
            match self.peek_kind() {
                Some(TokenKind::Star) => {
                    self.advance();
                    let right = self.parse_primary()?;
                    left = DslExpression::Binary {
                        left: Box::new(left),
                        op: DslBinaryOp::Multiply,
                        right: Box::new(right),
                    };
                }
                Some(TokenKind::Slash) => {
                    self.advance();
                    let right = self.parse_primary()?;
                    left = DslExpression::Binary {
                        left: Box::new(left),
                        op: DslBinaryOp::Divide,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<DslExpression, DslError> {
        let token = self.peek();
        match &token.kind {
            TokenKind::Number(value) => {
                let v = *value;
                self.advance();
                Ok(DslExpression::Number(v))
            }
            TokenKind::Minus => {
                self.advance();
                let (value, _) = self.expect_plain_number()?;
                Ok(DslExpression::Number(-value))
            }
            TokenKind::Identifier(id) => {
                let id = id.clone();
                self.advance();
                Ok(DslExpression::Reference(id))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expression()?;
                self.expect_rparen()?;
                Ok(inner)
            }
            _ => Err(DslError::InvalidExpression {
                message: "expected a number, identifier, or parenthesized expression".into(),
                line: token.span.start.line,
                column: token.span.start.column,
            }),
        }
    }

    fn expect_rparen(&mut self) -> Result<(), DslError> {
        match self.peek_kind() {
            Some(TokenKind::RParen) => {
                self.advance();
                Ok(())
            }
            _ => Err(self.unexpected("expected `)`")),
        }
    }
}

fn invalid_field(feature: &str, kw: Keyword, line: u32, column: u32) -> DslError {
    DslError::InvalidFeatureField {
        feature: feature.into(),
        field: format!("{:?}", kw),
        message: "field is not valid for this feature type in v1".into(),
        line,
        column,
    }
}

fn reject_duplicate(
    seen: &mut BTreeSet<&'static str>,
    name: &'static str,
    line: u32,
    column: u32,
) -> Result<(), DslError> {
    if !seen.insert(name) {
        return Err(DslError::DuplicateField {
            field: name.into(),
            line,
            column,
        });
    }
    Ok(())
}

fn missing_field(feature: &str, id: &str, missing: &str, token: &Token) -> DslError {
    DslError::MissingRequiredField {
        feature: format!("{} {}", feature, id),
        missing: missing.into(),
        line: token.span.start.line,
        column: token.span.start.column,
    }
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Keyword(kw) => format!("keyword {:?}", kw),
        TokenKind::Identifier(id) => format!("identifier `{}`", id),
        TokenKind::Number(v) => format!("number {}", v),
        TokenKind::Str(_) => "string".into(),
        TokenKind::Arrow => "`<-`".into(),
        TokenKind::Assign => "`=`".into(),
        TokenKind::ColonAssign => "`:=`".into(),
        TokenKind::Plus => "`+`".into(),
        TokenKind::Minus => "`-`".into(),
        TokenKind::Star => "`*`".into(),
        TokenKind::Slash => "`/`".into(),
        TokenKind::LParen => "`(`".into(),
        TokenKind::RParen => "`)`".into(),
        TokenKind::Newline => "end of line".into(),
        TokenKind::Eof => "end of file".into(),
    }
}

fn slot_from_keyword(kw: Keyword) -> Option<DslSlot> {
    use DslSlot::*;
    Some(match kw {
        Keyword::Width => Width,
        Keyword::Height => Height,
        Keyword::CenterX => CenterX,
        Keyword::CenterY => CenterY,
        Keyword::Diameter => Diameter,
        Keyword::Radius => Radius,
        Keyword::Length => Length,
        Keyword::Angle => Orientation,
        Keyword::Count => Count,
        Keyword::Pcd => PitchCircleDiameter,
        Keyword::StartAngle => StartAngle,
        Keyword::Rows => Rows,
        Keyword::Columns => Columns,
        Keyword::SpacingX => SpacingX,
        Keyword::SpacingY => SpacingY,
        Keyword::OffsetX => OffsetX,
        Keyword::OffsetY => OffsetY,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_one(source: &str) -> DslDocument {
        parse_document(source).unwrap()
    }

    #[test]
    fn parses_valid_document_with_header_and_statements() {
        let doc = parse_one("AICAD 1\nPART p \"Part\"\nBASE_RECT main\n  WIDTH 120\n  HEIGHT 80\n");
        assert_eq!(doc.version, 1);
        assert_eq!(doc.statements.len(), 2);
        match &doc.statements[0].kind {
            DslStatementKind::Part(part) => {
                assert_eq!(part.id, "p");
                assert_eq!(part.display_name.as_deref(), Some("Part"));
            }
            _ => panic!("expected part"),
        }
    }

    #[test]
    fn missing_header_is_rejected() {
        let err = parse_document("PART p\n").unwrap_err();
        assert!(matches!(err, DslError::MissingHeader { .. }));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let err = parse_document("AICAD 2\n").unwrap_err();
        assert!(matches!(
            err,
            DslError::UnsupportedVersion { version: 2, .. }
        ));
    }

    #[test]
    fn syntax_error_reports_line_and_column() {
        let err = parse_document("AICAD 1\nHOLE center_hole\n  CENTER 0\n  DIA 64\n").unwrap_err();
        match err {
            DslError::UnexpectedToken { line, column, .. } => {
                assert_eq!(line, 3);
                assert!(column >= 1);
            }
            other => panic!("expected unexpected token, got {other:?}"),
        }
    }

    #[test]
    fn missing_required_field_is_rejected() {
        let err = parse_document("AICAD 1\nHOLE h1\n  CENTER 0 0\n").unwrap_err();
        assert!(matches!(
            err,
            DslError::MissingRequiredField { missing, .. } if missing == "DIA"
        ));
    }

    #[test]
    fn duplicate_field_is_rejected() {
        let err = parse_document("AICAD 1\nHOLE h1\n  DIA 10\n  DIA 12\n").unwrap_err();
        assert!(matches!(
            err,
            DslError::DuplicateField { field, .. } if field == "DIA"
        ));
    }

    #[test]
    fn unknown_uppercase_keyword_is_rejected() {
        let err = parse_document("AICAD 1\nMAGIC_CUT x\n").unwrap_err();
        assert!(matches!(err, DslError::UnknownKeyword { .. }));
    }

    #[test]
    fn parses_through_slot() {
        let doc = parse_one(
            "AICAD 1\nSLOT s1\n  CENTER 10 -5\n  LENGTH 40\n  WIDTH 12\n  ANGLE 45\n  THRU\n",
        );
        match &doc.statements[0].kind {
            DslStatementKind::Slot(slot) => {
                assert_eq!(slot.id, "s1");
                assert_eq!(slot.center, (10.0, -5.0));
                assert_eq!(slot.length, 40.0);
                assert_eq!(slot.width, 12.0);
                assert_eq!(slot.angle, 45.0);
            }
            other => panic!("expected slot, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_features_are_rejected_not_ignored() {
        for source in [
            "AICAD 1\nFILLET f1\n",
            "AICAD 1\nPART p\nCONSTRAINT c1 CONCENTRIC a b\n",
        ] {
            let err = parse_document(source).unwrap_err();
            assert!(
                matches!(err, DslError::UnsupportedDslFeature { .. }),
                "expected unsupported feature for {source:?}"
            );
        }
    }

    #[test]
    fn unsupported_unit_is_rejected() {
        let err = parse_document("AICAD 1\nUNITS inch\n").unwrap_err();
        assert!(matches!(err, DslError::UnsupportedUnit { .. }));
    }

    #[test]
    fn unresolved_parameter_parses() {
        let doc = parse_one("AICAD 1\nPARAM edge_offset = UNRESOLVED\n");
        match &doc.statements[0].kind {
            DslStatementKind::Parameter(p) => {
                assert!(matches!(p.value, DslParameterValue::Unresolved));
            }
            _ => panic!("expected parameter"),
        }
    }

    #[test]
    fn derived_parameter_parses_expression() {
        let doc = parse_one("AICAD 1\nPARAM right_hole_x := plate_width / 2 - edge_offset\n");
        match &doc.statements[0].kind {
            DslStatementKind::Parameter(p) => {
                assert!(matches!(p.value, DslParameterValue::Expression(_)));
            }
            _ => panic!("expected parameter"),
        }
    }

    #[test]
    fn center_desugars_fields_are_parsed() {
        let doc = parse_one(
            "AICAD 1\nPART p\nHOLE center_hole\n  CENTER 0 0\n  DIA 64\nHOLE_PATTERN bolts CIRCULAR\n  CENTER 0 0\n  PCD 126\n  COUNT 8\n  DIA 10\n  START_ANGLE 0\n",
        );
        let holes: Vec<_> = doc
            .statements
            .iter()
            .filter_map(|s| match &s.kind {
                DslStatementKind::Hole(h) => Some(h),
                _ => None,
            })
            .collect();
        assert_eq!(holes.len(), 1);
        assert_eq!(holes[0].diameter, 64.0);
        assert_eq!(holes[0].center, Some((0.0, 0.0)));
    }

    // ---- expression grammar (Level 3) ----

    fn expr(source: &str) -> DslExpression {
        let doc = parse_one(&format!("AICAD 1\nPARAM x := {}\n", source));
        match &doc.statements[0].kind {
            DslStatementKind::Parameter(p) => match &p.value {
                DslParameterValue::Expression(e) => e.clone(),
                _ => panic!("expected expression"),
            },
            _ => panic!("expected parameter"),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        // A + B * 2 → Add(A, Multiply(B, 2))
        assert_eq!(
            expr("a + b * 2"),
            DslExpression::Binary {
                left: Box::new(DslExpression::Reference("a".into())),
                op: DslBinaryOp::Add,
                right: Box::new(DslExpression::Binary {
                    left: Box::new(DslExpression::Reference("b".into())),
                    op: DslBinaryOp::Multiply,
                    right: Box::new(DslExpression::Number(2.0)),
                }),
            }
        );
    }

    #[test]
    fn parentheses_override_precedence() {
        // (A + B) * 2 → Multiply(Add(A, B), 2)
        assert_eq!(
            expr("(a + b) * 2"),
            DslExpression::Binary {
                left: Box::new(DslExpression::Binary {
                    left: Box::new(DslExpression::Reference("a".into())),
                    op: DslBinaryOp::Add,
                    right: Box::new(DslExpression::Reference("b".into())),
                }),
                op: DslBinaryOp::Multiply,
                right: Box::new(DslExpression::Number(2.0)),
            }
        );
    }

    #[test]
    fn subtraction_is_left_associative() {
        // A - B - C → Subtract(Subtract(A, B), C)
        assert_eq!(
            expr("a - b - c"),
            DslExpression::Binary {
                left: Box::new(DslExpression::Binary {
                    left: Box::new(DslExpression::Reference("a".into())),
                    op: DslBinaryOp::Subtract,
                    right: Box::new(DslExpression::Reference("b".into())),
                }),
                op: DslBinaryOp::Subtract,
                right: Box::new(DslExpression::Reference("c".into())),
            }
        );
    }

    #[test]
    fn division_is_left_associative() {
        // A / B / C → Divide(Divide(A, B), C)
        assert_eq!(
            expr("a / b / c"),
            DslExpression::Binary {
                left: Box::new(DslExpression::Binary {
                    left: Box::new(DslExpression::Reference("a".into())),
                    op: DslBinaryOp::Divide,
                    right: Box::new(DslExpression::Reference("b".into())),
                }),
                op: DslBinaryOp::Divide,
                right: Box::new(DslExpression::Reference("c".into())),
            }
        );
    }

    #[test]
    fn negative_literal_is_supported_in_expression() {
        assert_eq!(
            expr("-5 + a"),
            DslExpression::Binary {
                left: Box::new(DslExpression::Number(-5.0)),
                op: DslBinaryOp::Add,
                right: Box::new(DslExpression::Reference("a".into())),
            }
        );
    }

    #[test]
    fn invalid_expression_is_rejected() {
        let err = parse_document("AICAD 1\nPARAM x := (1 + )\n").unwrap_err();
        assert!(matches!(err, DslError::InvalidExpression { .. }));
    }
}
