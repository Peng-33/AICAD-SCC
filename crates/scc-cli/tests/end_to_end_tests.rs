use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn scc_bin() -> &'static str {
    env!("CARGO_BIN_EXE_scc")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    workspace_root().join("examples").join(name)
}

#[test]
fn e2e_version_reports_package_version() {
    let output = Command::new(scc_bin())
        .arg("--version")
        .output()
        .expect("scc --version should spawn");

    assert!(output.status.success(), "scc --version should succeed");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("scc {}", env!("CARGO_PKG_VERSION"))
    );
}

fn e2e_temp_dir_for(test_name: &str) -> PathBuf {
    let dir = workspace_root()
        .join("target")
        .join("e2e-tests")
        .join(test_name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create e2e temp dir");
    dir
}

#[test]
fn e2e_compile_success() {
    let dir = e2e_temp_dir_for("e2e_compile_success");
    let input = fixture("bracket.proposal.json");
    let output = dir.join("bracket.ir.json");

    let status = Command::new(scc_bin())
        .arg("compile")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .status()
        .expect("scc compile should spawn");

    assert!(status.success(), "scc compile should exit successfully");
    assert!(output.exists(), "IR output file should exist");

    let ir: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert!(ir["entities"].is_array());
    assert!(ir["constraints"].is_array());
}

#[test]
fn e2e_compile_conflict() {
    let dir = e2e_temp_dir_for("e2e_compile_conflict");
    let input = fixture("bracket.conflict.proposal.json");
    let output = dir.join("bracket.conflict.json");

    let status = Command::new(scc_bin())
        .arg("compile")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .status()
        .expect("scc compile should spawn");

    assert!(
        status.success(),
        "scc compile should exit successfully even on conflict"
    );
    assert!(output.exists(), "conflict report should exist");

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(report["status"], "conflict");
    assert!(!report["conflicts"].as_array().unwrap().is_empty());
}

#[test]
fn e2e_execute_success() {
    let dir = e2e_temp_dir_for("e2e_execute_success");
    let proposal = fixture("bracket.proposal.json");
    let ir_output = dir.join("execute_success.ir.json");

    let compile_status = Command::new(scc_bin())
        .arg("compile")
        .arg(&proposal)
        .arg("--output")
        .arg(&ir_output)
        .status()
        .expect("compile should spawn");
    assert!(compile_status.success(), "scc compile should succeed");
    assert!(ir_output.exists(), "IR output file should exist");

    let trace_output = dir.join("execute_success.trace.json");
    let state_output = dir.join("execute_success.trace.state.json");

    let status = Command::new(scc_bin())
        .arg("execute")
        .arg(&ir_output)
        .arg("--trace")
        .arg(&trace_output)
        .status()
        .expect("scc execute should spawn");

    assert!(status.success(), "scc execute should exit successfully");
    assert!(trace_output.exists(), "trace output file should exist");
    assert!(state_output.exists(), "state output file should exist");

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state_output).unwrap()).unwrap();
    assert!(state["entities"].is_object());

    let trace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&trace_output).unwrap()).unwrap();
    assert!(trace["events"].is_array());
}

#[test]
fn e2e_execute_with_mutation() {
    let dir = e2e_temp_dir_for("e2e_execute_with_mutation");
    let proposal = fixture("bracket.proposal.json");
    let ir_output = dir.join("execute_mutation.ir.json");

    let compile_status = Command::new(scc_bin())
        .arg("compile")
        .arg(&proposal)
        .arg("--output")
        .arg(&ir_output)
        .status()
        .expect("compile should spawn");
    assert!(compile_status.success(), "scc compile should succeed");
    assert!(ir_output.exists(), "IR output file should exist");

    let mutation = fixture("bracket.mutation.json");
    let trace_output = dir.join("execute_mutation.trace.json");
    let state_output = dir.join("execute_mutation.trace.state.json");

    let status = Command::new(scc_bin())
        .arg("execute")
        .arg(&ir_output)
        .arg("--mutation")
        .arg(&mutation)
        .arg("--trace")
        .arg(&trace_output)
        .status()
        .expect("scc execute should spawn");

    assert!(
        status.success(),
        "scc execute with mutation should exit successfully"
    );
    assert!(trace_output.exists());
    assert!(state_output.exists());

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state_output).unwrap()).unwrap();
    assert_eq!(
        state["entities"]["hole"]["parameters"]["diameter"]["Numeric"],
        serde_json::json!(15.0)
    );
}

