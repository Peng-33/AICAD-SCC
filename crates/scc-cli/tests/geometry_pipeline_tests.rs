use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    workspace_root().join("examples").join(name)
}

fn output_dir(test_name: &str) -> PathBuf {
    let dir = workspace_root()
        .join("target")
        .join("e2e-tests")
        .join(test_name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create geometry pipeline output directory");
    dir
}

#[test]
fn proposal_compiles_executes_and_solves_centered_geometry() {
    let dir = output_dir("geometry_pipeline");
    let ir_path = dir.join("centered-plate.ir.json");
    let trace_path = dir.join("centered-plate.trace.json");
    let state_path = dir.join("centered-plate.trace.state.json");
    let geometry_path = dir.join("centered-plate.geometry.json");

    let compile = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("compile")
        .arg(fixture("centered-plate.proposal.json"))
        .args(["--output"])
        .arg(&ir_path)
        .status()
        .expect("compile command should spawn");
    assert!(compile.success(), "compile should succeed");

    let ir: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&ir_path).unwrap()).unwrap();
    assert!(ir["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|constraint| constraint["anchor_role"] == "fixed_center"));

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--trace"])
        .arg(&trace_path)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "execute should succeed");
    assert!(state_path.exists(), "execute should write GraphState");

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    assert!(state["constraints"]
        .as_object()
        .unwrap()
        .values()
        .any(|constraint| constraint["anchor_role"] == "fixed_center"));

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("solve")
        .arg(&state_path)
        .args(["--output"])
        .arg(&geometry_path)
        .status()
        .expect("solve command should spawn");
    assert!(solve.success(), "solve should succeed");

    let geometry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&geometry_path).unwrap()).unwrap();
    assert_eq!(
        geometry["solve_report"],
        serde_json::json!({
            "status": "fully_constrained",
            "method": "deterministic_direct",
            "convergence": "exact",
            "evaluated_constraints": 5,
            "degrees_of_freedom": {"remaining": 0}
        })
    );
    assert_eq!(
        geometry["entities"]["plate"]["primitive"],
        serde_json::json!({
            "kind": "rectangle",
            "origin": {"x": -50.0, "y": -30.0},
            "width": 100.0,
            "height": 60.0
        })
    );
    assert_eq!(
        geometry["entities"]["hole"]["primitive"],
        serde_json::json!({
            "kind": "circle",
            "center": {"x": 0.0, "y": 0.0},
            "radius": 10.0
        })
    );
}

#[test]
fn solved_geometry_exports_to_svg_and_dxf() {
    let dir = output_dir("geometry_export_pipeline");
    let geometry_path = dir.join("fixed-circle.geometry.json");
    let svg_path = dir.join("fixed-circle.svg");
    let dxf_path = dir.join("fixed-circle.dxf");

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("solve")
        .arg(fixture("fixed-circle.state.json"))
        .args(["--output"])
        .arg(&geometry_path)
        .status()
        .expect("solve command should spawn");
    assert!(solve.success());

    for (format, path) in [("svg", &svg_path), ("dxf", &dxf_path)] {
        let export = Command::new(env!("CARGO_BIN_EXE_scc"))
            .arg("export")
            .arg(&geometry_path)
            .args(["--format", format, "--output"])
            .arg(path)
            .status()
            .expect("export command should spawn");
        assert!(export.success(), "{format} export should succeed");
    }

    assert!(fs::read_to_string(&svg_path).unwrap().starts_with("<svg "));
    assert!(fs::read_to_string(&dxf_path)
        .unwrap()
        .starts_with("0\nSECTION\n2\nHEADER\n"));
}

