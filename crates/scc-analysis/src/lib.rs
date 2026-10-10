pub mod declaration_graph;
pub mod diagnostics;
pub mod error;
pub mod mcs;

pub use declaration_graph::{analyze_detailed, AnalysisOutcome};
pub use error::{AnalysisError, AnalysisErrors};
