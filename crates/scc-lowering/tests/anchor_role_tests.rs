use scc_core::constraint::AnchorRole;
use scc_core::entity::{Entity, EntityKind};
use scc_core::graph::DeclarationGraph;
use scc_core::ids::EntityId;
use scc_core::semantic_ir::{SemanticAnchor, SemanticIr};
use scc_lowering::lowering::lower;

#[test]
fn every_anchor_role_survives_lowering() {
    let ir = SemanticIr {
        entities: vec![
            Entity {
                id: EntityId("circle".into()),
                kind: EntityKind::Circle,
            },
            Entity {
                id: EntityId("line".into()),
                kind: EntityKind::Line,
            },
        ],
        relations: vec![],
        dimensions: vec![],
        arrays: vec![],
        anchors: vec![
            SemanticAnchor::OriginPoint {
                id: EntityId("origin".into()),
            },
            SemanticAnchor::FixedCenter {
                id: EntityId("circle_center".into()),
                entity: EntityId("circle".into()),
            },
            SemanticAnchor::ReferenceEdge {
                id: EntityId("edge_ref".into()),
                entity: EntityId("line".into()),
            },
            SemanticAnchor::DatumLine {
                id: EntityId("datum".into()),
                entity: EntityId("line".into()),
            },
        ],
        graph: DeclarationGraph {
            nodes: vec![],
            edges: vec![],
        },
    };

    let lowered = lower(&ir);
    let roles: Vec<_> = lowered
        .constraints
        .iter()
        .filter_map(|constraint| constraint.anchor_role)
        .collect();

    assert_eq!(
        roles,
        vec![
            AnchorRole::OriginPoint,
            AnchorRole::FixedCenter,
            AnchorRole::ReferenceEdge,
            AnchorRole::DatumLine,
        ]
    );
}

#[test]
fn absent_anchor_role_remains_legacy_compatible() {
    let legacy = r#"{
        "id":"c_1",
        "kind":"CenterOf",
        "source":"hole",
        "target":"plate",
        "context":{
            "origin":"DerivedFromIntent",
            "authority":"L1",
            "derivation":null,
            "version_hint":null
        },
        "parameters":{}
    }"#;

    let constraint: scc_core::constraint::Constraint =
        serde_json::from_str(legacy).expect("legacy constraint should deserialize");
    assert_eq!(constraint.anchor_role, None);
}