#[test]
fn polygon_and_array_proposals_reach_geometry() {
    let dir = output_dir("geometry_primitive_pipeline");

    let polygon_ir = dir.join("polygon.ir.json");
    let polygon_trace = dir.join("polygon.trace.json");
    let polygon_state = dir.join("polygon.trace.state.json");
    let polygon_geometry = dir.join("polygon.geometry.json");
    assert!(Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("compile")
        .arg(fixture("polygon.proposal.json"))
        .args(["--output"])
        .arg(&polygon_ir)
        .status()
        .unwrap()
        .success());
    assert!(Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&polygon_ir)
        .args(["--mutation"])
        .arg(fixture("polygon.mutation.json"))
        .args(["--trace"])
        .arg(&polygon_trace)
        .status()
        .unwrap()
        .success());
    assert!(polygon_state.exists());
    assert!(Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("solve")
        .arg(&polygon_state)
        .args(["--output"])
        .arg(&polygon_geometry)
        .status()
        .unwrap()
        .success());
    let polygon: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&polygon_geometry).unwrap()).unwrap();
    assert_eq!(
        polygon["entities"]["profile"]["primitive"]["kind"],
        "polygon"
    );

    let array_ir = dir.join("array.ir.json");
    let array_trace = dir.join("array.trace.json");
    let array_state = dir.join("array.trace.state.json");
    let array_geometry = dir.join("array.geometry.json");
    assert!(Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("compile")
        .arg(fixture("linear-array.proposal.json"))
        .args(["--output"])
        .arg(&array_ir)
        .status()
        .unwrap()
        .success());
    assert!(Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&array_ir)
        .args(["--trace"])
        .arg(&array_trace)
        .status()
        .unwrap()
        .success());
    assert!(array_state.exists());
    assert!(Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("solve")
        .arg(&array_state)
        .args(["--output"])
        .arg(&array_geometry)
        .status()
        .unwrap()
        .success());
    let array: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&array_geometry).unwrap()).unwrap();
    assert_eq!(array["entities"].as_object().unwrap().len(), 3);
    assert_eq!(
        array["entities"]["points[2]"]["primitive"]["position"]["x"],
        10.0
    );
}

#[test]
fn proposal_compiles_executes_and_reports_anchored_point_underconstraint() {
    let dir = output_dir("geometry_distance_pipeline");
    let ir_path = dir.join("anchored-points.ir.json");
    let trace_path = dir.join("anchored-points.trace.json");
    let state_path = dir.join("anchored-points.trace.state.json");
    let geometry_path = dir.join("anchored-points.geometry.json");

    let compile = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("compile")
        .arg(fixture("anchored-points.proposal.json"))
        .args(["--output"])
        .arg(&ir_path)
        .status()
        .expect("compile command should spawn");
    assert!(compile.success(), "compile should succeed");

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--trace"])
        .arg(&trace_path)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "execute should succeed");
    assert!(state_path.exists(), "execute should write GraphState");

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("--json-errors")
        .arg("solve")
        .arg(&state_path)
        .args(["--output"])
        .arg(&geometry_path)
        .output()
        .expect("solve command should spawn");
    assert_eq!(solve.status.code(), Some(4));
    assert!(
        !geometry_path.exists(),
        "underconstrained solve must not emit geometry"
    );

    let diagnostic: serde_json::Value =
        serde_json::from_slice(&solve.stderr).expect("structured geometry diagnostic");
    assert_eq!(diagnostic["status"], "error");
    assert_eq!(diagnostic["errors"][0]["code"], "SCC-GEOM-007");
    assert_eq!(diagnostic["errors"][0]["stage"], "geometry");
    assert_eq!(
        diagnostic["errors"][0]["entity_ids"],
        serde_json::json!(["tip"])
    );
}

