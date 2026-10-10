//! Phase 8.1 + 8.2 integration tests: solve an `EngineeringModel` through the
//! real SCC pipeline into a `GeometryModel`, then build a derived
//! `DrawingModel` and verify structure, provenance, roles, fail-closed
//! behavior, inverse queries, integrity, determinism, and serde.

use std::collections::BTreeMap;

use scc_core::ids::EntityId;
use scc_engineering::command::EngineeringCommand;
use scc_engineering::executor::CommandExecutor;
use scc_engineering::feature::{DrawingProperty, EngineeringFeature, HoleDefinition, HolePattern};
use scc_engineering::lowering::lower_and_solve;
use scc_engineering::model::{DrawingState, EngineeringModel};
use scc_engineering::source_map::EngineeringSourceMap;
use scc_engineering::{FeatureId, PartId, Point2D};
use scc_geometry::{GeometryEntity, GeometryModel, GeometryPrimitive, Point2};

use scc_drawing::builder::{build_drawing, compute_geometry_bounds, DrawingInputs};
use scc_drawing::model::DrawingPrimitive;
use scc_drawing::source_map::{
    validate_drawing_source_integrity, DrawingSource, SyntheticDrawingKind,
};
use scc_drawing::{
    DrawingModel, DrawingPoint, DrawingPrimitiveRole, DrawingScale, DrawingSheet, DrawingSheetId,
    DrawingViewId, DrawingViewKind,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A `120 × 80` plate with a four-corner hole pattern (offset 15, Ø10).
fn mounting_plate_model() -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("mounting_plate".into()),
            name: "mounting_plate".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("corner_holes".into()),
                pattern: HolePattern::Corner {
                    offset_x: 15.0,
                    offset_y: 15.0,
                    hole: HoleDefinition { diameter: 10.0 },
                },
            },
        },
    )
    .unwrap();
    model
}

/// A plate with an eight-hole circular pattern (PCD 126, Ø10).
fn circular_plate_model() -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("circular_plate".into()),
            name: "circular_plate".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 160.0,
                    height: 160.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("bolt_pattern".into()),
                pattern: HolePattern::Circular {
                    center: Point2D::new(0.0, 0.0),
                    pitch_circle_diameter: 126.0,
                    count: 8,
                    start_angle: 0.0,
                    hole: HoleDefinition { diameter: 10.0 },
                },
            },
        },
    )
    .unwrap();
    model
}

/// A part with a single center hole (Ø64).
fn center_hole_model() -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("center_hole_part".into()),
            name: "center_hole_part".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 120.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId("center_hole".into()),
                diameter: 64.0,
            },
        },
    )
    .unwrap();
    model
}

/// A part with a construction circle (Ø60 reference-only).
fn construction_circle_model() -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("construction_part".into()),
            name: "construction_part".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::ConstructionCircle {
                id: FeatureId("cc".into()),
                diameter: 60.0,
            },
        },
    )
    .unwrap();
    model
}

/// The Phase 8.4 vertical slice: a `120 × 80` plate with a display title.
fn titled_plate_model(name: &str, width: f64) -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("mounting_plate".into()),
            name: name.into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width,
                    height: 80.0,
                },
            },
        },
    )
    .unwrap();
    model
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Solve a model and build the derived drawing model (default drawing state).
fn solve_and_draw(model: &EngineeringModel) -> (DrawingModel, EngineeringSourceMap) {
    solve_and_draw_with_state(model, &model.drawing)
}

/// Solve a model and build the derived drawing model with an explicit
/// drawing-state override (e.g. a specific engineering scale).
fn solve_and_draw_with_state(
    model: &EngineeringModel,
    state: &DrawingState,
) -> (DrawingModel, EngineeringSourceMap) {
    let (lowering, geometry) = lower_and_solve(model).unwrap();
    let inputs = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        state,
        &model.features,
        Some(model),
        model.part.as_ref(),
    );
    let drawing = build_drawing(&inputs).unwrap();
    (drawing, lowering.source_map)
}

/// Collect the feature that a drawing primitive came from, if any.
fn source_feature(
    drawing: &DrawingModel,
    eng_map: &EngineeringSourceMap,
    prim_id: &scc_drawing::DrawingPrimitiveId,
) -> Option<FeatureId> {
    let entity = drawing.source_map.entity_for_primitive(prim_id)?;
    eng_map.entity_to_feature().get(entity).cloned()
}

/// The role of a drawing primitive by id.
fn role_of(
    drawing: &DrawingModel,
    prim_id: &scc_drawing::DrawingPrimitiveId,
) -> DrawingPrimitiveRole {
    let prim = drawing
        .sheets
        .values()
        .flat_map(|s| s.views.values())
        .flat_map(|v| v.primitives.iter())
        .find(|(id, _)| id == &prim_id)
        .expect("primitive present")
        .1;
    match prim {
        DrawingPrimitive::Point { role, .. }
        | DrawingPrimitive::Line { role, .. }
        | DrawingPrimitive::Circle { role, .. }
        | DrawingPrimitive::Arc { role, .. }
        | DrawingPrimitive::Rectangle { role, .. }
        | DrawingPrimitive::Polygon { role, .. } => *role,
    }
}

/// Build a hand-made `GeometryModel` from a list of (entity_id, primitive).
fn synthetic_geometry(entities: Vec<(&str, GeometryPrimitive)>) -> GeometryModel {
    let mut map = BTreeMap::new();
    for (id, prim) in entities {
        map.insert(
            EntityId(id.into()),
            GeometryEntity {
                id: EntityId(id.into()),
                primitive: prim,
            },
        );
    }
    let mut geometry = GeometryModel::new(0, 1, 0);
    geometry.entities = map;
    geometry
}

/// An engineering source map mapping each entity to `feature`.
fn feature_map_for_entities(entities: &[&str], feature: &FeatureId) -> EngineeringSourceMap {
    let mut map = EngineeringSourceMap::new();
    for e in entities {
        map.record_feature_entity(feature.clone(), EntityId((*e).into()));
    }
    map
}

/// A single-feature engineering feature set keyed by id.
fn features_with(feature: EngineeringFeature) -> BTreeMap<FeatureId, EngineeringFeature> {
    let mut map = BTreeMap::new();
    let id = feature.id().clone();
    map.insert(id, feature);
    map
}

// ---------------------------------------------------------------------------
// Phase 8.1 regression
// ---------------------------------------------------------------------------

#[test]
fn rectangle_vertical_slice_builds_one_principal_sheet_and_view() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    assert_eq!(drawing.sheets.len(), 1);
    let sheet = drawing
        .sheets
        .get(&DrawingSheetId("mounting_plate".into()))
        .expect("sheet keyed by part id");
    assert_eq!(sheet.views.len(), 1);
    let view = sheet
        .views
        .get(&DrawingViewId("principal".into()))
        .expect("principal view present");
    assert_eq!(view.kind, DrawingViewKind::Principal);
    let plate = drawing
        .source_map
        .entity_for_primitive(&scc_drawing::DrawingPrimitiveId("plate".into()))
        .expect("plate entity mapped");
    assert_eq!(plate, &EntityId("plate".into()));
    let plate_prim = &view.primitives[&scc_drawing::DrawingPrimitiveId("plate".into())];
    assert!(
        matches!(plate_prim, DrawingPrimitive::Rectangle { width, height, .. } if *width == 120.0 && *height == 80.0)
    );
}

#[test]
fn rectangle_provenance_traces_back_to_main_feature() {
    let model = mounting_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let feature = source_feature(
        &drawing,
        &eng_map,
        &scc_drawing::DrawingPrimitiveId("plate".into()),
    );
    assert_eq!(feature, Some(FeatureId("plate".into())));
}

#[test]
fn corner_hole_pattern_preserves_single_pattern_feature() {
    let model = mounting_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);

    assert_eq!(model.features.len(), 2, "plate + one pattern feature only");
    let standalone_holes = model
        .features
        .values()
        .filter(|f| matches!(f, EngineeringFeature::Hole { .. }))
        .count();
    assert_eq!(
        standalone_holes, 0,
        "pattern is never expanded into 4 engineering Holes"
    );

    let mut circles_from_pattern = 0;
    for (prim_id, prim) in drawing
        .sheets
        .values()
        .flat_map(|s| s.views.values())
        .flat_map(|v| v.primitives.iter())
    {
        if source_feature(&drawing, &eng_map, prim_id) == Some(FeatureId("corner_holes".into())) {
            assert!(matches!(prim, DrawingPrimitive::Circle { .. }));
            assert_eq!(role_of(&drawing, prim_id), DrawingPrimitiveRole::Object);
            circles_from_pattern += 1;
        }
    }
    assert_eq!(
        circles_from_pattern, 4,
        "four corner holes present as drawing circles"
    );
}

#[test]
fn circular_pattern_preserves_single_pattern_feature() {
    let model = circular_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);

    assert_eq!(model.features.len(), 2, "plate + one pattern feature only");
    let standalone_holes = model
        .features
        .values()
        .filter(|f| matches!(f, EngineeringFeature::Hole { .. }))
        .count();
    assert_eq!(
        standalone_holes, 0,
        "circular pattern is never expanded into separate engineering Holes"
    );

    let mut circles_from_pattern = 0;
    for (prim_id, prim) in drawing
        .sheets
        .values()
        .flat_map(|s| s.views.values())
        .flat_map(|v| v.primitives.iter())
    {
        if source_feature(&drawing, &eng_map, prim_id) == Some(FeatureId("bolt_pattern".into())) {
            assert!(matches!(prim, DrawingPrimitive::Circle { .. }));
            assert_eq!(role_of(&drawing, prim_id), DrawingPrimitiveRole::Object);
            circles_from_pattern += 1;
        }
    }
    assert_eq!(
        circles_from_pattern, 8,
        "eight holes present as drawing circles"
    );
}

// ---------------------------------------------------------------------------
// Phase 8.2 roles
// ---------------------------------------------------------------------------

#[test]
fn ordinary_rectangle_has_object_role() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    assert_eq!(
        role_of(&drawing, &scc_drawing::DrawingPrimitiveId("plate".into())),
        DrawingPrimitiveRole::Object
    );
}

#[test]
fn center_hole_has_object_role_and_provenance() {
    let model = center_hole_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let hole_id = scc_drawing::DrawingPrimitiveId("center_hole".into());
    assert_eq!(role_of(&drawing, &hole_id), DrawingPrimitiveRole::Object);
    assert_eq!(
        source_feature(&drawing, &eng_map, &hole_id),
        Some(FeatureId("center_hole".into()))
    );
}

#[test]
fn construction_circle_has_construction_role() {
    let model = construction_circle_model();
    let (drawing, _) = solve_and_draw(&model);
    let cc_id = scc_drawing::DrawingPrimitiveId("cc".into());
    let prim = drawing
        .sheets
        .values()
        .flat_map(|s| s.views.values())
        .flat_map(|v| v.primitives.iter())
        .find(|(id, _)| *id == &cc_id)
        .map(|(_, p)| p)
        .expect("construction circle primitive present");
    assert!(
        matches!(prim, DrawingPrimitive::Circle { radius, role, .. } if *radius == 30.0 && *role == DrawingPrimitiveRole::Construction)
    );
}

#[test]
fn construction_circle_provenance_traces_to_construction_feature() {
    let model = construction_circle_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let cc_id = scc_drawing::DrawingPrimitiveId("cc".into());
    assert_eq!(
        source_feature(&drawing, &eng_map, &cc_id),
        Some(FeatureId("cc".into()))
    );
}

#[test]
fn construction_circle_classification_is_via_provenance_not_shape() {
    // A ConstructionCircle and an identical-sized ordinary Hole must classify
    // differently even though their solved shapes are the same circle.
    let construction = construction_circle_model();
    let (drawing_cc, _) = solve_and_draw(&construction);
    let cc_id = scc_drawing::DrawingPrimitiveId("cc".into());
    assert_eq!(
        role_of(&drawing_cc, &cc_id),
        DrawingPrimitiveRole::Construction
    );

    // A Hole of the same radius is Object.
    let hole = center_hole_model();
    let (drawing_hole, _) = solve_and_draw(&hole);
    let hole_id = scc_drawing::DrawingPrimitiveId("center_hole".into());
    assert_eq!(
        role_of(&drawing_hole, &hole_id),
        DrawingPrimitiveRole::Object
    );
}

// ---------------------------------------------------------------------------
// Phase 8.2 fail-closed missing mapping
// ---------------------------------------------------------------------------

#[test]
fn missing_feature_mapping_fails_closed_not_synthetic() {
    // A solved entity with no engineering feature mapping must fail closed
    // (SCC-DRW-003), never become a synthetic fallback.
    let geometry = synthetic_geometry(vec![(
        "ghost",
        GeometryPrimitive::Circle {
            center: Point2 { x: 0.0, y: 0.0 },
            radius: 2.0,
        },
    )]);
    let eng_map = EngineeringSourceMap::new(); // empty: no feature linkage
    let features = BTreeMap::new();
    let state = DrawingState::new();
    let inputs = DrawingInputs::new(&geometry, &eng_map, &state, &features, None, None);
    let err = build_drawing(&inputs).unwrap_err();
    assert!(
        matches!(err, scc_drawing::DrawingError::MissingSourceMapping { .. }),
        "missing mapping must fail closed, not silently classify synthetic"
    );
    assert_eq!(err.code(), "SCC-DRW-003");
}

#[test]
fn feature_not_in_engineering_model_is_an_integrity_error() {
    // Entity maps to feature id, but that feature is absent from the passed
    // engineering features map → classification cannot proceed → fail closed.
    let geometry = synthetic_geometry(vec![(
        "e1",
        GeometryPrimitive::Circle {
            center: Point2 { x: 0.0, y: 0.0 },
            radius: 2.0,
        },
    )]);
    let eng_map = feature_map_for_entities(&["e1"], &FeatureId("f1".into()));
    let features = BTreeMap::new(); // f1 not present
    let state = DrawingState::new();
    let inputs = DrawingInputs::new(&geometry, &eng_map, &state, &features, None, None);
    let err = build_drawing(&inputs).unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidSourceIntegrity { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-006");
}

#[test]
fn synthetic_is_expressible_but_never_produced_by_builder() {
    // Synthetic is expressible as a type (future drawing-engine sources)...
    let _source = DrawingSource::Synthetic {
        kind: SyntheticDrawingKind::DrawingEngineGenerated,
    };
    // ...but the geometry→drawing builder never produces it.
    let model = mounting_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let sources = drawing.source_map.drawing_sources(&eng_map).unwrap();
    for source in sources.values() {
        assert!(
            matches!(source, DrawingSource::Engineering { .. }),
            "geometry-backed primitives are always Engineering, never Synthetic"
        );
    }
}

// ---------------------------------------------------------------------------
// Phase 8.2 inverse queries
// ---------------------------------------------------------------------------

#[test]
fn feature_to_primitive_inverse_query_returns_all_pattern_primitives() {
    let model = circular_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let prims = drawing
        .source_map
        .primitives_for_feature(&FeatureId("bolt_pattern".into()), &eng_map);
    assert_eq!(
        prims.len(),
        8,
        "8 drawing primitives for the 8-hole pattern"
    );
    for prim in &prims {
        assert_eq!(
            drawing
                .source_map
                .feature_for_primitive(prim, &eng_map)
                .unwrap(),
            Some(FeatureId("bolt_pattern".into()))
        );
    }
}

#[test]
fn entity_to_primitive_inverse_query_is_stable() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let prims = drawing
        .source_map
        .primitives_for_entity(&EntityId("plate".into()));
    assert_eq!(
        prims,
        std::collections::BTreeSet::from([scc_drawing::DrawingPrimitiveId("plate".into())])
    );
    // A solved hole entity maps to exactly its primitive.
    let hole = drawing
        .source_map
        .primitives_for_entity(&EntityId("corner_holes_array[0]".into()));
    assert_eq!(hole.len(), 1);
    assert!(drawing
        .source_map
        .entity_for_primitive(hole.iter().next().unwrap())
        .is_some());
}

#[test]
fn feature_for_primitive_is_fail_closed_on_unknown_primitive() {
    let model = mounting_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    // Unknown primitive id → Ok(None).
    assert_eq!(
        drawing
            .source_map
            .feature_for_primitive(&scc_drawing::DrawingPrimitiveId("ghost".into()), &eng_map)
            .unwrap(),
        None
    );
}

// ---------------------------------------------------------------------------
// Phase 8.2 source integrity
// ---------------------------------------------------------------------------

#[test]
fn source_integrity_validates_a_normal_drawing() {
    let model = circular_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let (_, geometry) = lower_and_solve(&model).unwrap();
    validate_drawing_source_integrity(&drawing, &geometry, &eng_map, &model.features).unwrap();
}

#[test]
fn source_integrity_fails_on_orphan_source_map_entry() {
    let model = mounting_plate_model();
    let (mut drawing, eng_map) = solve_and_draw(&model);
    let (_, geometry) = lower_and_solve(&model).unwrap();
    // Inject a source-map entry for a primitive that does not exist.
    drawing.source_map.record(
        scc_drawing::DrawingPrimitiveId("ghost".into()),
        EntityId("plate".into()),
    );
    let err = validate_drawing_source_integrity(&drawing, &geometry, &eng_map, &model.features)
        .unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidSourceIntegrity { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-006");
}

#[test]
fn source_integrity_fails_on_missing_source_map_entry() {
    let model = mounting_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let (_, geometry) = lower_and_solve(&model).unwrap();
    // A "damaged" drawing: a primitive present in the view but absent from the
    // source map. Rebuild the map omitting the plate entry.
    let plate_id = scc_drawing::DrawingPrimitiveId("plate".into());
    let mut pruned = scc_drawing::source_map::DrawingSourceMap::new();
    for (id, entity) in drawing.source_map.iter() {
        if id != &plate_id {
            pruned.record(id.clone(), entity.clone());
        }
    }
    let mut damaged = drawing.clone();
    damaged.source_map = pruned;
    let err = validate_drawing_source_integrity(&damaged, &geometry, &eng_map, &model.features)
        .unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidSourceIntegrity { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-006");
}

#[test]
fn source_integrity_fails_when_entity_missing_from_geometry() {
    let model = mounting_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let (_, mut geometry) = lower_and_solve(&model).unwrap();
    // Drop the plate entity from geometry: provenance now points at a
    // nonexistent entity.
    geometry.entities.remove(&EntityId("plate".into()));
    let err = validate_drawing_source_integrity(&drawing, &geometry, &eng_map, &model.features)
        .unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidSourceIntegrity { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-006");
}

// ---------------------------------------------------------------------------
// Phase 8.2 ShowConstructionGeometry decision
// ---------------------------------------------------------------------------

#[test]
fn construction_visible_by_default() {
    let model = construction_circle_model();
    let (drawing, _) = solve_and_draw(&model);
    let cc_id = scc_drawing::DrawingPrimitiveId("cc".into());
    assert_eq!(
        role_of(&drawing, &cc_id),
        DrawingPrimitiveRole::Construction
    );
}

#[test]
fn show_construction_false_omits_construction_but_keeps_provenance_intact() {
    let mut state = DrawingState::new();
    state.set(DrawingProperty::ShowConstructionGeometry { enabled: false });
    let model = construction_circle_model();
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let inputs = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        &state,
        &model.features,
        None,
        model.part.as_ref(),
    );
    let drawing = build_drawing(&inputs).unwrap();
    let cc_id = scc_drawing::DrawingPrimitiveId("cc".into());
    // The construction primitive is not part of this presentation.
    assert!(
        drawing
            .sheets
            .values()
            .flat_map(|s| s.views.values())
            .flat_map(|v| v.primitives.keys())
            .all(|id| id != &cc_id),
        "construction omitted from the drawing when ShowConstructionGeometry=false"
    );
    // Engineering provenance is untouched and re-derivable with visibility on.
    let state_on = DrawingState::new();
    let inputs_on = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        &state_on,
        &model.features,
        None,
        model.part.as_ref(),
    );
    let drawing_on = build_drawing(&inputs_on).unwrap();
    assert_eq!(
        role_of(&drawing_on, &cc_id),
        DrawingPrimitiveRole::Construction
    );
}

// ---------------------------------------------------------------------------
// Phase 8.2 determinism / serde
// ---------------------------------------------------------------------------

#[test]
fn build_is_deterministic_over_one_hundred_runs() {
    let model = circular_plate_model();
    let (drawing0, eng_map0) = solve_and_draw(&model);
    let ser0 = serde_json::to_string(&drawing0).unwrap();
    let sources0 = drawing0.source_map.drawing_sources(&eng_map0).unwrap();
    let view0 = drawing0.principal_view().unwrap();
    for _ in 1..100 {
        let (drawing, eng_map) = solve_and_draw(&model);
        let ser = serde_json::to_string(&drawing).unwrap();
        assert_eq!(ser, ser0);
        assert_eq!(drawing, drawing0);
        // Transform and bounds determinism.
        let view = drawing.principal_view().unwrap();
        assert_eq!(view.transform, view0.transform);
        assert_eq!(view.model_bounds, view0.model_bounds);
        assert_eq!(view.sheet_bounds(), view0.sheet_bounds());
        assert_eq!(
            drawing.principal_sheet().unwrap().bounds(),
            drawing0.principal_sheet().unwrap().bounds()
        );
        let sources = drawing.source_map.drawing_sources(&eng_map).unwrap();
        assert_eq!(sources, sources0);
        assert_eq!(
            drawing
                .source_map
                .primitives_for_feature(&FeatureId("bolt_pattern".into()), &eng_map),
            drawing0
                .source_map
                .primitives_for_feature(&FeatureId("bolt_pattern".into()), &eng_map0)
        );
    }
}

#[test]
fn serde_roundtrip_preserves_roles_source_map_and_provenance() {
    let model = construction_circle_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let json = serde_json::to_string(&drawing).unwrap();
    let back: DrawingModel = serde_json::from_str(&json).unwrap();
    assert_eq!(drawing, back);
    // Role survives round trip.
    let cc_id = scc_drawing::DrawingPrimitiveId("cc".into());
    assert_eq!(role_of(&back, &cc_id), DrawingPrimitiveRole::Construction);
    // Transform and derived sheet bounds survive the round trip.
    assert_eq!(
        back.principal_view().unwrap().sheet_bounds(),
        drawing.principal_view().unwrap().sheet_bounds()
    );
    assert_eq!(
        back.principal_sheet().unwrap().bounds(),
        drawing.principal_sheet().unwrap().bounds()
    );
    // Provenance survives round trip and still classifies as Engineering.
    let sources = back.source_map.drawing_sources(&eng_map).unwrap();
    assert!(matches!(
        sources.get(&cc_id),
        Some(DrawingSource::Engineering { .. })
    ));
}

#[test]
fn source_map_covers_all_primitives() {
    let model = circular_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    assert!(scc_drawing::source_map::DrawingSourceMap::covers_all(
        &drawing
    ));
}

#[test]
fn drawing_scale_is_stored_and_validated() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = &drawing.sheets[&DrawingSheetId("mounting_plate".into())].views
        [&DrawingViewId("principal".into())];
    assert_eq!(view.transform.scale, DrawingScale::default());
    assert_eq!(view.transform.scale.0, 1.0);
    assert!(view.transform.scale.is_valid());
}

