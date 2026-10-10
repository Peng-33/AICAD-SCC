use scc_core::constraint_ir::ConstraintIr;
use scc_core::error::SccError;
use scc_core::schema_envelope::{deserialize_envelope_or_legacy, DocumentType};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InspectReport {
    pub entity_count: usize,
    pub constraint_count: usize,
    pub node_count: usize,
    pub edge_count: usize,
    pub constraint_levels: ConstraintLevelSummary,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ConstraintLevelSummary {
    pub l0: usize,
    pub l1: usize,
    pub l2: usize,
    pub l3: usize,
}

/// Inspect a Constraint IR document and return a summary report.
///
/// Both legacy unversioned JSON and ADR-003 schema envelopes are accepted.
pub fn inspect(ir_json: &str) -> Result<InspectReport, SccError> {
    let ir: ConstraintIr = deserialize_envelope_or_legacy(DocumentType::ConstraintIr, ir_json)
        .map_err(|e| SccError::Structured(Box::new(e.into())))?;

    let levels = &ir.metadata.constraint_levels;
    Ok(InspectReport {
        entity_count: ir.entities.len(),
        constraint_count: ir.constraints.len(),
        node_count: ir.graph.nodes.len(),
        edge_count: ir.graph.edges.len(),
        constraint_levels: ConstraintLevelSummary {
            l0: levels.l0.len(),
            l1: levels.l1.len(),
            l2: levels.l2.len(),
            l3: levels.l3.len(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::constraint_ir::{IrConstraintLevels, IrMetadata};
    use scc_core::entity::{Entity, EntityKind};
    use scc_core::graph::DeclarationGraph;
    use scc_core::ids::EntityId;

    fn minimal_ir() -> ConstraintIr {
        ConstraintIr {
            entities: vec![Entity {
                id: EntityId("a".into()),
                kind: EntityKind::Circle,
            }],
            constraints: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        }
    }

    #[test]
    fn inspect_legacy_ir() {
        let ir = minimal_ir();
        let json = serde_json::to_string(&ir).unwrap();
        let report = inspect(&json).unwrap();
        assert_eq!(report.entity_count, 1);
        assert_eq!(report.constraint_count, 0);
    }

    #[test]
    fn inspect_envelope_ir() {
        let ir = minimal_ir();
        let json =
            scc_core::schema_envelope::serialize_envelope(DocumentType::ConstraintIr, &ir).unwrap();
        let report = inspect(&json).unwrap();
        assert_eq!(report.entity_count, 1);
    }
}