#[test]
fn partial_axis_center_reports_dof_then_solves_after_axis_mutation() {
    let dir = output_dir("geometry_partial_center_pipeline");
    let ir_path = dir.join("partial-center.ir.json");
    let under_trace = dir.join("partial-center.trace.json");
    let under_state = dir.join("partial-center.trace.state.json");
    let under_geometry = dir.join("partial-center.geometry.json");
    let complete_trace = dir.join("partial-center-complete.trace.json");
    let complete_state = dir.join("partial-center-complete.trace.state.json");
    let complete_geometry = dir.join("partial-center-complete.geometry.json");

    let compile = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("compile")
        .arg(fixture("partial-center.proposal.json"))
        .args(["--output"])
        .arg(&ir_path)
        .status()
        .expect("compile command should spawn");
    assert!(compile.success(), "compile should succeed");

    let ir: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&ir_path).unwrap()).unwrap();
    let center = ir["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|constraint| constraint["kind"] == "CenterOf")
        .expect("center constraint");
    assert_eq!(center["parameters"]["axis_x"], 1.0);
    assert!(center["parameters"].get("axis_y").is_none());

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--trace"])
        .arg(&under_trace)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "execute should succeed");
    assert!(under_state.exists(), "execute should write GraphState");

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("--json-errors")
        .arg("solve")
        .arg(&under_state)
        .args(["--output"])
        .arg(&under_geometry)
        .output()
        .expect("solve command should spawn");
    assert_eq!(solve.status.code(), Some(4));
    assert!(!under_geometry.exists());
    let diagnostic: serde_json::Value =
        serde_json::from_slice(&solve.stderr).expect("structured geometry diagnostic");
    assert_eq!(diagnostic["errors"][0]["code"], "SCC-GEOM-007");
    assert_eq!(
        diagnostic["errors"][0]["entity_ids"],
        serde_json::json!(["hole"])
    );
    assert_eq!(
        diagnostic["errors"][0]["details"]["remaining_degrees_of_freedom"],
        1
    );

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--mutation"])
        .arg(fixture("partial-center.mutation.json"))
        .args(["--trace"])
        .arg(&complete_trace)
        .status()
        .expect("execute with mutation should spawn");
    assert!(execute.success(), "execute with mutation should succeed");
    assert!(
        complete_state.exists(),
        "execute should write mutated GraphState"
    );

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&complete_state).unwrap()).unwrap();
    assert_eq!(
        state["entities"]["hole"]["parameters"]["y"]["Numeric"],
        12.0
    );

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("solve")
        .arg(&complete_state)
        .args(["--output"])
        .arg(&complete_geometry)
        .status()
        .expect("solve command should spawn");
    assert!(
        solve.success(),
        "fully positioned partial center should solve"
    );

    let geometry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&complete_geometry).unwrap()).unwrap();
    assert_eq!(
        geometry["entities"]["hole"]["primitive"],
        serde_json::json!({
            "kind": "circle",
            "center": {"x": 0.0, "y": 12.0},
            "radius": 10.0
        })
    );
    assert_eq!(
        geometry["solve_report"]["degrees_of_freedom"]["remaining"],
        0
    );
}

