//! Phase 9.3 integration tests — the projection model end-to-end (ADR-034).
//!
//! Builder behavior, provenance, source integrity, determinism, and the
//! `validate_against` gate. The `SpatialModel` argument is constructed
//! directly via `serde_json` from its canonical serialization (all
//! `SpatialModel` fields are `pub`, matching the scc-spatial test style;
//! `Point2`/`FeatureId` are not re-exported by `scc-spatial`, so the fixture
//! is assembled through serde instead of struct literals).

use scc_projection::{
    PrincipalViewKind, ProjectedGeometry, ProjectedPoint2, ProjectionBounds2D,
    ProjectionDepthRange, ProjectionError, ProjectionModel, ProjectionModelBuilder,
    ProjectionPrimitiveId, ProjectionPrimitiveRole, SpatialElementRef,
};
use scc_spatial::SpatialCutId;
use serde_json::json;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// The standard fixture: plate Rectangle origin (−60, −40) 120×80, z ∈
/// [0, 10], ONE through-hole cut `a_hole:cut:0000` center (0, 0) r5, bounds
/// [−60, −40, 0] / [60, 40, 10].
const SPATIAL_FIXTURE: &str = r#"{
    "body": {
        "footprint": {
            "Rectangle": {
                "origin": { "x": -60.0, "y": -40.0 },
                "width": 120.0,
                "height": 80.0
            }
        },
        "z": { "min": 0.0, "max": 10.0 },
        "source_feature": "plate",
        "source_entity": "plate"
    },
    "cuts": {
        "a_hole:cut:0000": {
            "center": { "x": 0.0, "y": 0.0 },
            "radius": 5.0,
            "z": { "min": 0.0, "max": 10.0 },
            "kind": { "kind": "through_all" },
            "source_feature": "a_hole",
            "source_entity": "a_hole"
        }
    },
    "bounds": {
        "min_x": -60.0, "max_x": 60.0,
        "min_y": -40.0, "max_y": 40.0,
        "min_z": 0.0, "max_z": 10.0
    },
    "construction_circles": {},
    "source": { "geometry_source_root": 1, "geometry_source_step": 7 }
}"#;

/// A second cut for the coincident-primitives test (two distinct cut sources
/// in one spatial model).
const TWO_CUT_FIXTURE: &str = r#"{
    "body": {
        "footprint": {
            "Rectangle": {
                "origin": { "x": -60.0, "y": -40.0 },
                "width": 120.0,
                "height": 80.0
            }
        },
        "z": { "min": 0.0, "max": 10.0 },
        "source_feature": "plate",
        "source_entity": "plate"
    },
    "cuts": {
        "a_hole:cut:0000": {
            "center": { "x": 0.0, "y": 0.0 },
            "radius": 5.0,
            "z": { "min": 0.0, "max": 10.0 },
            "kind": { "kind": "through_all" },
            "source_feature": "a_hole",
            "source_entity": "a_hole"
        },
        "b_slot:cut:0000": {
            "center": { "x": 30.0, "y": 10.0 },
            "radius": 3.0,
            "z": { "min": 4.0, "max": 10.0 },
            "kind": { "kind": "blind", "start_face": "opposite" },
            "source_feature": "b_slot",
            "source_entity": "b_slot"
        }
    },
    "bounds": {
        "min_x": -60.0, "max_x": 60.0,
        "min_y": -40.0, "max_y": 40.0,
        "min_z": 0.0, "max_z": 10.0
    },
    "construction_circles": {},
    "source": { "geometry_source_root": 1, "geometry_source_step": 7 }
}"#;

/// A fixture whose cut id contains colons in unusual places — the opacity
/// probe of `spatial_cut_id_stays_opaque_through_projection`.
const WEIRD_ID_FIXTURE: &str = r#"{
    "body": {
        "footprint": {
            "Rectangle": {
                "origin": { "x": -60.0, "y": -40.0 },
                "width": 120.0,
                "height": 80.0
            }
        },
        "z": { "min": 0.0, "max": 10.0 },
        "source_feature": "plate",
        "source_entity": "plate"
    },
    "cuts": {
        "we:ird": {
            "center": { "x": 0.0, "y": 0.0 },
            "radius": 5.0,
            "z": { "min": 0.0, "max": 10.0 },
            "kind": { "kind": "through_all" },
            "source_feature": "weird",
            "source_entity": "weird"
        }
    },
    "bounds": {
        "min_x": -60.0, "max_x": 60.0,
        "min_y": -40.0, "max_y": 40.0,
        "min_z": 0.0, "max_z": 10.0
    },
    "construction_circles": {},
    "source": { "geometry_source_root": 1, "geometry_source_step": 7 }
}"#;