#[test]
fn e2e_full_pipeline() {
    let dir = e2e_temp_dir_for("e2e_full_pipeline");
    let proposal = fixture("bracket.proposal.json");
    let ir_output = dir.join("pipeline.ir.json");

    let compile_status = Command::new(scc_bin())
        .arg("compile")
        .arg(&proposal)
        .arg("--output")
        .arg(&ir_output)
        .status()
        .expect("compile should spawn");
    assert!(compile_status.success());
    assert!(ir_output.exists());

    let trace_output = dir.join("pipeline.trace.json");
    let state_output = dir.join("pipeline.trace.state.json");

    let execute_status = Command::new(scc_bin())
        .arg("execute")
        .arg(&ir_output)
        .arg("--trace")
        .arg(&trace_output)
        .status()
        .expect("execute should spawn");
    assert!(execute_status.success());
    assert!(trace_output.exists());
    assert!(state_output.exists());

    let trace: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&trace_output).unwrap()).unwrap();
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state_output).unwrap()).unwrap();
    assert!(!trace["events"].as_array().unwrap().is_empty());
    assert!(!state["entities"].as_object().unwrap().is_empty());
}

#[test]
fn e2e_compile_refuses_overwrite_without_force() {
    let dir = e2e_temp_dir_for("e2e_compile_refuses_overwrite");
    let input = fixture("bracket.proposal.json");
    let output = dir.join("bracket.ir.json");

    fs::write(&output, "pre-existing content").unwrap();

    let status = Command::new(scc_bin())
        .arg("compile")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .status()
        .expect("scc compile should spawn");

    assert!(
        !status.success(),
        "compile should fail when target exists without --force"
    );
    assert_eq!(
        fs::read_to_string(&output).unwrap(),
        "pre-existing content",
        "existing file should not be overwritten"
    );
}

#[test]
fn e2e_compile_allows_overwrite_with_force() {
    let dir = e2e_temp_dir_for("e2e_compile_allows_overwrite");
    let input = fixture("bracket.proposal.json");
    let output = dir.join("bracket.ir.json");

    fs::write(&output, "pre-existing content").unwrap();

    let status = Command::new(scc_bin())
        .arg("compile")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--force")
        .status()
        .expect("scc compile should spawn");

    assert!(status.success(), "compile should succeed with --force");

    let ir: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert!(ir["entities"].is_array());
}