#[test]
fn invalid_drawing_scale_fails_closed() {
    let mut state = DrawingState::new();
    state.set(DrawingProperty::Scale { value: 0.0 });
    let geometry = synthetic_geometry(vec![(
        "c",
        GeometryPrimitive::Circle {
            center: Point2 { x: 0.0, y: 0.0 },
            radius: 1.0,
        },
    )]);
    let eng_map = EngineeringSourceMap::new();
    let features = BTreeMap::new();
    let inputs = DrawingInputs::new(&geometry, &eng_map, &state, &features, None, None);
    let err = build_drawing(&inputs).unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidDrawingScale { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-004");
}

#[test]
fn every_supportable_geometry_primitive_converts_losslessly() {
    let cases: Vec<(&str, GeometryPrimitive)> = vec![
        (
            "p",
            GeometryPrimitive::Point {
                position: Point2 { x: 0.0, y: 0.0 },
            },
        ),
        (
            "l",
            GeometryPrimitive::Line {
                start: Point2 { x: 0.0, y: 0.0 },
                end: Point2 { x: 1.0, y: 1.0 },
            },
        ),
        (
            "c",
            GeometryPrimitive::Circle {
                center: Point2 { x: 0.0, y: 0.0 },
                radius: 5.0,
            },
        ),
        (
            "a",
            GeometryPrimitive::Arc {
                center: Point2 { x: 0.0, y: 0.0 },
                radius: 5.0,
                start_angle: 0.0,
                end_angle: std::f64::consts::FRAC_PI_2,
            },
        ),
        (
            "r",
            GeometryPrimitive::Rectangle {
                origin: Point2 { x: -60.0, y: -40.0 },
                width: 120.0,
                height: 80.0,
            },
        ),
        (
            "poly",
            GeometryPrimitive::Polygon {
                vertices: vec![
                    Point2 { x: 0.0, y: 0.0 },
                    Point2 { x: 1.0, y: 0.0 },
                    Point2 { x: 0.0, y: 1.0 },
                ],
            },
        ),
    ];
    let ids: Vec<&str> = cases.iter().map(|(id, _)| *id).collect();
    let geometry = synthetic_geometry(cases.clone());
    let eng_map = feature_map_for_entities(&ids, &FeatureId("base".into()));
    let features = features_with(EngineeringFeature::BaseProfile {
        id: FeatureId("base".into()),
        name: "base".into(),
        profile: scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
    });
    let state = DrawingState::new();
    let inputs = DrawingInputs::new(&geometry, &eng_map, &state, &features, None, None);
    let drawing = build_drawing(&inputs).unwrap();
    let dprims: Vec<&DrawingPrimitive> = drawing
        .sheets
        .values()
        .flat_map(|s| s.views.values())
        .flat_map(|v| v.primitives.values())
        .collect();
    assert_eq!(dprims.len(), 6);
    assert!(dprims
        .iter()
        .any(|p| matches!(p, DrawingPrimitive::Point { .. })));
    assert!(dprims
        .iter()
        .any(|p| matches!(p, DrawingPrimitive::Line { .. })));
    assert!(dprims
        .iter()
        .any(|p| matches!(p, DrawingPrimitive::Circle { .. })));
    assert!(dprims
        .iter()
        .any(|p| matches!(p, DrawingPrimitive::Arc { .. })));
    assert!(dprims
        .iter()
        .any(|p| matches!(p, DrawingPrimitive::Rectangle { .. })));
    assert!(dprims
        .iter()
        .any(|p| matches!(p, DrawingPrimitive::Polygon { .. })));
    // All are Object-role (BaseProfile-backed).
    for p in &dprims {
        let role = match p {
            DrawingPrimitive::Point { role, .. }
            | DrawingPrimitive::Line { role, .. }
            | DrawingPrimitive::Circle { role, .. }
            | DrawingPrimitive::Arc { role, .. }
            | DrawingPrimitive::Rectangle { role, .. }
            | DrawingPrimitive::Polygon { role, .. } => role,
        };
        assert_eq!(*role, DrawingPrimitiveRole::Object);
    }
}

#[test]
fn invalid_geometry_value_fails_closed() {
    // A degenerate polygon (2 vertices) fails geometry validation even though
    // provenance is present.
    let geometry = synthetic_geometry(vec![(
        "poly",
        GeometryPrimitive::Polygon {
            vertices: vec![Point2 { x: 0.0, y: 0.0 }, Point2 { x: 1.0, y: 0.0 }],
        },
    )]);
    let eng_map = feature_map_for_entities(&["poly"], &FeatureId("base".into()));
    let features = features_with(EngineeringFeature::BaseProfile {
        id: FeatureId("base".into()),
        name: "base".into(),
        profile: scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
    });
    let state = DrawingState::new();
    let inputs = DrawingInputs::new(&geometry, &eng_map, &state, &features, None, None);
    let err = build_drawing(&inputs).unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidGeometryValue { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-002");
}

#[test]
fn empty_geometry_fails_closed() {
    let geometry = GeometryModel::new(0, 1, 0);
    let eng_map = EngineeringSourceMap::new();
    let state = DrawingState::new();
    let features = BTreeMap::new();
    let inputs = DrawingInputs::new(&geometry, &eng_map, &state, &features, None, None);
    let err = build_drawing(&inputs).unwrap_err();
    assert!(matches!(err, scc_drawing::DrawingError::EmptyDrawing));
    assert_eq!(err.code(), "SCC-DRW-005");
}

#[test]
fn negative_coordinate_bounds_are_supported() {
    let geometry = synthetic_geometry(vec![(
        "plate",
        GeometryPrimitive::Rectangle {
            origin: Point2 { x: -60.0, y: -40.0 },
            width: 120.0,
            height: 80.0,
        },
    )]);
    let bounds = compute_geometry_bounds(&geometry).unwrap();
    assert_eq!(bounds.min_x, -60.0);
    assert_eq!(bounds.min_y, -40.0);
    assert_eq!(bounds.max_x, 60.0);
    assert_eq!(bounds.max_y, 40.0);
    assert!(bounds.is_finite());
}

#[test]
fn drawing_ids_are_deterministic_and_sorted() {
    let model = mounting_plate_model();
    let (a, _) = solve_and_draw(&model);
    let (b, _) = solve_and_draw(&model);
    let a_ids: Vec<String> = a.sheets.keys().map(|k| k.0.clone()).collect();
    let b_ids: Vec<String> = b.sheets.keys().map(|k| k.0.clone()).collect();
    assert_eq!(a_ids, b_ids);
    let mut sorted = a_ids.clone();
    sorted.sort();
    assert_eq!(a_ids, sorted);
}

#[test]
fn principal_view_is_deterministically_placed_in_the_margin() {
    // Plate model bounds: x −60..60, y −40..40; scale 1:1; margin 10.
    // Placement rule (ADR-024): translation = margin − min × scale
    //   tx = 10 − (−60 × 1) = 70 ; ty = 10 − (−40 × 1) = 50
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().expect("principal view present");
    assert_eq!(view.transform.scale.0, 1.0);
    assert_eq!(view.model_bounds.min_x, -60.0);
    assert_eq!(view.model_bounds.min_y, -40.0);
    assert_eq!(view.transform.translation.x, 70.0);
    assert_eq!(view.transform.translation.y, 50.0);
}

// ---------------------------------------------------------------------------
// Phase 8.3 — coordinate spaces
// ---------------------------------------------------------------------------

#[test]
fn one_to_one_scale_places_content_at_margin_and_derives_sheet_bounds() {
    // 120×80 plate @1:1, margin 10:
    //   sheet content x 10..130, y 10..90
    //   paper envelope 0..140 × 0..100
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    let content = view.sheet_bounds();
    assert_eq!(content.min_x, 10.0);
    assert_eq!(content.min_y, 10.0);
    assert_eq!(content.max_x, 130.0);
    assert_eq!(content.max_y, 90.0);

    let sheet = drawing.principal_sheet().unwrap();
    assert_eq!(DrawingSheet::DEFAULT_SHEET_MARGIN_MM, 10.0);
    let envelope = sheet.bounds();
    assert_eq!(envelope.min_x, 0.0);
    assert_eq!(envelope.min_y, 0.0);
    // Geometry (120+20) widened by the height-dimension extension to x=140,
    // and geometry (80+20) raised by the width-dimension extension to y=100.
    assert_eq!(envelope.max_x, 150.0); // 130 content + 2×10
    assert_eq!(envelope.max_y, 110.0); // 100 content + 2×10
}

#[test]
fn half_scale_places_scaled_content_one_to_two() {
    // 120×80 plate @1:2 → presentation 60×40; envelope 80×60.
    let mut state = DrawingState::new();
    state.set(DrawingProperty::Scale { value: 0.5 });
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw_with_state(&model, &state);
    let view = drawing.principal_view().unwrap();
    assert_eq!(view.transform.scale.0, 0.5);
    // translation = margin − min × scale = 10 − (−60 × 0.5) = 40 / 30
    assert_eq!(view.transform.translation.x, 40.0);
    assert_eq!(view.transform.translation.y, 30.0);
    let content = view.sheet_bounds();
    assert_eq!(content.min_x, 10.0);
    assert_eq!(content.min_y, 10.0);
    assert_eq!(content.max_x, 70.0); // −60..60 × 0.5 + 40 → 10..70
    assert_eq!(content.max_y, 50.0);
    let envelope = drawing.principal_sheet().unwrap().bounds();
    assert_eq!(envelope.max_x, 90.0); // (60+10) content + 20
    assert_eq!(envelope.max_y, 70.0); // (40+10) content + 20
}

#[test]
fn double_scale_places_scaled_content_two_to_one() {
    // 120×80 plate @2:1 → presentation 240×160; envelope 260×180.
    let mut state = DrawingState::new();
    state.set(DrawingProperty::Scale { value: 2.0 });
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw_with_state(&model, &state);
    let view = drawing.principal_view().unwrap();
    assert_eq!(view.transform.scale.0, 2.0);
    // translation = 10 − (−60 × 2) = 130 / 90
    assert_eq!(view.transform.translation.x, 130.0);
    assert_eq!(view.transform.translation.y, 90.0);
    let content = view.sheet_bounds();
    assert_eq!(content.min_x, 10.0);
    assert_eq!(content.min_y, 10.0);
    assert_eq!(content.max_x, 250.0); // 240 + 10
    assert_eq!(content.max_y, 170.0); // 160 + 10
    let envelope = drawing.principal_sheet().unwrap().bounds();
    assert_eq!(envelope.max_x, 270.0); // (240+10) content + 20
    assert_eq!(envelope.max_y, 190.0); // (160+10) content + 20
}

#[test]
fn primitives_stay_in_model_space_while_sheet_positions_move_positive() {
    // Model-space geometry is untouched by placement; only derived sheet
    // coordinates move into the positive margin region.
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();

    let rect = &view.primitives[&scc_drawing::DrawingPrimitiveId("plate".into())];
    match rect {
        DrawingPrimitive::Rectangle { origin, .. } => {
            // Model space unchanged: negative lower-left origin preserved.
            assert_eq!(origin.x, -60.0);
            assert_eq!(origin.y, -40.0);
        }
        other => panic!("expected rectangle, got {other:?}"),
    }
    // Sheet position of the same corner is positive and inside the margin.
    let sheet_origin = view
        .transform
        .model_to_sheet(DrawingPoint::new(-60.0, -40.0));
    assert_eq!(sheet_origin.x, 10.0);
    assert_eq!(sheet_origin.y, 10.0);

    let hole_id = scc_drawing::DrawingPrimitiveId("corner_holes_array[0]".into());
    match &view.primitives[&hole_id] {
        DrawingPrimitive::Circle { center, .. } => {
            assert_eq!(center.x, -45.0); // model space unchanged
            assert_eq!(center.y, -25.0);
        }
        other => panic!("expected circle, got {other:?}"),
    }
}

#[test]
fn pattern_relative_geometry_is_preserved_under_transform() {
    // Corner holes are 90 mm apart in model space (x): (−45,−25)..(45,−25).
    // At 1:1 the sheet distance is 90; at 1:2 it is exactly 45 (scale-only).
    for (scale_value, expected_distance) in [(1.0, 90.0), (0.5, 45.0)] {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: scale_value });
        let model = mounting_plate_model();
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let view = drawing.principal_view().unwrap();
        let to_sheet = |p: DrawingPoint| view.transform.model_to_sheet(p);
        let a = to_sheet(DrawingPoint::new(-45.0, -25.0));
        let b = to_sheet(DrawingPoint::new(45.0, -25.0));
        let distance = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        assert!(
            (distance - expected_distance).abs() < 1.0e-9,
            "scale {scale_value}: hole spacing {distance} != {expected_distance}"
        );
    }
}

#[test]
fn circular_pattern_sheet_pcd_scales_with_drawing_scale() {
    // PCD 126 (radius 63) @1:2 → sheet-space PCD 63 (max pairwise distance).
    let mut state = DrawingState::new();
    state.set(DrawingProperty::Scale { value: 0.5 });
    let model = circular_plate_model();
    let (drawing, eng_map) = solve_and_draw_with_state(&model, &state);
    let view = drawing.principal_view().unwrap();
    let prims = drawing
        .source_map
        .primitives_for_feature(&FeatureId("bolt_pattern".into()), &eng_map);
    let centers: Vec<DrawingPoint> = prims
        .iter()
        .map(|id| match &view.primitives[id] {
            DrawingPrimitive::Circle { center, .. } => view.transform.model_to_sheet(*center),
            other => panic!("expected circle, got {other:?}"),
        })
        .collect();
    assert_eq!(centers.len(), 8);
    let mut max_sq = 0.0_f64;
    for i in 0..centers.len() {
        for j in (i + 1)..centers.len() {
            let dx = centers[i].x - centers[j].x;
            let dy = centers[i].y - centers[j].y;
            max_sq = max_sq.max(dx * dx + dy * dy);
        }
    }
    let pcd_sheet = max_sq.sqrt();
    assert!(
        (pcd_sheet - 63.0).abs() < 1.0e-9,
        "sheet PCD {pcd_sheet} != 63 (126 × 0.5)"
    );
}

#[test]
fn provenance_and_roles_are_identical_across_scales() {
    // Scale / translation must not affect provenance or classification.
    let model = circular_plate_model();
    let (_, eng_map) = solve_and_draw(&model);
    let mut reference: Option<(serde_json::Value, Vec<DrawingPrimitiveRole>)> = None;
    for scale_value in [1.0, 0.5, 2.0] {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: scale_value });
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let sources = drawing.source_map.drawing_sources(&eng_map).unwrap();
        let serialized = serde_json::to_value(&sources).unwrap();
        let roles: Vec<DrawingPrimitiveRole> = drawing
            .principal_view()
            .unwrap()
            .primitives
            .values()
            .map(|p| match p {
                DrawingPrimitive::Point { role, .. }
                | DrawingPrimitive::Line { role, .. }
                | DrawingPrimitive::Circle { role, .. }
                | DrawingPrimitive::Arc { role, .. }
                | DrawingPrimitive::Rectangle { role, .. }
                | DrawingPrimitive::Polygon { role, .. } => *role,
            })
            .collect();
        match &reference {
            None => reference = Some((serialized, roles)),
            Some((ref_ser, ref_roles)) => {
                assert_eq!(&serialized, ref_ser, "provenance changed with scale");
                assert_eq!(&roles, ref_roles, "roles changed with scale");
            }
        }
    }
}

#[test]
fn primitive_to_view_to_sheet_relationship_is_locatable() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let plate_id = scc_drawing::DrawingPrimitiveId("plate".into());
    let (sheet_id, view_id) = drawing
        .locate_primitive(&plate_id)
        .expect("plate primitive located");
    assert_eq!(sheet_id, &DrawingSheetId("mounting_plate".into()));
    assert_eq!(view_id, &DrawingViewId("principal".into()));
    assert!(drawing
        .locate_primitive(&scc_drawing::DrawingPrimitiveId("ghost".into()))
        .is_none());

    let sheet = drawing.principal_sheet().unwrap();
    assert!(sheet.views.contains_key(view_id));
    let view = drawing.principal_view().unwrap();
    assert_eq!(view.id, *view_id);
    assert!(view.primitives.contains_key(&plate_id));
}

#[test]
fn construction_visibility_changes_presentation_bounds_deterministically() {
    // Plate 120×80 plus an oversized construction circle Ø200 (radius 100).
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("mixed_part".into()),
            name: "mixed_part".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::ConstructionCircle {
                id: FeatureId("cc".into()),
                diameter: 200.0,
            },
        },
    )
    .unwrap();

    // Visible (default): presentation bounds include the construction circle.
    let (visible, eng_map) = solve_and_draw(&model);
    let view = visible.principal_view().unwrap();
    assert_eq!(
        view.model_bounds,
        scc_drawing::DrawingBounds {
            min_x: -100.0,
            min_y: -100.0,
            max_x: 100.0,
            max_y: 100.0,
        }
    );
    // Provenance still covers both features.
    assert_eq!(
        source_feature(
            &visible,
            &eng_map,
            &scc_drawing::DrawingPrimitiveId("cc".into())
        ),
        Some(FeatureId("cc".into()))
    );

    // Hidden: bounds follow the filtered presentation (plate only).
    let mut hidden_state = DrawingState::new();
    hidden_state.set(DrawingProperty::ShowConstructionGeometry { enabled: false });
    let (hidden, _) = solve_and_draw_with_state(&model, &hidden_state);
    let view = hidden.principal_view().unwrap();
    assert_eq!(
        view.model_bounds,
        scc_drawing::DrawingBounds {
            min_x: -60.0,
            min_y: -40.0,
            max_x: 60.0,
            max_y: 40.0,
        }
    );
}

#[test]
fn empty_presentation_has_a_deterministic_minimal_envelope() {
    // Construction-only part with construction hidden: valid empty view.
    let model = construction_circle_model();
    let mut hidden_state = DrawingState::new();
    hidden_state.set(DrawingProperty::ShowConstructionGeometry { enabled: false });
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let inputs = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        &hidden_state,
        &model.features,
        None,
        model.part.as_ref(),
    );
    let drawing = build_drawing(&inputs).unwrap();
    let view = drawing.principal_view().unwrap();
    assert!(view.primitives.is_empty());
    // Zero model bounds; degenerate sheet content at the margin corner.
    assert_eq!(view.model_bounds, scc_drawing::DrawingBounds::zero());
    let content = view.sheet_bounds();
    assert_eq!(content.min_x, 10.0);
    assert_eq!(content.min_y, 10.0);
    assert_eq!(content.max_x, 10.0);
    assert_eq!(content.max_y, 10.0);
    // Minimal deterministic envelope 20×20, all finite, no negatives.
    let envelope = drawing.principal_sheet().unwrap().bounds();
    assert_eq!(envelope.min_x, 0.0);
    assert_eq!(envelope.min_y, 0.0);
    assert_eq!(envelope.max_x, 20.0);
    assert_eq!(envelope.max_y, 20.0);
    assert!(envelope.is_finite());
    // The transform itself is finite.
    assert!(view.transform.translation.x.is_finite());
    assert!(view.transform.translation.y.is_finite());
    // Truly empty geometry stays fail-closed (distinct case).
    let empty_geometry = GeometryModel::new(0, 1, 0);
    let eng_map = EngineeringSourceMap::new();
    let features = BTreeMap::new();
    let state = DrawingState::new();
    let inputs = DrawingInputs::new(&empty_geometry, &eng_map, &state, &features, None, None);
    let err = build_drawing(&inputs).unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-005");
}