fn spatial() -> scc_spatial::SpatialModel {
    serde_json::from_str::<scc_spatial::SpatialModel>(SPATIAL_FIXTURE)
        .expect("spatial fixture must deserialize")
}

fn spatial_from(fixture: &str) -> scc_spatial::SpatialModel {
    serde_json::from_str::<scc_spatial::SpatialModel>(fixture)
        .expect("spatial fixture must deserialize")
}

fn cut_ref(id: &str) -> SpatialElementRef {
    SpatialElementRef::Cut {
        id: SpatialCutId(id.into()),
    }
}

fn segment(u1: f64, v1: f64, u2: f64, v2: f64) -> ProjectedGeometry {
    ProjectedGeometry::Segment {
        start: ProjectedPoint2 { u: u1, v: v1 },
        end: ProjectedPoint2 { u: u2, v: v2 },
    }
}

fn depth(min: f64, max: f64) -> ProjectionDepthRange {
    ProjectionDepthRange::new(min, max).unwrap()
}

fn add_full_body_segment(
    builder: &mut ProjectionModelBuilder,
    kind: PrincipalViewKind,
    u1: f64,
    v1: f64,
    u2: f64,
    v2: f64,
) {
    let id = builder.next_id(kind, &SpatialElementRef::Body);
    builder
        .add_primitive(
            kind,
            id,
            segment(u1, v1, u2, v2),
            ProjectionPrimitiveRole::BodyBoundary,
            depth(0.0, 10.0),
            SpatialElementRef::Body,
            &spatial(),
        )
        .unwrap();
}

fn builder_with_all_views() -> ProjectionModelBuilder {
    let mut builder = ProjectionModelBuilder::new();
    for kind in [
        PrincipalViewKind::Front,
        PrincipalViewKind::Top,
        PrincipalViewKind::Left,
    ] {
        builder.add_view(kind).unwrap();
    }
    builder
}

/// A representative model: body outline in all three views plus the cut's
/// near-face and far-face opening edges in Front and its circle in Top.
fn sample_model() -> (ProjectionModel, scc_spatial::SpatialModel) {
    let spatial = spatial();
    let mut builder = builder_with_all_views();
    // Body footprint rectangle: x ∈ [-60, 60], y ∈ [-40, 40], z ∈ [0, 10].
    // Front (u = x, v = y): the outline rectangle.
    for (u1, v1, u2, v2) in [
        (-60.0, -40.0, 60.0, -40.0),
        (60.0, -40.0, 60.0, 40.0),
        (60.0, 40.0, -60.0, 40.0),
        (-60.0, 40.0, -60.0, -40.0),
    ] {
        add_full_body_segment(&mut builder, PrincipalViewKind::Front, u1, v1, u2, v2);
    }
    // Top (u = x, v = −z): the outline rectangle.
    for (u1, v1, u2, v2) in [
        (-60.0, -10.0, 60.0, -10.0),
        (60.0, -10.0, 60.0, 0.0),
        (60.0, 0.0, -60.0, 0.0),
        (-60.0, 0.0, -60.0, -10.0),
    ] {
        add_full_body_segment(&mut builder, PrincipalViewKind::Top, u1, v1, u2, v2);
    }
    // Left (u = z, v = y): the outline rectangle.
    for (u1, v1, u2, v2) in [
        (0.0, -40.0, 10.0, -40.0),
        (10.0, -40.0, 10.0, 40.0),
        (10.0, 40.0, 0.0, 40.0),
        (0.0, 40.0, 0.0, -40.0),
    ] {
        add_full_body_segment(&mut builder, PrincipalViewKind::Left, u1, v1, u2, v2);
    }
    // The a_hole cut: a circle in Top (axis toward viewer) and its
    // silhouette edges in Front (near-face opening at z = 0 → depth 0, and
    // far-face opening at z = T → depth T).
    let cut = cut_ref("a_hole:cut:0000");
    builder
        .add_primitive(
            PrincipalViewKind::Top,
            builder.next_id(PrincipalViewKind::Top, &cut),
            ProjectedGeometry::Circle {
                center: ProjectedPoint2 { u: 0.0, v: 0.0 },
                radius: 5.0,
            },
            ProjectionPrimitiveRole::CutBoundary,
            depth(0.0, 0.0),
            cut.clone(),
            &spatial,
        )
        .unwrap();
    builder
        .add_primitive(
            PrincipalViewKind::Front,
            builder.next_id(PrincipalViewKind::Front, &cut),
            segment(-5.0, -40.0, -5.0, 40.0),
            ProjectionPrimitiveRole::CutBoundary,
            depth(0.0, 0.0),
            cut.clone(),
            &spatial,
        )
        .unwrap();
    builder
        .add_primitive(
            PrincipalViewKind::Front,
            builder.next_id(PrincipalViewKind::Front, &cut),
            segment(5.0, -40.0, 5.0, 40.0),
            ProjectionPrimitiveRole::CutBoundary,
            depth(10.0, 10.0),
            cut,
            &spatial,
        )
        .unwrap();
    (builder.finish().unwrap(), spatial)
}

