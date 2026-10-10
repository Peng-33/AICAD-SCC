use scc_core::authority::AuthorityLevel;
use scc_core::constraint_ir::ConstraintIr;
use std::collections::BTreeSet;

pub struct EditableSubgraph {
    pub editable_entities: BTreeSet<String>,
    pub editable_constraints: BTreeSet<String>,
}

impl EditableSubgraph {
    pub fn from_ir(ir: &ConstraintIr) -> Self {
        let editable_entities: BTreeSet<String> =
            ir.entities.iter().map(|e| e.id.0.clone()).collect();

        let editable_constraints: BTreeSet<String> = ir
            .constraints
            .iter()
            .filter(|c| c.context.authority != AuthorityLevel::L0)
            .map(|c| c.id.0.clone())
            .collect();

        EditableSubgraph {
            editable_entities,
            editable_constraints,
        }
    }
}
