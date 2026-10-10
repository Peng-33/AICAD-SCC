use std::fs;

#[test]
fn parse_mutation_fixture() {
    let content = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/bracket.mutation.json"
    ))
    .unwrap();
    let patch: Vec<scc_execution::overlay_view::MutationOp> =
        serde_json::from_str(&content).unwrap();
    assert_eq!(patch.len(), 1);
    match &patch[0] {
        scc_execution::overlay_view::MutationOp::UpdateParameter {
            target_id,
            parameter,
            value,
        } => {
            assert_eq!(target_id, "hole");
            assert_eq!(parameter, "diameter");
            assert_eq!(*value, 15.0);
        }
        _ => panic!("expected UpdateParameter"),
    }
}

#[test]
fn execute_with_mutation_applies_overlay() {
    let ir = scc_core::constraint_ir::ConstraintIr {
        entities: vec![scc_core::entity::Entity {
            id: scc_core::ids::EntityId("hole".into()),
            kind: scc_core::entity::EntityKind::Circle,
        }],
        constraints: vec![],
        graph: scc_core::graph::DeclarationGraph {
            nodes: vec![],
            edges: vec![],
        },
        metadata: scc_core::constraint_ir::IrMetadata {
            constraint_levels: scc_core::constraint_ir::IrConstraintLevels::default(),
        },
    };
    let patch = Some(vec![
        scc_execution::overlay_view::MutationOp::UpdateParameter {
            target_id: "hole".into(),
            parameter: "diameter".into(),
            value: 15.0,
        },
    ]);
    let ir_json = serde_json::to_string(&ir).unwrap();
    let result = scc_cli::execute::execute(&ir_json, patch, false).unwrap();
    let state: serde_json::Value = serde_json::from_str(&result.state_json).unwrap();
    assert_eq!(
        state["entities"]["hole"]["parameters"]["diameter"]["Numeric"],
        serde_json::json!(15.0)
    );
}