// ---------------------------------------------------------------------------
// builder behavior
// ---------------------------------------------------------------------------

/// 24: `add_view` installs the frozen canonical frame for the kind.
#[test]
fn add_view_installs_canonical_frame() {
    let mut builder = ProjectionModelBuilder::new();
    builder.add_view(PrincipalViewKind::Front).unwrap();
    builder.add_view(PrincipalViewKind::Top).unwrap();
    builder.add_view(PrincipalViewKind::Left).unwrap();
    let model = builder.finish().unwrap();
    let front = &model.views[&PrincipalViewKind::Front];
    assert_eq!(front.frame, scc_projection::OrthographicFrame::front());
    assert_eq!(
        model.views[&PrincipalViewKind::Top].frame,
        scc_projection::OrthographicFrame::top()
    );
    assert_eq!(
        model.views[&PrincipalViewKind::Left].frame,
        scc_projection::OrthographicFrame::left()
    );
    for view in model.views.values() {
        assert_eq!(view.primitives.len(), 0);
        assert_eq!(
            view.bounds,
            ProjectionBounds2D {
                min_u: 0.0,
                max_u: 0.0,
                min_v: 0.0,
                max_v: 0.0
            },
            "an empty view stores the degenerate 0..0 envelope"
        );
    }
}

/// 25: `add_view` on an existing kind is a no-op success (idempotent).
#[test]
fn add_view_twice_is_noop() {
    let mut builder = ProjectionModelBuilder::new();
    builder.add_view(PrincipalViewKind::Front).unwrap();
    builder.add_view(PrincipalViewKind::Front).unwrap();
    builder.add_view(PrincipalViewKind::Front).unwrap();
    let model = builder.finish().unwrap();
    assert_eq!(
        model.views.len(),
        1,
        "duplicate add_view must not duplicate"
    );
}

/// 26: a staged primitive carries geometry, role, depth, and provenance.
#[test]
fn add_primitive_records_geometry_role_depth_source() {
    let (model, _spatial) = sample_model();
    let id = ProjectionPrimitiveId("front:body:p:0000".into());
    let primitive = &model.views[&PrincipalViewKind::Front].primitives[&id];
    assert_eq!(primitive.geometry, segment(-60.0, -40.0, 60.0, -40.0));
    assert_eq!(primitive.role, ProjectionPrimitiveRole::BodyBoundary);
    assert_eq!(primitive.depth, depth(0.0, 10.0));
    assert_eq!(
        model.element_for_primitive(&id),
        Some(&SpatialElementRef::Body)
    );
}

/// 27: a duplicate id in the same view fails closed.
#[test]
fn add_primitive_rejects_duplicate_id() {
    let mut builder = builder_with_all_views();
    let spatial = spatial();
    let first = builder.next_id(PrincipalViewKind::Front, &SpatialElementRef::Body);
    builder
        .add_primitive(
            PrincipalViewKind::Front,
            first.clone(),
            segment(0.0, 0.0, 1.0, 1.0),
            ProjectionPrimitiveRole::BodyBoundary,
            depth(0.0, 1.0),
            SpatialElementRef::Body,
            &spatial,
        )
        .unwrap();
    assert_eq!(
        builder.add_primitive(
            PrincipalViewKind::Front,
            first,
            segment(2.0, 2.0, 3.0, 3.0),
            ProjectionPrimitiveRole::BodyBoundary,
            depth(0.0, 1.0),
            SpatialElementRef::Body,
            &spatial,
        ),
        Err(ProjectionError::DuplicateProjectionPrimitive {
            id: ProjectionPrimitiveId("front:body:p:0000".into()),
            kind: PrincipalViewKind::Front,
        })
    );
}

