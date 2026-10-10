use std::collections::BTreeMap;
use std::env;

use scc_cad_backend::CadBackendClient;
use scc_manufacturing::SolidBuildSpec;
use serde_json::json;

#[test]
fn concave_closed_profile_roundtrips_through_rust_python_cadquery_step() {
    let Some(python) = env::var_os("SCC_CAD_PYTHON") else {
        // The normal host test matrix does not require CadQuery. The same
        // test runs against the real worker in the CadQuery-enabled runtime.
        return;
    };
    let spec: SolidBuildSpec = serde_json::from_value(json!({
        "schema_version": 1,
        "part": {
            "body": {
                "footprint": {
                    "kind": "closed_profile",
                    "segments": [
                        {"kind": "line", "start": {"x": 0.0, "y": 0.0}, "end": {"x": 100.0, "y": 0.0}},
                        {"kind": "line", "start": {"x": 100.0, "y": 0.0}, "end": {"x": 100.0, "y": 60.0}},
                        {"kind": "line", "start": {"x": 100.0, "y": 60.0}, "end": {"x": 40.0, "y": 60.0}},
                        {"kind": "line", "start": {"x": 40.0, "y": 60.0}, "end": {"x": 40.0, "y": 100.0}},
                        {"kind": "line", "start": {"x": 40.0, "y": 100.0}, "end": {"x": 0.0, "y": 100.0}},
                        {"kind": "line", "start": {"x": 0.0, "y": 100.0}, "end": {"x": 0.0, "y": 0.0}}
                    ]
                },
                "z": {"min": 0.0, "max": 10.0}
            },
            "bounds": {"min_x": 0.0, "max_x": 100.0, "min_y": 0.0, "max_y": 100.0, "min_z": 0.0, "max_z": 10.0}
        },
        "operations": BTreeMap::<String, serde_json::Value>::new(),
        "source_map": {
            "body": {"feature": "closed_profile", "entity": "closed_profile"},
            "operations": BTreeMap::<String, String>::new()
        }
    })).expect("closed profile fixture must decode");
    spec.validate()
        .expect("closed profile fixture must validate");

    let result = CadBackendClient::new(python)
        .with_args(["-m", "aicad_cad.step_delivery"])
        .execute_step_delivery_backend(&spec)
        .expect("closed profile must pass the real STEP delivery chain");
    assert!(result.equivalence.all_match);
    assert!(result.equivalence.surface_area_match);
    assert!(result.equivalence.center_of_mass_match);
    assert!(result.equivalence.topology_match);
    assert_eq!(result.material_witness.probe_count, 25);
}

#[test]
fn translated_closed_profile_preserves_translation_through_real_step_delivery() {
    let Some(python) = env::var_os("SCC_CAD_PYTHON") else {
        return;
    };
    let origin: SolidBuildSpec = serde_json::from_str(include_str!(
        "../../scc-manufacturing/tests/goldens/closed_profile_solid_spec.json"
    ))
    .expect("ClosedProfile golden must decode");
    let translated: SolidBuildSpec = serde_json::from_str(include_str!(
        "fixtures/translated_closed_profile_solid_build_spec.json"
    ))
    .expect("translated ClosedProfile fixture must decode");
    origin.validate().expect("origin fixture must validate");
    translated
        .validate()
        .expect("translated fixture must validate");

    let origin_result = CadBackendClient::new(python.clone())
        .with_args(["-m", "aicad_cad.step_delivery"])
        .execute_step_delivery_backend(&origin)
        .expect("origin ClosedProfile must pass real STEP delivery");
    let translated_result = CadBackendClient::new(python)
        .with_args(["-m", "aicad_cad.step_delivery"])
        .execute_step_delivery_backend(&translated)
        .expect("translated ClosedProfile must pass real STEP delivery");

    let expected_bbox = (30.0, 130.0, -20.0, 40.0, 0.0, 10.0);
    for witness in [
        &translated_result.source_solid,
        &translated_result.reimported_solid,
    ] {
        assert_eq!(
            (
                witness.bbox.x_min,
                witness.bbox.x_max,
                witness.bbox.y_min,
                witness.bbox.y_max,
                witness.bbox.z_min,
                witness.bbox.z_max,
            ),
            expected_bbox
        );
    }
    for (index, offset) in [(0, 30.0), (1, -20.0), (2, 0.0)] {
        assert!(
            (translated_result.source_solid.center_of_mass[index]
                - origin_result.source_solid.center_of_mass[index]
                - offset)
                .abs()
                <= 1.0e-6
        );
        assert!(
            (translated_result.reimported_solid.center_of_mass[index]
                - origin_result.reimported_solid.center_of_mass[index]
                - offset)
                .abs()
                <= 1.0e-6
        );
    }
    assert!(translated_result.equivalence.all_match);
    assert!(translated_result.equivalence.surface_area_match);
    assert!(translated_result.equivalence.center_of_mass_match);
    assert!(translated_result.equivalence.topology_match);
    assert!(translated_result.equivalence.material_witness_match);
    assert_eq!(
        translated_result.source_solid.edge_count,
        translated_result.reimported_solid.edge_count
    );
    assert_eq!(
        translated_result.source_solid.face_count,
        translated_result.reimported_solid.face_count
    );
    assert_eq!(
        translated_result.source_solid.vertex_count,
        translated_result.reimported_solid.vertex_count
    );
    assert!(translated_result.material_witness.all_match);
}
