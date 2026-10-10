//! Phase 9.1 back compatibility (ADR-032).
//!
//! A Phase-8 engineering model JSON has no `spatial` key. The
//! `PartSpatialDefinition` field on `EngineeringModel` must therefore:
//! 1. deserialize from legacy documents via `#[serde(default)]`, and
//! 2. never appear in serialized output while empty
//!    (`skip_serializing_if`), so a loaded-and-saved Phase-8 document is
//!    byte-for-byte identical to its source.
//!
//! The fixture `fixtures/model_phase8_legacy.json` is the canonical serialized
//! shape of a representative Phase-8 model (part, base profile, hole, hole
//! pattern, parameter, feature→slot binding, drawing property; no `spatial`
//! key).

use scc_engineering::EngineeringModel;

const LEGACY_JSON_RAW: &str = include_str!("fixtures/model_phase8_legacy.json");

#[test]
fn phase8_legacy_fixture_deserializes_with_empty_spatial() {
    let model: EngineeringModel = serde_json::from_str(LEGACY_JSON_RAW).unwrap();
    assert!(model.spatial.is_empty());
    assert_eq!(model.part.as_ref().unwrap().id.0, "mounting_plate");
    assert_eq!(model.features.len(), 3);
    assert!(model
        .features
        .contains_key(&scc_engineering::FeatureId("bolt_circle".into())));
    assert!(model
        .features
        .contains_key(&scc_engineering::FeatureId("center_hole".into())));
    assert!(model
        .features
        .contains_key(&scc_engineering::FeatureId("plate".into())));
    assert_eq!(model.parameters.len(), 1);
    assert_eq!(
        model
            .feature_parameters
            .get(&scc_engineering::FeatureId("plate".into()))
            .map(|b| b.len()),
        Some(1)
    );
}

#[test]
fn phase8_legacy_reserialization_is_byte_stable_over_100_roundtrips() {
    let model: EngineeringModel = serde_json::from_str(LEGACY_JSON_RAW).unwrap();
    // The canonical bytes have no trailing newline (serde pretty output has
    // none); the fixture file conventionally keeps one.
    let expected = LEGACY_JSON_RAW.trim_end_matches('\n');
    for _ in 0..100 {
        let output = serde_json::to_string_pretty(&model).unwrap();
        assert_eq!(output, expected, "reserialized document diverged");
    }
}