/// 28: staging a primitive for a view that was never added fails closed —
/// and the view-first gate wins over later gates (deterministic order).
#[test]
fn add_primitive_requires_view() {
    let mut builder = ProjectionModelBuilder::new();
    let spatial = spatial();
    // Even an invalid geometry cannot outrank the missing-view gate.
    assert_eq!(
        builder.add_primitive(
            PrincipalViewKind::Left,
            ProjectionPrimitiveId("left:body:p:0000".into()),
            ProjectedGeometry::Segment {
                start: ProjectedPoint2 {
                    u: f64::NAN,
                    v: 0.0
                },
                end: ProjectedPoint2 { u: 1.0, v: 0.0 },
            },
            ProjectionPrimitiveRole::BodyBoundary,
            depth(0.0, 1.0),
            SpatialElementRef::Body,
            &spatial,
        ),
        Err(ProjectionError::MissingProjectionView {
            kind: PrincipalViewKind::Left,
        })
    );
    // After adding the view the same geometry fails with the geometry gate.
    builder.add_view(PrincipalViewKind::Left).unwrap();
    assert!(matches!(
        builder.add_primitive(
            PrincipalViewKind::Left,
            ProjectionPrimitiveId("left:body:p:0000".into()),
            ProjectedGeometry::Segment {
                start: ProjectedPoint2 {
                    u: f64::NAN,
                    v: 0.0
                },
                end: ProjectedPoint2 { u: 1.0, v: 0.0 },
            },
            ProjectionPrimitiveRole::BodyBoundary,
            depth(0.0, 1.0),
            SpatialElementRef::Body,
            &spatial,
        ),
        Err(ProjectionError::InvalidProjectionGeometry { .. })
    ));
}

/// 29: a `Cut` source absent from `spatial.cuts` fails closed at insert
/// time; `Body` is always accepted.
#[test]
fn add_primitive_rejects_unknown_cut_id() {
    let mut builder = builder_with_all_views();
    let spatial = spatial();
    assert_eq!(
        builder.add_primitive(
            PrincipalViewKind::Front,
            ProjectionPrimitiveId("front:ghost:cut:0000:p:0000".into()),
            segment(0.0, 0.0, 1.0, 1.0),
            ProjectionPrimitiveRole::CutBoundary,
            depth(0.0, 1.0),
            cut_ref("ghost:cut:0000"),
            &spatial,
        ),
        Err(ProjectionError::InvalidProjectionSource {
            id: ProjectionPrimitiveId("front:ghost:cut:0000:p:0000".into()),
        })
    );
}

/// 30: `finish` recomputes per-view bounds from the staged primitives
/// (segment endpoints and circle center ± radius); the stored value is the
/// contract.
#[test]
fn finish_computes_view_bounds_from_primitives() {
    let (model, _spatial) = sample_model();
    let front = &model.views[&PrincipalViewKind::Front];
    // Body outline (±60, ±40) plus the cut silhouette at u = ±25.
    assert_eq!(
        front.bounds,
        ProjectionBounds2D {
            min_u: -60.0,
            max_u: 60.0,
            min_v: -40.0,
            max_v: 40.0
        }
    );
    let top = &model.views[&PrincipalViewKind::Top];
    // Circle at (−20, 0) r=5 widens u to [−25, −15] and v to [−5, +5];
    // body v ∈ [−10, 0].
    assert_eq!(
        top.bounds,
        ProjectionBounds2D {
            min_u: -60.0,
            max_u: 60.0,
            min_v: -10.0,
            max_v: 5.0
        }
    );
    let left = &model.views[&PrincipalViewKind::Left];
    assert_eq!(
        left.bounds,
        ProjectionBounds2D {
            min_u: 0.0,
            max_u: 10.0,
            min_v: -40.0,
            max_v: 40.0
        }
    );
}