#[test]
fn aligned_line_pipeline_accepts_collinear_and_rejects_offset_lines() {
    let dir = output_dir("geometry_alignment_pipeline");
    let ir_path = dir.join("aligned-lines.ir.json");
    let aligned_trace = dir.join("aligned-lines.trace.json");
    let aligned_state = dir.join("aligned-lines.trace.state.json");
    let aligned_geometry = dir.join("aligned-lines.geometry.json");
    let conflict_trace = dir.join("aligned-lines-conflict.trace.json");
    let conflict_state = dir.join("aligned-lines-conflict.trace.state.json");
    let conflict_geometry = dir.join("aligned-lines-conflict.geometry.json");

    let compile = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("compile")
        .arg(fixture("aligned-lines.proposal.json"))
        .args(["--output"])
        .arg(&ir_path)
        .status()
        .expect("compile command should spawn");
    assert!(compile.success(), "compile should succeed");

    let ir: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&ir_path).unwrap()).unwrap();
    assert!(ir["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|constraint| constraint["kind"] == "Align"));

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--mutation"])
        .arg(fixture("aligned-lines.mutation.json"))
        .args(["--trace"])
        .arg(&aligned_trace)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "aligned execution should succeed");
    assert!(aligned_state.exists());

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("solve")
        .arg(&aligned_state)
        .args(["--output"])
        .arg(&aligned_geometry)
        .status()
        .expect("solve command should spawn");
    assert!(solve.success(), "collinear lines should solve");

    let geometry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&aligned_geometry).unwrap()).unwrap();
    assert_eq!(
        geometry["entities"]["subject"]["primitive"],
        serde_json::json!({
            "kind": "line",
            "start": {"x": 8.0, "y": 0.0},
            "end": {"x": 2.0, "y": 0.0}
        })
    );
    assert_eq!(geometry["solve_report"]["evaluated_constraints"], 1);
    assert_eq!(
        geometry["solve_report"]["degrees_of_freedom"]["remaining"],
        0
    );

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--mutation"])
        .arg(fixture("aligned-lines-conflict.mutation.json"))
        .args(["--trace"])
        .arg(&conflict_trace)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "offset execution should succeed");
    assert!(conflict_state.exists());

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("--json-errors")
        .arg("solve")
        .arg(&conflict_state)
        .args(["--output"])
        .arg(&conflict_geometry)
        .output()
        .expect("solve command should spawn");
    assert_eq!(solve.status.code(), Some(4));
    assert!(
        !conflict_geometry.exists(),
        "conflicting alignment must not emit geometry"
    );

    let diagnostic: serde_json::Value =
        serde_json::from_slice(&solve.stderr).expect("structured geometry diagnostic");
    assert_eq!(diagnostic["errors"][0]["code"], "SCC-GEOM-006");
    assert_eq!(
        diagnostic["errors"][0]["entity_ids"],
        serde_json::json!(["subject"])
    );
}

#[test]
fn parallel_line_pipeline_accepts_offset_lines_and_rejects_direction_conflict() {
    let dir = output_dir("geometry_parallel_pipeline");
    let ir_path = dir.join("parallel-lines.ir.json");
    let parallel_trace = dir.join("parallel-lines.trace.json");
    let parallel_state = dir.join("parallel-lines.trace.state.json");
    let parallel_geometry = dir.join("parallel-lines.geometry.json");
    let conflict_trace = dir.join("parallel-lines-conflict.trace.json");
    let conflict_state = dir.join("parallel-lines-conflict.trace.state.json");
    let conflict_geometry = dir.join("parallel-lines-conflict.geometry.json");

    let compile = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("compile")
        .arg(fixture("parallel-lines.proposal.json"))
        .args(["--output"])
        .arg(&ir_path)
        .status()
        .expect("compile command should spawn");
    assert!(compile.success(), "compile should succeed");

    let ir: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&ir_path).unwrap()).unwrap();
    assert!(ir["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|constraint| constraint["kind"] == "Parallel"));

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--mutation"])
        .arg(fixture("parallel-lines.mutation.json"))
        .args(["--trace"])
        .arg(&parallel_trace)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "parallel execution should succeed");
    assert!(parallel_state.exists());

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("solve")
        .arg(&parallel_state)
        .args(["--output"])
        .arg(&parallel_geometry)
        .status()
        .expect("solve command should spawn");
    assert!(solve.success(), "offset parallel lines should solve");

    let geometry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&parallel_geometry).unwrap()).unwrap();
    assert_eq!(
        geometry["entities"]["subject"]["primitive"],
        serde_json::json!({
            "kind": "line",
            "start": {"x": 8.0, "y": 3.0},
            "end": {"x": 2.0, "y": 3.0}
        })
    );
    assert_eq!(geometry["solve_report"]["evaluated_constraints"], 1);
    assert_eq!(
        geometry["solve_report"]["degrees_of_freedom"]["remaining"],
        0
    );

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--mutation"])
        .arg(fixture("parallel-lines-conflict.mutation.json"))
        .args(["--trace"])
        .arg(&conflict_trace)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "conflict execution should succeed");
    assert!(conflict_state.exists());

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("--json-errors")
        .arg("solve")
        .arg(&conflict_state)
        .args(["--output"])
        .arg(&conflict_geometry)
        .output()
        .expect("solve command should spawn");
    assert_eq!(solve.status.code(), Some(4));
    assert!(
        !conflict_geometry.exists(),
        "conflicting parallel relation must not emit geometry"
    );

    let diagnostic: serde_json::Value =
        serde_json::from_slice(&solve.stderr).expect("structured geometry diagnostic");
    assert_eq!(diagnostic["errors"][0]["code"], "SCC-GEOM-006");
    assert_eq!(
        diagnostic["errors"][0]["entity_ids"],
        serde_json::json!(["subject"])
    );
}

