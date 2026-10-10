# Phase 8 — Engineering Drawing Engine: Implementation Specification

- Status: Proposal (investigation & design only — NO code in this round)
- Date: 2026-08-23
- Related: ADR-015..ADR-021, `docs/specs/aicad-engineering-dsl-v1.md`
- Evidence basis: current source code (authoritative) > ADR > specs. Every claim below anchors to real files/line numbers verified during audit.

---

## Phase 8 Readiness

**READY WITH BLOCKERS.**

- **READY** for a 2-D, derivation-driven DrawingModel + Dimension Engine + Renderer over the *solved GeometryModel*. The dependency chain and source map already exist: `EngineeringModel → GeometryModel` with `EngineeringSourceMap` carrying `FeatureId → EntityId` + inverse `EntityId → FeatureId`, plus a `DrawingState` vocabulary in the model (`Scale`, `ProjectionMethod|FirstAngle|ThirdAngle`, `ShowCenterlines`, `ShowConstructionGeometry`).
- **BLOCKED** for strict Front/Top/Left three-views: the model and solved geometry contain **no thickness, extrusion direction, hole depth/direction, through/blind, or 3-D orientation** anywhere in the Rust engineering pipe. Three-views require an explicit 2.5-D spatial-semantics phase (Phase 9) — see [Three-view Readiness](#three-view-readiness) and [Spatial Semantics Gap](#spatial-semantics-gap).

Do **not** attempt three-views inside DrawingModel. Treat "Drawing Engine" (Phase 8) and "Projection Engine" (Phase 9) as two independently acceptable stages.

---

## Current Pipeline

Current working pipeline (2-D engineering), verified end-to-end:

```
EngineeringModel (scc-engineering)
   │  lower_engineering_model()                lowering.rs:148
   ▼
Proposal (scc-proposal schema)                 lowering.rs:232
   │  analyze_detailed → try_lower
   ▼
SemanticIr → ConstraintIr (scc-core)
   │  execute
   ▼
GraphState (scc-execution)
   │  DeterministicGeometrySolver.solve
   ▼
GeometryModel (scc-geometry)                   model.rs:83  {entities: BTreeMap<EntityId, GeometryEntity>}
   │  export_svg / export_dxf                  lib.rs:29/35  (SVG + ASCII DXF AC1027)
   ▼
model.svg / model.dxf
```

- `lower_and_solve` (`lowering.rs:160`) returns `(EngineeringLoweringResult{proposal, source_map}, GeometryModel)`.
- `EngineeringSourceMap` (`source_map.rs:16`) = `feature_entities: FeatureId→BTreeSet<EntityId>` + `constraint_ids: EngineeringConstraintId→BTreeSet<ConstraintId>` + `pattern_arrays: FeatureId→String`; inverse `entity_to_feature()` (source_map.rs:88).
- The CLI engineering bridge (`scc-cli/src/engineering.rs`) already packages `{model, proposal, geometry, workbench, source_map, entity_to_feature}` in `solve_payload` (engineering.rs:31).
- **DrawingState is currently ignored by lowering** (`lowering.rs:226-228`, ADR-017 Decision 8). The 2-D preview is a naive client-side SVG over `geometry.entities` (`apps/scc-ui/public/js/engineering-workbench.js:124-167`) rendering only `circle`+`rectangle`, with no construction/centerline/dimension/scale handling.
- The **3-D FeatureGraph path is entirely separate**: it uses a Python CadQuery/OCCT `feature_graph_v1` contract (`apps/scc-ui/cad/aicad_cad/feature_graph.py`), does NOT flow through `Proposal`, and carries spatial info (box `size:{x,y,z}`, cylinder `length`+`axis`+`origin`) that the Rust 2-D model does not. It is out of scope for Phase 8.

---

## Reusable Existing Components

| Layer | File / Type / Function | What it gives Phase 8 |
|---|---|---|
| Solved geometry | `scc-geometry/src/model.rs` — `GeometryModel{entities:BTreeMap<EntityId,GeometryEntity>}`, `GeometryEntity{id, primitive}`, `GeometryPrimitive` (Point/Line/Circle/Arc/Rectangle/Polygon), `Point2{x,y}` | Exact 2-D positions/radii — the drawing's geometric truth |
| Source map | `scc-engineering/src/source_map.rs` — `EngineeringSourceMap`, `entities_for`, `entity_to_feature` | `FeatureId→EntityId` and inverse — selection & construction-geometry classification |
| Drawing vocabulary | `scc-engineering/src/feature.rs:299-327` — `DrawingProperty` (Scale, ProjectionMethod, ShowCenterlines, ShowConstructionGeometry), `DrawingPropertyKind`, `ProjectionMethod{FirstAngle,ThirdAngle}`; `model.rs:84` `DrawingState` + `set/get` | The model-level drawing intent already in the authoritative state |
| Parameters (dimension source) | `scc-engineering/src/model.rs` — `Parameter{value,origin,authority}`, `evaluate_parameter`, `ParameterDependencyGraph` | Drive dimension numbers by `ParameterId` — never text |
| Feature binding | `scc-engineering/src/binding.rs` — `FeatureParameterSlot` (16 typed slots), `feature_parameters` | Tool/command-level way to attach parameters to features for dimensions |
| SVG writer | `scc-geometry/src/svg.rs` — `render_svg`, `write_svg_entity`, `number()`, `escape_xml()`, `Bounds` | Reusable primitive emission + XML number/escape helpers |
| DXF writer | `scc-geometry/src/dxf.rs` — `render_dxf`, `write_dxf_entity`, `point_groups`, `write_polyline`, `group` | Reusable primitive emission + group-code writer (AC1027) |
| Newtypes | `scc-engineering/src/lib.rs:47-55` — `FeatureId/ParameterId/EngineeringConstraintId(String)` | Pattern for new strong-typed drawing IDs |
| CLI bridge | `scc-cli/src/engineering.rs` — `solve_payload`, `dsl_solve`, `tools_apply`, `workbench`, `context`; `apps/scc-ui/lib/scc-cli.mjs` `SccCliAdapter.#withWorkspace` | Where a `drawing` key and `engineering drawing` subcommand plug in |
| Screenshot surface | `apps/scc-ui/public/js/engineering-workbench.js` — `renderCenter`/`renderPrimitive` | The preview to be upgraded from "geometry camera" to real drawing (later) |

---

## Missing Capabilities

1. **No drawing model / drawing primitive type anywhere.** `GeometryPrimitive` is pure geometry; there is no Text variant, no dimension, no centerline, no view/sheet, no construction flag (`model.rs:48`, `svg.rs`/`dxf.rs` grep: nothing).
2. **No Dimension / Annotation representation** — nothing renders dimensions; values are not associated to `ParameterId`.
3. **No Centerline / CenterMark** and **no dashed linetype** — SVG hardcodes one `<g fill=none stroke=black>` (svg.rs:62); DXF hardcodes layer `0` (dxf.rs:81); no per-entity style.
4. **Construction vs solid is lost at export**: `ConstructionCircle` lowers via `lower_circle` (lowering.rs:285) into a plain `Circle`, and both exporters emit it identically to a material circle. There is no `construction` flag on `GeometryEntity` (model.rs:77).
5. **No Sheet/View/Layout/Scale** — exporters emit a single flat viewBox / ENTITIES section at model-unit scale; `scale` prop exists in the model but is never consumed.
6. **2-D workbench preview only renders circle+rectangle**; arcs/lines have no SVG branch (workbench.js:143-167).
7. **No EngineeringModel→drawing export path** — `scc export` consumes a solved *GeometryModel* JSON only; there is no `engineering drawing` command.
8. **No bounding-box / extents on GeometryModel** (derivable but not stored).

---

## DrawingModel Proposal

### Layering decision (core question 3)

**Recommend: the linear chain.**

```
EngineeringModel          (authoritative engineering truth — only mutable state)
   │  lower_engineering_model / lower_and_solve        [existing]
   ▼
GeometryModel + EngineeringSourceMap                  [existing]
   │  drawing derivation (pure function)
   ▼
DrawingModel                (derived, read-only — recomputed, never stored as truth)
   │
   ▼
Renderer (Drawing→SVG | Drawing→DXF)
```

**Why this over the fork (`EngineeringModel → GeometryModel` + `DrawingModel` as siblings):**

- A drawing is a *projection/presentation of solved geometry*. Every placed line, arc, dimension endpoint, and centerline is derived from a coordinate in `GeometryModel` or a parameter in `EngineeringModel`. If `DrawingModel` were a sibling of `GeometryModel`, its positions would have to be re-derived from the model independently — either duplicating geometry derivation or inventing a second coordinate source. That is exactly the "second state" the user forbids.
- `GeometryModel` is already derived and already the single solved-geometry truth for the whole 2-D path. Chaining `DrawingModel` below it keeps **one** geometric truth and makes `DrawingModel` provably a pure function of `(GeometryModel, EngineeringSourceMap, EngineeringModel.parameters, DrawingState)`.
- The fork is only justified if a drawing could exist **without** solving geometry. It cannot for a mechanical part drawing, so the chain wins.

`DrawingModel` reads (never writes) all four inputs. Its construction is a pure function; it is rebuilt on every draw and never persisted as authoritative state (see Data Ownership).

### Minimal first-version type set

Do **not** adopt all 17 suggested types. Minimal coherent set that the audit supports:

- **`DrawingSheet`** — one page: `id`, `DrawingScale`, `views: Vec<DrawingViewId in layout order>`, `title` (from part name; optional), portrait placeholder geometry. (No title-block/units field yet — see note.)
- **`DrawingView`** — one 2-D orthographic view: `id`, `projection: ProjectionMethod`, `origin_on_sheet`, `scale`, `primitives: Vec<DrawingPrimitiveId>`, `dimensions: Vec<DimensionId>`, `centerlines`, plus a derived `bounds`. For Phase 8 only the principal (Front) 2-D view is realistic; it still gets a `DrawingView` so multi-view can extend without rework.
- **`DrawingPrimitive`** — tagged enum; minimal for MVP: `Line{start,end,style}`, `Circle{center,radius,style,diameter_derived}`, `Arc{center,radius,start,end,style}`, `Text{position,string,height,anchor}`. (Polyline is a render convenience — can be added in the Renderer phase as a group of Lines; do not force it into the model yet.)
- **`LineStyle`** — minimal: `Solid | Dashed (for hidden/construction) | DashDot (for centerlines)`. **Do not** build a full line table.
- **`Centerline`** (explicit, separate from hidden lines) + **`CenterMark`** (cross at a circle center). Needed because neighbors must be able to toggle centerlines on/off via `ShowCenterlines` and construction geometry via `ShowConstructionGeometry`.
- **`ConstructionGeometry`** — a grouping/flag carried on primitives (or a `construction: bool` on `DrawingPrimitive`) that selects `Dashed` style and respects the toggle. Since the model already has `ShowConstructionGeometry`, this is a real requirement, not an abstraction.
- **`DrawingDimension`** + `DimensionValue` (see Dimension Architecture) — `Linear|Diameter|Radius|Pattern` MVP.
- **`Annotation`** — deferred unless a leader/text requirement is concrete; MVP only needs dimension text, so keep it inline in `DrawingDimension` and skip a separate `Annotation` type in round 1.
- **`DrawingScale`** — a simple `f64` wrapper already implied by `DrawingProperty::Scale`; expose as `scale` on `DrawingSheet`/`DrawingView`.

Deferred entirely: `DrawingPolyline` (renderer group), `DrawingLayout` as a stored type (becomes builder logic, not state), free `Annotation`, angular/coordinate dimensions.

Note: the model has **no material, no title, no sheet name, no units field** (verified). Units are implicitly mm. If a drawing title block is required, add `title`/`units` to `PartDefinition` or `DrawingState` in a later phase — do **not** invent them inside `DrawingModel` as second-source data.

### Rendering entry point

`DrawingBuilder::build(sheet: DrawingSheetId, ctx: DrawingContext) -> Result<DrawingModel, DrawingError>` where `DrawingContext` = `(&GeometryModel, &EngineeringSourceMap, &resolved parameters, &DrawingState)`. Pure, deterministic, `BTreeMap`-backed.

---

## Data Ownership

| Data | Owner / Is it authoritative? | Mutated by |
|---|---|---|
| Part, features, parameters, constraints, bindings, `DrawingState` | **`EngineeringModel` — the only authoritative engineering state** | Only `CommandExecutor` via `EngineeringCommand` (DSL / tools / intent) |
| Solved 2-D geometry | `GeometryModel` — derived, read-only | Only the solve pipeline (consumes GraphState, never mutates it) |
| Feature↔Entity linkage | `EngineeringSourceMap` — derived at lowering, read-only | Only lowering |
| **`DrawingModel`** | **Derived, read-only, ephemeral** | **Never mutated; recomputed per draw; not persisted as truth** |
| Renderer output | SVG/DXF strings — derived artifacts | Generated on demand; never fed back |

**Invariant:** `DrawingModel` is NOT a second engineering state. It has no command surface, no write path, no persistence as authoritative data. Rendering a drawing never changes the part. This mirrors ADR-016 Decision 1 (single authoritative engineering state) and ADR-021 Decision 6 (view is derived, no write path back). `DrawingState` stays the only place drawing *intent* lives, and it remains authoritative on the model.

---

## Source Mapping Plan

Extend the existing strong-typed map philosophy — **never parse id strings** (per ADR-017 Decision 7 / ADR-021 Decision 7). Current and proposed:

```
Existing:
  FeatureId ──EngineeringSourceMap──▶ BTreeSet<EntityId>   (feature_entities)
  EntityId  ───────────▶ FeatureId                          (entity_to_feature inverse)

Proposed drawing chain (new strong-typed maps in the drawing crate):
  FeatureId ─▶ Geometry EntityId ─▶ DrawingPrimitiveId ─▶ DrawingViewId ─▶ DrawingSheetId
        (source_map)        (drawing derivation      (drawing model       (sheet owns views)
                             records, in the SAME    owns primitive→view)
                             map pattern, no prefix)
```

Concretely:
- New newtype IDs: `DrawingId`, `DrawingPrimitiveId`, `DrawingViewId`, `DrawingSheetId` (single-field String wrappers, mirroring `FeatureId`/`EntityId`). Deterministic naming for readability only; maps are the only mechanism.
- `DrawingModel` carries a map `primitive_source: BTreeMap<DrawingPrimitiveId, EntityId>` (and, for dimensions, `dimension_parameter: BTreeMap<DimensionId, ParameterId>`). Because every drawing primitive is derived from a solved `GeometryEntity`, the drawing builder records the originating `EntityId`; combined with `entity_to_feature` this yields `DrawingPrimitiveId → EntityId → FeatureId` with zero string parsing.
- Keep `EntityId` (not a new geometry key) as the anchor between the two layers — `GeometryModel.entities` is already keyed by `EntityId`, so the drawing maps onto it directly.
- The pattern-array case already produces element ids `{array_id}[{index}]` (lowering.rs:187); drawing primitives derived from array elements record those `EntityId`s and thus resolve back to the single pattern `FeatureId` (ADR-021 Decision 8).

This replaces the current crude `EngineeringSourceMap` gap (no `feature_to_entity` named method) with explicit forward + inverse maps at each hop.

---

## Dimension Architecture

**Authoritative dimensions are `ParameterRef`s, never stored text.** (Forbidden: `Text("Ø64")`.)

```rust
enum DimensionValue {                  // never the stored string; resolved on draw
    ParameterRef { parameter: ParameterId },                  // single param
    DerivedExpression (expression over ParameterId),          // e.g. PCD = d1 + offset
    Literal { mm: f64 },                                      // only for dimensionless annotation/emergency
}
struct DrawingDimension {
    id: DrawingId,
    view: DrawingViewId,
    kind: DrawingDimensionKind,
    source_parameter: Option<ParameterId>,
    value: DimensionValue,              // REQUIRED: never a pre-rendered string
    measurement: (EntityId or coord pair),  // geometric basis so layout knows where to draw
}
enum DrawingDimensionKind { Linear, Horizontal, Vertical, Diameter, Radius, Pattern }
```

- When `center_hole.diameter` goes `64 → 72`, the dimension resolves through the parameter dependency graph (`evaluate_parameter`) at render time → `Ø64 → Ø72` automatically. No stored string, no refresh choreography.
- Reuse the existing "strong-typed slot, never infer from a NAMED id" discipline: the builder binds dimensions to features/entities via the source map, and to numbers via `ParameterId`.
- **MVP dimension kinds (earned by actual model vocabulary):** `Linear` (= Horizontal/Vertical collapsed with an orientation), `Diameter` (circles + hole patterns), `Radius`, `Pattern` (PCD + count, since a `HolePattern` is a first-class feature).
- **Defer:** `Angular` (no model support for angle measurement today), `Coordinate` (origin-ladder), separate `Horizontal`/`Vertical` disambiguation — fold orientation into `Linear` for MVP.
- `ResolvedValue` already yields typed `Scalar/Integer/…` (model.rs, parameter.rs:17); format mm per part; keep numeric formatting deterministic (reuse a trimmed-number formatter analogous to `svg::number`).

---

## Construction Geometry Strategy

Resolve **at the geometry/drawing boundary**, never inside the renderers:

1. `lowering.rs` **already knows** which features are construction: `ConstructionCircle` → `lower_circle` (lowering.rs:285). That is the correct choke point.
2. Add a per-solved-entity flag so construction survives to the drawing layer without the renderers having to guess. Two viable placements — pick **one**:
   - **(A) `GeometryEntity.construction: bool`** (additive, default false) in `scc-geometry/model.rs`, set by lowering based on feature type. Cleanest: the drawing builder reads it directly. Requires an additive field on the solved geometry type (touches GeometryModel serde — a small, deliberate change; golden fixtures unaffected for existing exports since default false).
   - **(B) derive in the drawing builder** from `EngineeringSourceMap`: for each primitive, `entity_to_feature(entity) → feature.kind == ConstructionCircle → style=Dashed`. Zero geometry-crate change, but re-derives the classification per draw.
   **Recommend (A)** with the builder also honoring (B) for robustness/fail-closed (if a construction feature somehow loses the flag, treat it as construction anyway via the source map). Either way the exporters (svg.rs/dxf.rs) stay **ignorant** of construction — they just render whatever `LineStyle` the DrawingModel already resolved, and `ShowConstructionGeometry` controls inclusion at the DrawingModel build step, not at render.
3. **MVP rule:** `ShowConstructionGeometry=false` (default false unless set) drops construction primitives entirely; `true` renders them dashed. This is the model's existing `DrawingProperty` respected for the first time.
4. Deferred: true *construction geometry* beyond circles (sketch lines, datum planes, projected guides) — those need Phase 9 spatial semantics, not a drawing flag.

---

## Drawing Layout Strategy

**No LLM auto-layout in Phase 8 (explicitly forbidden by the brief).** Deterministic rules only. Split:

**Phase 8 MVP (deterministic, simple):**
- **View placement:** fixed sheet origin → first view at a constant margin; **not** auto-nesting. (In practice Phase 8 MVP has a single principal view, so this is trivial.)
- **Dimension offset:** a fixed `DIMENSION_OFFSET` (e.g. 10 mm) from the measured primitive's bounding box, outboard of the part.
- **Dimension stacking:** successive dimensions on the same edge advance by a fixed `DIMENSION_STACK_PITCH` (e.g. 8 mm) along the normal — no packing algorithm.
- **Text:** single-line, fixed height, center-anchored at the dimension midpoint; text does not collide-manage against the part.
- **Centerline extension:** extend hole/arc centerlines by a fixed `CENTERLINE_OVERHANG` past the primitive.
- **No collision detection** beyond the fixed offsets — acceptable for MVP because placement is outboard with large fixed margins.

**Later optimization (deferred, will not block):**
- Text bounding-box overlap resolution, true collision detection, leader-line auto-routing, dimension clustering/compaction, annotation spacing, view auto-arrangement, section/detail-view placement. These are the "deterministic geometry layout" follow-ups — give them their own slice, not the MVP.

Because MVP layout is position-only math over `Point2`, it stays deterministic and testable (golden SVG/DXF anchors).

---

## Renderer Strategy

- **`DrawingModel` knows nothing about SVG/DXF/PDF.** It carries typed primitives + resolved styles + text; renderers translate. No `(<path>…)` or DXF group codes in the model. This preserves the boundary the brief requires.
- **Reuse, don't fork, geometry primitive emission:**
  - Reuse `svg.rs::number` / `escape_xml`, and (where the shape matches `GeometryPrimitive`) the same per-primitive SVG/DXF emission *as a low-level helper*, but wrap them so the Drawing renderers are **new** entry paths: `drawing_to_svg(&DrawingModel) -> String`, `drawing_to_dxf(&DrawingModel) -> String`.
  - **Avoid two divergent SVG logics.** The existing `svg.rs`/`dxf.rs` operate on `GeometryModel` (flat, no style, layer 0). The new drawing renderers operate on `DrawingModel` (views, styles, text, dimensions). Do **not** bolt drawing features onto `svg.rs`/`dxf.rs` (which would couple geometry export to drawing), and do **not** copy them wholesale. Instead: extract the *mechanical* writers (number formatting, XML/DXF group emission, primitive geometry paths) into reusable helpers, and add drawing-specific writers (Text/DIMENSION/centerline dashed styles, per-view layers) on top.
- **PDF:** out of scope for Phase 8. The existing `dxf`→(external pdftotext) and UI PDF paths are unrelated to a drawing PDF renderer; a `Drawing→PDF` renderer is a later additive target behind the same `DrawingModel` interface.
- **Two renderers, one model:** `Drawing→SVG` (browser preview + file) and `Drawing→DXF` (CAD interop) share 100% of `DrawingModel`.

---

## Three-view Readiness

**Current reality: strict Front/Top/Left three-views are NOT possible from today's 2-D EngineeringModel + GeometryModel.**

The audit (informed by full-crate greps) confirms the following — do **not** assume 2-D shapes yield three-views:

| Requirement | Status (evidence) |
|---|---|
| thickness / z-extent of a part | **Missing.** `BaseProfile{width,height}` only (feature.rs:23-28). No `thickness`/`z` anywhere. |
| extrusion direction / facing / plane | **Missing.** No axis/plane/normal in the Rust engineering pipe. `ProposalRelation.axis`/`ProposalArray.direction` are `None` hardcoded (lowering.rs). |
| hole direction / which face a hole is on | **Missing.** `HoleDefinition{diameter}` only (feature.rs:128-130); `Hole{id,diameter}`; no direction. |
| through vs blind / hole depth | **Missing.** No depth field. Only the *separate 3-D* CadDocument carries `length`+`axis` (feature_graph.py) — and that lives in a different (Python/OCCT) world with no bridge back. |
| feature depth / slot depth / fillet radius in 3-D | **Missing** in 2-D model; slots/fillets don't even lower. |
| visible / hidden line semantics | **Missing** entirely. No hidden-line engine, no style for it. |
| a Z-up world / coordinate-frame convention shared by all views | **Missing** in the Rust engineering layer. The 3-D gearbox COORDS (natural-language-3d-cad.md:81) apply only to that layer. |

**Classification (per brief):**
- **A. Already available:** 2-D plan-view geometry (outline, circles, arcs, positions), per-entity feature provenance (source map), drawing intent (scale/projection/centerlines on-off), parameter-driven dimension values.
- **B. Deterministically derivable today:** the **single orthographic Front view** of the 2-D profile (width×height and every hole position/radius are exactly known); centerlines & center marks; parameter-driven dimensions; construction-vs-solid differentiation.
- **C. Currently missing:** part thickness, hole depth/direction, through-vs-blind, extrusion axis, facing, hidden-line geometry, a unified 2.5-D coordinate semantics shared across views.
- **D. Phase 8 must add:** nothing 3-D — the DrawingModel is deliberately 2-D. (Optionally, later, `title`/`units` on the model for a title block.)
- **E. Defer / Phase 9:** a minimal 2.5-D `EngineeringSpatialSemantics` + `Projection Engine` + `HiddenLine` + `ProjectionConsistencyValidator`.

**Conclusion:** Phase 8 delivers a correct single-view (Front/plan) engineering drawing. Strict three-views are a **Phase 9** concern and require the spatial semantics below. Never let the LLM "fill in" coordinates for missing views.

---

## Spatial Semantics Gap

The blocker for three-views. If strict Front/Top/Left are a hard requirement, Phase 9 must introduce a **minimal 2.5-D semantics — NOT full 3-D B-Rep CAD**:

```
EngineeringModel
   │
   ▼
EngineeringSpatialSemantics      (new, thin, derived/part of lowering)
   │  per-feature: height(z extent), hole depth, hole direction/axis (x|y|z),
   │               through | blind, facing plane; part-level thickness
   ▼
Projection Engine                (pure math: project 2.5-D model onto Front/Top/Left)
   │
   ▼
Front / Top / Left  (three DrawingViews, one authoritative source — NOT 3 independent states)
```

- Keep it **2.5-D**: an explicit small set of fields, not a B-Rep kernel. Resist pulling the Python/OCCT CadDocument semantics into Rust in Phase 8.
- `ProjectionConsistencyValidator` must check across views and **fail closed**:
  `front.width == top.width; front.height == left.height; top.depth == left.depth`; per-hole positional agreement across views; pattern-count agreement; centerline/axis agreement; hidden-line agreement; source-feature agreement. Any mismatch → structured error (`SCC-DRW-*`), never a "close enough" drawing.
- Only **then** do three-views become valid. Until `EngineeringSpatialSemantics` exists, cap Phase 8 at a single principal view.

---

## Recommended Phase Split

**Split "Drawing Engine" (Phase 8) and "Projection Engine / three-views" (Phase 9) into two independently shippable stages** — the architecture demands it; do not contort Phase 8 to hold three-views.

### Phase 8.1 — DrawingModel
- **Goal:** define the derived, read-only `scc-drawing` crate: `DrawingSheet`, `DrawingView`, `DrawingPrimitive` (Line/Circle/Arc/Text), `LineStyle`, `DrawingId`/`DrawingViewId` newtypes, `DrawingBuilder`.
- **Scope:** types + builder that consumes `(GeometryModel, EngineeringSourceMap, resolved params, DrawingState)` and emits a deterministic `DrawingModel` with a single principal view; primitive→Entity source map inside the drawing.
- **Non-goals:** no renderers yet, no dimensions, no centerlines, no 3-D.
- **Inputs:** solved `GeometryModel` + `EngineeringSourceMap`.
- **Outputs:** `DrawingModel` value type.
- **Tests:** determinism (N-run identical); pure-function property (same inputs → same output); every primitive carries a resolvable `EntityId→FeatureId` back-link; view bounds correct for the mounting-plate fixture.
- **Acceptance:** `build()` is a pure fn; `DrawingModel` has no mutation API; golden serialization stable.

### Phase 8.2 — Drawing Primitive + Source Mapping
- **Goal:** complete the `DrawingPrimitive` set (add Arc, robust Line) and the full `primitive→Entity→Feature` map; add `feature_to_entity` forward inverse to `EngineeringSourceMap`.
- **Scope:** map plumbing, provenance integrity test (round-trip every solved entity through the drawing and back).
- **Non-goals:** renderers, dimensions.
- **Inputs/Outputs/Tests:** map round-trip; no string-prefix recovery anywhere.
- **Acceptance:** every drawing primitive provably maps to exactly one originating solved entity/feature without parsing id strings.

### Phase 8.3 — Drawing View / Sheet Space
- **Goal:** multi-view-ready `DrawingSheet`+`DrawingView` layout with scale placement; single principal view fully placed.
- **Scope:** sheet/view coordinates, `DrawingScale` application, bounds, deterministic placement.
- **Non-goals:** layout optimization, three-view projection.
- **Tests:** placing the plan view at expected offsets; scale math exact.
- **Acceptance:** a 120×80 plate at scale 1 maps to exact sheet/view coordinates.

### Phase 8.4 — Basic Engineering Annotations
- **Goal:** `DrawingText` on the model + deterministic text placement; title from part name; fixed offset/stack rules.
- **Scope:** text primitive in model, `renderText` helper, annotation spacing baseline.
- **Non-goals:** collision detection.
- **Tests:** text position/string exact; determinism.
- **Acceptance:** text renders at exact computed positions.

### Phase 8.5 — Dimension Model
- **Goal:** `DrawingDimension` with `ParameterRef`-driven values (never text), `DimensionValue::ParameterRef|DerivedExpression`, MVP kinds Linear/Diameter/Radius/Pattern.
- **Scope:** parameter-resolution path into dimension values; `64→72` provenance test.
- **Non-goals:** Angular/Coordinate dimensions.
- **Tests:** parameter change re-resolves dimension string without touching stored model; unresolvable parameter → structured error (fail closed), never guessed text.
- **Acceptance:** dimensions are authoritative parameter references; no `Text("Ø64")` anywhere.

### Phase 8.6 — Deterministic Dimension Layout
- **Goal:** MVP outboard dimension placement (fixed offset, fixed stack pitch), dimension→edge geometric basis.
- **Scope:** builder layout math, extension lines/arrows primitives.
- **Non-goals:** auto-compaction, collision resolution.
- **Tests:** two stacked dimensions land at exact pitches; golden layout stable.
- **Acceptance:** dimension figures deterministically placed relative to the measured geometry.

### Phase 8.7 — Centerline / Construction Geometry
- **Goal:** `Centerline`+`CenterMark` primitives; `ShowCenterlines` and `ShowConstructionGeometry` honored; construction→dashed resolution at build (strategy A/B above).
- **Scope:** builder classification, toggle filtering, dashed style.
- **Non-goals:** hidden-line engine.
- **Tests:** construction circle renders dashed or is dropped per flag; centerlines on/off; source-map-verified classification.
- **Acceptance:** construction vs solid never ambiguous in output; flags fully control inclusion.

### Phase 8.8 — SVG Drawing Renderer
- **Goal:** `drawing_to_svg(&DrawingModel)`; reuse `number`/`escape_xml`; text + centerline + dimension styles; deterministic.
- **Scope:** lone SVG renderer over `DrawingModel` (not `GeometryModel`).
- **Tests:** golden SVG for the mounting plate + dimensions; byte-for-byte stable; arcs/lines now render (fixing workbench gap).
- **Acceptance:** Drawing SVG asset produced from an `EngineeringModel` end-to-end via CLI.

### Phase 8.9 — DXF Drawing Renderer
- **Goal:** `drawing_to_dxf(&DrawingModel)`; reuse group-code writer; per-view layers; TEXT/DIMENSION/centerline entities; layer 0 no longer blind for construction.
- **Scope:** lone DXF renderer over `DrawingModel`.
- **Tests:** golden DXF; opens with correct entities per view/style.
- **Acceptance:** interop DXF from an `EngineeringModel`, deterministic, layer-aware.

### Phase 9 (separate, if strict three-views are required)
1. **9.1 Spatial Semantics** — thin 2.5-D `EngineeringSpatialSemantics` (thickness, per-feature depth/direction/through-or-blind/facing, part thickness) on the model/lowering.
2. **9.2 Projection Engine** — project 2.5-D model → Front/Top/Left `DrawingView`s from one authoritative source.
3. **9.3 Hidden Line** — visible/hidden resolution into `LineStyle::Dashed`.
4. **9.4 Three-view consistency** — `ProjectionConsistencyValidator` (front.width==top.width, etc.) with fail-closed.

These are **not** "keeping old numbering" gymnastics — they are a real architectural stage boundary (2-D drawing vs 2.5-D projection). If three-views are not a near-term requirement, **defer all of Phase 9**; Phase 8 ships single-view drawings first.

---

## Technical Debt

Checked per the brief:

1. **Constraint SourceMap counter prediction.** `finalize_constraint_source_map` (`lowering.rs:253-266`) predicts `c_{N}` ids from emission order (`relation_count + dimension_count + 2*array_count + index + 1`), assuming scc-lowering processes relations→dimensions→arrays(2 each)→anchors. It is **never cross-checked** against actual scc-lowering output, and the anchor formula folds base-profile anchors in via a global index with an unstated feature-first ordering dependency.
   - **Impact on Phase 8:** **does not block it.** The drawing chain anchors on the **feature→entity** map and on geometry `EntityId`s, neither of which is predicted — both are *recorded* directly during lowering. The predicted map matters **only** for `EngineeringConstraintId → ConstraintId`, which Phase 8 MVP dimensions do not need (dimensions bind to `ParameterId`/entities, not to SCC constraint ids).
   - **Recorded as debt:** yes — a dimension feature that must later reference an SCC constraint (e.g. "distance between two edges") would consume the predicted map and be fragile. Plan to replace prediction with the lowerer's *returned* mappings (ADR-017/018 note) **before** building constraint-backed dimensions, not before DrawingModel.
2. **ConstructionCircle export semantics.** Currently lost to identical circle output (svg.rs/dxf.rs). This is **the** core of Phase 8.7 — do not defer; it is the drawing engine's named debt. (Construction *styling* at export is a DrawingEngine responsibility; the flag must exist by Phase 8.7.)
3. **Complex two-pass planner.** ADR-020 defers a two-pass planner for very complex parts. If a part is too large for one DSL/tool batch, the single-pass path fails. Track as debt; not a Phase 8 blocker (drawing consumes a solved model regardless of how it was solved), but document a future ``--solve --chunk`` planner.
4. **No `feature_to_entity` named method** on `EngineeringSourceMap` (only `entities_for` field + `entity_to_feature` inverse). Minor; Phase 8.2 adds it.
5. **Workbench "Drawing View" is a naive client-side camera** (workbench.js:124-167): only circle+rectangle, no construction/centerline/dimension/scale, doesn't call `/api/export`. Phase 8.8's SVG renderer is the intended upgrade; the current view is a preview, not a drawing.
6. **Non-zero Circular `start_angle` patterns** expand in pure Rust to `{feature}_expanded_{index}` (`lowering.rs:410-429`), losing the ArrayRelation abstraction. Affects Phase 9 pattern consistency, not Phase 8.2.
7. **Inconsistent fixture naming:** `mounting_plate` (model.rs) vs `Mounting Plate` (vertical_slices.rs). Cosmetic; unify when promoting to the benchmark.
8. **Polygon unsolved by the solver** (`resolution.rs` rejects `EntityKind::Polygon`; only exportable). If the mounting-plate benchmark needs arbitrary windows/polygons, the solver must learn them (SCC-GEOM-004) — out of Phase 8 drawing scope but a prerequisite for the complex-plate benchmark outline.

---

## Files Expected To Be Added Later (recommendations only — NOT created now)

- `crates/scc-drawing/` — the DrawingModel crate (Phase 8.1+): `Cargo.toml`, `src/lib.rs`, `src/drawing_model.rs` (Sheet/View/Primitive/LineStyle), `src/builder.rs` (pure `DrawingBuilder`), `src/source_map.rs` (DrawingPrimitive→Entity→Feature), `src/dimension.rs`, `src/centerline.rs`, `src/construction.rs`, `src/layout.rs`, `src/svg.rs`, `src/dxf.rs`, `src/error.rs` (new `SCC-DRW-*` codes).
- `crates/scc-engineering/src/projection.rs` (Phase 9.1/9.2, separate stage).
- Workspace member entry in root `Cargo.toml`.
- New error family in `docs/error-codes.md` (`SCC-DRW-*`).

## Files Expected To Be Modified Later (recommendations only)

- `crates/scc-geometry/src/model.rs` — add `GeometryEntity.construction: bool` (Phase 8.7, strategy A), if that placement is chosen.
- `crates/scc-engineering/src/source_map.rs` — add forward `feature_to_entity()` (Phase 8.2).
- `crates/scc-engineering/src/lowering.rs` — set construction flag / surface solved-entity style, and (later) replace constraint-id prediction with lowerer-returned maps.
- `crates/scc-cli/src/engineering.rs` + `main.rs` — add `engineering drawing` / `drawing` subcommand + `DrawingExportFormat` (svg/dxf) beyond the current `ExportFormat{Svg,Dxf}`.
- `apps/scc-ui/lib/scc-cli.mjs` + `server.mjs` — engineering drawing bridge + `/api/engineering/drawing` endpoint.
- `apps/scc-ui/public/js/engineering-workbench.js` — replace naive `renderCenter` with an SVG served from the drawing pipeline.
- `docs/adr/` — a new ADR-022 for the DrawingModel layering/data-ownership decision.

---

## Architecture Risks

1. **DrawingModel becoming a second engineering state.** Mitigate: no command surface, no write API, derive-only, ephemeral (`Data Ownership`).
2. **GeometryModel vs DrawingPrimitive role overlap.** GeometryModel = positions; DrawingPrimitive = *presented* drawing geometry. Keep them separate; do not duplicate geometry derivation in DrawingModel.
3. **UI re-saving engineering parameters.** Already prevented by ADR-021 (UI emits tool calls; backend applies). Keep drawing UI read-only.
4. **Drawing layer mutating EngineeringModel.** Forbidden: drawing inputs are passed by reference; no `&mut` crosses into the model.
5. **SourceMap loss.** Keep the map in the drawing (8.2) so feature↔primitive round-trips; never recover by string prefix.
6. **Construction geometry mixed into solid outline.** Enforced by Phase 8.7 classification + fail-closed; construction never merged with outline.
7. **Dimensions storing text.** Banned by design (`DimensionValue::ParameterRef`); audit: no `Text("...Ø...")` in the model.
8. **Three-views from three independent states.** Banned: one `GeometryModel`/spatial source → Projection Engine → three views, with consistency validator.
9. **LLM entering coordinate/renderer layer.** LLM is an intent translator upstream of `EngineeringCommand` only; never feeds coordinates or drawing geometry (ADR-020).
10. **Projection silently guessing missing info.** `fail-closed` on any unresolved depth/direction/hidden data.
11. **New Drawing Engine bypassing SCC.** The drawing chain rides the existing `lower_and_solve` pipeline; no parallel proposal→drawing shortcut.
12. **Two divergent SVG/DXF logic sets.** Reuse mechanical writers; new drawing renderers are the single source for drawing output; geometry exporters stay for geometry preview.

---

## Recommendation

**Execute Phase 8.1 next** as a new `crates/scc-drawing` crate: define the derived, read-only `DrawingModel` + `DrawingBuilder` consuming `(GeometryModel, EngineeringSourceMap, resolved parameters, DrawingState)`, with **no renderer, dimension, centerline, or 3-D** yet. It is the smallest independently acceptable slice, unblocks every later drawing phase, and forces the critical architecture decisions (layer-at-a-time, single geometric truth, no second state, strong-typed source mapping) to be pinned before any rendering code exists.

Do **not** start Phase 9 (three-views) until a decision is confirmed that strict Front/Top/Left views are a near-term requirement — if not, ship single-view drawings (Phase 8.1–8.9) first and defer spatial semantics.

**Stop here (investigation & design complete; no code written).**
