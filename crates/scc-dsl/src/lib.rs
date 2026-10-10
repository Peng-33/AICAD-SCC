//! AICAD Engineering DSL v1.
//!
//! A controlled, parseable, verifiable engineering language that lowers to
//! [`scc_engineering::command::EngineeringCommand`] values. The DSL is **not**
//! a new CAD kernel and not a new state layer: its only output is commands,
//! and the real engineering truth stays in
//! [`scc_engineering::model::EngineeringModel`] (ADR-018).
//!
//! Pipeline (strictly layered):
//!
//! ```text
//! Source Text → Lexer → Tokens → Parser → DslDocument → CommandBuilder → EngineeringCommand[]
//! ```
//!
//! The parser never touches a model; the builder never mutates a model; the
//! whole document is applied atomically by `CommandExecutor::apply_all`.

pub mod ast;
pub mod command_builder;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod token;

pub use ast::{DslDocument, DslStatement, DslStatementKind};
pub use command_builder::{apply_document, build_commands, compile, ApplyError};
pub use error::DslError;
pub use parser::parse_document;