#[test]
fn perpendicular_line_pipeline_accepts_right_angle_and_rejects_direction_conflict() {
    let dir = output_dir("geometry_perpendicular_pipeline");
    let ir_path = dir.join("perpendicular-lines.ir.json");
    let perpendicular_trace = dir.join("perpendicular-lines.trace.json");
    let perpendicular_state = dir.join("perpendicular-lines.trace.state.json");
    let perpendicular_geometry = dir.join("perpendicular-lines.geometry.json");
    let conflict_trace = dir.join("perpendicular-lines-conflict.trace.json");
    let conflict_state = dir.join("perpendicular-lines-conflict.trace.state.json");
    let conflict_geometry = dir.join("perpendicular-lines-conflict.geometry.json");

    let compile = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("compile")
        .arg(fixture("perpendicular-lines.proposal.json"))
        .args(["--output"])
        .arg(&ir_path)
        .status()
        .expect("compile command should spawn");
    assert!(compile.success(), "compile should succeed");

    let ir: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&ir_path).unwrap()).unwrap();
    assert!(ir["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|constraint| constraint["kind"] == "Perpendicular"));

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--mutation"])
        .arg(fixture("perpendicular-lines.mutation.json"))
        .args(["--trace"])
        .arg(&perpendicular_trace)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "perpendicular execution should succeed");
    assert!(perpendicular_state.exists());

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("solve")
        .arg(&perpendicular_state)
        .args(["--output"])
        .arg(&perpendicular_geometry)
        .status()
        .expect("solve command should spawn");
    assert!(solve.success(), "perpendicular lines should solve");

    let geometry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&perpendicular_geometry).unwrap()).unwrap();
    assert_eq!(
        geometry["entities"]["subject"]["primitive"],
        serde_json::json!({
            "kind": "line",
            "start": {"x": 3.0, "y": 8.0},
            "end": {"x": 3.0, "y": 2.0}
        })
    );
    assert_eq!(geometry["solve_report"]["evaluated_constraints"], 1);
    assert_eq!(
        geometry["solve_report"]["degrees_of_freedom"]["remaining"],
        0
    );

    let execute = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("execute")
        .arg(&ir_path)
        .args(["--mutation"])
        .arg(fixture("perpendicular-lines-conflict.mutation.json"))
        .args(["--trace"])
        .arg(&conflict_trace)
        .status()
        .expect("execute command should spawn");
    assert!(execute.success(), "conflict execution should succeed");
    assert!(conflict_state.exists());

    let solve = Command::new(env!("CARGO_BIN_EXE_scc"))
        .arg("--json-errors")
        .arg("solve")
        .arg(&conflict_state)
        .args(["--output"])
        .arg(&conflict_geometry)
        .output()
        .expect("solve command should spawn");
    assert_eq!(solve.status.code(), Some(4));
    assert!(
        !conflict_geometry.exists(),
        "conflicting perpendicular relation must not emit geometry"
    );

    let diagnostic: serde_json::Value =
        serde_json::from_slice(&solve.stderr).expect("structured geometry diagnostic");
    assert_eq!(diagnostic["errors"][0]["code"], "SCC-GEOM-006");
    assert_eq!(
        diagnostic["errors"][0]["entity_ids"],
        serde_json::json!(["subject"])
    );
}