#[test]
fn invalid_transform_fails_closed() {
    use scc_drawing::{DrawingError, DrawingViewTransform};
    // Non-finite translation is rejected at construction.
    let err = DrawingViewTransform::new(
        scc_drawing::DrawingScale(1.0),
        DrawingPoint::new(f64::NAN, 0.0),
    )
    .unwrap_err();
    assert!(matches!(err, DrawingError::InvalidDrawingTransform { .. }));
    assert_eq!(err.code(), "SCC-DRW-007");
    // Non-positive scale is rejected too.
    let err =
        DrawingViewTransform::new(scc_drawing::DrawingScale(0.0), DrawingPoint::new(0.0, 0.0))
            .unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-007");
    // A valid transform composes exactly.
    let t = DrawingViewTransform::new(
        scc_drawing::DrawingScale(0.5),
        DrawingPoint::new(40.0, 30.0),
    )
    .unwrap();
    let sheet = t.model_to_sheet(DrawingPoint::new(-60.0, -40.0));
    assert_eq!(sheet.x, 10.0);
    assert_eq!(sheet.y, 10.0);
    // Composition identity: model→view→sheet == model→sheet.
    let p = DrawingPoint::new(-33.5, 12.25);
    let composed = t.view_to_sheet(t.model_to_view(p));
    assert_eq!(composed, t.model_to_sheet(p));
}

#[test]
fn every_primitive_kind_has_finite_derived_sheet_bounds() {
    // Reuse the lossless six-kind fixture; verify sheet bounds derive from
    // model bounds through the transform and stay finite/positive-placed.
    let cases: Vec<(&str, GeometryPrimitive)> = vec![
        (
            "p",
            GeometryPrimitive::Point {
                position: Point2 { x: 0.0, y: 0.0 },
            },
        ),
        (
            "l",
            GeometryPrimitive::Line {
                start: Point2 { x: 0.0, y: 0.0 },
                end: Point2 { x: 1.0, y: 1.0 },
            },
        ),
        (
            "c",
            GeometryPrimitive::Circle {
                center: Point2 { x: 0.0, y: 0.0 },
                radius: 5.0,
            },
        ),
        (
            "a",
            GeometryPrimitive::Arc {
                center: Point2 { x: 0.0, y: 0.0 },
                radius: 5.0,
                start_angle: 0.0,
                end_angle: std::f64::consts::FRAC_PI_2,
            },
        ),
        (
            "r",
            GeometryPrimitive::Rectangle {
                origin: Point2 { x: -6.0, y: -4.0 },
                width: 12.0,
                height: 8.0,
            },
        ),
        (
            "poly",
            GeometryPrimitive::Polygon {
                vertices: vec![
                    Point2 { x: 0.0, y: 0.0 },
                    Point2 { x: 1.0, y: 0.0 },
                    Point2 { x: 0.0, y: 1.0 },
                ],
            },
        ),
    ];
    let ids: Vec<&str> = cases.iter().map(|(id, _)| *id).collect();
    let geometry = synthetic_geometry(cases);
    let eng_map = feature_map_for_entities(&ids, &FeatureId("base".into()));
    let features = features_with(EngineeringFeature::BaseProfile {
        id: FeatureId("base".into()),
        name: "base".into(),
        profile: scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
    });
    let state = DrawingState::new();
    let inputs = DrawingInputs::new(&geometry, &eng_map, &state, &features, None, None);
    let drawing = build_drawing(&inputs).unwrap();
    let view = drawing.principal_view().unwrap();
    // Model bounds: rectangle spans x −6..6; the radius-5 circle spans
    // y −5..5 and dominates the Y extent.
    assert_eq!(view.model_bounds.min_x, -6.0);
    assert_eq!(view.model_bounds.min_y, -5.0);
    assert_eq!(view.model_bounds.max_x, 6.0);
    assert_eq!(view.model_bounds.max_y, 5.0);
    let sheet = view.sheet_bounds();
    // Placement rule: scaled minimum lands on the margin → content min is
    // exactly (10, 10); max = translation + positive extent.
    assert_eq!(sheet.min_x, 10.0);
    assert_eq!(sheet.min_y, 10.0);
    assert_eq!(sheet.max_x, 22.0); // 6 + (10 + 6)
    assert_eq!(sheet.max_y, 20.0); // 5 + (10 + 5)
    assert!(sheet.is_finite());
}

// ---------------------------------------------------------------------------
// Phase 8.4 — basic engineering annotations (PartTitle)
// ---------------------------------------------------------------------------

use scc_drawing::{
    DrawingAnnotationId, DrawingAnnotationKind, DrawingAnnotationSource, DrawingTextAnchor,
    DEFAULT_ANNOTATION_TEXT_HEIGHT_MM,
};
use scc_engineering::model::PartDefinition;

#[test]
fn part_title_vertical_slice() {
    // Part { id = mounting_plate, name = "Mounting Plate" }, 120×80 @1:1.
    let model = titled_plate_model("Mounting Plate", 120.0);
    let (drawing, _) = solve_and_draw(&model);
    let title_id = DrawingAnnotationId("part_title".into());
    let sheet = drawing.principal_sheet().unwrap();
    let title = sheet.annotation(&title_id).expect("part title present");
    assert_eq!(title.kind, DrawingAnnotationKind::PartTitle);
    assert_eq!(title.text, "Mounting Plate");
    // Deterministic placement inside the bottom margin: (margin, margin/2).
    assert_eq!(title.position.x, 10.0);
    assert_eq!(title.position.y, 5.0);
    assert_eq!(title.anchor, DrawingTextAnchor::Start);
    assert_eq!(title.text_height_mm, 3.5);
    assert_eq!(title.text_height_mm, DEFAULT_ANNOTATION_TEXT_HEIGHT_MM);
    assert!(matches!(
        &title.source,
        DrawingAnnotationSource::PartTitle { part } if part == &PartId("mounting_plate".into())
    ));
}

#[test]
fn title_is_sheet_level_and_never_touches_geometry_placement() {
    // The same geometry built with and without part metadata must produce
    // identical view bounds and transform; the annotation lives on the sheet.
    let geometry_model = titled_plate_model("Mounting Plate", 120.0);
    let (with_title, eng_map) = solve_and_draw(&geometry_model);

    let (lowering, geometry) = lower_and_solve(&geometry_model).unwrap();
    let state = DrawingState::new();
    let features = geometry_model.features.clone();
    let no_part_inputs = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        &state,
        &features,
        None,
        None,
    );
    let without_title = build_drawing(&no_part_inputs).unwrap();

    let v_titled = with_title.principal_view().unwrap();
    let v_plain = without_title.principal_view().unwrap();
    assert_eq!(v_titled.transform, v_plain.transform);
    assert_eq!(v_titled.model_bounds, v_plain.model_bounds);
    assert_eq!(v_titled.sheet_bounds(), v_plain.sheet_bounds());
    // Geometry placement is identical; the dimensioned build may only grow
    // the envelope toward +X/+Y (ADR-027) — never move the origin.
    let with_bounds = with_title.principal_sheet().unwrap().bounds();
    let plain_bounds = without_title.principal_sheet().unwrap().bounds();
    assert_eq!(with_bounds.min_x, plain_bounds.min_x);
    assert_eq!(with_bounds.min_y, plain_bounds.min_y);
    assert!(with_bounds.max_x >= plain_bounds.max_x);
    assert!(with_bounds.max_y >= plain_bounds.max_y);
    assert!(without_title
        .principal_sheet()
        .unwrap()
        .annotations
        .is_empty());
    // Primitive provenance is unaffected by the annotation's existence.
    let plate_id = scc_drawing::DrawingPrimitiveId("plate".into());
    assert_eq!(
        source_feature(&with_title, &eng_map, &plate_id),
        source_feature(&without_title, &lowering.source_map, &plate_id),
    );
}

#[test]
fn drawing_scale_never_scales_the_title() {
    // Same title text / position / height at 1:2, 1:1, and 2:1; only the
    // geometry presentation changes.
    let mut reference: Option<(String, [f64; 2], f64)> = None;
    for scale_value in [0.5, 1.0, 2.0] {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: scale_value });
        let model = titled_plate_model("Mounting Plate", 120.0);
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let title = drawing
            .annotation(&DrawingAnnotationId("part_title".into()))
            .unwrap()
            .clone();
        let current = (
            title.text.clone(),
            [title.position.x, title.position.y],
            title.text_height_mm,
        );
        match &reference {
            None => reference = Some(current),
            Some(r) => assert_eq!(&current, r, "title changed with drawing scale"),
        }
    }
    // And the geometry presentation really did change between scales
    // (model bounds are scale-invariant by design; sheet bounds are not):
    let model = titled_plate_model("Mounting Plate", 120.0);
    let (half, _) = {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: 0.5 });
        solve_and_draw_with_state(&model, &state)
    };
    let (double, _) = {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: 2.0 });
        solve_and_draw_with_state(&model, &state)
    };
    assert_eq!(
        half.principal_view().unwrap().model_bounds,
        double.principal_view().unwrap().model_bounds
    );
    assert_ne!(
        half.principal_view().unwrap().sheet_bounds(),
        double.principal_view().unwrap().sheet_bounds()
    );
    assert_ne!(
        half.principal_sheet().unwrap().bounds(),
        double.principal_sheet().unwrap().bounds()
    );
}

#[test]
fn title_provenance_is_strongly_typed_to_the_part() {
    let model = titled_plate_model("Mounting Plate", 120.0);
    let (drawing, _) = solve_and_draw(&model);
    let title = drawing
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    match &title.source {
        DrawingAnnotationSource::PartTitle { part } => {
            assert_eq!(part, &PartId("mounting_plate".into()));
        }
    }
    // Query by provenance works without string scanning.
    let found = drawing.annotations_for_part(&PartId("mounting_plate".into()));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.id, DrawingAnnotationId("part_title".into()));
    assert!(drawing
        .annotations_for_part(&PartId("other".into()))
        .is_empty());
}

#[test]
fn unicode_title_roundtrips_exactly() {
    let name = "复杂异形精密安装板";
    let model = titled_plate_model(name, 120.0);
    let (drawing, _) = solve_and_draw(&model);
    let title = drawing
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    assert_eq!(title.text, name);
    let back: scc_drawing::DrawingModel =
        serde_json::from_str(&serde_json::to_string(&drawing).unwrap()).unwrap();
    assert_eq!(back, drawing);
    assert_eq!(
        back.annotation(&DrawingAnnotationId("part_title".into()))
            .unwrap()
            .text,
        name
    );
}

#[test]
fn special_characters_stored_raw_not_escaped() {
    let name = r#"A&B <Plate> "V2""#;
    let model = titled_plate_model(name, 120.0);
    let (drawing, _) = solve_and_draw(&model);
    let title = drawing
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    // Raw string preserved — SVG/DXF escaping is a renderer concern.
    assert_eq!(title.text, name);
    assert!(!title.text.contains("&amp;"));
    assert!(!title.text.contains("&lt;"));
}

#[test]
fn renamed_part_yields_new_title_on_rebuild() {
    // No rename command exists; derivation is proven with two models that
    // differ only in the part name.
    let a = titled_plate_model("Mounting Plate", 120.0);
    let b = titled_plate_model("Precision Mounting Plate", 120.0);
    let (da, _) = solve_and_draw(&a);
    let (db, _) = solve_and_draw(&b);
    let ta = da
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    let tb = db
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    assert_eq!(ta.text, "Mounting Plate");
    assert_eq!(tb.text, "Precision Mounting Plate");
    assert_ne!(ta.text, tb.text);
}

#[test]
fn parameter_change_keeps_title_stable_while_geometry_moves() {
    // plate width 120 -> 160: geometry / sheet bounds change, the part
    // name does not, so the title is byte-identical.
    let (narrow, _) = solve_and_draw(&titled_plate_model("Mounting Plate", 120.0));
    let (wide, _) = solve_and_draw(&titled_plate_model("Mounting Plate", 160.0));
    let tn = narrow
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    let tw = wide
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    assert_eq!(tn.text, tw.text);
    assert_eq!(tn.position, tw.position);
    assert_eq!(tn.text_height_mm, tw.text_height_mm);
    assert_eq!(tn.source, tw.source);
    assert_ne!(
        narrow.principal_view().unwrap().model_bounds,
        wide.principal_view().unwrap().model_bounds
    );
}

#[test]
fn missing_part_metadata_builds_without_a_title() {
    // Geometry-only drawing (part = None) is legitimate: no title, no failure.
    let model = titled_plate_model("Mounting Plate", 120.0);
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let state = DrawingState::new();
    let features = model.features.clone();
    let inputs = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        &state,
        &features,
        None,
        None,
    );
    let drawing = build_drawing(&inputs).unwrap();
    assert!(drawing.principal_sheet().unwrap().annotations.is_empty());
    assert!(drawing
        .annotation(&DrawingAnnotationId("part_title".into()))
        .is_none());
}

#[test]
fn blank_part_name_skips_the_title_instead_of_failing() {
    // Hand-built PartDefinition with a blank display name: the geometry
    // drawing must still build; it simply has no title.
    let model = titled_plate_model("Mounting Plate", 120.0);
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let state = DrawingState::new();
    let features = model.features.clone();
    let blank_part = PartDefinition {
        id: PartId("mounting_plate".into()),
        name: String::new(),
    };
    let inputs = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        &state,
        &features,
        None,
        Some(&blank_part),
    );
    let drawing = build_drawing(&inputs).unwrap();
    assert!(drawing.principal_sheet().unwrap().annotations.is_empty());
}

#[test]
fn control_character_name_fails_closed() {
    // An interior newline cannot come from CreatePart validation but can be
    // hand-constructed; the drawing layer surfaces it (SCC-DRW-008) instead
    // of silently altering or rendering it.
    let model = titled_plate_model("Mounting Plate", 120.0);
    let (lowering, geometry) = lower_and_solve(&model).unwrap();
    let state = DrawingState::new();
    let features = model.features.clone();
    let bad_part = PartDefinition {
        id: PartId("mounting_plate".into()),
        name: "Bad\nName".into(),
    };
    let inputs = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        &state,
        &features,
        None,
        Some(&bad_part),
    );
    let err = build_drawing(&inputs).unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidDrawingAnnotation { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-008");
}

#[test]
fn annotation_integrity_validator_passes_valid_and_catches_damage() {
    let model = titled_plate_model("Mounting Plate", 120.0);
    let (drawing, _) = solve_and_draw(&model);
    let part_ref = model.part.as_ref();

    // A freshly built drawing passes.
    scc_drawing::validate_annotation_integrity(&drawing.sheets, part_ref).unwrap();

    let title_id = DrawingAnnotationId("part_title".into());

    // Damaged: text no longer matches the authoritative part name.
    let mut damaged = drawing.clone();
    let sheet = damaged.sheets.values_mut().next().unwrap();
    if let Some(title) = sheet.annotations.get_mut(&title_id) {
        title.text = "Tampered".into();
    }
    assert_eq!(
        scc_drawing::validate_annotation_integrity(&damaged.sheets, part_ref)
            .unwrap_err()
            .code(),
        "SCC-DRW-008"
    );

    // Damaged: non-positive text height.
    let mut damaged = drawing.clone();
    let sheet = damaged.sheets.values_mut().next().unwrap();
    if let Some(title) = sheet.annotations.get_mut(&title_id) {
        title.text_height_mm = 0.0;
    }
    assert_eq!(
        scc_drawing::validate_annotation_integrity(&damaged.sheets, part_ref)
            .unwrap_err()
            .code(),
        "SCC-DRW-008"
    );

    // Damaged: non-finite position.
    let mut damaged = drawing;
    let sheet = damaged.sheets.values_mut().next().unwrap();
    if let Some(title) = sheet.annotations.get_mut(&title_id) {
        title.position.x = f64::NAN;
    }
    assert_eq!(
        scc_drawing::validate_annotation_integrity(&damaged.sheets, part_ref)
            .unwrap_err()
            .code(),
        "SCC-DRW-008"
    );
}

#[test]
fn part_title_is_deterministic_over_one_hundred_runs() {
    let model = titled_plate_model("Mounting Plate", 120.0);
    let (drawing0, _) = solve_and_draw(&model);
    let ser0 = serde_json::to_string(&drawing0).unwrap();
    for _ in 1..100 {
        let (drawing, _) = solve_and_draw(&model);
        assert_eq!(drawing, drawing0);
        assert_eq!(serde_json::to_string(&drawing).unwrap(), ser0);
        let t0 = drawing0
            .annotation(&DrawingAnnotationId("part_title".into()))
            .unwrap();
        let t = drawing
            .annotation(&DrawingAnnotationId("part_title".into()))
            .unwrap();
        assert_eq!(t.id, t0.id);
        assert_eq!(t.text, t0.text);
        assert_eq!(t.position, t0.position);
        assert_eq!(t.text_height_mm, t0.text_height_mm);
        assert_eq!(t.anchor, t0.anchor);
        assert_eq!(t.source, t0.source);
    }
}
// ---------------------------------------------------------------------------
// Phase 8.5 — dimension model
// ---------------------------------------------------------------------------

use scc_drawing::{
    DimensionValue, DimensionValueSource, DrawingDimensionId, DrawingDimensionKind, LinearAxis,
    ResolvedDimensionValue,
};
use scc_engineering::binding::FeatureParameterSlot;
use scc_engineering::expression::Expression;
use scc_engineering::parameter::ParameterValue;
use scc_engineering::{ParameterId as EngParameterId, ParameterOrigin};

/// The resolved value of a single-measurement dimension.
fn measured_resolved(
    view: &scc_drawing::DrawingView,
    id: &DrawingDimensionId,
) -> ResolvedDimensionValue {
    match &view.dimensions[id].value {
        DimensionValue::Measured { resolved, .. } => *resolved,
        DimensionValue::Pattern { .. }
        | DimensionValue::HoleCallout { .. }
        | DimensionValue::SlotCallout { .. }
        | DimensionValue::ParametricHoleCallout { .. } => {
            panic!("expected a measured dimension")
        }
    }
}

/// A plate whose width is driven by the `plate_width` parameter.
fn parameter_plate_model(width: f64) -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("main".into()),
            name: "Main Plate".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::DeclareParameter {
            id: EngParameterId("plate_width".into()),
            value: ParameterValue::Scalar(width),
            origin: ParameterOrigin::User,
            authority: scc_core::authority::AuthorityLevel::L0,
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("main".into()),
                name: "main".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::BindFeatureParameter {
            feature: FeatureId("main".into()),
            slot: FeatureParameterSlot::Width,
            parameter: EngParameterId("plate_width".into()),
        },
    )
    .unwrap();
    model
}

/// A part with a hole whose diameter is derived:
/// `hole_diameter := plate_height / 8`.
fn derived_hole_model(plate_height: f64) -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("derived_part".into()),
            name: "derived_part".into(),
        },
    )
    .unwrap();
    for (pid, value) in [
        ("plate_height", ParameterValue::Scalar(plate_height)),
        ("plate_width", ParameterValue::Scalar(120.0)),
        ("hole_diameter", ParameterValue::Unresolved),
    ] {
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::DeclareParameter {
                id: EngParameterId(pid.into()),
                value,
                origin: ParameterOrigin::User,
                authority: scc_core::authority::AuthorityLevel::L0,
            },
        )
        .unwrap();
    }
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::SetExpression {
            target: EngParameterId("hole_diameter".into()),
            expression: Expression::Divide {
                left: Box::new(Expression::Reference {
                    parameter: EngParameterId("plate_height".into()),
                }),
                right: Box::new(Expression::Constant { value: 8.0 }),
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::BindFeatureParameter {
            feature: FeatureId("plate".into()),
            slot: FeatureParameterSlot::Height,
            parameter: EngParameterId("plate_height".into()),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId("center_hole".into()),
                diameter: 10.0,
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::BindFeatureParameter {
            feature: FeatureId("center_hole".into()),
            slot: FeatureParameterSlot::Diameter,
            parameter: EngParameterId("hole_diameter".into()),
        },
    )
    .unwrap();
    model
}

#[test]
fn base_profile_linear_dimensions() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();

    let w = &view.dimensions[&DrawingDimensionId("plate.width".into())];
    assert_eq!(
        w.kind,
        DrawingDimensionKind::Linear {
            axis: LinearAxis::Horizontal
        }
    );
    assert_eq!(
        measured_resolved(view, &DrawingDimensionId("plate.width".into())),
        ResolvedDimensionValue::LengthMm { mm: 120.0 }
    );
    assert!(matches!(
        &w.value,
        DimensionValue::Measured {
            source: DimensionValueSource::FeatureSlot { feature, slot },
            ..
        } if feature == &FeatureId("plate".into()) && *slot == FeatureParameterSlot::Width
    ));

    let h = &view.dimensions[&DrawingDimensionId("plate.height".into())];
    assert_eq!(
        h.kind,
        DrawingDimensionKind::Linear {
            axis: LinearAxis::Vertical
        }
    );
    assert_eq!(
        measured_resolved(view, &DrawingDimensionId("plate.height".into())),
        ResolvedDimensionValue::LengthMm { mm: 80.0 }
    );

    // No formatted strings anywhere in the dimension model.
    let json = serde_json::to_string(&w).unwrap();
    assert!(!json.contains("\"120\""), "values must stay numeric");
}