#[test]
fn e2e_compile_supports_bare_relative_output_path() {
    let dir = e2e_temp_dir_for("e2e_bare_relative_output");
    let input = fixture("bracket.proposal.json");

    let output = Command::new(scc_bin())
        .current_dir(&dir)
        .arg("compile")
        .arg(&input)
        .arg("--output")
        .arg("result.ir.json")
        .output()
        .expect("scc compile should spawn");

    assert!(
        output.status.success(),
        "bare relative output failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.join("result.ir.json").exists());
}

#[test]
fn e2e_invalid_input_fails() {
    let dir = e2e_temp_dir_for("e2e_invalid_input_fails");
    let bad_input = dir.join("bad.json");
    fs::write(&bad_input, "not valid json").unwrap();

    let status = Command::new(scc_bin())
        .arg("compile")
        .arg(&bad_input)
        .status()
        .expect("scc compile should spawn");

    assert!(
        !status.success(),
        "invalid JSON should cause compile to fail"
    );
}

#[test]
fn e2e_fail_on_conflict_returns_exit_code_2() {
    let dir = e2e_temp_dir_for("e2e_fail_on_conflict");
    let input = fixture("bracket.conflict.proposal.json");
    let output = dir.join("bracket.conflict.json");

    let status = Command::new(scc_bin())
        .arg("compile")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--fail-on-conflict")
        .status()
        .expect("scc compile should spawn");

    assert_eq!(
        status.code(),
        Some(2),
        "--fail-on-conflict should return exit code 2"
    );
    assert!(output.exists(), "conflict report should still be written");
}

#[test]
fn e2e_json_errors_emits_valid_json() {
    let dir = e2e_temp_dir_for("e2e_json_errors");
    let bad_input = dir.join("bad.json");
    fs::write(&bad_input, "not valid json").unwrap();

    let output = Command::new(scc_bin())
        .arg("compile")
        .arg(&bad_input)
        .arg("--json-errors")
        .output()
        .expect("scc compile should spawn");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&stderr).expect("--json-errors output must be valid JSON");
    assert_eq!(parsed["status"], "error");
    assert!(
        !parsed["errors"].as_array().unwrap().is_empty(),
        "expected at least one structured error"
    );
}

#[test]
fn e2e_json_errors_covers_cli_argument_failures() {
    let output = Command::new(scc_bin())
        .arg("--json-errors")
        .arg("compile")
        .output()
        .expect("scc compile should spawn");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("stderr must be valid JSON");
    assert_eq!(parsed["errors"][0]["code"], "SCC-CLI-010");
    assert_eq!(parsed["errors"][0]["stage"], "cli");
}