/// 31: primitives staged without their view are rejected — the insert-time
/// gate makes that state unrepresentable, so the insert-time rejection is
/// asserted here (per the frozen design: `finish` cannot see an unadded
/// view's primitive because `add_primitive` gates on the view FIRST).
#[test]
fn finish_rejects_primitives_without_view() {
    let mut builder = ProjectionModelBuilder::new();
    let spatial = spatial();
    // The gate is insert-time: staging into a view that was never added
    // fails closed, so `finish` can never receive such a primitive.
    assert_eq!(
        builder.add_primitive(
            PrincipalViewKind::Top,
            ProjectionPrimitiveId("top:body:p:0000".into()),
            segment(0.0, 0.0, 1.0, 1.0),
            ProjectionPrimitiveRole::BodyBoundary,
            depth(0.0, 1.0),
            SpatialElementRef::Body,
            &spatial,
        ),
        Err(ProjectionError::MissingProjectionView {
            kind: PrincipalViewKind::Top,
        })
    );
    let model = builder.finish().unwrap();
    assert!(
        !model.views.contains_key(&PrincipalViewKind::Top),
        "no Top view may exist after the failed insert"
    );

    // Deserialization never auto-validates: a tampered model must FAIL
    // `validate_integrity` (a non-canonical frame is not the frozen one).
    let (good_model, _spatial) = sample_model();
    let json = serde_json::to_string(&good_model).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["views"]["left"]["frame"]["toward_viewer"] = json!("pos_x");
    let tampered: ProjectionModel = serde_json::from_value(value).unwrap();
    assert!(matches!(
        tampered.validate_integrity(),
        Err(ProjectionError::InvalidProjectionGeometry { .. })
    ));

    // An orphan source-map entry (provenance for a primitive no view holds)
    // also fails the integrity gate.
    let json = serde_json::to_string(&good_model).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["sources"]["primitive_sources"]["ghost:body:p:9999"] = json!({"element": "body"});
    let orphaned: ProjectionModel = serde_json::from_value(value).unwrap();
    assert!(matches!(
        orphaned.validate_integrity(),
        Err(ProjectionError::InvalidProjectionSource { .. })
    ));
}

/// 32: an empty model (zero views + zero primitives) is legal.
#[test]
fn finish_accepts_empty_model() {
    let model = ProjectionModelBuilder::new().finish().unwrap();
    assert!(model.views.is_empty());
    assert_eq!(model.sources.iter().count(), 0);
    model.validate_integrity().unwrap();
}

// ---------------------------------------------------------------------------
// sources & validation
// ---------------------------------------------------------------------------

/// 33: a deserialized model passes `validate_integrity` (the explicit
/// post-deserialize gate; deserialization itself never auto-validates).
#[test]
fn deserialized_model_passes_validate_integrity() {
    let (model, spatial) = sample_model();
    let json = serde_json::to_string_pretty(&model).unwrap();
    let deserialized: ProjectionModel = serde_json::from_str(&json).unwrap();
    deserialized.validate_integrity().unwrap();
    deserialized.validate_against(&spatial).unwrap();
    assert_eq!(deserialized, model);
}

/// 34: `validate_against` rejects a cut missing from the spatial model —
/// the post-build / post-deserialize source-integrity gate.
#[test]
fn validate_against_rejects_cut_missing_from_spatial_model() {
    let (model, _spatial) = sample_model();
    model.validate_against(&spatial()).unwrap();
    // An empty spatial model has no cuts: every Cut reference must fail.
    let empty_fixture = r#"{
        "body": {
            "footprint": { "Rectangle": { "origin": { "x": 0.0, "y": 0.0 }, "width": 1.0, "height": 1.0 } },
            "z": { "min": 0.0, "max": 1.0 },
            "source_feature": "plate",
            "source_entity": "plate"
        },
        "cuts": {},
        "bounds": { "min_x": 0.0, "max_x": 1.0, "min_y": 0.0, "max_y": 1.0, "min_z": 0.0, "max_z": 1.0 },
        "construction_circles": {},
        "source": { "geometry_source_root": 0, "geometry_source_step": 0 }
    }"#;
    let empty: scc_spatial::SpatialModel = serde_json::from_str(empty_fixture).unwrap();
    let error = model.validate_against(&empty).unwrap_err();
    assert_eq!(error.code(), "SCC-PROJ-005");
    assert!(
        matches!(error, ProjectionError::InvalidProjectionSource { .. }),
        "the failing entry must be the cut reference"
    );
}