#[test]
fn parameter_backed_width_and_update_120_to_160() {
    // Bound: source must be the Parameter, not the stale feature literal.
    let model = parameter_plate_model(120.0);
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    assert_eq!(
        measured_resolved(view, &DrawingDimensionId("main.width".into())),
        ResolvedDimensionValue::LengthMm { mm: 120.0 }
    );
    assert!(matches!(
        &view.dimensions[&DrawingDimensionId("main.width".into())].value,
        DimensionValue::Measured {
            source: DimensionValueSource::Parameter { parameter },
            ..
        } if parameter == &EngParameterId("plate_width".into())
    ));

    // Update 120 -> 160: same id, same source; only the resolved value moves.
    let updated = parameter_plate_model(160.0);
    let (drawing2, _) = solve_and_draw(&updated);
    let view2 = drawing2.principal_view().unwrap();
    assert_eq!(
        measured_resolved(view2, &DrawingDimensionId("main.width".into())),
        ResolvedDimensionValue::LengthMm { mm: 160.0 }
    );
    let before = &view.dimensions[&DrawingDimensionId("main.width".into())];
    let after = &view2.dimensions[&DrawingDimensionId("main.width".into())];
    assert_eq!(before.id, after.id);
    assert_eq!(before.kind, after.kind);
    assert_eq!(before.target, after.target);
    match (&before.value, &after.value) {
        (
            DimensionValue::Measured { source: s1, .. },
            DimensionValue::Measured { source: s2, .. },
        ) => assert_eq!(s1, s2),
        _ => panic!("expected measured values"),
    }
}

#[test]
fn hole_diameter_dimension_semantics() {
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    assert_eq!(
        measured_resolved(view, &DrawingDimensionId("center_hole.diameter".into())),
        ResolvedDimensionValue::LengthMm { mm: 64.0 }
    );
    assert_eq!(
        view.dimensions[&DrawingDimensionId("center_hole.diameter".into())].kind,
        DrawingDimensionKind::Diameter
    );
    assert!(matches!(
        &view.dimensions[&DrawingDimensionId("center_hole.diameter".into())].value,
        DimensionValue::Measured {
            source: DimensionValueSource::FeatureSlot { feature, slot },
            ..
        } if feature == &FeatureId("center_hole".into())
            && *slot == FeatureParameterSlot::Diameter
    ));
}

#[test]
fn derived_hole_diameter_propagates_10_to_12() {
    // plate_height 80 -> hole_diameter := 80/8 = 10.
    let model = derived_hole_model(80.0);
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    assert_eq!(
        measured_resolved(view, &DrawingDimensionId("center_hole.diameter".into())),
        ResolvedDimensionValue::LengthMm { mm: 10.0 }
    );
    assert!(matches!(
        &view.dimensions[&DrawingDimensionId("center_hole.diameter".into())].value,
        DimensionValue::Measured {
            source: DimensionValueSource::Parameter { parameter },
            ..
        } if parameter == &EngParameterId("hole_diameter".into())
    ));

    // plate_height 96 -> hole_diameter becomes 12 automatically; the source is
    // unchanged and the expression was never touched by the drawing layer.
    let model96 = derived_hole_model(96.0);
    let (drawing2, _) = solve_and_draw(&model96);
    let view2 = drawing2.principal_view().unwrap();
    assert_eq!(
        measured_resolved(view2, &DrawingDimensionId("center_hole.diameter".into())),
        ResolvedDimensionValue::LengthMm { mm: 12.0 }
    );
    assert_eq!(
        view.dimensions[&DrawingDimensionId("center_hole.diameter".into())].sources(),
        view2.dimensions[&DrawingDimensionId("center_hole.diameter".into())].sources()
    );
}

#[test]
fn circular_pattern_single_structured_dimension() {
    let model = circular_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();

    // Exactly one Pattern dimension for the whole pattern feature.
    let pattern_id = DrawingDimensionId("bolt_pattern.pattern".into());
    assert!(view.dimensions.contains_key(&pattern_id));
    let dim = &view.dimensions[&pattern_id];
    assert_eq!(dim.kind, DrawingDimensionKind::Pattern);
    assert_eq!(
        dim.target,
        scc_drawing::DrawingDimensionTarget::Feature {
            feature: FeatureId("bolt_pattern".into())
        }
    );

    // Structured components: count 8, PCD 126, hole Ø10 — from engineering.
    let DimensionValue::Pattern { shape } = &dim.value else {
        panic!("expected pattern value");
    };
    let scc_drawing::PatternShape::Circular {
        count,
        pitch_circle_diameter,
        hole_diameter,
    } = shape
    else {
        panic!("expected circular shape");
    };
    assert_eq!(count.resolved, ResolvedDimensionValue::Count { count: 8 });
    assert_eq!(
        pitch_circle_diameter.resolved,
        ResolvedDimensionValue::LengthMm { mm: 126.0 }
    );
    assert_eq!(
        hole_diameter.resolved,
        ResolvedDimensionValue::LengthMm { mm: 10.0 }
    );

    // No per-circle diameter dimensions: the only Diameter dims are none, and
    // exactly 8 primitives still map to the single pattern feature.
    assert!(!view
        .dimensions
        .keys()
        .any(|id| id.0.starts_with("bolt_pattern_array")));
    let prims = drawing
        .source_map
        .primitives_for_feature(&FeatureId("bolt_pattern".into()), &eng_map);
    assert_eq!(prims.len(), 8);
}

#[test]
fn corner_and_grid_patterns_get_structured_dimensions() {
    // Corner (mounting plate): offsets 15/15, hole Ø10.
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    let dim = &view.dimensions[&DrawingDimensionId("corner_holes.pattern".into())];
    let DimensionValue::Pattern {
        shape:
            scc_drawing::PatternShape::Corner {
                offset_x,
                offset_y,
                hole_diameter,
            },
    } = &dim.value
    else {
        panic!("expected corner shape");
    };
    assert_eq!(
        offset_x.resolved,
        ResolvedDimensionValue::LengthMm { mm: 15.0 }
    );
    assert_eq!(
        offset_y.resolved,
        ResolvedDimensionValue::LengthMm { mm: 15.0 }
    );
    assert_eq!(
        hole_diameter.resolved,
        ResolvedDimensionValue::LengthMm { mm: 10.0 }
    );

    // Grid 2×3 spacing 40/40 Ø6.
    let mut grid = EngineeringModel::new();
    CommandExecutor::apply(
        &mut grid,
        &EngineeringCommand::CreatePart {
            id: PartId("grid_part".into()),
            name: "grid_part".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut grid,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("grid_holes".into()),
                pattern: HolePattern::Grid {
                    rows: 2,
                    columns: 3,
                    spacing_x: 40.0,
                    spacing_y: 40.0,
                    hole: HoleDefinition { diameter: 6.0 },
                },
            },
        },
    )
    .unwrap();
    let (drawing_g, _) = solve_and_draw(&grid);
    let gview = drawing_g.principal_view().unwrap();
    let gdim = &gview.dimensions[&DrawingDimensionId("grid_holes.pattern".into())];
    let DimensionValue::Pattern {
        shape:
            scc_drawing::PatternShape::Grid {
                rows,
                columns,
                spacing_x,
                spacing_y,
                hole_diameter,
            },
    } = &gdim.value
    else {
        panic!("expected grid shape");
    };
    assert_eq!(rows.resolved, ResolvedDimensionValue::Count { count: 2 });
    assert_eq!(columns.resolved, ResolvedDimensionValue::Count { count: 3 });
    assert_eq!(
        spacing_x.resolved,
        ResolvedDimensionValue::LengthMm { mm: 40.0 }
    );
    assert_eq!(
        spacing_y.resolved,
        ResolvedDimensionValue::LengthMm { mm: 40.0 }
    );
    assert_eq!(
        hole_diameter.resolved,
        ResolvedDimensionValue::LengthMm { mm: 6.0 }
    );
}

#[test]
fn dimension_values_are_independent_of_drawing_scale() {
    // The engineering value stays 120 at 1:1, 1:2 and 2:1 while the paper
    // presentation size follows the scale.
    for scale_value in [1.0, 0.5, 2.0] {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: scale_value });
        let model = titled_plate_model("Mounting Plate", 120.0);
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let view = drawing.principal_view().unwrap();
        assert_eq!(
            measured_resolved(view, &DrawingDimensionId("plate.width".into())),
            ResolvedDimensionValue::LengthMm { mm: 120.0 },
            "scale {scale_value} must not change the engineering value"
        );
        // Paper size differs with scale (sheet width = scaled width + margin).
        let sheet_width = drawing.principal_sheet().unwrap().bounds().max_x;
        match scale_value {
            1.0 => assert_eq!(sheet_width, 150.0),
            0.5 => assert_eq!(sheet_width, 90.0),
            2.0 => assert_eq!(sheet_width, 270.0),
            _ => unreachable!(),
        }
    }
}

#[test]
fn geometry_tampering_does_not_change_dimension_sources() {
    // Hand-build a solved geometry whose circle radii are wrong (99 instead of
    // 63) but keep the engineering model and maps honest: dimensions must come
    // from engineering semantics, never from measuring geometry.
    let model = circular_plate_model();
    let (_, eng_map) = solve_and_draw(&model);

    let mut entities = BTreeMap::new();
    entities.insert(
        EntityId("plate".into()),
        GeometryEntity {
            id: EntityId("plate".into()),
            primitive: GeometryPrimitive::Rectangle {
                origin: Point2 { x: -80.0, y: -80.0 },
                width: 160.0,
                height: 160.0,
            },
        },
    );
    for i in 0..8u32 {
        entities.insert(
            EntityId(format!("bolt_pattern_array[{i}]")),
            GeometryEntity {
                id: EntityId(format!("bolt_pattern_array[{i}]")),
                primitive: GeometryPrimitive::Circle {
                    center: Point2 { x: 0.0, y: 0.0 },
                    radius: 99.0, // tampered
                },
            },
        );
    }
    let mut tampered_geometry = GeometryModel::new(0, 1, 0);
    tampered_geometry.entities = entities;

    let state = DrawingState::new();
    let inputs = DrawingInputs::new(
        &tampered_geometry,
        &eng_map,
        &state,
        &model.features,
        Some(&model),
        model.part.as_ref(),
    );
    let drawing = build_drawing(&inputs).unwrap();
    let view = drawing.principal_view().unwrap();

    let DimensionValue::Pattern { shape } =
        &view.dimensions[&DrawingDimensionId("bolt_pattern.pattern".into())].value
    else {
        panic!("expected pattern");
    };
    let scc_drawing::PatternShape::Circular {
        count,
        pitch_circle_diameter,
        hole_diameter,
    } = shape
    else {
        panic!("expected circular shape");
    };
    // Engineering truth, not the tampered radius × 2 or measured geometry.
    assert_eq!(count.resolved, ResolvedDimensionValue::Count { count: 8 });
    assert_eq!(
        pitch_circle_diameter.resolved,
        ResolvedDimensionValue::LengthMm { mm: 126.0 }
    );
    assert_eq!(
        hole_diameter.resolved,
        ResolvedDimensionValue::LengthMm { mm: 10.0 }
    );
}

#[test]
fn unresolved_bound_parameter_fails_closed() {
    // Defensive gate: normally lowering rejects bound-but-unresolved earlier
    // (SCC-ENG-027). Feed the builder a consistent-looking geometry plus a
    // model whose Width binding points at an Unresolved parameter.
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("p".into()),
            name: "p".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::DeclareParameter {
            id: EngParameterId("unresolved_width".into()),
            value: ParameterValue::Unresolved,
            origin: ParameterOrigin::User,
            authority: scc_core::authority::AuthorityLevel::L0,
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::BindFeatureParameter {
            feature: FeatureId("plate".into()),
            slot: FeatureParameterSlot::Width,
            parameter: EngParameterId("unresolved_width".into()),
        },
    )
    .unwrap();

    let geometry = synthetic_geometry(vec![(
        "plate",
        GeometryPrimitive::Rectangle {
            origin: Point2 { x: -60.0, y: -40.0 },
            width: 120.0,
            height: 80.0,
        },
    )]);
    let mut eng_map = EngineeringSourceMap::new();
    eng_map.record_feature_entity(FeatureId("plate".into()), EntityId("plate".into()));
    let state = DrawingState::new();
    let features = model.features.clone();
    let inputs = DrawingInputs::new(
        &geometry,
        &eng_map,
        &state,
        &features,
        Some(&model),
        model.part.as_ref(),
    );
    let err = build_drawing(&inputs).unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::UnresolvedDimensionValue { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-010");
}

#[test]
fn dimension_target_without_primitives_fails_closed() {
    // The plate exists in the model but has no represented primitive in this
    // presentation: its dimension must fail closed rather than dangle.
    let model = mounting_plate_model();
    let (_, eng_map) = solve_and_draw(&model);

    // Geometry containing only the hole circles, not the plate rectangle.
    let mut entities = BTreeMap::new();
    entities.insert(
        EntityId("corner_holes_array[0]".into()),
        GeometryEntity {
            id: EntityId("corner_holes_array[0]".into()),
            primitive: GeometryPrimitive::Circle {
                center: Point2 { x: -45.0, y: -25.0 },
                radius: 5.0,
            },
        },
    );
    let mut partial_geometry = GeometryModel::new(0, 1, 0);
    partial_geometry.entities = entities;

    let state = DrawingState::new();
    let features = model.features.clone();
    let inputs = DrawingInputs::new(
        &partial_geometry,
        &eng_map,
        &state,
        &features,
        Some(&model),
        model.part.as_ref(),
    );
    let err = build_drawing(&inputs).unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidDimensionTarget { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-011");
}

#[test]
fn dimensions_are_deterministic_over_one_hundred_runs() {
    let model = circular_plate_model();
    let (drawing0, _) = solve_and_draw(&model);
    let ser0 = serde_json::to_string(&drawing0).unwrap();
    for _ in 1..100 {
        let (drawing, _) = solve_and_draw(&model);
        assert_eq!(drawing, drawing0);
        assert_eq!(serde_json::to_string(&drawing).unwrap(), ser0);
        let v0 = drawing0.principal_view().unwrap();
        let v = drawing.principal_view().unwrap();
        assert_eq!(v.dimensions.len(), v0.dimensions.len());
        for (id, dim) in &v0.dimensions {
            assert_eq!(v.dimensions.get(id), Some(dim), "dim {id} unstable");
        }
    }
}

#[test]
fn dimensions_serde_roundtrip_preserves_everything() {
    let model = circular_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let json = serde_json::to_string(&drawing).unwrap();
    let back: DrawingModel = serde_json::from_str(&json).unwrap();
    assert_eq!(drawing, back);
    let v = back.principal_view().unwrap();
    let dim = &v.dimensions[&DrawingDimensionId("bolt_pattern.pattern".into())];
    let DimensionValue::Pattern { shape } = &dim.value else {
        panic!("expected pattern");
    };
    // Typed sources survive: parameter/feature ids stay structured strings.
    let sources: Vec<String> = dim
        .sources()
        .iter()
        .map(|s| serde_json::to_string(s).unwrap())
        .collect();
    assert_eq!(sources.len(), 3);
    let _ = shape; // full equality already asserted above
}
// ---------------------------------------------------------------------------
// Phase 8.6 — deterministic dimension layout
// ---------------------------------------------------------------------------

use scc_drawing::{
    DimensionLayoutGeometry, DrawingDimensionLayout, LayoutSegment, TextOrientation,
    DEFAULT_DIMENSION_OFFSET_MM, DEFAULT_DIMENSION_STACK_PITCH_MM,
};

/// The linear layout of a dimension id, if it is one.
fn linear_layout<'a>(
    view: &'a scc_drawing::DrawingView,
    id: &DrawingDimensionId,
) -> (&'a LayoutSegment, &'a [LayoutSegment; 2], TextOrientation) {
    match &view.dimension_layouts[id].geometry {
        DimensionLayoutGeometry::Linear {
            dimension_line,
            extension_lines,
            text_orientation,
            ..
        } => (dimension_line, extension_lines, *text_orientation),
        other => panic!("expected linear layout, got {other:?}"),
    }
}

#[test]
fn width_linear_layout_sits_above_the_object() {
    // Plate content 10..130 × 10..90 @1:1 → width dim line y = 90 + 8 = 98,
    // span 120 paper mm; extensions run from the object top (+1 gap) past the
    // line (+2 overhang); text sits mid-line above the line.
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    assert_eq!(DEFAULT_DIMENSION_OFFSET_MM, 8.0);
    let (line, exts, orientation) = linear_layout(view, &DrawingDimensionId("plate.width".into()));
    assert_eq!(line.start.y, line.end.y);
    assert_eq!(line.start.y, 98.0);
    let span = line.end.x - line.start.x;
    assert!((span - 120.0).abs() < 1.0e-9);
    assert_eq!(line.start.x, 10.0);
    assert_eq!(line.end.x, 130.0);
    // Extension lines: vertical at both witness x positions.
    for (x, ext) in [(10.0, &exts[0]), (130.0, &exts[1])] {
        assert_eq!(ext.start.x, x);
        assert_eq!(ext.end.x, x);
        assert_eq!(ext.start.y, 91.0); // rect top 90 + gap 1
        assert_eq!(ext.end.y, 100.0); // dim line 98 + overhang 2
    }
    assert_eq!(orientation, TextOrientation::Horizontal);
}

#[test]
fn height_linear_layout_sits_right_of_the_object() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    let (line, _exts, orientation) =
        linear_layout(view, &DrawingDimensionId("plate.height".into()));
    assert_eq!(line.start.x, line.end.x);
    assert_eq!(line.start.x, 138.0); // right edge 130 + offset 8
    let span = line.end.y - line.start.y;
    assert!((span - 80.0).abs() < 1.0e-9);
    assert_eq!(line.start.y, 10.0);
    assert_eq!(line.end.y, 90.0);
    assert_eq!(orientation, TextOrientation::Vertical);
}

#[test]
fn diameter_layout_crosses_center_at_fixed_angle() {
    // Ø64 hole centered at model origin; plate 120×120 centers the sheet
    // translation so the circle centre lands at sheet (70,70).
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    let layout = &view.dimension_layouts[&DrawingDimensionId("center_hole.diameter".into())];
    let DimensionLayoutGeometry::Diameter {
        center,
        axis_line,
        arrows,
        text_anchor,
    } = &layout.geometry
    else {
        panic!("expected diameter layout");
    };
    assert_eq!(*center, DrawingPoint::new(70.0, 70.0));
    // Physical axis length is the scaled diameter: 64 mm at 1:1...
    let dx = axis_line.end.x - axis_line.start.x;
    let dy = axis_line.end.y - axis_line.start.y;
    assert!(((dx * dx + dy * dy).sqrt() - 64.0).abs() < 1.0e-9);
    // ...at the fixed 45° angle, endpoints on the circle edge.
    let expected = 64.0 / 2.0 / std::f64::consts::SQRT_2;
    assert!((axis_line.start.x - (center.x - expected)).abs() < 1.0e-9);
    assert!((axis_line.end.y - (center.y + expected)).abs() < 1.0e-9);
    // Arrows point inward along the axis.
    assert!(arrows[0].direction.x > 0.0 && arrows[1].direction.x < 0.0);
    // Semantic engineering value is untouched by layout.
    assert_eq!(
        measured_resolved(view, &DrawingDimensionId("center_hole.diameter".into())),
        ResolvedDimensionValue::LengthMm { mm: 64.0 }
    );
    assert!(text_anchor.x > axis_line.end.x);
}

#[test]
fn diameter_layout_physical_length_follows_scale_value_does_not() {
    // At 1:2 the witness axis halves to 32 mm while the semantic value stays 64.
    let mut state = DrawingState::new();
    state.set(DrawingProperty::Scale { value: 0.5 });
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw_with_state(&model, &state);
    let view = drawing.principal_view().unwrap();
    let DimensionLayoutGeometry::Diameter { axis_line, .. } =
        &view.dimension_layouts[&DrawingDimensionId("center_hole.diameter".into())].geometry
    else {
        panic!("expected diameter layout");
    };
    let dx = axis_line.end.x - axis_line.start.x;
    let dy = axis_line.end.y - axis_line.start.y;
    assert!(((dx * dx + dy * dy).sqrt() - 32.0).abs() < 1.0e-9);
    assert_eq!(
        measured_resolved(view, &DrawingDimensionId("center_hole.diameter".into())),
        ResolvedDimensionValue::LengthMm { mm: 64.0 }
    );
}

