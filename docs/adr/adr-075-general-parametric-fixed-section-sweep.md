# ADR-075 — General Parametric Fixed-Section Sweep

- Status: Accepted for the bounded G7A feature set
- Date: 2026-09-27
- Scope: EngineeringModel v2, ordered feature rebuild, derived Drawing/Section A-A, and the existing canonical STEP authority

## Context

G1–G6B established Sketch profiles, Datum reference geometry, ordered parametric rebuild, semantic topology references, Extrude, Revolve, machining features, associative Drawing and the canonical STEP delivery path. The kernel did not yet provide a general fixed-section Sweep. A Sweep must remain ordinary editable EngineeringModel history and must reach the existing spatial, drawing and manufacturing consumers without introducing a second model or shape authority.

## Decision

1. Add `ParametricFeature::Path` and `ParametricFeature::Sweep` to EngineeringModel v2. A Path is an ordered, stable-ID sequence of analytic Line and Arc entities on a principal plane or supported DatumPlane. A Sweep references a Sketch and Path, carries ADD/CUT, forward/reverse direction, the profile anchor, and its orientation rule. Neither transient OCP shapes nor cached solids become editable engineering state.
2. Bound G7A to open planar paths composed of Lines and Arcs. Reject empty, closed, degenerate, discontinuous, self-intersecting, and unsupported paths. Adjacent joins must be C1; sharp-corner transition behavior is not inferred. Validate finite geometry, declared radii, segment identities, path support and ordered dependencies before kernel execution.
3. Require the Sketch profile anchor to be in the profile material region and map its declared local frame to the directed start tangent of the Path. The only G7A orientation is `fixed_binormal`: the support-plane normal supplies a stable binormal, and the section does not use a user twist or a Frenet frame. Do not rely on unspecified OCP defaults.
4. Rebuild Sweep through the existing Rust lowering and transient CadQuery/OCP request. Build the fixed profile and ordered Path wires, then call `cq.Solid.sweep(..., makeSolid=True, isFrenet=False, mode=None, transitionMode="transformed")`. Require a valid, positive-volume single-solid tool. ADD must fuse into the existing single body, or initialize it when Sweep is the first material feature; reject a detached or non-fusing addition. CUT requires an existing body and actual material intersection, must retain a valid solid, and must not remove the whole body.
5. Keep distinct stable diagnostics for Path/profile validation, invalid profile attachment, Sweep kernel failure, self-intersection, detached ADD, and CUT no-intersection/body-removal. Candidate edits and suppress/unsuppress continue through the existing deterministic full-history rebuild; a failed rebuild does not commit a partial shape.
6. Reuse the existing Spatial → Projection → Visibility → DrawingModel path and formal Section A-A. Sweep-derived analytic surfaces enter generic geometric projection. Do not add part-specific outlines or infer hidden dimensions from an outer silhouette. Section evidence is required independently of three-view evidence.
7. Keep semantic topology references based on the existing stable source-feature/provenance and geometric qualifiers. Do not persist OCP Face/Edge ordinals. G7A does not promise a stable semantic selector for every Sweep-generated curved face or edge; such topology-reference use remains partial where the current selector contract cannot identify one qualified element deterministically.
8. Lower Sweep into the current formal manufacturing protocol and canonical STEP authority. STEP must still pass shape validity, single-solid checks, serialization, STEP re-import, geometric/material equivalence, and exact validated-byte delivery. Drawing runtime shapes are not a STEP source, and no second export endpoint or looser equivalence tolerance is introduced.
9. Extend the six existing CAD Skills and the one Engineering Workbench with generic Path/Sweep planning, inspectable references and diagnostics. Missing radius, endpoints, profile dimensions or other manufacturing-critical input remains an ambiguity; do not guess values or add a seventh Skill or a second UI.
10. G7A does not implement arbitrary 3D paths, sharp corners, closed paths, variable sections, scaling, user twist, Frenet transport, Loft, NURBS/freeform, assemblies, or multi-solid manufacturing. These require separate feature and verification stages.

## Consequences

- A generic fixed-section planar Sweep is now a formal v2 history feature and uses real OCP ADD/CUT booleans.
- Curved paths affect the B-Rep and reach front/top/left projection, Section A-A, SVG/DXF, and canonical STEP re-import/equivalence.
- Path/profile edits, suppression and restoration are rebuilt from EngineeringModel and remain deterministic; kernel shapes are disposable derived values.
- The model supports analytic Line/Arc paths with C1 joins. Sharp-corner paths and arbitrary spatial paths are rejected rather than approximated.
- Generic geometry is supported, but semantic face/edge selection on all Sweep-generated surfaces is not claimed. Complex G6B annotation and topology limitations remain unchanged.

## Verification

See the [Phase G7A acceptance report](../testing/phase-g7a-parametric-sweep/acceptance-report.md) and the recorded [isolated-provider Chromium Workbench screenshot](../testing/phase-g7a-parametric-sweep/aicad-g7a-sweep-workbench.png). The report maps S1–S18 to real OCP tests and records Drawing, Section, formal STEP and browser evidence. Real DeepSeek remains `NOT_RUN`.