/// 35: the reverse query groups every primitive of one element per view.
#[test]
fn primitives_for_element_returns_all_views() {
    let (model, _spatial) = sample_model();
    let body = SpatialElementRef::Body;
    let grouped = model.primitives_for_element(&body);
    assert_eq!(grouped.len(), 3, "the body appears in all three views");
    for (kind, ids) in &grouped {
        assert_eq!(ids.len(), 4, "4 outline segments in view {kind:?}");
    }
    // Front order: Front < Top < Left (BTreeMap over the enum).
    let kinds: Vec<PrincipalViewKind> = grouped.keys().copied().collect();
    assert_eq!(
        kinds,
        vec![
            PrincipalViewKind::Front,
            PrincipalViewKind::Top,
            PrincipalViewKind::Left
        ]
    );
    let cut = cut_ref("a_hole:cut:0000");
    let grouped = model.primitives_for_element(&cut);
    assert_eq!(grouped.len(), 2, "the cut appears in Front and Top");
    assert_eq!(grouped[&PrincipalViewKind::Front].len(), 2);
    assert_eq!(grouped[&PrincipalViewKind::Top].len(), 1);
    // The source map agrees (no second index to drift).
    assert_eq!(
        model.sources.primitives_for_element(&cut),
        grouped
            .values()
            .flat_map(|ids| ids.iter().cloned())
            .collect()
    );
}

/// 36: a `SpatialCutId` survives into ids, provenance, and back through
/// serde untouched — never parsed, never re-derived. Lookups work purely by
/// map equality, shown with a cut id (`we:ird`) whose content would break
/// any id-splitting parse.
#[test]
fn spatial_cut_id_stays_opaque_through_projection() {
    let spatial = spatial_from(WEIRD_ID_FIXTURE);
    let cut = cut_ref("we:ird");
    let mut builder = ProjectionModelBuilder::new();
    builder.add_view(PrincipalViewKind::Top).unwrap();
    builder.add_view(PrincipalViewKind::Front).unwrap();
    let expected_top_id = "top:we:ird:p:0000";
    builder
        .add_primitive(
            PrincipalViewKind::Top,
            builder.next_id(PrincipalViewKind::Top, &cut),
            ProjectedGeometry::Circle {
                center: ProjectedPoint2 { u: 0.0, v: 0.0 },
                radius: 5.0,
            },
            ProjectionPrimitiveRole::CutBoundary,
            depth(0.0, 0.0),
            cut.clone(),
            &spatial,
        )
        .unwrap();
    builder
        .add_primitive(
            PrincipalViewKind::Front,
            builder.next_id(PrincipalViewKind::Front, &cut),
            segment(-5.0, -40.0, -5.0, 40.0),
            ProjectionPrimitiveRole::CutBoundary,
            depth(0.0, 0.0),
            cut.clone(),
            &spatial,
        )
        .unwrap();
    let model = builder.finish().unwrap();

    // The weird id is embedded verbatim (formatting INTO the id is fine;
    // the deterministic format keeps it opaque inside its own segment).
    assert_eq!(
        model
            .primitives_for_element(&cut)
            .values()
            .flat_map(|ids| ids.iter().map(|id| id.to_string()))
            .collect::<Vec<_>>(),
        vec![
            "front:we:ird:p:0000".to_string(),
            "top:we:ird:p:0000".to_string(),
        ],
        "ids embed the colon-laden id verbatim"
    );

    // The reverse chain resolves by exact map comparison, not by parsing:
    // element_for_primitive returns the SAME SpatialElementRef value, whose
    // opaque id keys spatial.cuts directly.
    let resolved = model
        .element_for_primitive(&ProjectionPrimitiveId(expected_top_id.into()))
        .expect("the weird cut must resolve");
    assert_eq!(resolved, &cut);
    let resolved_id = match resolved {
        SpatialElementRef::Cut { id } => id,
        other => panic!("unexpected non-cut element {other:?}"),
    };
    assert!(
        spatial.cuts.contains_key(resolved_id),
        "the resolved opaque id is a live key of spatial.cuts"
    );

    // And it survives serde untouched.
    let json = serde_json::to_string(&cut).unwrap();
    assert!(
        json.contains("we:ird"),
        "serialization must carry the id verbatim"
    );
    let deserialized: ProjectionModel =
        serde_json::from_str(&serde_json::to_string(&model).unwrap()).unwrap();
    assert_eq!(
        deserialized.element_for_primitive(&ProjectionPrimitiveId(expected_top_id.into())),
        Some(&cut)
    );
}