#[test]
fn derived_diameter_layout_tracks_new_geometry_after_resolve() {
    // hole_d := plate_height / 8: 80 → Ø10, then 96 → Ø12. The layout witness
    // must follow the freshly solved circle, never stay at the old size.
    for (height, expected_mm) in [(80.0, 10.0), (96.0, 12.0)] {
        let model = derived_hole_model(height);
        let (drawing, _) = solve_and_draw(&model);
        let view = drawing.principal_view().unwrap();
        assert_eq!(
            measured_resolved(view, &DrawingDimensionId("center_hole.diameter".into())),
            ResolvedDimensionValue::LengthMm { mm: expected_mm }
        );
        let DimensionLayoutGeometry::Diameter { axis_line, .. } =
            &view.dimension_layouts[&DrawingDimensionId("center_hole.diameter".into())].geometry
        else {
            panic!("expected diameter layout");
        };
        let dx = axis_line.end.x - axis_line.start.x;
        let dy = axis_line.end.y - axis_line.start.y;
        assert!(
            ((dx * dx + dy * dy).sqrt() - expected_mm).abs() < 1.0e-9,
            "layout witness must track the re-solved geometry"
        );
    }
}

#[test]
fn circular_pattern_gets_exactly_one_callout_over_full_footprint() {
    // 8 circles on PCD 126 with Ø10: footprint extends ±(63+5) = ±68 around
    // the pattern centre; plate 160×160 puts the origin at sheet (90,90).
    let model = circular_plate_model();
    let (drawing, eng_map) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();

    let pattern_id = DrawingDimensionId("bolt_pattern.pattern".into());
    assert_eq!(
        view.dimension_layouts.len(),
        view.dimensions.len(),
        "every dimension has exactly one layout"
    );
    let layout = &view.dimension_layouts[&pattern_id];
    let DimensionLayoutGeometry::PatternCallout {
        anchor_on_pattern,
        leader,
        text_anchor,
        footprint,
    } = &layout.geometry
    else {
        panic!("expected pattern callout");
    };
    assert_eq!(footprint.min_x, 22.0); // 90 − 68
    assert_eq!(footprint.min_y, 22.0);
    assert_eq!(footprint.max_x, 158.0); // 90 + 68
    assert_eq!(footprint.max_y, 158.0);
    assert_eq!(*anchor_on_pattern, DrawingPoint::new(158.0, 158.0));
    assert_eq!(
        leader.end.x,
        anchor_on_pattern.x + DEFAULT_DIMENSION_OFFSET_MM
    );
    assert_eq!(
        leader.end.y,
        anchor_on_pattern.y + DEFAULT_DIMENSION_OFFSET_MM
    );
    assert!(text_anchor.x > leader.end.x);
    // One pattern feature still owns all eight primitives.
    assert_eq!(
        drawing
            .source_map
            .primitives_for_feature(&FeatureId("bolt_pattern".into()), &eng_map)
            .len(),
        8
    );
}

#[test]
fn stacked_linear_dimensions_use_fixed_pitch_in_id_order() {
    // Two BaseProfile features: "aaa" (120×80) and "bbb" (60×40), both centred.
    // Width dims stack above the object in semantic id order; height dims to
    // the right. Same input always yields the same stacking.
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("stacked".into()),
            name: "stacked".into(),
        },
    )
    .unwrap();
    for (fid, w, h) in [("aaa", 120.0, 80.0), ("bbb", 60.0, 40.0)] {
        CommandExecutor::apply(
            &mut model,
            &EngineeringCommand::AddFeature {
                feature: EngineeringFeature::BaseProfile {
                    id: FeatureId(fid.into()),
                    name: fid.into(),
                    profile: scc_engineering::BaseProfile::Rectangle {
                        width: w,
                        height: h,
                    },
                },
            },
        )
        .unwrap();
    }
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();

    let (line_aaa, _, _) = linear_layout(view, &DrawingDimensionId("aaa.width".into()));
    let (line_bbb, _, _) = linear_layout(view, &DrawingDimensionId("bbb.width".into()));
    assert_eq!(line_aaa.start.y, 90.0 + DEFAULT_DIMENSION_OFFSET_MM);
    assert_eq!(
        line_bbb.start.y,
        line_aaa.start.y + DEFAULT_DIMENSION_STACK_PITCH_MM
    );
    // Spans follow each feature's own witness geometry.
    assert!((line_aaa.end.x - line_aaa.start.x - 120.0).abs() < 1.0e-9);
    assert!((line_bbb.end.x - line_bbb.start.x - 60.0).abs() < 1.0e-9);

    let (va, _, _) = linear_layout(view, &DrawingDimensionId("aaa.height".into()));
    let (vb, _, _) = linear_layout(view, &DrawingDimensionId("bbb.height".into()));
    assert_eq!(vb.start.x, va.start.x + DEFAULT_DIMENSION_STACK_PITCH_MM);
}

#[test]
fn sheet_expands_for_dimensions_without_moving_geometry_or_title() {
    // With dimensions the envelope grows toward +X/+Y only; min stays (0,0),
    // the principal transform is byte-identical, and the part title stays at
    // its Phase 8.4 position inside the bottom margin.
    let model = titled_plate_model("Mounting Plate", 120.0);

    let (lowering, geometry) = lower_and_draw_parts(&model);
    let state = DrawingState::new();
    let features = model.features.clone();
    let plain_inputs = DrawingInputs::new(
        &geometry,
        &lowering.source_map,
        &state,
        &features,
        None,
        None,
    );
    let plain = build_drawing(&plain_inputs).unwrap();

    let (with_dims, eng_map) = solve_and_draw(&model);

    let pv = with_dims.principal_view().unwrap();
    let pp = plain.principal_view().unwrap();
    assert_eq!(pv.transform, pp.transform, "geometry placement frozen");
    let wb = with_dims.principal_sheet().unwrap().bounds();
    let pb = plain.principal_sheet().unwrap().bounds();
    assert_eq!(wb.min_x, 0.0);
    assert_eq!(wb.min_y, 0.0);
    assert_eq!(pb.min_x, 0.0);
    assert!(wb.max_x > pb.max_x && wb.max_y > pb.max_y);

    let title = with_dims
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    assert_eq!(title.position, DrawingPoint::new(10.0, 5.0));
    // Provenance untouched by layout work.
    assert_eq!(
        source_feature(
            &with_dims,
            &eng_map,
            &scc_drawing::DrawingPrimitiveId("plate".into())
        ),
        Some(FeatureId("plate".into()))
    );
}

/// Solve without building (helper for mixed comparisons).
fn lower_and_draw_parts(
    model: &EngineeringModel,
) -> (
    scc_engineering::lowering::EngineeringLoweringResult,
    scc_geometry::GeometryModel,
) {
    lower_and_solve(model).unwrap()
}

#[test]
fn engineering_geometry_mismatch_fails_closed() {
    // Engineering says width=120 but the presented rectangle measures 100:
    // the two truth chains disagree → no layout, no drawing. All other
    // entities are presented honestly so only the witness check can fire.
    let model = mounting_plate_model();
    let (_, eng_map) = solve_and_draw(&model);
    let mut cases: Vec<(&str, GeometryPrimitive)> = vec![(
        "plate",
        GeometryPrimitive::Rectangle {
            origin: Point2 { x: -50.0, y: -40.0 },
            width: 100.0, // tampered witness
            height: 80.0,
        },
    )];
    for i in 0..4u32 {
        let (x, y) = [(-45.0, -25.0), (45.0, -25.0), (-45.0, 25.0), (45.0, 25.0)][i as usize];
        cases.push((
            Box::leak(format!("corner_holes_array[{i}]").into_boxed_str()),
            GeometryPrimitive::Circle {
                center: Point2 { x, y },
                radius: 5.0,
            },
        ));
    }
    let geometry = synthetic_geometry(cases);
    let state = DrawingState::new();
    let features = model.features.clone();
    let inputs = DrawingInputs::new(
        &geometry,
        &eng_map,
        &state,
        &features,
        Some(&model),
        model.part.as_ref(),
    );
    let err = build_drawing(&inputs).unwrap_err();
    assert!(matches!(
        err,
        scc_drawing::DrawingError::InvalidDimensionWitness { .. }
    ));
    assert_eq!(err.code(), "SCC-DRW-014");
}

#[test]
fn dimension_layout_integrity_catches_missing_orphan_and_nan() {
    use scc_drawing::{validate_dimension_layout_integrity, ArrowAnchor};
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);

    // Healthy build passes.
    validate_dimension_layout_integrity(drawing.principal_view().unwrap()).unwrap();

    // Missing layout for an existing dimension.
    let mut damaged = drawing.clone();
    let view = damaged.principal_view_mut().unwrap();
    view.dimension_layouts
        .remove(&DrawingDimensionId("plate.width".into()));
    let err = validate_dimension_layout_integrity(damaged.principal_view().unwrap()).unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-013");

    // Orphan layout referencing an unknown dimension.
    let mut damaged = drawing.clone();
    let orphan = DrawingDimensionLayout {
        dimension: DrawingDimensionId("ghost".into()),
        geometry: DimensionLayoutGeometry::PatternCallout {
            anchor_on_pattern: DrawingPoint::new(1.0, 1.0),
            leader: LayoutSegment {
                start: DrawingPoint::new(1.0, 1.0),
                end: DrawingPoint::new(2.0, 2.0),
            },
            text_anchor: DrawingPoint::new(3.0, 1.0),
            footprint: scc_drawing::DrawingBounds::zero(),
        },
    };
    damaged
        .principal_view_mut()
        .unwrap()
        .dimension_layouts
        .insert(DrawingDimensionId("ghost".into()), orphan);
    let err = validate_dimension_layout_integrity(damaged.principal_view().unwrap()).unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-012");

    // NaN coordinate anywhere in a layout.
    let mut damaged = drawing;
    let view = damaged.principal_view_mut().unwrap();
    if let Some(layout) = view
        .dimension_layouts
        .get_mut(&DrawingDimensionId("plate.height".into()))
    {
        if let DimensionLayoutGeometry::Linear { arrows, .. } = &mut layout.geometry {
            arrows[0] = ArrowAnchor {
                point: DrawingPoint::new(f64::NAN, 0.0),
                direction: DrawingPoint::new(0.0, 1.0),
            };
        }
    }
    let err = validate_dimension_layout_integrity(damaged.principal_view().unwrap()).unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-012");
}

#[test]
fn layouts_are_deterministic_over_one_hundred_runs_and_serde_stable() {
    let model = circular_plate_model();
    let (drawing0, _) = solve_and_draw(&model);
    let ser0 = serde_json::to_string(&drawing0).unwrap();
    for _ in 1..100 {
        let (drawing, _) = solve_and_draw(&model);
        assert_eq!(drawing, drawing0);
        assert_eq!(serde_json::to_string(&drawing).unwrap(), ser0);
        let v0 = drawing0.principal_view().unwrap();
        let v = drawing.principal_view().unwrap();
        assert_eq!(v.dimension_layouts, v0.dimension_layouts);
    }
    // Round-trip preserves every anchor exactly.
    let back: DrawingModel = serde_json::from_str(&ser0).unwrap();
    assert_eq!(back, drawing0);
}
// ---------------------------------------------------------------------------
// Phase 8.7 — centerline + construction presentation
// ---------------------------------------------------------------------------

use scc_drawing::{
    validate_center_geometry_integrity, CenterlineAxis, DrawingCenterGeometry,
    DrawingCenterGeometryId, DEFAULT_CENTERLINE_OVERHANG_MM, DEFAULT_CENTER_MARK_HALF_LENGTH_MM,
};

/// Count center elements of each kind for a view.
fn center_counts(view: &scc_drawing::DrawingView) -> (usize, usize) {
    let mut marks = 0;
    let mut lines = 0;
    for element in view.center_geometry.values() {
        match element {
            DrawingCenterGeometry::CenterMark(_) => marks += 1,
            DrawingCenterGeometry::Centerline(_) => lines += 1,
        }
    }
    (marks, lines)
}

#[test]
fn hole_gets_cross_mark_and_two_axial_centerlines() {
    // Ø64 hole centred at model origin; plate 120×120 puts it at sheet
    // (70,70) with radius 32. Mark cross: ±3 paper mm. Lines: radius+2mm.
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    let (marks, lines) = center_counts(view);
    assert_eq!(marks, 1);
    assert_eq!(lines, 2);

    let mark_id = DrawingCenterGeometryId("center_mark:center_hole".into());
    let DrawingCenterGeometry::CenterMark(mark) = &view.center_geometry[&mark_id] else {
        panic!("expected center mark");
    };
    assert_eq!(mark.source_feature, FeatureId("center_hole".into()));
    assert_eq!(mark.horizontal.start, DrawingPoint::new(67.0, 70.0));
    assert_eq!(mark.horizontal.end, DrawingPoint::new(73.0, 70.0));
    assert_eq!(mark.vertical.start, DrawingPoint::new(70.0, 67.0));
    assert_eq!(mark.vertical.end, DrawingPoint::new(70.0, 73.0));

    let h_id = DrawingCenterGeometryId("h_centerline:center_hole".into());
    let v_id = DrawingCenterGeometryId("v_centerline:center_hole".into());
    let DrawingCenterGeometry::Centerline(h) = &view.center_geometry[&h_id] else {
        panic!("expected horizontal centerline");
    };
    let DrawingCenterGeometry::Centerline(v) = &view.center_geometry[&v_id] else {
        panic!("expected vertical centerline");
    };
    assert_eq!(h.axis, CenterlineAxis::Horizontal);
    assert_eq!(h.segment.start, DrawingPoint::new(36.0, 70.0)); // 70−32−2
    assert_eq!(h.segment.end, DrawingPoint::new(104.0, 70.0));
    assert_eq!(v.axis, CenterlineAxis::Vertical);
    assert_eq!(v.segment.start, DrawingPoint::new(70.0, 36.0));
    assert_eq!(v.segment.end, DrawingPoint::new(70.0, 104.0));
}

#[test]
fn center_paper_mm_constants_never_scale() {
    // Cross stays 6 mm total and overhang stays 2 mm at every scale; only the
    // transformed centre/radius move.
    for (scale_value, cx, cy, reach_half) in [
        (1.0, 70.0, 70.0, 34.0),
        (0.5, 40.0, 40.0, 18.0),
        (2.0, 130.0, 130.0, 66.0),
    ] {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: scale_value });
        let model = center_hole_model();
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let view = drawing.principal_view().unwrap();
        let mark_id = DrawingCenterGeometryId("center_mark:center_hole".into());
        let DrawingCenterGeometry::CenterMark(mark) = &view.center_geometry[&mark_id] else {
            panic!("expected mark");
        };
        assert_eq!(
            mark.horizontal.end.x - mark.horizontal.start.x,
            2.0 * DEFAULT_CENTER_MARK_HALF_LENGTH_MM
        );
        assert_eq!(
            mark.horizontal.start,
            DrawingPoint::new(cx - DEFAULT_CENTER_MARK_HALF_LENGTH_MM, cy)
        );
        let h_id = DrawingCenterGeometryId("h_centerline:center_hole".into());
        let DrawingCenterGeometry::Centerline(h) = &view.center_geometry[&h_id] else {
            panic!("expected centerline");
        };
        assert!((h.segment.start.x - (cx - reach_half)).abs() < 1.0e-9);
        assert!((h.segment.end.x - (cx + reach_half)).abs() < 1.0e-9);
        assert!(
            (reach_half - (32.0 * scale_value + DEFAULT_CENTERLINE_OVERHANG_MM)).abs() < 1.0e-9
        );
    }
}

#[test]
fn pattern_elements_keep_single_feature_provenance() {
    // 8-hole circular pattern → 8 marks + 16 lines, ALL sourced to the one
    // bolt_pattern feature; the engineering model keeps exactly one feature.
    let model = circular_plate_model();
    let (drawing, _eng_map) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    let (marks, lines) = center_counts(view);
    assert_eq!(marks, 8);
    assert_eq!(lines, 16);
    assert_eq!(model.features.len(), 2, "never expanded into 8 Holes");
    for element in view.center_geometry.values() {
        assert_eq!(element.source_feature(), &FeatureId("bolt_pattern".into()));
    }
    // Distinct deterministic ids per element.
    assert_eq!(view.center_geometry.len(), 24);
}

#[test]
fn grid_pattern_centers_follow_elements() {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("grid_centers".into()),
            name: "grid_centers".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 120.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("grid_holes".into()),
                pattern: HolePattern::Grid {
                    rows: 2,
                    columns: 3,
                    spacing_x: 30.0,
                    spacing_y: 30.0,
                    hole: HoleDefinition { diameter: 8.0 },
                },
            },
        },
    )
    .unwrap();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    let (marks, lines) = center_counts(view);
    assert_eq!(marks, 6, "one mark per represented element");
    assert_eq!(lines, 12, "two axial lines per element");
    for element in view.center_geometry.values() {
        assert_eq!(element.source_feature(), &FeatureId("grid_holes".into()));
    }
}

/// A plate with a construction circle AND a hole (toggle-matrix fixture).
fn hole_plus_construction_model() -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("mixed".into()),
            name: "mixed".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 120.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId("center_hole".into()),
                diameter: 40.0,
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::ConstructionCircle {
                id: FeatureId("cc".into()),
                diameter: 90.0,
            },
        },
    )
    .unwrap();
    model
}

#[test]
fn construction_circle_gets_no_center_presentation() {
    // The construction circle is reference geometry itself — it never gains
    // marks/lines, even when fully visible.
    let model = construction_circle_model();
    let (drawing, _) = solve_and_draw(&model);
    let view = drawing.principal_view().unwrap();
    assert!(view.center_geometry.is_empty());
    // And it is present as a Construction primitive.
    let cc = &view.primitives[&scc_drawing::DrawingPrimitiveId("cc".into())];
    assert!(
        matches!(cc, DrawingPrimitive::Circle { role, .. } if *role == DrawingPrimitiveRole::Construction)
    );
}

#[test]
fn construction_and_centerline_toggles_are_independent() {
    let combos = [(true, true), (true, false), (false, true), (false, false)];
    for (show_construction, show_centers) in combos {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::ShowConstructionGeometry {
            enabled: show_construction,
        });
        state.set(DrawingProperty::ShowCenterlines {
            enabled: show_centers,
        });
        let model = hole_plus_construction_model();
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let view = drawing.principal_view().unwrap();
        let has_cc = view
            .primitives
            .values()
            .any(|p| matches!(p, DrawingPrimitive::Circle { role, .. } if *role == DrawingPrimitiveRole::Construction));
        let (marks, lines) = center_counts(view);
        assert_eq!(
            has_cc, show_construction,
            "construction toggle at ({show_construction},{show_centers})"
        );
        assert_eq!(
            (marks > 0),
            show_centers,
            "centers toggle at ({show_construction},{show_centers})"
        );
        assert_eq!(lines, if show_centers { 2 } else { 0 });
    }
}

#[test]
fn showcenterlines_false_keeps_dimension_axis_but_no_centers() {
    // The diameter dimension and its 45° layout axis exist regardless of the
    // centerline toggle; the axis is dimension geometry, never a centerline.
    let mut state = DrawingState::new();
    state.set(DrawingProperty::ShowCenterlines { enabled: false });
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw_with_state(&model, &state);
    let view = drawing.principal_view().unwrap();

    assert!(view.center_geometry.is_empty());
    // Circle primitive survives.
    assert!(matches!(
        view.primitives[&scc_drawing::DrawingPrimitiveId("center_hole".into())],
        DrawingPrimitive::Circle { .. }
    ));
    // Dimension + layout survive, including the axis line.
    assert!(view
        .dimensions
        .contains_key(&DrawingDimensionId("center_hole.diameter".into())));
    let layout = &view.dimension_layouts[&DrawingDimensionId("center_hole.diameter".into())];
    assert!(matches!(
        layout.geometry,
        scc_drawing::DimensionLayoutGeometry::Diameter { .. }
    ));
}

