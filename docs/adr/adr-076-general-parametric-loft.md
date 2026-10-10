# ADR-076 — General Parametric Loft

- Status: Accepted for the bounded G7B feature set
- Date: 2026-09-27
- Scope: EngineeringModel v2, ordered Sketch dependencies, ruled Loft ADD/CUT, derived Drawing/Section A-A, and the existing canonical STEP authority

## Context

G1–G7A established an authoritative EngineeringModel v2 feature history, Sketch and Datum references, stateless CadQuery/OCP rebuild, orthographic Drawing, section views, and one formal STEP delivery path. The model did not yet express a transition among multiple section Sketches. A Loft must remain an ordinary history feature and must not create a second editable surface model or make transient OCP topology persistent.

Loft correspondence also needs an explicit contract. OCP cannot be asked to infer the intended match between arbitrary section edges when sections have different topology, loop structure, direction, or frame.

## Decision

1. Add `ParametricFeature::Loft` to EngineeringModel v2. It references an ordered list of existing Sketch feature IDs, an ADD/CUT operation, `RULED` mode, and `ALIGNED` orientation policy. Section geometry is never copied into the Loft feature. At least two and at most 32 unique, earlier Sketch references are required.
2. Bound the supported section frames to parallel, co-oriented principal or Datum planes with the same local axes and a strict monotonic offset along one common principal direction. Preserve the complete plane transforms; do not hard-code world Z. Reject lateral plane offsets, reversed/non-monotonic ordering, non-parallel planes, and unsupported orientation rather than ignoring pose data.
3. Require one closed outer loop per section, nonzero area, consistent loop winding, supported finite Line/Arc/Circle geometry, no self-intersections, and a verified one-to-one ordered curve correspondence. Different curve counts or curve kinds fail closed with a typed Loft diagnostic. Inner loops are explicitly unsupported until loop correspondence and hollow-solid evidence are established.
4. Build section wires and faces from their EngineeringModel Sketch frames through the shared versioned Rust→CadQuery/OCP contract. Use OCP `BRepOffsetAPI_ThruSections` in solid, ruled mode with the existing kernel tolerance. A result must be one valid positive-volume solid. `SMOOTH`, guide curves, twist/Frenet behavior, arbitrary non-parallel frames, and freeform controls are not implied by this contract.
5. Keep ADD and CUT on the shared ordered body rebuild. A first-feature ADD may initialize the body; later ADD must fuse into exactly one connected body. CUT requires a real intersection and must reduce material while preserving a valid one-solid result. Detached ADD, no-hit CUT, whole-body removal, invalid kernel output, and boolean failures keep distinct stable error codes. No failed candidate Shape is committed or cached as engineering state.
6. Resolve dependencies by stable EngineeringModel feature IDs. Full-history rebuild after edits or suppression reparses current Sketch geometry and dependent features. Missing/suppressed references fail closed and never bind to another Sketch. OCP Face/Edge ordinals are not stored in the model.
7. Reuse Spatial → Projection → Visibility → DrawingModel → SVG/DXF and the formal Section A-A pipeline. Loft faces remain exact OCP geometry; B-spline Spatial face evidence is explicitly sampled with 0.05 mm linear deviation and 0.1 rad angular tolerance. Non-analytic projected curves are emitted with their declared 0.01 mm approximation tolerance; Section A-A uses the existing 0.01 mm chord tolerance. Such display evidence is not represented as an exact analytic circle or a persistent semantic topology selector.
8. Lower Loft into the existing canonical formal Manufacturing/STEP authority. The formal exporter must still validate one solid, serialize STEP, re-import it, compare geometry/material evidence at the existing tolerances, and deliver exactly the validated bytes. Drawing runtime geometry is not a STEP source; no second STEP endpoint or relaxed equivalence threshold is added.
9. Extend the one Engineering Workbench, existing Engineering Intent Tool registry, and six existing CAD Skills with the generic `create_loft` capability, ordered section properties, and bilingual stable diagnostics. Preserve all existing feature history and do not add a part-specific Builder or a new Skill.
10. Keep semantic topology support partial for Loft-generated surfaces. Existing stable references may be used only where their source and geometric qualifiers resolve uniquely. G7B does not promise a selector for every side face, section edge, or generated side edge, and does not claim generic Fillet/Chamfer applicability to those edges.

## Consequences

- Two or more compatible existing Sketches can now define a real ruled Loft ADD or CUT in EngineeringModel v2.
- Intermediate sections affect the real B-rep; section edits, suppression, restoration, downstream Hole history, Drawing rebuild, and formal STEP are derived from current model history.
- The bounded implementation supports a generic two-circle ADD, non-monotonic three-circle ADD with changing centers and sizes, compatible rectangular profiles, an X-axis example, fused ADD, and cavity CUT.
- Smooth C1/C2 continuity, holes/inner loops, mismatched profile topology, non-parallel sections, local section twist, guide curves, multi-body Loft, and stable selectors for every Loft-generated face/edge remain unsupported.
- A three-view silhouette is not a complete station-dimension definition. Use an appropriate section or other formally supported geometry evidence when a station’s internal dimensions are not uniquely observable.

## Verification

See the [Phase G7B acceptance report](../testing/phase-g7b-loft/acceptance-report.md) and [real Chromium Engineering Workbench screenshot](../testing/phase-g7b-loft/aicad-g7b-engineering-workbench.png). The report records L1–L24, generic ADD/CUT fixtures, DrawingModel versus re-imported STEP evidence, final-image CadQuery/OCP and Rust→OCP runs, exact browser-delivered STEP bytes, and bounded topology/continuity claims. Real DeepSeek remains `NOT_RUN`.
