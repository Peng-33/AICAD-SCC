# ADR-074 — General Countersink and Explicit Placement Pattern

- Status: Accepted for the bounded G6B feature set
- Date: 2026-09-27
- Scope: EngineeringModel v2, deterministic OCP rebuild, derived Drawing, and the existing canonical STEP authority

## Context

G2 already represented a countersink as a `HoleKind::Countersink`, but formal lowering stopped before real conical geometry. G2 also provided ordered linear and circular `Pattern` features, but not explicit local-plane placements. G6A.3 added stable, typed parameter binding and candidate rebuild before committing an edit. G5/G5.1 established the only formal STEP route: lower the EngineeringModel, build a solid, serialize, re-import, compare independent geometric/material witnesses, and return the validated bytes.

## Decision

1. Keep Countersink inside the existing Hole feature. The mouth diameter and included angle are the source parameters; the cone depth is derived by `(mouth_diameter - main_diameter) / (2 * tan(included_angle / 2))`. Do not create a separate editable cone feature, treat Countersink as Counterbore, or substitute an edge chamfer.
2. Lower the Hole’s main bore first, then emit a transient `ParametricCountersinkOperation` containing the source feature identity, support plane, direction, bore/mouth diameters, included angle, and optional semantic support face. The shared CadQuery/OCP kernel uses one analytic `makeCone` boolean cut. The transient operation is not persisted as EngineeringModel history.
3. Use the Hole axis and support frame as provided by EngineeringModel. Validate finite dimensions, positive bore, larger mouth, angle strictly inside `(0, 180)` degrees, unit frame vectors, normal-aligned direction, source ordering, and actual body intersection. Preserve distinct parameter, dependency, no-intersection, invalid-body, and kernel-failure diagnostics.
4. Extend the existing generic `PatternDefinition` with `explicit_placement`. A pattern keeps one stable `seed_feature_id`, a principal or DatumPlane `reference_plane`, and deterministic local U/V target placements. The seed is the implicit instance at its authored position; each listed placement is one derived instance. Transform each target through the reference-plane frame. Do not persist copies of seed Features or an editable instance tree.
5. Bound this stage’s explicit-placement seed capability to Hole and Extrude CUT. Keep LinearPattern and CircularPattern semantics unchanged. Reject unsupported seed types, invalid/non-finite or duplicate placements, missing/suppressed seeds, and failed instances rather than silently skipping them. Do not claim arbitrary 3D rotations or Extrude ADD array support.
6. Route Countersink and ExplicitPlacement through the shared Rust lowering and OCP rebuild consumed by Spatial/Projection/Visibility/Drawing and by the existing canonical STEP exporter. STEP remains gated by re-import, equivalence, and exact validated-byte delivery. No new export endpoint or geometry authority is added.
7. Reuse G6A.3 typed parameter binding for countersink mouth diameter and angle. A candidate model must pass engineering validation and real rebuild before the API returns it. The source feature literal stays intact when a user edit creates a typed parameter binding; the resolved parameter drives the rebuilt shape and Drawing.
8. Keep Drawing geometry derived from verified OCP geometry. The generic plate fixture exposes a planar countersink mouth in projection; A–A sections expose actual cut geometry. The existing annotation contract cannot express the full standard countersink diameter-and-angle callout, so `GEOMETRY_SUPPORTED = PASS` and `ANNOTATION_SUPPORTED = PARTIAL`; do not insert plain text as a substitute.
9. Permit deterministically sampled Spatial B-spline edges with a maximum declared 0.05 mm deviation, using bounded recursive sampling and no more than 512 emitted points per edge. Tessellate non-restorable HLR curves within 0.01 mm. Keep the worker response bounded at 4 MiB to accommodate the resulting full view geometry; do not relax STEP geometric equivalence tolerances.
10. Preserve Project v1, EngineeringModel v2, the single Engineering Workbench, six existing CAD Skills, and the existing manufacturing/STEP contract. Do not implement Sweep, Loft, NURBS, assembly, multi-solid manufacturing, broad annotation redesign, or part-specific logic in G6B.

## Consequences

- A Countersink remains a typed Hole feature and now reaches real OCP geometry, Drawing/section output, and canonical STEP with analytic cone evidence.
- Explicit placements are deterministic derived instances of a stable seed and can target Hole and Extrude CUT features on principal or DatumPlane frames.
- A typed mouth/angle edit rebuilds the solid and refreshes the Drawing. Invalid edits leave the previously committed model and Drawing intact.
- Countersink’s geometry is supported, while a standard countersink annotation is not yet expressible. That distinction remains visible in Drawing diagnostics.
- One experimental countersink on a tangent curved barrel exceeded the existing strict STEP equivalence limits by `7.53699933e-5 mm³` volume and `2.57869997e-5 mm²` area. No tolerance was changed. The accepted Revolve+Countersink fixture uses a planar end face; tangent curved-surface placement needs a separate robust kernel/STEP investigation before it can be claimed.
- The full `AICAD-COMPLEX-001` remains blocked by the pre-existing ambiguity in the requested C1 edge set. Passing generic G6B fixtures does not make the full complex part supported.

## Verification

See the [Phase G6B acceptance report](../testing/phase-g6b-machining-features/acceptance-report.md) and the actual [Chromium Workbench screenshot](../testing/phase-g6b-machining-features/aicad-g6b-engineering-workbench.png). Real DeepSeek remains `NOT_RUN`; no API key was read or used.