#[test]
fn noncircle_hole_representation_fails_closed() {
    // A Hole feature whose represented primitive is a rectangle must fail:
    // the builder never guesses a centre from a shape that is not a circle.
    // (Through the full pipeline the earlier dimension-witness gate fires
    // first, so this exercises the center generator directly.)
    let model = center_hole_model();
    let features = model.features.clone();

    let mut prims = BTreeMap::new();
    prims.insert(
        scc_drawing::DrawingPrimitiveId("center_hole".into()),
        DrawingPrimitive::Rectangle {
            role: DrawingPrimitiveRole::Object,
            origin: DrawingPoint::new(-10.0, -10.0),
            width: 20.0,
            height: 20.0,
        },
    );
    let mut dmap = scc_drawing::source_map::DrawingSourceMap::new();
    dmap.record(
        scc_drawing::DrawingPrimitiveId("center_hole".into()),
        EntityId("center_hole".into()),
    );
    let mut emap = EngineeringSourceMap::new();
    emap.record_feature_entity(
        FeatureId("center_hole".into()),
        EntityId("center_hole".into()),
    );

    let err = scc_drawing::center_geometry::generate_center_geometry(
        &prims,
        &dmap,
        &emap,
        &features,
        &scc_drawing::DrawingViewTransform::identity(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-015");
}

#[test]
fn tampered_center_sources_fail_closed() {
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw(&model);

    // Unknown source feature.
    let mut damaged = drawing.clone();
    let view = damaged.principal_view_mut().unwrap();
    let any_id = view.center_geometry.keys().next().unwrap().clone();
    if let DrawingCenterGeometry::CenterMark(m) =
        &mut view.center_geometry.get_mut(&any_id).unwrap()
    {
        m.source_feature = FeatureId("ghost".into());
    }
    let err = validate_center_geometry_integrity(
        damaged.principal_view().unwrap(),
        &damaged.source_map,
        &{
            let (_, m) = solve_and_draw(&model);
            m
        },
        &model.features,
    )
    .unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-016");

    // NaN coordinate (mutate whichever element kind comes first).
    let mut damaged = drawing.clone();
    let view = damaged.principal_view_mut().unwrap();
    let any_id = view.center_geometry.keys().next().unwrap().clone();
    match view.center_geometry.get_mut(&any_id).unwrap() {
        DrawingCenterGeometry::CenterMark(m) => m.horizontal.end.x = f64::NAN,
        DrawingCenterGeometry::Centerline(l) => l.segment.end.x = f64::NAN,
    }
    let (_, fresh_map) = solve_and_draw(&model);
    let err = validate_center_geometry_integrity(
        damaged.principal_view().unwrap(),
        &damaged.source_map,
        &fresh_map,
        &model.features,
    )
    .unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-015");

    // Sourced feature no longer represented by a circle in this view.
    let mut damaged = drawing;
    let view = damaged.principal_view_mut().unwrap();
    view.primitives
        .remove(&scc_drawing::DrawingPrimitiveId("center_hole".into()));
    let (_, fresh_map) = solve_and_draw(&model);
    let err = validate_center_geometry_integrity(
        damaged.principal_view().unwrap(),
        &damaged.source_map,
        &fresh_map,
        &model.features,
    )
    .unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-016");
}

/// A single Ø64 hole with no base profile: no linear dimensions exist, so
/// centerlines are the only top/right envelope contributors beyond the
/// diameter layout.
fn lone_hole_model() -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("lone_hole".into()),
            name: "lone_hole".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Hole {
                id: FeatureId("center_hole".into()),
                diameter: 64.0,
            },
        },
    )
    .unwrap();
    model
}

#[test]
fn sheet_envelope_grows_for_centerlines_without_moving_geometry() {
    // Circle r32 centred at model origin → sheet centre (42,42). Centers off:
    // content ends at the diameter text (~65.7) → envelope < 86. Centers on:
    // axial lines reach 42±(32+2)=8..76 → content max 76 → envelope exactly
    // 86. Origin frozen; geometry placement byte-identical.
    let model = lone_hole_model();
    let mut centers_off = DrawingState::new();
    centers_off.set(DrawingProperty::ShowCenterlines { enabled: false });
    let (off, _) = solve_and_draw_with_state(&model, &centers_off);
    let (on, _) = solve_and_draw(&model);

    let wb = on.principal_sheet().unwrap().bounds();
    let ob = off.principal_sheet().unwrap().bounds();
    assert_eq!(wb.min_x, 0.0);
    assert_eq!(wb.min_y, 0.0);
    // Content spans 8..76 (centre 42 ± r32+overhang2): width 68 + margin 20.
    assert_eq!(wb.max_x, 88.0);
    assert_eq!(wb.max_y, 88.0);
    assert!(ob.max_x < wb.max_x && ob.max_y < wb.max_y);
    // Center extents helper agrees with the envelope math.
    let extents = on
        .principal_view()
        .unwrap()
        .center_geometry_extents()
        .unwrap();
    assert_eq!(extents.max_x, 76.0);
    assert_eq!(extents.min_x, 8.0);
    // Geometry placement byte-identical between the two builds.
    assert_eq!(
        on.principal_view().unwrap().transform,
        off.principal_view().unwrap().transform
    );
    assert_eq!(
        on.principal_view().unwrap().model_bounds,
        off.principal_view().unwrap().model_bounds
    );
}

#[test]
fn centers_deterministic_serde_and_sourcemap_isolated() {
    let model = circular_plate_model();
    let (drawing0, _) = solve_and_draw(&model);
    let ser0 = serde_json::to_string(&drawing0).unwrap();

    // Source map isolation: primitive map counts are exactly the solved
    // entities (plate + 8 array holes) — center elements never appear there.
    assert_eq!(drawing0.source_map.iter().count(), 9);
    for (_, entity) in drawing0.source_map.iter() {
        assert!(!entity.0.starts_with("center_mark"));
        assert!(!entity.0.contains("centerline"));
    }

    for _ in 1..100 {
        let (drawing, _) = solve_and_draw(&model);
        assert_eq!(drawing, drawing0);
        assert_eq!(serde_json::to_string(&drawing).unwrap(), ser0);
    }
    let back: DrawingModel = serde_json::from_str(&ser0).unwrap();
    assert_eq!(back, drawing0);
}
// ---------------------------------------------------------------------------
// Phase 8.8 — SVG drawing renderer
// ---------------------------------------------------------------------------

use scc_drawing::dimension::{format_dimension, DrawingDimensionSpecification};
use scc_drawing::{
    drawing_to_svg, DimensionValue as DimVal, DrawingDimension, DrawingDimensionId as DimId,
    DrawingDimensionKind as DimKind, DrawingDimensionTarget, ResolvedDimensionValue as Rsv,
};
use scc_engineering::specification::{
    DecimalPlaces, DimensionalSpecification, DimensionalTolerance, EngineeringDimensionRef,
};

/// A dimension value source stub for formatter-only constructions.
fn fmt_source() -> DimensionValueSource {
    DimensionValueSource::FeatureSlot {
        feature: FeatureId("f".into()),
        slot: FeatureParameterSlot::Width,
    }
}

fn measured_dim(kind: DimKind, mm: f64) -> DrawingDimension {
    DrawingDimension {
        id: DimId("t".into()),
        target: DrawingDimensionTarget::Feature {
            feature: FeatureId("f".into()),
        },
        kind,
        specification: None,
        value: DimVal::Measured {
            source: fmt_source(),
            resolved: Rsv::LengthMm { mm },
        },
    }
}

#[test]
fn dimension_formatter_canonical_forms() {
    use scc_drawing::dimension::format_dimension;
    // Linear trims zeros, locale-free.
    assert_eq!(
        format_dimension(&measured_dim(
            DimKind::Linear {
                axis: LinearAxis::Horizontal
            },
            120.0
        )),
        "120"
    );
    assert_eq!(
        format_dimension(&measured_dim(
            DimKind::Linear {
                axis: LinearAxis::Vertical
            },
            120.5
        )),
        "120.5"
    );
    // Diameter / Radius prefixes come from the typed kind.
    assert_eq!(
        format_dimension(&measured_dim(DimKind::Diameter, 64.0)),
        "Ø64"
    );
    let radius = DrawingDimension {
        id: DimId("r".into()),
        target: DrawingDimensionTarget::Feature {
            feature: FeatureId("f".into()),
        },
        kind: DimKind::Radius,
        specification: None,
        value: DimVal::Measured {
            source: fmt_source(),
            resolved: Rsv::LengthMm { mm: 30.0 },
        },
    };
    assert_eq!(format_dimension(&radius), "R30");
}

#[test]
fn phase115_formatter_fixed_precision_and_tolerance_forms() {
    let specified = |kind, mm, tolerance| {
        let mut dimension = measured_dim(kind, mm);
        dimension.specification = Some(DrawingDimensionSpecification {
            reference: EngineeringDimensionRef::FeatureSlot {
                feature: FeatureId("f".into()),
                slot: FeatureParameterSlot::Width,
            },
            value: DimensionalSpecification::new(tolerance, Some(DecimalPlaces(2))).unwrap(),
        });
        dimension
    };

    assert_eq!(
        format_dimension(&specified(
            DimKind::Linear {
                axis: LinearAxis::Horizontal,
            },
            120.0,
            DimensionalTolerance::symmetric(0.10).unwrap(),
        )),
        "120.00 ±0.10"
    );
    assert_eq!(
        format_dimension(&specified(
            DimKind::Linear {
                axis: LinearAxis::Vertical,
            },
            80.0,
            DimensionalTolerance::bilateral(0.20, -0.05).unwrap(),
        )),
        "80.00 +0.20/-0.05"
    );
    assert_eq!(
        format_dimension(&specified(
            DimKind::Diameter,
            10.0,
            DimensionalTolerance::symmetric(0.05).unwrap(),
        )),
        "Ø10.00 ±0.05"
    );
}

#[test]
fn viewbox_comes_from_sheet_bounds_at_every_scale() {
    for (scale_value, expect) in [
        (1.0, "0 0 150 110"),
        (0.5, "0 0 90 70"),
        (2.0, "0 0 270 190"),
    ] {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: scale_value });
        let model = mounting_plate_model();
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let svg = drawing_to_svg(&drawing).unwrap();
        assert!(
            svg.contains(&format!("viewBox=\"{}\"", expect)),
            "scale {scale_value}: viewBox must match sheet bounds exactly"
        );
        // And the renderer never re-measures values: engineering text stays.
        assert!(svg.contains(">120</text>"), "width value text");
        assert!(svg.contains(">80</text>"), "height value text");
    }
}

#[test]
fn every_primitive_kind_renders_with_arc_flags() {
    let model = mounting_plate_model();
    let (_, eng_map) = solve_and_draw(&model);
    let cases: Vec<(&str, GeometryPrimitive)> = vec![
        (
            "p",
            GeometryPrimitive::Point {
                position: Point2 { x: 1.0, y: 1.0 },
            },
        ),
        (
            "l",
            GeometryPrimitive::Line {
                start: Point2 { x: 0.0, y: 0.0 },
                end: Point2 { x: 5.0, y: 5.0 },
            },
        ),
        (
            "c",
            GeometryPrimitive::Circle {
                center: Point2 { x: 20.0, y: 20.0 },
                radius: 4.0,
            },
        ),
        (
            "a",
            GeometryPrimitive::Arc {
                center: Point2 { x: 40.0, y: 20.0 },
                radius: 6.0,
                start_angle: 0.0,
                end_angle: std::f64::consts::FRAC_PI_2,
            },
        ),
        (
            "a2",
            GeometryPrimitive::Arc {
                center: Point2 { x: 60.0, y: 20.0 },
                radius: 5.0,
                start_angle: 0.0,
                end_angle: 3.0 * std::f64::consts::FRAC_PI_2,
            },
        ),
        (
            "r",
            GeometryPrimitive::Rectangle {
                origin: Point2 { x: -6.0, y: -4.0 },
                width: 12.0,
                height: 8.0,
            },
        ),
        (
            "poly",
            GeometryPrimitive::Polygon {
                vertices: vec![
                    Point2 { x: 0.0, y: 0.0 },
                    Point2 { x: 2.0, y: 0.0 },
                    Point2 { x: 0.0, y: 2.0 },
                ],
            },
        ),
    ];
    let ids: Vec<&str> = cases.iter().map(|(id, _)| *id).collect();
    let geometry = synthetic_geometry(cases.clone());
    let eng_map2 = feature_map_for_entities(&ids, &FeatureId("base".into()));
    let features = features_with(EngineeringFeature::BaseProfile {
        id: FeatureId("base".into()),
        name: "base".into(),
        profile: scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
    });
    let _ = eng_map;
    let state = DrawingState::new();
    let inputs = DrawingInputs::new(&geometry, &eng_map2, &state, &features, None, None);
    let drawing = build_drawing(&inputs).unwrap();
    let svg = drawing_to_svg(&drawing).unwrap();

    assert!(svg.contains("<circle"), "point+circle");
    assert!(svg.contains("<line"), "line");
    assert!(svg.contains("<rect"), "rectangle");
    assert!(svg.contains("<polygon"), "polygon");
    // Arc path flags after the Y-flip: sweep always 0 (orientation reversed),
    // large-arc 0 for the 90° arc and 1 for the 270° arc.
    assert!(svg.contains(" A "), "arc path");
    let mut arcs = Vec::new();
    for chunk in svg.split("d=\"M ").skip(1) {
        if chunk.contains(" A ") {
            arcs.push(chunk.split('"').next().unwrap().to_string());
        }
    }
    assert_eq!(arcs.len(), 2, "two arc paths");
    for (fragment, expected_large) in arcs.iter().zip(["0", "1"]) {
        let tokens: Vec<&str> = fragment.split(' ').collect();
        // M x y A r r 0 large 0 x y
        assert_eq!(tokens[6], expected_large, "large-arc flag in {fragment}");
        assert_eq!(tokens[7], "0", "sweep flag in {fragment}");
    }
}

#[test]
fn style_groups_are_distinct_and_ordered() {
    let model = hole_plus_construction_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();

    let i_objects = svg.find("<g id=\"objects\"").unwrap();
    let i_construction = svg.find("<g id=\"construction\"").unwrap();
    let i_centers = svg.find("<g id=\"centers\"").unwrap();
    let i_dimensions = svg.find("<g id=\"dimensions\"").unwrap();
    let i_annotations = svg.find("<g id=\"annotations\"").unwrap();
    assert!(i_objects < i_construction && i_construction < i_centers);
    assert!(i_centers < i_dimensions && i_dimensions < i_annotations);

    // Object group: solid thick; construction: dashed thin; centers: dash-dot.
    let objects = &svg[i_objects..i_construction];
    assert!(!objects.contains("stroke-dasharray"));
    let construction = &svg[i_construction..i_centers];
    assert!(construction.contains("stroke-dasharray=\"4 2\""));
    assert!(construction.contains("stroke-width=\"0.25\""));
    let centers = &svg[i_centers..i_dimensions];
    assert!(centers.contains("stroke-dasharray=\"8 2 2 2\""));

    // Object outline uses the object width.
    assert!(svg.contains("stroke-width=\"0.5\""));
}

#[test]
fn diameter_full_stack_with_distinct_styles() {
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();

    // Semantic text from the typed dimension.
    assert!(svg.contains(">Ø64</text>"));
    // One mark cross (2 segments) + 2 axial lines = 4 dash-dot lines inside
    // the centers group (the pattern lives on the group element).
    let c_start = svg.find("<g id=\"centers\"").unwrap();
    let c_end = svg.find("<g id=\"dimensions\"").unwrap();
    let centers_group = &svg[c_start..c_end];
    assert!(centers_group.contains("stroke-dasharray=\"8 2 2 2\""));
    assert_eq!(centers_group.matches("<line").count(), 4);
    // The dimension axis is solid thin — never the centerline pattern.
    let i_dims = svg.find("<g id=\"dimensions\"").unwrap();
    let i_ann = svg.find("<g id=\"annotations\"").unwrap();
    let dims = &svg[i_dims..i_ann];
    assert!(!dims.contains("stroke-dasharray"));
    // Two arrowheads on the diameter axis (plate linear dims excluded).
    assert_eq!(
        dims.matches("<polygon id=\"a_ecenter_hole").count(),
        2,
        "diameter arrows"
    );
}

#[test]
fn derived_diameter_text_follows_resolve_10_to_12() {
    for (height, expected) in [(80.0, "Ø10"), (96.0, "Ø12")] {
        let model = derived_hole_model(height);
        let (drawing, _) = solve_and_draw(&model);
        let svg = drawing_to_svg(&drawing).unwrap();
        assert!(
            svg.contains(&format!(">{expected}</text>")),
            "height {height}: expected {expected}"
        );
    }
}

#[test]
fn pattern_formatter_texts() {
    // Circular.
    let model = circular_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    assert!(svg.contains("8×Ø10 PCD Ø126</text>"));

    // Grid 2×3 Ø8 X30 Y30.
    let mut grid = EngineeringModel::new();
    CommandExecutor::apply(
        &mut grid,
        &EngineeringCommand::CreatePart {
            id: PartId("g".into()),
            name: "g".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut grid,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 120.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut grid,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("grid_holes".into()),
                pattern: HolePattern::Grid {
                    rows: 2,
                    columns: 3,
                    spacing_x: 30.0,
                    spacing_y: 30.0,
                    hole: HoleDefinition { diameter: 8.0 },
                },
            },
        },
    )
    .unwrap();
    let (drawing_g, _) = solve_and_draw(&grid);
    let svg_g = drawing_to_svg(&drawing_g).unwrap();
    assert!(svg_g.contains("2×3 Ø8 X30 Y30</text>"));

    // Corner (mounting plate): 4×Ø10 OFF X15 Y15.
    let model = mounting_plate_model();
    let (drawing_c, _) = solve_and_draw(&model);
    let svg_c = drawing_to_svg(&drawing_c).unwrap();
    assert!(svg_c.contains("4×Ø10 OFF X15 Y15</text>"));
}

#[test]
fn linear_dimensions_render_lines_arrows_and_extension_lines() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let i_dims = svg.find("<g id=\"dimensions\"").unwrap();
    let i_ann = svg.find("<g id=\"annotations\"").unwrap();
    let dims = &svg[i_dims..i_ann];
    assert!(dims.contains("x1=\"10\"") || dims.contains("x2=\"10\""));
    assert_eq!(dims.matches("<polygon id=\"a_").count(), 4);
    // Vertical dim text is rotated around its anchor.
    assert!(dims.contains("rotate(-90"));
}

#[test]
fn parttitle_unicode_and_xml_escaping() {
    let unicode = titled_plate_model("复杂异形精密安装板", 120.0);
    let (drawing_u, _) = solve_and_draw(&unicode);
    let svg_u = drawing_to_svg(&drawing_u).unwrap();
    assert!(svg_u.contains(">复杂异形精密安装板</text>"));

    let special = titled_plate_model("A&B <Plate> \"V2\"", 120.0);
    let (drawing_s, _) = solve_and_draw(&special);
    let svg_s = drawing_to_svg(&drawing_s).unwrap();
    assert!(svg_s.contains("A&amp;B &lt;Plate&gt; &quot;V2&quot;</text>"));
    assert!(!svg_s.contains("<Plate>")); // raw must be escaped away

    // And the model still stores the raw string.
    let title = drawing_s
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    assert_eq!(title.text, "A&B <Plate> \"V2\"");
}

#[test]
fn renderer_rejects_tampered_models_without_lumping_errors() {
    // Empty model → missing sheet.
    let empty = DrawingModel {
        sheets: BTreeMap::new(),
        source_map: scc_drawing::source_map::DrawingSourceMap::new(),
        associative_dimensions: BTreeMap::new(),
    };
    let err = drawing_to_svg(&empty).unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-017");

    // Missing layout propagates its own code (013), not a generic render one.
    let model = mounting_plate_model();
    let (mut damaged, _) = solve_and_draw(&model);
    let view = damaged.principal_view_mut().unwrap();
    view.dimension_layouts.remove(&DimId("plate.width".into()));
    let err = drawing_to_svg(&damaged).unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-013");
}

#[test]
fn showcenterlines_false_regression_in_svg() {
    let mut state = DrawingState::new();
    state.set(DrawingProperty::ShowCenterlines { enabled: false });
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw_with_state(&model, &state);
    let svg = drawing_to_svg(&drawing).unwrap();
    let i_centers = svg.find("<g id=\"centers\"").unwrap();
    let i_dims = svg.find("<g id=\"dimensions\"").unwrap();
    let centers = &svg[i_centers..i_dims];
    assert_eq!(centers.matches("<line").count(), 0, "no center elements");
    // Diameter stack intact.
    assert!(svg.contains(">Ø64</text>"));
    assert!(dims_slice(&svg).contains("A ") || dims_slice(&svg).contains("<line"));
}

fn dims_slice(svg: &str) -> &str {
    let a = svg.find("<g id=\"dimensions\"").unwrap_or(0);
    let b = svg.find("<g id=\"annotations\"").unwrap_or(svg.len());
    &svg[a..b]
}

// ---------------------------------------------------------------------------
// Phase 8.8 — golden SVG + determinism
// ---------------------------------------------------------------------------

/// The golden fixture: titled mounting plate with corner holes — objects,
/// centers, linear + pattern dimensions, and the part title.
fn golden_plate_model() -> EngineeringModel {
    let mut model = EngineeringModel::new();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::CreatePart {
            id: PartId("mounting_plate".into()),
            name: "Mounting Plate".into(),
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::BaseProfile {
                id: FeatureId("plate".into()),
                name: "plate".into(),
                profile: scc_engineering::BaseProfile::Rectangle {
                    width: 120.0,
                    height: 80.0,
                },
            },
        },
    )
    .unwrap();
    CommandExecutor::apply(
        &mut model,
        &EngineeringCommand::AddFeature {
            feature: EngineeringFeature::HolePattern {
                id: FeatureId("corner_holes".into()),
                pattern: HolePattern::Corner {
                    offset_x: 15.0,
                    offset_y: 15.0,
                    hole: HoleDefinition { diameter: 10.0 },
                },
            },
        },
    )
    .unwrap();
    model
}

