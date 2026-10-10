pub mod atomic_write;
pub mod compile;
pub mod engineering;
pub mod execute;
pub mod inspect;
pub mod output;
pub mod parametric_rebuild;
pub mod solve;
pub mod validate;

use scc_core::error::SccError;
use scc_geometry::ExportFormat;

/// Export a legacy or envelope GeometryModel as deterministic CAD text.
pub fn export_geometry(geometry_json: &str, format: ExportFormat) -> Result<String, SccError> {
    scc_geometry::export_geometry_model_json(geometry_json, format)
}