#[test]
fn e2e_json_errors_preserves_all_analysis_diagnostics() {
    let dir = e2e_temp_dir_for("e2e_json_analysis_batch");
    let bad_input = dir.join("analysis-errors.json");
    fs::write(
        &bad_input,
        r#"{
            "version": "1.0",
            "entities": [
                { "id": "circle", "kind": "circle" },
                { "id": "point", "kind": "point" },
                { "id": "line", "kind": "line" }
            ],
            "relations": [
                { "kind": "inside", "subject": "circle", "reference": "point" },
                { "kind": "parallel_to", "subject": "circle", "reference": "line" }
            ],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#,
    )
    .unwrap();

    let output = Command::new(scc_bin())
        .arg("compile")
        .arg(&bad_input)
        .arg("--json-errors")
        .output()
        .expect("scc compile should spawn");

    assert_eq!(output.status.code(), Some(1));
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("stderr must be valid JSON");
    let errors = parsed["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert!(errors.iter().all(|error| error["code"] == "SCC-ANAL-002"));
}

#[test]
fn e2e_json_conflict_error_is_machine_readable() {
    let dir = e2e_temp_dir_for("e2e_json_conflict");
    let input = fixture("bracket.conflict.proposal.json");
    let report = dir.join("conflict.json");

    let output = Command::new(scc_bin())
        .arg("compile")
        .arg(&input)
        .arg("--output")
        .arg(&report)
        .arg("--json-errors")
        .arg("--fail-on-conflict")
        .output()
        .expect("scc compile should spawn");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "machine-error mode must keep stdout clean"
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("stderr must be valid JSON");
    assert_eq!(parsed["errors"][0]["code"], "SCC-CLI-009");
    assert_eq!(
        parsed["errors"][0]["path"],
        report.to_string_lossy().as_ref()
    );
    assert!(report.exists(), "conflict report must still be written");
}

#[test]
fn e2e_execution_error_returns_exit_code_3() {
    let dir = e2e_temp_dir_for("e2e_execution_error");
    let ir = dir.join("dangling.ir.json");
    fs::write(
        &ir,
        serde_json::json!({
            "entities": [],
            "constraints": [
                {
                    "id": "c_bad",
                    "kind": "Distance",
                    "source": "ghost",
                    "target": "phantom",
                    "context": {
                        "origin": "UserSpecified",
                        "authority": "L1",
                        "derivation": null,
                        "version_hint": null
                    },
                    "parameters": {}
                }
            ],
            "graph": { "nodes": [], "edges": [] },
            "metadata": { "constraint_levels": { "l0": [], "l1": ["c_bad"], "l2": [], "l3": [] } }
        })
        .to_string(),
    )
    .unwrap();

    let status = Command::new(scc_bin())
        .arg("execute")
        .arg(&ir)
        .status()
        .expect("scc execute should spawn");

    assert_eq!(
        status.code(),
        Some(3),
        "execution-stage error should return exit code 3"
    );
}

#[test]
fn e2e_validate_valid_proposal() {
    let input = fixture("bracket.proposal.json");
    let status = Command::new(scc_bin())
        .arg("validate")
        .arg(&input)
        .status()
        .expect("scc validate should spawn");

    assert!(
        status.success(),
        "valid proposal should validate successfully"
    );
}

#[test]
fn e2e_validate_invalid_proposal() {
    let dir = e2e_temp_dir_for("e2e_validate_invalid");
    let input = dir.join("invalid.json");
    fs::write(
        &input,
        r#"{"version": "", "entities": [{"id": "", "kind": "circle"}], "relations": [{"kind": "distance", "subject": "", "reference": ""}], "dimensions": [], "arrays": [], "anchors": [], "metadata": {}}"#,
    )
    .unwrap();

    let status = Command::new(scc_bin())
        .arg("validate")
        .arg(&input)
        .status()
        .expect("scc validate should spawn");

    assert!(!status.success(), "invalid proposal should fail validation");
}

#[test]
fn e2e_inspect_ir_summary() {
    let dir = e2e_temp_dir_for("e2e_inspect");
    let proposal = fixture("bracket.proposal.json");
    let ir_output = dir.join("inspect.ir.json");

    let compile_status = Command::new(scc_bin())
        .arg("compile")
        .arg(&proposal)
        .arg("--output")
        .arg(&ir_output)
        .status()
        .expect("compile should spawn");
    assert!(compile_status.success(), "scc compile should succeed");

    let output = Command::new(scc_bin())
        .arg("inspect")
        .arg(&ir_output)
        .output()
        .expect("scc inspect should spawn");

    assert!(output.status.success(), "scc inspect should succeed");
    let report: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert!(report["entity_count"].is_number());
    assert!(report["constraint_count"].is_number());
}

#[test]
fn e2e_quiet_suppresses_success_output() {
    let dir = e2e_temp_dir_for("e2e_quiet_suppresses_success_output");
    let output_path = dir.join("quiet.ir.json");

    let output = Command::new(scc_bin())
        .args(["--quiet", "compile"])
        .arg(fixture("bracket.proposal.json"))
        .args(["--output"])
        .arg(&output_path)
        .output()
        .expect("quiet compile should spawn");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output_path.exists());
}

#[test]
fn e2e_solve_fixed_circle() {
    let dir = e2e_temp_dir_for("e2e_solve_fixed_circle");
    let input = fixture("fixed-circle.state.json");
    let geometry_output = dir.join("fixed-circle.geometry.json");

    let status = Command::new(scc_bin())
        .arg("solve")
        .arg(&input)
        .arg("--output")
        .arg(&geometry_output)
        .status()
        .expect("scc solve should spawn");

    assert!(status.success(), "scc solve should succeed");
    let geometry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&geometry_output).unwrap()).unwrap();
    assert_eq!(
        geometry["entities"]["circle"]["primitive"]["kind"],
        "circle"
    );
    assert_eq!(geometry["entities"]["circle"]["primitive"]["radius"], 5.0);
    assert_eq!(
        geometry["entities"]["circle"]["primitive"]["center"],
        serde_json::json!({"x": 0.0, "y": 0.0})
    );
}