#[test]
fn golden_engineering_svg_is_byte_stable() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();

    // Structural assertions on the golden content (never eyeballing).
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("viewBox=\"0 0 150 110\""));
    for group in [
        "objects",
        "construction",
        "centers",
        "dimensions",
        "annotations",
    ] {
        assert!(svg.contains(&format!("<g id=\"{group}\"")));
    }
    assert!(svg.contains(">Mounting Plate</text>"));
    assert!(svg.contains(">120</text>"));
    assert!(svg.contains(">80</text>"));
    assert!(svg.contains("4×Ø10 OFF X15 Y15</text>"));
    // 4 corner holes → 4 marks (8 segments) + 8 axial lines = 16 dash-dot
    // lines inside the centers group.
    let c_start = svg.find("<g id=\"centers\"").unwrap();
    let c_end = svg.find("<g id=\"dimensions\"").unwrap();
    assert_eq!(svg[c_start..c_end].matches("<line").count(), 16);
    // Byte-for-byte over 100 renders.
    for _ in 1..100 {
        assert_eq!(drawing_to_svg(&drawing).unwrap(), svg);
    }

    // Golden fixture file: create once, then compare forever.
    let golden_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/engineering_plate.svg");
    if !golden_path.exists() {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        std::fs::write(&golden_path, &svg).unwrap();
    }
    let stored = std::fs::read_to_string(&golden_path).unwrap();
    assert_eq!(stored, svg, "golden SVG drifted");
}

// ---------------------------------------------------------------------------
// Phase 8.10.1 — dimension text paint isolation (structure regression)
// ---------------------------------------------------------------------------

/// Extract one full element by its `id` attribute (open tag through `>`).
fn element_with_id<'a>(svg: &'a str, id: &str) -> &'a str {
    let marker = format!("id=\"{id}\"");
    let i = svg
        .find(&marker)
        .unwrap_or_else(|| panic!("missing element {id}"));
    let start = svg[..i].rfind('<').unwrap();
    let end = svg[i..].find('>').unwrap() + i + 1;
    &svg[start..end]
}

/// Extract the `<g id="dimensions">` fragment (no nested groups inside).
fn dimensions_group(svg: &str) -> &str {
    let a = svg.find("<g id=\"dimensions\"").unwrap();
    let b = a + svg[a..].find("</g>").unwrap();
    &svg[a..b]
}

#[test]
fn dimensions_group_texts_are_stroke_isolated() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let dims = dimensions_group(&svg);
    // The group-level stroke stays for the geometry it styles ...
    assert!(dims.starts_with("<g id=\"dimensions\" fill=\"none\" stroke=\"black\">"));
    // ... and EVERY text inside carries its own isolation (checked per
    // element, not just once globally).
    let texts: Vec<usize> = dims.match_indices("<text").map(|(i, _)| i).collect();
    assert!(!texts.is_empty(), "golden fixture must contain dimensions");
    for start in texts {
        let end = start + dims[start..].find("</text>").unwrap();
        let element = &dims[start..end];
        assert!(
            element.contains("stroke=\"none\""),
            "dimension text lacks paint isolation: {element}"
        );
    }
}

#[test]
fn pattern_callout_leader_vs_text_paint_split() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let dims = dimensions_group(&svg);
    // The callout leader keeps the thin inherited group stroke.
    let text_i = dims.find("id=\"t_ecorner_holes.pattern\"").unwrap();
    let leader = dims[..text_i].rfind("<line").unwrap();
    let leader_end = leader + dims[leader..].find("/>").unwrap();
    assert!(dims[leader..leader_end].contains("stroke-width=\"0.25\""));
    // While its callout text opts out of that stroke entirely.
    assert!(element_with_id(&svg, "t_ecorner_holes.pattern").contains("stroke=\"none\""));
}

#[test]
fn part_title_text_has_own_fill_only() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let title = element_with_id(&svg, "t_epart_title");
    assert!(title.contains("fill=\"black\""));
    assert!(title.contains("stroke=\"none\""));
}

#[test]
fn vertical_dimension_text_rotation_with_paint_isolation() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let height_text = element_with_id(&svg, "t_eplate.height");
    assert!(height_text.contains("rotate(-90"));
    assert!(height_text.contains("stroke=\"none\""));
}

#[test]
fn legacy_geometry_svg_export_still_works() {
    // The old GeometryModel exporter is untouched and independent.
    let geometry = synthetic_geometry(vec![(
        "plate",
        GeometryPrimitive::Rectangle {
            origin: Point2 { x: -60.0, y: -40.0 },
            width: 120.0,
            height: 80.0,
        },
    )]);
    let svg = scc_geometry::export_svg(&geometry).unwrap();
    assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
}
// ---------------------------------------------------------------------------
// Phase 8.9 — DXF drawing renderer
// ---------------------------------------------------------------------------

use scc_drawing::{
    drawing_to_dxf, LAYER_ANNOTATION, LAYER_CENTER, LAYER_CONSTRUCTION, LAYER_DIMENSION,
    LAYER_OBJECT,
};

/// Entity count for a given DXF entity type on a given layer.
fn dxf_count(dxf: &str, entity: &str, layer: &str) -> usize {
    // DXF R13+ entities place an AcDbEntity subclass marker before group 8;
    // parse each group-code/value pair rather than assuming the layer is the
    // first field after the entity name.
    let lines = dxf.lines().collect::<Vec<_>>();
    let mut count = 0;
    for index in 0..lines.len().saturating_sub(1) {
        if lines[index] != "0" || lines[index + 1] != entity {
            continue;
        }
        let mut pair = index + 2;
        while pair + 1 < lines.len() && lines[pair] != "0" {
            if lines[pair] == "8" && lines[pair + 1] == layer {
                count += 1;
                break;
            }
            pair += 2;
        }
    }
    count
}

#[test]
fn dxf_structure_units_and_extents() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let dxf = drawing_to_dxf(&drawing).unwrap();

    // Section order: HEADER → TABLES → ENTITIES → EOF.
    let i_h = dxf.find("2\nHEADER").unwrap();
    let i_t = dxf.find("2\nTABLES").unwrap();
    let i_e = dxf.find("2\nENTITIES").unwrap();
    assert!(i_h < i_t && i_t < i_e);
    assert!(dxf.ends_with("0\nENDSEC\n0\nEOF\n"));
    // Version + millimetre units.
    assert!(dxf.contains("$ACADVER\n1\nAC1027"));
    assert!(dxf.contains("$INSUNITS\n70\n4"));
    // Extents strictly from DrawingSheet::bounds (150×110 at 1:1).
    assert!(dxf.contains("$EXTMIN\n10\n0\n20\n0"));
    assert!(dxf.contains("$EXTMAX\n10\n150\n20\n110"));
}

#[test]
fn dxf_layers_and_linetypes_declared_and_used() {
    let model = hole_plus_construction_model();
    let (drawing, _) = solve_and_draw(&model);
    let dxf = drawing_to_dxf(&drawing).unwrap();

    for layer in [
        LAYER_OBJECT,
        LAYER_CONSTRUCTION,
        LAYER_CENTER,
        LAYER_DIMENSION,
        LAYER_ANNOTATION,
    ] {
        let uses = dxf.matches(&format!("8\n{layer}\n")).count();
        assert!(uses > 0, "layer {layer} is used");
        assert!(
            dxf.contains(&format!("2\n{layer}\n")),
            "layer {layer} is declared"
        );
    }
    for ltype in ["CONTINUOUS", "AICAD_DASHED", "AICAD_CENTERLINE"] {
        assert!(
            dxf.contains(&format!("2\n{ltype}\n")),
            "ltype {ltype} declared"
        );
    }
    // Construction entities reference the dashed linetype layer; centers use
    // their own — never swapped.
    assert!(dxf_count(&dxf, "CIRCLE", LAYER_CONSTRUCTION) >= 1);
    assert!(dxf_count(&dxf, "LINE", LAYER_CENTER) == 4); // mark cross ×2 + 2 axial
}

#[test]
fn dxf_primitives_all_kinds_and_arc_angles_pass_through() {
    let cases: Vec<(&str, GeometryPrimitive)> = vec![
        (
            "p",
            GeometryPrimitive::Point {
                position: Point2 { x: 1.0, y: 1.0 },
            },
        ),
        (
            "l",
            GeometryPrimitive::Line {
                start: Point2 { x: 0.0, y: 0.0 },
                end: Point2 { x: 5.0, y: 5.0 },
            },
        ),
        (
            "c",
            GeometryPrimitive::Circle {
                center: Point2 { x: 20.0, y: 20.0 },
                radius: 4.0,
            },
        ),
        (
            "a",
            GeometryPrimitive::Arc {
                center: Point2 { x: 40.0, y: 20.0 },
                radius: 6.0,
                start_angle: -45.0f64.to_radians(),
                end_angle: std::f64::consts::FRAC_PI_2,
            },
        ),
        (
            "r",
            GeometryPrimitive::Rectangle {
                origin: Point2 { x: -6.0, y: -4.0 },
                width: 12.0,
                height: 8.0,
            },
        ),
        (
            "poly",
            GeometryPrimitive::Polygon {
                vertices: vec![
                    Point2 { x: 0.0, y: 0.0 },
                    Point2 { x: 2.0, y: 0.0 },
                    Point2 { x: 0.0, y: 2.0 },
                ],
            },
        ),
    ];
    let ids: Vec<&str> = cases.iter().map(|(id, _)| *id).collect();
    let geometry = synthetic_geometry(cases.clone());
    let emap = feature_map_for_entities(&ids, &FeatureId("base".into()));
    let features = features_with(EngineeringFeature::BaseProfile {
        id: FeatureId("base".into()),
        name: "base".into(),
        profile: scc_engineering::BaseProfile::Rectangle {
            width: 120.0,
            height: 80.0,
        },
    });
    let state = DrawingState::new();
    let inputs = DrawingInputs::new(&geometry, &emap, &state, &features, None, None);
    let drawing = build_drawing(&inputs).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();

    for entity in ["POINT", "LINE", "CIRCLE", "ARC", "LWPOLYLINE"] {
        assert!(dxf.contains(&format!("0\n{entity}\n")), "{entity} present");
    }
    // Arc angles pass through in degrees without remapping (cross-zero arc:
    // start −45° stays −45°).
    assert!(dxf.contains("\n50\n-45\n"), "start angle degrees preserved");
    // Closed rectangle polyline: exactly one 4-vertex closed polyline.
    assert!(dxf.contains("90\n4\n"));
    assert!(dxf.contains("70\n1\n"));
}

#[test]
fn dxf_dimension_stack_lines_arrows_text() {
    let model = mounting_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let dxf = drawing_to_dxf(&drawing).unwrap();

    // Dimension texts from the shared formatter.
    assert!(dxf.contains("\n1\n120\n"));
    assert!(dxf.contains("\n1\n80\n"));
    assert!(dxf.contains("\n1\n4×Ø10 OFF X15 Y15\n"));
    // SOLID arrowheads on the dimension layer (2 per linear dim).
    assert!(dxf_count(&dxf, "SOLID", LAYER_DIMENSION) >= 4);
    // Extension/dimension lines present on the dimension layer.
    assert!(dxf_count(&dxf, "LINE", LAYER_DIMENSION) >= 6);
    // Vertical height text rotates +90° in the Y-up DXF plane.
    let height_text_pos = dxf.find("\n1\n80\n").unwrap();
    let tail = &dxf[height_text_pos..];
    assert!(tail.contains("\n50\n90\n"), "vertical text rotation is 90");
}

#[test]
fn dxf_center_vs_construction_distinct() {
    let model = hole_plus_construction_model();
    let (drawing, _) = solve_and_draw(&model);
    let dxf = drawing_to_dxf(&drawing).unwrap();
    // Center geometry on AICAD_CENTER; construction circle on
    // AICAD_CONSTRUCTION — different layers by definition.
    assert!(dxf_count(&dxf, "LINE", LAYER_CENTER) == 4);
    assert!(dxf_count(&dxf, "CIRCLE", LAYER_CONSTRUCTION) == 1);
    assert!(dxf_count(&dxf, "CIRCLE", LAYER_OBJECT) >= 1);
    // The construction circle is NOT on the object layer.
    assert_eq!(dxf_count(&dxf, "CIRCLE", LAYER_OBJECT), 1, "hole only");
}

#[test]
fn dxf_scale_fixtures_values_constant_spans_scale() {
    for (scale_value, _span) in [(1.0, "120.0"), (0.5, "60"), (2.0, "240")] {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: scale_value });
        let model = titled_plate_model("Mounting Plate", 120.0);
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let dxf = drawing_to_dxf(&drawing).unwrap();
        // Engineering value constant in text...
        assert!(dxf.contains("\n1\n120\n"), "scale {scale_value}");
        // ...entity spans follow scale via $EXTMAX (envelope incl. dims):
        // 1:1 → 150, 1:2 → 90, 2:1 → 270.
        let expected_max_x = ["150", "90", "270"][match scale_value {
            1.0 => 0,
            0.5 => 1,
            _ => 2,
        }];
        assert!(dxf.contains(&format!("$EXTMAX\n10\n{expected_max_x}\n")));
    }
}

#[test]
fn dxf_derived_diameter_10_to_12() {
    for (height, expected) in [(80.0, "Ø10"), (96.0, "Ø12")] {
        let model = derived_hole_model(height);
        let (drawing, _) = solve_and_draw(&model);
        let dxf = drawing_to_dxf(&drawing).unwrap();
        assert!(dxf.contains(&format!("\n1\n{expected}\n")));
    }
}

#[test]
fn no_native_dimension_entity_and_no_svg_flip() {
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw(&model);
    let dxf = drawing_to_dxf(&drawing).unwrap();
    // No native DIMENSION entity / DIMSTYLE table / anonymous blocks.
    assert!(!dxf.contains("0\nDIMENSION\n"));
    assert!(!dxf.contains("DIMSTYLE"));
    assert!(!dxf.contains("0\nBLOCK\n"));
    // Sheet Y-up maps directly: hole centre sheet y=70 must appear as y=70,
    // not flipped to H−70=40.
    assert!(dxf.contains("\n20\n70\n"), "unflipped Y-up coordinate");
    assert!(!dxf.contains("\n20\n40\n"), "no SVG-style flip leaked");
}

#[test]
fn dxf_unicode_and_special_characters_not_xml_escaped() {
    let unicode = titled_plate_model("复杂异形精密安装板", 120.0);
    let (du, _) = solve_and_draw(&unicode);
    let dxu = drawing_to_dxf(&du).unwrap();
    assert!(dxu.contains("\n1\n复杂异形精密安装板\n"));

    let special = titled_plate_model("A&B <Plate> \"V2\"", 120.0);
    let (ds, _) = solve_and_draw(&special);
    let dxs = drawing_to_dxf(&ds).unwrap();
    assert!(dxs.contains("A&B <Plate> \"V2\""));
    assert!(!dxs.contains("&amp;"));
    assert!(!dxs.contains("&lt;"));

    // Model still stores raw text.
    let title = ds
        .annotation(&DrawingAnnotationId("part_title".into()))
        .unwrap();
    assert_eq!(title.text, "A&B <Plate> \"V2\"");
}

#[test]
fn dxf_fail_closed_propagates_integrity_errors() {
    // Missing layout propagates its own code through the DXF path.
    let model = mounting_plate_model();
    let (mut damaged, _) = solve_and_draw(&model);
    let view = damaged.principal_view_mut().unwrap();
    view.dimension_layouts
        .remove(&DrawingDimensionId("plate.width".into()));
    let err = drawing_to_dxf(&damaged).unwrap_err();
    assert_eq!(err.code(), "SCC-DRW-013");
    // Sheet-bounds containment is structurally guaranteed (bounds derive from
    // the same presented elements), so the renderer treats non-finite or
    // inconsistent input via the upstream validators instead.
}
#[test]
fn dxf_byte_deterministic_over_one_hundred_runs() {
    let model = circular_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let dxf0 = drawing_to_dxf(&drawing).unwrap();
    for _ in 1..100 {
        assert_eq!(drawing_to_dxf(&drawing).unwrap(), dxf0);
    }

    // Golden fixture: create once, compare forever.
    let golden_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/engineering_plate.dxf");
    if !golden_path.exists() || std::env::var("SCC_WRITE_GOLDENS").ok().as_deref() == Some("1") {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        std::fs::write(&golden_path, &dxf0).unwrap();
    }
    let stored = std::fs::read_to_string(&golden_path).unwrap();
    assert_eq!(stored, dxf0, "golden DXF drifted");

    // Golden structural assertions.
    assert!(stored.contains("$INSUNITS\n70\n4"));
    assert!(stored.ends_with("0\nENDSEC\n0\nEOF\n"));
    assert!(stored.contains("\n1\n8×Ø10 PCD Ø126\n"));
}

#[test]
fn svg_dxf_semantic_parity_on_one_model() {
    // One DrawingModel, two renderers: engineering content must agree.
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();

    // Same dimension texts.
    for expected in ["120", "80", "4×Ø10 OFF X15 Y15"] {
        assert!(
            svg.contains(&format!(">{expected}</text>")),
            "svg {expected}"
        );
        assert!(
            dxf.contains(&format!("\n1\n{expected}\n")),
            "dxf {expected}"
        );
    }
    // Same title.
    assert!(svg.contains(">Mounting Plate</text>"));
    assert!(dxf.contains("\n1\nMounting Plate\n"));
    // Same center element counts (16 dash-dot SVG lines ↔ 16 CENTER-layer
    // DXF lines).
    let c_start = svg.find("<g id=\"centers\"").unwrap();
    let c_end = svg.find("<g id=\"dimensions\"").unwrap();
    assert_eq!(svg[c_start..c_end].matches("<line").count(), 16);
    assert_eq!(dxf_count(&dxf, "LINE", LAYER_CENTER), 16);
    // Same object circle count (4 corner holes).
    let o_start = svg.find("<g id=\"objects\"").unwrap();
    let o_end = svg.find("<g id=\"construction\"").unwrap();
    assert_eq!(svg[o_start..o_end].matches("<circle").count(), 4);
    assert_eq!(dxf_count(&dxf, "CIRCLE", LAYER_OBJECT), 4);
}

// ---------------------------------------------------------------------------
// Phase 8.9.1 — endpoint-level SVG geometry + SVG/DXF geometry parity
//
// The golden snapshot only guards against future output *change*; it cannot
// prove the output is *correct* (it locked B-1's wrong bytes for two phases).
// These tests assert actual sheet→SVG coordinates: both endpoints of every
// segment must pass through the single Y-flip rule svg_y = H − sheet_y.
// ---------------------------------------------------------------------------

/// Parse one numeric attribute out of a `<line .../>` attribute string.
fn svg_attr_num(elem: &str, key: &str) -> f64 {
    let marker = format!("{key}=\"");
    let rest = elem.split(&marker).nth(1).expect("attribute present");
    let raw: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    raw.parse().unwrap()
}

/// Extract (x1, y1, x2, y2) from one `<line .../>` element body.
fn parse_svg_line(elem: &str) -> (f64, f64, f64, f64) {
    (
        svg_attr_num(elem, "x1"),
        svg_attr_num(elem, "y1"),
        svg_attr_num(elem, "x2"),
        svg_attr_num(elem, "y2"),
    )
}

/// All `<line>` elements carrying `data-drawing-id="id"`.
fn svg_lines_with_id(svg: &str, id: &str) -> Vec<(f64, f64, f64, f64)> {
    let needle = format!("data-drawing-id=\"{id}\"");
    let mut out = Vec::new();
    for chunk in svg.split("<line ").skip(1) {
        let elem = &chunk[..chunk.find("/>").expect("self-closing line")];
        if elem.contains(&needle) {
            out.push(parse_svg_line(elem));
        }
    }
    out
}

/// Every `<line>` inside `<g id="{id}">` (up to the next group open tag).
fn svg_group_lines(svg: &str, id: &str) -> Vec<(f64, f64, f64, f64)> {
    let open = format!("<g id=\"{id}\"");
    let start = svg.find(&open).expect("group exists");
    let after = start + open.len();
    let end = svg[after..]
        .find("<g id=")
        .map(|i| after + i)
        .unwrap_or(svg.len());
    svg[after..end]
        .split("<line ")
        .skip(1)
        .map(|chunk| parse_svg_line(&chunk[..chunk.find("/>").expect("self-closing line")]))
        .collect()
}

