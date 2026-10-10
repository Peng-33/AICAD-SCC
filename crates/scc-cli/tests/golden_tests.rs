use std::fs;

fn golden_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn read_golden(name: &str) -> String {
    fs::read_to_string(golden_dir().join(name)).expect("golden file missing")
}

#[test]
fn golden_compile_bracket_matches_snapshot() {
    let proposal = read_golden("bracket.proposal.json");
    let result = scc_cli::compile::compile(&proposal, false).expect("compile must succeed");
    assert!(
        result.ir_json.is_some(),
        "expected IR output, got conflicts"
    );

    let actual = result.ir_json.unwrap();
    let expected = read_golden("bracket.ir.json");
    assert_eq!(
        actual, expected,
        "compiled IR must match golden snapshot byte-for-byte"
    );
}

#[test]
fn golden_execute_bracket_matches_snapshot() {
    let ir_json = read_golden("bracket.ir.json");

    let result = scc_cli::execute::execute(&ir_json, None, false).expect("execute must succeed");

    let expected_state = read_golden("bracket.state.json");
    assert_eq!(
        result.state_json, expected_state,
        "graph state must match golden snapshot byte-for-byte"
    );

    let expected_trace = read_golden("bracket.trace.json");
    assert_eq!(
        result.trace_json, expected_trace,
        "execution trace must match golden snapshot byte-for-byte"
    );
}

#[test]
fn golden_compile_is_deterministic() {
    let proposal = read_golden("bracket.proposal.json");
    let run1 = scc_cli::compile::compile(&proposal, false)
        .unwrap()
        .ir_json
        .unwrap();
    let run2 = scc_cli::compile::compile(&proposal, false)
        .unwrap()
        .ir_json
        .unwrap();
    assert_eq!(run1, run2, "compile output must be deterministic");
}

#[test]
fn golden_execute_is_deterministic() {
    let ir_json = read_golden("bracket.ir.json");
    let run1 = scc_cli::execute::execute(&ir_json, None, false).unwrap();
    let run2 = scc_cli::execute::execute(&ir_json, None, false).unwrap();
    assert_eq!(run1.state_json, run2.state_json);
    assert_eq!(run1.trace_json, run2.trace_json);
}

#[test]
fn golden_end_to_end_pipeline_matches_snapshot() {
    let proposal = read_golden("bracket.proposal.json");
    let compiled = scc_cli::compile::compile(&proposal, false).expect("compile must succeed");
    let ir_json = compiled.ir_json.unwrap();
    assert_eq!(ir_json, read_golden("bracket.ir.json"));

    let executed = scc_cli::execute::execute(&ir_json, None, false).expect("execute must succeed");
    assert_eq!(executed.state_json, read_golden("bracket.state.json"));
    assert_eq!(executed.trace_json, read_golden("bracket.trace.json"));
}
