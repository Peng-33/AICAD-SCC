# ADR-067: Semantic Topology References, Fillet, and Chamfer

- Status: Accepted for Phase G3
- Decision owners: SCC architecture
- Scope: derived semantic face/edge topology, constant-radius edge Fillet, and equal-distance edge Chamfer in EngineeringModel v2 rebuilds

## Context

ADR-064 makes `EngineeringModel` the sole editable design truth. ADR-065 and ADR-066 provide a stateless exact CadQuery/OCP rebuild and typed machining history. General edge treatments need references that survive deterministic rebuild and valid upstream edits without persisting OCP topology ordinals. Treated geometry must continue through the same Spatial → Projection → Visibility → Drawing → SVG/DXF path.

## Decision

### 1. Semantic references and provenance

- EngineeringModel stores `EdgeReference { source_feature_id, selector, qualifier }`. Selectors cover stable profile-boundary identity on the top or bottom of an Extrude ADD, pocket entrance identity, Hole entrance/exit, edges between two semantic planar faces, and convex/concave edges between those faces. Existing G2 `PlanarFaceReference` remains authoritative for planar supports.
- The model and Rust→CadQuery wire schema reject unknown fields; no raw OCP Edge/Face index or `Edge7`/`Face13` value is accepted. The Rust request is a versioned ordered operation enum, and CadQuery Shapes remain invocation-local derived values.
- The worker resolves selectors on the source-feature snapshot, then requires the same exact curve geometry and qualifiers on the current rebuild prefix. Zero matches, multiple matches, stale topology, and duplicate edge resolution fail closed with stable codes. No closest-edge rebinding or first-candidate selection is allowed.
- Derived Spatial edge and face evidence includes source feature IDs inferred from per-operation topology geometry deltas. Canonical geometry hashes identify evidence records; they are independent of kernel enumeration order. These provenance values are derived and never written back to EngineeringModel.
- Hole entrance/exit resolution uses the source Hole axis, placement, diameter, direction, and end-plane projection. Face selectors use source feature provenance, semantic orientation, signed plane offset, and tolerance.

### 2. Feature execution

- `Fillet { edge_refs, radius }` and `Chamfer { edge_refs, distance }` are formal EngineeringModel v2 features in the ordered history. References must be unique and point to an earlier active compatible geometry feature. Suppression omits the feature from a complete rebuild; edits rebuild from the full source history; illegal reordering fails dependency validation.
- The worker calls CadQuery/OCP `Shape.fillet(radius, edges)` for constant-radius edge fillets and `Shape.chamfer(distance, None, edges)` for equal-distance edge chamfers. Every operation must yield a valid one-solid body. There is no automatic radius reduction, global treatment, renderer approximation, or partial success.
- Fillet and Chamfer retain different EngineeringModel meanings from a Hole Countersink. Countersink exact geometry remains deferred; a user-requested ordinary hole-mouth chamfer is represented by a generic Chamfer feature.

### 3. Drawing, compatibility, and manufacturing boundary

- The final treated B-Rep produces exact analytic edge/face evidence and kernel HLR for Front, Top, and Left. Elliptical B-Rep edges from true fillets, toroidal boss fillet faces, and conical faces from true chamfers are preserved as spatial evidence; they are not forged in the renderer. Projected circles remain analytic when their axis aligns with the view. Edge-on circles are restored to bounded derived segments at 1e-5 mm; oblique projected circles use bounded derived polylines at 1e-4 mm. Rust validates that every derived projection carries a finite positive tolerance. These projection approximations never modify EngineeringModel dimensions or the OCP body. The established Rust Projection → Visibility → Drawing → SVG/DXF stages remain the only formal drawing path.
- G1B/G2, legacy Project v1, and the established formal STEP authority remain unchanged. Parametric G3 shapes do not enter SolidBuildSpec v1 or formal STEP. No Revolve, Sweep, Loft, NURBS, assembly, or multi-body capability is inferred from this phase.

## Rejected alternatives

- Persisting kernel ordinals would make references change across rebuilds.
- Rebinding to a nearby edge or choosing the first match could alter engineering intent silently.
- Applying a treatment to all edges or drawing a rounded 2D approximation would bypass explicit feature intent and the validated B-Rep.
- Treating a generic Chamfer as a Countersink would erase distinct engineering semantics.
- Sending the transient treated B-Rep through the legacy STEP contract would bypass the formal manufacturing boundary.

## Verification contract

G3 acceptance uses generic fixtures and the real isolated CadQuery/OCP runtime. Tests cover top/bottom profile edges, Hole entrance/exit, Pocket entrance, semantic face adjacency, convex/concave selectors, stale and ambiguous references, dimension edits, downstream history, Fillet/Chamfer parameter edits, suppression, determinism, Front/Top/Left, SVG/DXF, and existing Rust/API/CLI/STEP regressions. Browser acceptance uses the isolated fake Provider and makes no paid model call. Exact test counts, timings, screenshot path, and any unrun gates are recorded in the Phase G3 completion report and the verification evidence below.

## Evidence

- EngineeringModel v2 stores semantic references and treatment dimensions only; no kernel index is present in the persisted feature schema.
- Isolated CadQuery/OCP: 85/85 passed. Rust→CadQuery/OCP and formal STEP integration: 24/24 passed. Host Node suite: 155 passed, 0 failed, 11 runtime-gated skips; the gated CAD/STEP flows ran in the isolated integration image. Rust fmt/check/test/strict Clippy, Compose config, and diff check passed.
- Playwright acceptance: `PASS` with isolated fake Provider. `/` and `/engineering.html` both use the Engineering Workbench; Engineering Intent, semantic edge treatments, Front/Top/Left, SVG/DXF downloads, and Chinese/English switching were verified. Screenshot: `docs/testing/phase-g3-browser/aicad-g3-engineering-workbench.png`.
- Generic fixture timings in ms: topology 15.569; pre-treatment ADD/CUT history 52.477; Fillet 29.725; Chamfer 13.670; complete worker call 2766.499; three-view projection/drawing 0.335; SVG 0.175; DXF 0.122. The complex subset recorded 9.926, 54.499, 55.216, 157.080, 3369.878, 0.329, 0.177, and 0.132 ms, respectively. The full worker call includes process startup, B-Rep rebuild, and Spatial/HLR evidence. Treatment timings include topology resolution and validation.
- The bounded AICAD-COMPLEX-001 G3 subset passed; this does not establish general complex-part capability. Concave-edge Fillet, variable/face/full-round Fillet, Distance-Angle Chamfer, Countersink, Revolve/Sweep/Loft/NURBS, and parametric STEP remain unimplemented or deferred as specified. Existing explicit G2 patterns and the formal STEP path passed regression.
- Real DeepSeek API: `NOT_RUN` by design; no paid Provider request was made.