/// 64/65 analogue: the provenance chain stays one hop wide —
/// `element_for_primitive` resolves for every recorded primitive and
/// `primitives_for_element` × `element_for_primitive` agree (no second
/// truth).
#[test]
fn provenance_roundtrip_consistency() {
    let (model, _spatial) = sample_model();
    for (primitive, element) in model.sources.iter() {
        assert_eq!(
            model.element_for_primitive(primitive),
            Some(element),
            "primitive {primitive} must resolve to its recorded element"
        );
        assert!(
            model
                .primitives_for_element(element)
                .values()
                .any(|ids| ids.contains(primitive)),
            "primitive {primitive} must appear in its element's reverse query"
        );
    }
}

/// 64/65 analogue: roles split body vs cut primitives without any
/// visibility semantics; depths differ per opening edge (near face /
/// far face read directly off `d`).
#[test]
fn roles_and_depths_split_body_and_cut_evidence() {
    let (model, _spatial) = sample_model();
    let front = &model.views[&PrincipalViewKind::Front];
    let near = ProjectionPrimitiveId("front:a_hole:cut:0000:p:0000".into());
    let far = ProjectionPrimitiveId("front:a_hole:cut:0000:p:0001".into());
    assert_eq!(
        front.primitives[&near].role,
        ProjectionPrimitiveRole::CutBoundary
    );
    assert_eq!(
        front.primitives[&far].role,
        ProjectionPrimitiveRole::CutBoundary
    );
    assert_eq!(front.primitives[&near].depth, depth(0.0, 0.0));
    assert_eq!(front.primitives[&far].depth, depth(10.0, 10.0));
    for (id, primitive) in &front.primitives {
        let expected_role = if id.0.starts_with("front:body") {
            ProjectionPrimitiveRole::BodyBoundary
        } else {
            ProjectionPrimitiveRole::CutBoundary
        };
        assert_eq!(primitive.role, expected_role, "role of {id}");
    }
}

/// Coincident primitives are preserved: two primitives with identical
/// segment geometry but different sources/depths are both retained — no
/// dedup, no visibility semantics in 9.3.
#[test]
fn coincident_primitives_are_preserved() {
    let mut builder = builder_with_all_views();
    let spatial = spatial_from(TWO_CUT_FIXTURE);
    let geometry = segment(-5.0, 0.0, 5.0, 0.0);
    builder
        .add_primitive(
            PrincipalViewKind::Front,
            ProjectionPrimitiveId("front:a_hole:cut:0000:p:0000".into()),
            geometry.clone(),
            ProjectionPrimitiveRole::CutBoundary,
            depth(0.0, 0.0),
            cut_ref("a_hole:cut:0000"),
            &spatial,
        )
        .unwrap();
    builder
        .add_primitive(
            PrincipalViewKind::Front,
            ProjectionPrimitiveId("front:b_slot:cut:0000:p:0000".into()),
            geometry,
            ProjectionPrimitiveRole::CutBoundary,
            depth(4.0, 10.0),
            cut_ref("b_slot:cut:0000"),
            &spatial,
        )
        .unwrap();
    let model = builder.finish().unwrap();
    let front = &model.views[&PrincipalViewKind::Front];
    assert_eq!(front.primitives.len(), 2, "both coincident primitives stay");
    assert_eq!(model.sources.iter().count(), 2);
    let depths: Vec<_> = front
        .primitives
        .values()
        .map(|primitive| (primitive.depth.min, primitive.depth.max))
        .collect();
    assert_eq!(depths, vec![(0.0, 0.0), (4.0, 10.0)]);
}

// ---------------------------------------------------------------------------
// determinism
// ---------------------------------------------------------------------------

/// 37: rebuilding the model from scratch 100 times produces byte-identical
/// pretty JSON every run (ADR-034 Decision 21, mirroring ADR-033 Decision 9).
#[test]
fn projection_determinism_100_runs() {
    let (reference, _spatial) = sample_model();
    let reference_json = serde_json::to_string_pretty(&reference).unwrap();
    for run in 0..100 {
        let (model, _spatial) = sample_model();
        let json = serde_json::to_string_pretty(&model).unwrap();
        assert_eq!(
            json, reference_json,
            "run {run} must be byte-identical to run 0"
        );
        let deserialized: ProjectionModel = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_string_pretty(&deserialized).unwrap(),
            reference_json,
            "run {run} must survive a serde roundtrip byte-identically"
        );
    }
}

