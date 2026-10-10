//! Lexer for the AICAD Engineering DSL.
//!
//! Produces a flat, deterministic token stream with source spans. Lexing is
//! pure: it never touches an [`scc_engineering::model::EngineeringModel`].

use crate::error::DslError;
use crate::token::{keyword_from_str, Position, SourceSpan, Token, TokenKind};

/// The engineering ID rule shared with the rest of AICAD:
/// `[a-z][a-z0-9_-]{0,63}` (no Unicode identifiers).
pub fn is_valid_identifier(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    let mut chars = id.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Lex `source` into a token stream ending with [`TokenKind::Eof`].
pub fn lex(source: &str) -> Result<Vec<Token>, DslError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut column = 1u32;

    let pos = |line: u32, column: u32| Position::new(line, column);

    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => {
                i += 1;
                column += 1;
            }
            '\r' => {
                // Ignore CR so CRLF documents work; the newline token comes
                // from the following `\n`.
                i += 1;
            }
            '\n' => {
                tokens.push(Token::new(
                    TokenKind::Newline,
                    SourceSpan::new(pos(line, column), pos(line, column + 1)),
                ));
                i += 1;
                line += 1;
                column = 1;
            }
            '#' => {
                // Comment: skip to end of line.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                    column += 1;
                }
            }
            '"' => {
                let start = pos(line, column);
                i += 1;
                column += 1;
                let mut value = String::new();
                let mut terminated = false;
                while i < chars.len() {
                    let ch = chars[i];
                    match ch {
                        '"' => {
                            terminated = true;
                            i += 1;
                            column += 1;
                            break;
                        }
                        '\\' => {
                            i += 1;
                            column += 1;
                            if i >= chars.len() {
                                break;
                            }
                            let escaped = chars[i];
                            match escaped {
                                '"' => value.push('"'),
                                '\\' => value.push('\\'),
                                other => {
                                    // Unknown escape: keep the character as-is
                                    // rather than guessing a meaning.
                                    value.push(other);
                                }
                            }
                            i += 1;
                            column += 1;
                        }
                        '\n' => break,
                        other => {
                            value.push(other);
                            i += 1;
                            column += 1;
                        }
                    }
                }
                if !terminated {
                    return Err(DslError::UnterminatedString {
                        line: start.line,
                        column: start.column,
                    });
                }
                tokens.push(Token::new(
                    TokenKind::Str(value),
                    SourceSpan::new(start, pos(line, column)),
                ));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = pos(line, column);
                let mut word = String::new();
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                {
                    word.push(chars[i]);
                    i += 1;
                    column += 1;
                }
                let is_all_uppercase = word
                    .chars()
                    .all(|ch| ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit());
                if is_all_uppercase {
                    match keyword_from_str(&word) {
                        Some(kw) => tokens.push(Token::new(
                            TokenKind::Keyword(kw),
                            SourceSpan::new(start, pos(line, column)),
                        )),
                        None => {
                            return Err(DslError::UnknownKeyword {
                                keyword: word,
                                line: start.line,
                                column: start.column,
                            });
                        }
                    }
                } else {
                    if !is_valid_identifier(&word) {
                        return Err(DslError::InvalidIdentifier {
                            identifier: word,
                            line: start.line,
                            column: start.column,
                        });
                    }
                    tokens.push(Token::new(
                        TokenKind::Identifier(word),
                        SourceSpan::new(start, pos(line, column)),
                    ));
                }
            }
            c if c.is_ascii_digit() => {
                let start = pos(line, column);
                let mut text = String::new();
                while i < chars.len() && chars[i].is_ascii_digit() {
                    text.push(chars[i]);
                    i += 1;
                    column += 1;
                }
                if i < chars.len() && chars[i] == '.' {
                    text.push(chars[i]);
                    i += 1;
                    column += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        text.push(chars[i]);
                        i += 1;
                        column += 1;
                    }
                }
                if i < chars.len() && (chars[i].is_ascii_alphabetic() || chars[i] == '_') {
                    return Err(DslError::InvalidNumber {
                        text: format!("{}{}", text, chars[i]),
                        line: start.line,
                        column: start.column,
                    });
                }
                let value: f64 = text.parse().map_err(|_| DslError::InvalidNumber {
                    text: text.clone(),
                    line: start.line,
                    column: start.column,
                })?;
                if !value.is_finite() {
                    return Err(DslError::InvalidNumber {
                        text,
                        line: start.line,
                        column: start.column,
                    });
                }
                tokens.push(Token::new(
                    TokenKind::Number(value),
                    SourceSpan::new(start, pos(line, column)),
                ));
            }
            '-' => {
                tokens.push(Token::new(
                    TokenKind::Minus,
                    SourceSpan::new(pos(line, column), pos(line, column + 1)),
                ));
                i += 1;
                column += 1;
            }
            '+' => {
                tokens.push(Token::new(
                    TokenKind::Plus,
                    SourceSpan::new(pos(line, column), pos(line, column + 1)),
                ));
                i += 1;
                column += 1;
            }
            '*' => {
                tokens.push(Token::new(
                    TokenKind::Star,
                    SourceSpan::new(pos(line, column), pos(line, column + 1)),
                ));
                i += 1;
                column += 1;
            }
            '/' => {
                tokens.push(Token::new(
                    TokenKind::Slash,
                    SourceSpan::new(pos(line, column), pos(line, column + 1)),
                ));
                i += 1;
                column += 1;
            }
            '(' => {
                tokens.push(Token::new(
                    TokenKind::LParen,
                    SourceSpan::new(pos(line, column), pos(line, column + 1)),
                ));
                i += 1;
                column += 1;
            }
            ')' => {
                tokens.push(Token::new(
                    TokenKind::RParen,
                    SourceSpan::new(pos(line, column), pos(line, column + 1)),
                ));
                i += 1;
                column += 1;
            }
            '=' => {
                tokens.push(Token::new(
                    TokenKind::Assign,
                    SourceSpan::new(pos(line, column), pos(line, column + 1)),
                ));
                i += 1;
                column += 1;
            }
            ':' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::new(
                        TokenKind::ColonAssign,
                        SourceSpan::new(pos(line, column), pos(line, column + 2)),
                    ));
                    i += 2;
                    column += 2;
                } else {
                    return Err(DslError::UnexpectedToken {
                        message: "unexpected character ':' (expected ':=')".into(),
                        line,
                        column,
                    });
                }
            }
            '<' => {
                if i + 1 < chars.len() && chars[i + 1] == '-' {
                    tokens.push(Token::new(
                        TokenKind::Arrow,
                        SourceSpan::new(pos(line, column), pos(line, column + 2)),
                    ));
                    i += 2;
                    column += 2;
                } else {
                    return Err(DslError::UnexpectedToken {
                        message: "unexpected character '<' (expected '<-')".into(),
                        line,
                        column,
                    });
                }
            }
            other => {
                return Err(DslError::UnexpectedToken {
                    message: format!("unexpected character '{}'", other),
                    line,
                    column,
                });
            }
        }
    }

    tokens.push(Token::new(
        TokenKind::Eof,
        SourceSpan::new(pos(line, column), pos(line, column)),
    ));
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Keyword;

    #[test]
    fn lexes_keywords_identifiers_numbers_and_comments() {
        let tokens = lex(
            "# comment\nAICAD 1\nPART mounting_plate \"Mounting Plate\"\nHOLE center_hole\n  CENTER 0 0\n  DIA -10\n",
        )
        .unwrap();
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Keyword(Keyword::Aicad))));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Keyword(Keyword::Part))));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Identifier(s) if s == "mounting_plate")));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Str(s) if s == "Mounting Plate")));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Number(v) if *v == 0.0)));
        // Negative numbers are lexed as `-` + number and combined by the parser.
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Minus)));
        assert!(tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Number(v) if *v == 10.0)));
        // Comment text must not appear.
        assert!(!tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Identifier(s) if s.contains("comment"))));
    }

    #[test]
    fn lexes_operators_and_arrow() {
        let tokens = lex("a + b * (c - 2) / 3\nx <- y\nz := w\n").unwrap();
        let kinds: Vec<&TokenKind> = tokens.iter().map(|t| &t.kind).collect();
        assert!(kinds.contains(&&TokenKind::Plus));
        assert!(kinds.contains(&&TokenKind::Minus));
        assert!(kinds.contains(&&TokenKind::Star));
        assert!(kinds.contains(&&TokenKind::Slash));
        assert!(kinds.contains(&&TokenKind::LParen));
        assert!(kinds.contains(&&TokenKind::RParen));
        assert!(kinds.contains(&&TokenKind::Arrow));
        assert!(kinds.contains(&&TokenKind::ColonAssign));
    }

    #[test]
    fn unknown_uppercase_keyword_is_rejected() {
        let err = lex("MAGIC_CUT x\n").unwrap_err();
        assert!(matches!(err, DslError::UnknownKeyword { .. }));
    }

    #[test]
    fn invalid_identifier_is_rejected() {
        let err = lex("Bad\n").unwrap_err();
        assert!(matches!(err, DslError::InvalidIdentifier { .. }));
    }

    #[test]
    fn unterminated_string_is_rejected() {
        let err = lex("PART x \"oops\n").unwrap_err();
        assert!(matches!(err, DslError::UnterminatedString { .. }));
    }

    #[test]
    fn source_spans_track_line_and_column() {
        let tokens = lex("AICAD 1\nPART p\n").unwrap();
        let part = tokens
            .iter()
            .find(|t| matches!(t.kind, TokenKind::Keyword(Keyword::Part)))
            .unwrap();
        assert_eq!(part.span.start.line, 2);
        assert_eq!(part.span.start.column, 1);
    }

    #[test]
    fn whitespace_and_tabs_are_uniform() {
        let a = lex("WIDTH 120\n").unwrap();
        let b = lex("WIDTH\t   120\n").unwrap();
        // Both produce the same token *kinds* (spans naturally differ).
        assert_eq!(a.len(), b.len());
        for (ta, tb) in a.iter().zip(b.iter()) {
            assert_eq!(ta.kind, tb.kind);
        }
    }
}