/// Every DXF `LINE` entity on `layer`, as (x1, y1, x2, y2) in sheet space.
fn dxf_lines_on_layer(dxf: &str, layer: &str) -> Vec<(f64, f64, f64, f64)> {
    let toks: Vec<&str> = dxf.lines().collect();
    assert_eq!(toks.len() % 2, 0, "DXF is group-code/value pairs");
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < toks.len() {
        if toks[i] == "0" && toks[i + 1] == "LINE" {
            let mut j = i + 2;
            let mut entity_layer = None;
            let (mut x1, mut y1, mut x2, mut y2) = (None, None, None, None);
            while j + 1 < toks.len() && toks[j] != "0" {
                match toks[j] {
                    "8" => entity_layer = Some(toks[j + 1]),
                    "10" => x1 = toks[j + 1].parse::<f64>().ok(),
                    "20" => y1 = toks[j + 1].parse::<f64>().ok(),
                    "11" => x2 = toks[j + 1].parse::<f64>().ok(),
                    "21" => y2 = toks[j + 1].parse::<f64>().ok(),
                    _ => {}
                }
                j += 2;
            }
            if entity_layer == Some(layer) {
                out.push((
                    x1.expect("10"),
                    y1.expect("20"),
                    x2.expect("11"),
                    y2.expect("21"),
                ));
            }
            i = j;
        } else {
            i += 2;
        }
    }
    out
}

fn seg_len(s: (f64, f64, f64, f64)) -> f64 {
    ((s.2 - s.0) * (s.2 - s.0) + (s.3 - s.1) * (s.3 - s.1)).sqrt()
}

/// True when an SVG segment is the sheet-space Y-mirror of a DXF
/// segment — compared as unordered {start, end} sets so BOTH endpoints are
/// checked (checking only the start point is how B-1 escaped). Tolerance is
/// `WITNESS_EPSILON_MM` because `number()` truncates to 12 decimal places.
fn svg_is_flip_of_dxf(svg: (f64, f64, f64, f64), dxf: (f64, f64, f64, f64), h: f64) -> bool {
    let eps = scc_drawing::WITNESS_EPSILON_MM;
    let close = |a: f64, b: f64| (a - b).abs() < eps;
    let flipped = (dxf.0, h - dxf.1, dxf.2, h - dxf.3);
    let direct = close(svg.0, flipped.0)
        && close(svg.1, flipped.1)
        && close(svg.2, flipped.2)
        && close(svg.3, flipped.3);
    let swapped = close(svg.0, flipped.2)
        && close(svg.1, flipped.3)
        && close(svg.2, flipped.0)
        && close(svg.3, flipped.1);
    direct || swapped
}

#[test]
fn svg_center_mark_and_centerline_endpoints_are_exact() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();

    // Hole [0] sits at sheet (25, 25); H=110 → mirrored centre y = 85.
    let mark = svg_lines_with_id(&svg, "center_mark:corner_holes_array[0]");
    assert_eq!(mark.len(), 2);
    assert_eq!(mark[0], (22.0, 85.0, 28.0, 85.0), "horizontal arm");
    assert_eq!(mark[1], (25.0, 88.0, 25.0, 82.0), "vertical arm");
    assert_eq!(mark[0].1, mark[0].3, "horizontal arm stays horizontal");
    assert_eq!(mark[1].0, mark[1].2, "vertical arm stays vertical");

    // Centerlines keep their axis and their sheet-space length (±7 overhang).
    assert_eq!(
        svg_lines_with_id(&svg, "h_centerline:corner_holes_array[0]"),
        vec![(18.0, 85.0, 32.0, 85.0)]
    );
    let v = svg_lines_with_id(&svg, "v_centerline:corner_holes_array[0]");
    assert_eq!(v, vec![(25.0, 92.0, 25.0, 78.0)]);
    for line in v {
        assert!(
            (seg_len(line) - 14.0).abs() < 1.0e-9,
            "Y mirror preserves length"
        );
    }
}

#[test]
fn svg_linear_dimension_and_extension_lines_stay_axis_aligned() {
    for model in [golden_plate_model(), circular_plate_model()] {
        let (drawing, _) = solve_and_draw(&model);
        let svg = drawing_to_svg(&drawing).unwrap();
        let lines = svg_group_lines(&svg, "dimensions");

        // Exactly one non-axis-aligned line per model: the pattern leader.
        // Everything else (dimension lines + extension lines) must remain
        // horizontal or vertical after the flip.
        let diagonal: Vec<_> = lines.iter().filter(|l| l.0 != l.2 && l.1 != l.3).collect();
        assert_eq!(diagonal.len(), 1, "only the pattern leader is diagonal");

        // Golden fixture specifics: width dim truly horizontal (span 120),
        // height dim truly vertical (span 80), extension lines keep their
        // original orientation.
        // Golden fixture specifics are asserted below on the exact fixture.
        let has_width_dim = lines
            .iter()
            .any(|l| (l.1 - l.3).abs() < 1.0e-9 && (l.2 - l.0).abs() >= 119.999);
        let has_height_dim = lines
            .iter()
            .any(|l| (l.0 - l.2).abs() < 1.0e-9 && (l.3 - l.1).abs() >= 79.999);
        assert!(has_width_dim || has_height_dim);
    }

    // Exact coordinates on the golden fixture (H=110).
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let lines = svg_group_lines(&svg, "dimensions");
    for expected in [
        (138.0, 100.0, 138.0, 20.0),  // height dimension line
        (131.0, 100.0, 140.0, 100.0), // height ext line (horizontal)
        (131.0, 20.0, 140.0, 20.0),   // height ext line (horizontal)
        (10.0, 12.0, 130.0, 12.0),    // width dimension line
        (10.0, 19.0, 10.0, 10.0),     // width ext line (vertical)
        (130.0, 19.0, 130.0, 10.0),   // width ext line (vertical)
        (120.0, 30.0, 128.0, 22.0),   // pattern leader (both ends flipped)
    ] {
        assert!(lines.contains(&expected), "missing {expected:?}");
    }
}

#[test]
fn pattern_leader_both_endpoints_flipped_circular_fixture() {
    let model = circular_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let h = drawing.principal_sheet().unwrap().bounds().max_y;
    let svg = drawing_to_svg(&drawing).unwrap();
    let dxf = drawing_to_dxf(&drawing).unwrap();

    let leaders = svg_group_lines(&svg, "dimensions")
        .into_iter()
        .filter(|l| l.0 != l.2 && l.1 != l.3)
        .collect::<Vec<_>>();
    assert_eq!(leaders.len(), 1, "one circular-pattern callout leader");
    let mut dimension_layer = dxf_lines_on_layer(&dxf, LAYER_DIMENSION);
    let matched = dimension_layer
        .iter()
        .position(|d| svg_is_flip_of_dxf(leaders[0], *d, h))
        .expect("leader matches a DIMENSION-layer line under the Y mirror, both endpoints");
    dimension_layer.swap_remove(matched);
}

#[test]
fn svg_dxf_geometry_parity_both_endpoints_all_segments() {
    for model in [golden_plate_model(), circular_plate_model()] {
        let (drawing, _) = solve_and_draw(&model);
        let h = drawing.principal_sheet().unwrap().bounds().max_y;
        let svg = drawing_to_svg(&drawing).unwrap();
        let dxf = drawing_to_dxf(&drawing).unwrap();

        for group_id in ["centers", "dimensions"] {
            let svg_lines = svg_group_lines(&svg, group_id);
            let layer = match group_id {
                "centers" => LAYER_CENTER,
                _ => LAYER_DIMENSION,
            };
            let mut dxf_lines = dxf_lines_on_layer(&dxf, layer);
            assert_eq!(
                svg_lines.len(),
                dxf_lines.len(),
                "{group_id}: segment count parity"
            );
            for s in &svg_lines {
                let idx = dxf_lines
                    .iter()
                    .position(|d| svg_is_flip_of_dxf(*s, *d, h))
                    .unwrap_or_else(|| panic!("{group_id}: no DXF mirror for {s:?}"));
                // The mirror must also preserve Euclidean length.
                assert!(
                    (seg_len(*s) - seg_len(dxf_lines[idx])).abs() < 1.0e-6,
                    "{group_id}: length preserved across the flip"
                );
                dxf_lines.swap_remove(idx);
            }
            assert!(dxf_lines.is_empty(), "{group_id}: bijection complete");
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 8.9.2 — arrowhead vertex-level SVG geometry
//
// The Phase 8.9.1 fix covered segment endpoints; arrowhead `<polygon>`
// vertices were still emitted in raw sheet space. These tests pin every
// emitted triangle: tips attach to the (already flipped) dimension
// geometry, winding points inward after the flip, triangle size stays a
// paper-mm constant across scales, and every SVG vertex has a Y-mirrored
// DXF SOLID counterpart (all three corners, unordered).
// ---------------------------------------------------------------------------

use scc_drawing::{ArrowAnchor, DEFAULT_ARROW_SIZE_MM};

/// Parse the `points` attribute of one `<polygon .../>` element body into
/// its vertex list.
fn parse_svg_polygon(elem: &str) -> Vec<(f64, f64)> {
    let rest = elem.split("points=\"").nth(1).expect("points present");
    let pts = rest.split('"').next().unwrap();
    pts.split_whitespace()
        .map(|pair| {
            let mut it = pair.split(',');
            let x: f64 = it.next().unwrap().parse().unwrap();
            let y: f64 = it.next().unwrap().parse().unwrap();
            (x, y)
        })
        .collect()
}

/// Every `<polygon>` inside `<g id="{id}">` with its `id="..."` attribute and
/// parsed vertex list (same group-slicing pattern as [`svg_group_lines`]).
fn svg_group_polygons(svg: &str, id: &str) -> Vec<(String, Vec<(f64, f64)>)> {
    let open = format!("<g id=\"{id}\"");
    let start = svg.find(&open).expect("group exists");
    let after = start + open.len();
    let end = svg[after..]
        .find("<g id=")
        .map(|i| after + i)
        .unwrap_or(svg.len());
    svg[after..end]
        .split("<polygon ")
        .skip(1)
        .map(|chunk| {
            let elem = &chunk[..chunk.find("/>").expect("self-closing polygon")];
            let id_rest = elem.split("id=\"").nth(1).expect("polygon id");
            let poly_id = id_rest.split('"').next().unwrap().to_string();
            (poly_id, parse_svg_polygon(elem))
        })
        .collect()
}

/// All dimension-group polygons whose captured `id` contains `needle`.
fn svg_dim_polygons_with_id(svg: &str, needle: &str) -> Vec<Vec<(f64, f64)>> {
    svg_group_polygons(svg, "dimensions")
        .into_iter()
        .filter(|(poly_id, _)| poly_id.contains(needle))
        .map(|(_, verts)| verts)
        .collect()
}

/// Every DXF `SOLID` entity on `layer`, as its three corner points in sheet
/// space (codes 10/20 → tip, 11/21, 12/22).
fn dxf_solids_on_layer(dxf: &str, layer: &str) -> Vec<[(f64, f64); 3]> {
    let toks: Vec<&str> = dxf.lines().collect();
    assert_eq!(toks.len() % 2, 0, "DXF is group-code/value pairs");
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < toks.len() {
        if toks[i] == "0" && toks[i + 1] == "SOLID" {
            let mut j = i + 2;
            let mut entity_layer = None;
            let (mut x10, mut y20): (f64, f64) = (0.0, 0.0);
            let (mut x11, mut y21): (f64, f64) = (0.0, 0.0);
            let (mut x12, mut y22): (f64, f64) = (0.0, 0.0);
            while j + 1 < toks.len() && toks[j] != "0" {
                match toks[j] {
                    "8" => entity_layer = Some(toks[j + 1]),
                    "10" => x10 = toks[j + 1].parse().unwrap(),
                    "20" => y20 = toks[j + 1].parse().unwrap(),
                    "11" => x11 = toks[j + 1].parse().unwrap(),
                    "21" => y21 = toks[j + 1].parse().unwrap(),
                    "12" => x12 = toks[j + 1].parse().unwrap(),
                    "22" => y22 = toks[j + 1].parse().unwrap(),
                    _ => {}
                }
                j += 2;
            }
            if entity_layer == Some(layer) {
                out.push([(x10, y20), (x11, y21), (x12, y22)]);
            }
            i = j;
        } else {
            i += 2;
        }
    }
    out
}

#[test]
fn svg_width_dim_arrowheads_attach_to_dimension_line_endpoints() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();

    // The width dim line is the flipped segment (10,12)→(130,12); each
    // arrowhead's FIRST vertex is its tip and must land exactly on an endpoint.
    let tips = svg_dim_polygons_with_id(&svg, "a_eplate.width")
        .into_iter()
        .map(|verts| verts[0])
        .collect::<Vec<_>>();
    assert_eq!(tips.len(), 2);
    assert!(tips.contains(&(10.0, 12.0)), "left tip {tips:?}");
    assert!(tips.contains(&(130.0, 12.0)), "right tip {tips:?}");
}

#[test]
fn svg_height_dim_arrowheads_attach_to_dimension_line_endpoints() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();

    // The height dim line is the flipped vertical segment (138,100)→(138,20).
    let tips = svg_dim_polygons_with_id(&svg, "a_eplate.height")
        .into_iter()
        .map(|verts| verts[0])
        .collect::<Vec<_>>();
    assert_eq!(tips.len(), 2);
    assert!(tips.contains(&(138.0, 100.0)), "top tip {tips:?}");
    assert!(tips.contains(&(138.0, 20.0)), "bottom tip {tips:?}");
}

#[test]
fn svg_linear_arrows_point_inward_and_sit_on_the_dimension_line() {
    let model = golden_plate_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let eps = scc_drawing::WITNESS_EPSILON_MM;
    let view = drawing.principal_view().unwrap();

    for (dim_id, on_dim_line) in [("plate.width", 12.0), ("plate.height", 138.0)] {
        let layout = &view.dimension_layouts[&DrawingDimensionId(dim_id.into())];
        let arrows: [ArrowAnchor; 2] = match &layout.geometry {
            DimensionLayoutGeometry::Linear { arrows, .. } => *arrows,
            _ => panic!("{dim_id}: linear layout expected"),
        };
        let polys = svg_dim_polygons_with_id(&svg, &format!("a_e{dim_id}"));
        assert_eq!(polys.len(), 2, "{dim_id}: two arrowheads");
        for (arrow, verts) in arrows.iter().zip(&polys) {
            let tip = verts[0];
            // Sheet-space base midpoint flips to base_mid − tip =
            // (−dir.x·L, +dir.y·L): winding reverses, direction stays inward.
            let base_mid = (
                (verts[1].0 + verts[2].0) / 2.0,
                (verts[1].1 + verts[2].1) / 2.0,
            );
            let dx = base_mid.0 - tip.0;
            let dy = base_mid.1 - tip.1;
            assert!(
                (dx - (-arrow.direction.x * DEFAULT_ARROW_SIZE_MM)).abs() < eps,
                "{dim_id}: base midpoint x offset"
            );
            assert!(
                (dy - (arrow.direction.y * DEFAULT_ARROW_SIZE_MM)).abs() < eps,
                "{dim_id}: base midpoint y offset"
            );
            // The tip sits on the (flipped) dimension line itself.
            if dim_id == "plate.width" {
                assert_eq!(tip.1, on_dim_line, "{dim_id}: tip y on dim line");
            } else {
                assert_eq!(tip.0, on_dim_line, "{dim_id}: tip x on dim line");
            }
        }
    }
}

#[test]
fn svg_diameter_arrows_attach_to_flipped_axis_endpoints() {
    let model = center_hole_model();
    let (drawing, _) = solve_and_draw(&model);
    let svg = drawing_to_svg(&drawing).unwrap();
    let eps = scc_drawing::WITNESS_EPSILON_MM;

    // H comes from the viewBox (4th number), never hard-coded.
    let vb = svg.split("viewBox=\"").nth(1).expect("viewBox present");
    let vb = vb.split('"').next().unwrap();
    let h: f64 = vb.split_whitespace().nth(3).unwrap().parse().unwrap();

    let polys = svg_dim_polygons_with_id(&svg, "a_ecenter_hole");
    assert_eq!(polys.len(), 2, "exactly two diameter arrowheads");
    // The 45° axis is the ONLY non-axis-aligned dimension line (width/height
    // extension + dimension lines are all horizontal or vertical).
    let diagonals: Vec<_> = svg_group_lines(&svg, "dimensions")
        .into_iter()
        .filter(|l| l.0 != l.2 && l.1 != l.3)
        .collect();
    assert_eq!(diagonals.len(), 1, "only the diameter axis line");
    let axis = diagonals[0];
    // The axis <line> is emitted through the SAME single flip rule as the
    // arrowheads, so its parsed endpoints are already SVG-space: each tip
    // must coincide with one of them exactly (no second flip).
    let end_a = (axis.0, axis.1);
    let end_b = (axis.2, axis.3);
    for p in &polys {
        let tip = p[0];
        let hits_a = (tip.0 - end_a.0).abs() < eps && (tip.1 - end_a.1).abs() < eps;
        let hits_b = (tip.0 - end_b.0).abs() < eps && (tip.1 - end_b.1).abs() < eps;
        assert!(hits_a || hits_b, "tip {tip:?} on an axis endpoint");
    }

    // Exact cross-check: centre sheet (70,70), r=32, axis at 45° so
    // e = r/√2 along both axes; tips flip through H.
    let e = 32.0 / std::f64::consts::SQRT_2;
    for expected in [(70.0 - e, h - (70.0 - e)), (70.0 + e, h - (70.0 + e))] {
        assert!(
            polys
                .iter()
                .any(|p| (p[0].0 - expected.0).abs() < eps && (p[0].1 - expected.1).abs() < eps),
            "expected tip {expected:?}"
        );
    }
}

#[test]
fn svg_arrow_triangles_are_scale_invariant_paper_mm_constants() {
    for scale_value in [1.0, 0.5, 2.0] {
        let mut state = DrawingState::new();
        state.set(DrawingProperty::Scale { value: scale_value });
        let model = center_hole_model();
        let (drawing, _) = solve_and_draw_with_state(&model, &state);
        let svg = drawing_to_svg(&drawing).unwrap();

        // The measured axis grows with scale...
        let diagonals: Vec<_> = svg_group_lines(&svg, "dimensions")
            .into_iter()
            .filter(|l| l.0 != l.2 && l.1 != l.3)
            .collect();
        assert_eq!(diagonals.len(), 1, "scale {scale_value}: one axis line");
        let axis_len = seg_len(diagonals[0]);
        assert!(
            (axis_len - 64.0 * scale_value).abs() < scc_drawing::WITNESS_EPSILON_MM,
            "scale {scale_value}: axis length {axis_len}"
        );

        // ...while every arrowhead stays a constant paper-mm triangle:
        // longest edge √7.25 (two tip→wing edges), shortest edge 2.0 (base).
        let polys = svg_dim_polygons_with_id(&svg, "a_ecenter_hole");
        assert_eq!(polys.len(), 2, "scale {scale_value}: two arrowheads");
        for verts in &polys {
            let dist =
                |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
            let edges = [
                dist(verts[0], verts[1]),
                dist(verts[0], verts[2]),
                dist(verts[1], verts[2]),
            ];
            let max_edge = edges.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let min_edge = edges.iter().cloned().fold(f64::INFINITY, f64::min);
            assert!((max_edge - 7.25_f64.sqrt()).abs() < 1.0e-6);
            assert!((min_edge - 2.0).abs() < 1.0e-6);
        }
    }
}

#[test]
fn svg_dxf_arrow_triangle_parity_all_three_vertices() {
    for model in [golden_plate_model(), center_hole_model()] {
        let (drawing, _) = solve_and_draw(&model);
        let h = drawing.principal_sheet().unwrap().bounds().max_y;
        let svg = drawing_to_svg(&drawing).unwrap();
        let dxf = drawing_to_dxf(&drawing).unwrap();
        let eps = scc_drawing::WITNESS_EPSILON_MM;

        let svg_polys: Vec<Vec<(f64, f64)>> = svg_group_polygons(&svg, "dimensions")
            .into_iter()
            .filter(|(poly_id, _)| poly_id.starts_with("a_"))
            .map(|(_, verts)| verts)
            .collect();
        let mut solids = dxf_solids_on_layer(&dxf, LAYER_DIMENSION);
        assert_eq!(svg_polys.len(), solids.len(), "arrowhead count parity");

        let close =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < eps && (a.1 - b.1).abs() < eps;
        for verts in &svg_polys {
            let idx = solids
                .iter()
                .position(|solid| {
                    // Flip each SOLID corner through the mirror rule, then
                    // compare as UNORDERED 3-vertex sets (winding differs).
                    let flipped: Vec<(f64, f64)> =
                        solid.iter().map(|(x, y)| (*x, h - *y)).collect();
                    verts.iter().all(|v| flipped.iter().any(|f| close(*v, *f)))
                        && flipped.iter().all(|f| verts.iter().any(|v| close(*v, *f)))
                })
                .unwrap_or_else(|| panic!("no DXF SOLID mirrors SVG polygon {verts:?}"));
            solids.swap_remove(idx);
        }
        assert!(solids.is_empty(), "SVG↔DXF arrowhead bijection complete");
    }
}