/// The builder's `next_id` allocation is deterministic and gapless across
/// interleaved (view, source) streams.
#[test]
fn next_id_streams_are_deterministic_and_gapless() {
    let mut builder = builder_with_all_views();
    let spatial = spatial();
    let body = SpatialElementRef::Body;
    let cut = cut_ref("a_hole:cut:0000");
    // Interleave: body, cut, body, cut in Front.
    let expected = [
        (PrincipalViewKind::Front, &body, "front:body:p:0000"),
        (
            PrincipalViewKind::Front,
            &cut,
            "front:a_hole:cut:0000:p:0000",
        ),
        (PrincipalViewKind::Front, &body, "front:body:p:0001"),
        (
            PrincipalViewKind::Front,
            &cut,
            "front:a_hole:cut:0000:p:0001",
        ),
        (PrincipalViewKind::Top, &body, "top:body:p:0000"),
    ];
    for (kind, source, expected_id) in expected {
        assert_eq!(builder.next_id(kind, source).0, expected_id);
        let id = builder.next_id(kind, source);
        builder
            .add_primitive(
                kind,
                id,
                segment(0.0, 0.0, 1.0, 1.0),
                ProjectionPrimitiveRole::BodyBoundary,
                depth(0.0, 1.0),
                source.clone(),
                &spatial,
            )
            .unwrap();
    }
}

/// The finished model exposes no accidental view for an unadded kind, and
/// the BTreeMap keeps views in canonical order after serde roundtrips.
#[test]
fn views_map_keeps_canonical_order() {
    let (model, _spatial) = sample_model();
    let kinds: Vec<PrincipalViewKind> = model.views.keys().copied().collect();
    assert_eq!(
        kinds,
        vec![
            PrincipalViewKind::Front,
            PrincipalViewKind::Top,
            PrincipalViewKind::Left
        ]
    );
    let deserialized: ProjectionModel =
        serde_json::from_str(&serde_json::to_string(&model).unwrap()).unwrap();
    let kinds: Vec<PrincipalViewKind> = deserialized.views.keys().copied().collect();
    assert_eq!(
        kinds,
        vec![
            PrincipalViewKind::Front,
            PrincipalViewKind::Top,
            PrincipalViewKind::Left
        ]
    );
}

/// Every staged primitive is counted exactly once by the source map
/// (insert-time record + integrity coverage).
#[test]
fn source_map_covers_every_primitive() {
    let (model, _spatial) = sample_model();
    let total_primitives: usize = model.views.values().map(|view| view.primitives.len()).sum();
    assert_eq!(model.sources.iter().count(), total_primitives);
    assert!(model.validate_integrity().is_ok());
}

/// Regression: view bounds accumulate from the FIRST primitive, not from a
/// fixed `{0,0,0,0}` seed — a view whose true envelope lies entirely on the
/// negative side of an axis must not be widened to 0.0 by the seed
/// (`finish` used the scc-drawing `Option`-accumulator pattern for this).
#[test]
fn finish_bounds_exact_for_envelope_off_the_origin() {
    let spatial = spatial();
    let mut builder = ProjectionModelBuilder::new();
    builder.add_view(PrincipalViewKind::Front).unwrap();
    builder
        .add_primitive(
            PrincipalViewKind::Front,
            ProjectionPrimitiveId("front:body:p:0000".into()),
            ProjectedGeometry::Segment {
                start: ProjectedPoint2 {
                    u: -120.0,
                    v: -80.0,
                },
                end: ProjectedPoint2 { u: -60.0, v: 0.0 },
            },
            ProjectionPrimitiveRole::BodyBoundary,
            depth(0.0, 10.0),
            SpatialElementRef::Body,
            &spatial,
        )
        .unwrap();
    let model = builder.finish().unwrap();
    assert_eq!(
        model.views[&PrincipalViewKind::Front].bounds,
        ProjectionBounds2D {
            min_u: -120.0,
            max_u: -60.0,
            min_v: -80.0,
            max_v: 0.0,
        },
        "bounds must be the exact primitive envelope, never seed-widened"
    );
}
