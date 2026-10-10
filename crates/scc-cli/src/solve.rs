use scc_core::error::SccError;

/// Solve a legacy or envelope GraphState into a GeometryModel JSON document.
pub fn solve(state_json: &str, use_envelope: bool) -> Result<String, SccError> {
    scc_geometry::solve_graph_state_json(state_json, use_envelope)
}
