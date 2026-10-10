use std::fs;

fn workspace_root() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../..")
}

fn fixture_path(name: &str) -> String {
    format!("{}/examples/{}", workspace_root(), name)
}

#[test]
fn integration_compile_bracket() {
    let input =
        fs::read_to_string(fixture_path("bracket.proposal.json")).expect("fixture not found");
    let result = scc_cli::compile::compile(&input, false).expect("compilation failed");

    assert!(result.ir_json.is_some());
    assert!(result.conflict_json.is_none());

    let ir: serde_json::Value =
        serde_json::from_str(result.ir_json.as_ref().unwrap()).expect("invalid ir json");
    assert!(ir["entities"].is_array());
    assert!(ir["constraints"].is_array());
    assert!(ir["graph"].is_object());

    let entities = ir["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 3);

    let constraints = ir["constraints"].as_array().unwrap();
    assert!(!constraints.is_empty());
    assert!(
        constraints.iter().any(|c| c["kind"] == "Fixed"),
        "origin_point anchor must lower to a Fixed constraint"
    );

    let levels = &ir["metadata"]["constraint_levels"];
    assert!(levels["l0"].is_array());
    assert!(!levels["l0"].as_array().unwrap().is_empty());
    assert!(levels["l1"].is_array());
    assert!(levels["l2"].is_array());
    assert!(levels["l3"].is_array());
}

#[test]
fn integration_compile_conflict() {
    let input = fs::read_to_string(fixture_path("bracket.conflict.proposal.json"))
        .expect("fixture not found");
    let result = scc_cli::compile::compile(&input, false).expect("compilation failed");

    assert!(result.ir_json.is_none());
    assert!(result.conflict_json.is_some());

    let conflict: serde_json::Value = serde_json::from_str(result.conflict_json.as_ref().unwrap())
        .expect("invalid conflict json");
    assert_eq!(conflict["status"], "conflict");
    assert!(conflict["conflicts"].is_array());
    assert!(!conflict["conflicts"].as_array().unwrap().is_empty());
}

#[test]
fn integration_determinism() {
    let input =
        fs::read_to_string(fixture_path("bracket.proposal.json")).expect("fixture not found");

    let result1 = scc_cli::compile::compile(&input, false).expect("compilation failed");
    let result2 = scc_cli::compile::compile(&input, false).expect("compilation failed");

    assert_eq!(result1.ir_json, result2.ir_json);
}

#[test]
fn integration_array_preserved_as_node() {
    let input =
        fs::read_to_string(fixture_path("bracket.proposal.json")).expect("fixture not found");
    let result = scc_cli::compile::compile(&input, false).expect("compilation failed");

    let ir: serde_json::Value =
        serde_json::from_str(result.ir_json.as_ref().unwrap()).expect("invalid ir json");

    let nodes = ir["graph"]["nodes"].as_array().unwrap();
    let array_nodes: Vec<_> = nodes.iter().filter(|n| n["node_kind"] == "Array").collect();
    assert!(!array_nodes.is_empty());

    let constraints = ir["constraints"].as_array().unwrap();
    let has_array_relation = constraints.iter().any(|c| c["kind"] == "ArrayRelation");
    let has_instance_of = constraints.iter().any(|c| c["kind"] == "InstanceOf");
    assert!(has_array_relation);
    assert!(has_instance_of);
}

#[test]
fn integration_type_mismatches_are_structured_analysis_errors() {
    let input = r#"{
        "version": "1.0",
        "entities": [
            { "id": "circle_1", "kind": "circle" },
            { "id": "point_1", "kind": "point" },
            { "id": "line_1", "kind": "line" }
        ],
        "relations": [
            { "kind": "inside", "subject": "circle_1", "reference": "point_1" },
            { "kind": "parallel_to", "subject": "circle_1", "reference": "line_1" }
        ],
        "dimensions": [],
        "arrays": [],
        "anchors": [],
        "metadata": {}
    }"#;

    let error = scc_cli::compile::compile(input, false).unwrap_err();
    match error {
        scc_core::error::SccError::StructuredBatch(errors) => {
            assert_eq!(errors.len(), 2);
            assert!(errors.iter().all(|error| error.code == "SCC-ANAL-002"));
            assert_eq!(errors[0].path.as_deref(), Some("/relations/0"));
            assert_eq!(errors[1].path.as_deref(), Some("/relations/1"));
        }
        other => panic!("expected structured analysis errors, got {other:?}"),
    }
}
