# ADR-072 — Bounded General Associative Dimensions and Native DXF

- Status: Accepted for the verified G6A.2 slice; generic spacing/depth/Fillet coverage remains partial or deferred
- Date: 2026-09-27
- Scope: Derived EngineeringModel v2 DrawingModel annotations and DXF output; no geometry Feature, project format, or STEP authority change

## Context

ADR-071 established source-linked dimensions for overall body envelope, an additive base Extrude thickness, and validated Hole/Counterbore callouts. G6A.2 requires reusable references and measures, Fillet radius, circular additive dimensions, feature spacing, generic cut depth, and actual native DXF entities. Existing spatial evidence provides exact source-attributed faces and edges. Orthographic HLR curves carry stable projection identity but do not themselves identify a feature. Section A-A is sampled and does not expose a unique semantic arc for arbitrary Fillets.

## Decision

1. Keep EngineeringModel v2 as the only editable geometry truth. Associations and their witnesses remain derived DrawingModel data; rebuild them after EngineeringModel and Spatial/Projection/Visibility validation.
2. Add typed reference variants for Feature, Face, Edge, Datum, and validated geometry sources. A typed variant alone does not make every reference measurable. Emit a resolved annotation only when the implementation has a stable engineering source, exact current-rebuild geometry evidence, and one legal Drawing view/layout witness.
3. Support an additive cylindrical boss diameter for the bounded case of one circular XY Sketch loop followed by an active normal-Z Extrude ADD. The source is the stable profile entity reference; the witness includes its exact source-attributed cylinder/circle evidence and a unique projected Front circle. Unsupported plane, profile, or projection cases fail closed.
4. Emit native DXF DIMENSION entities for resolved simple Linear, Diameter, and Radius dimensions. Use anonymous graphical blocks, an explicit millimetre DIMSTYLE, correct definition points and text, and preserve the established visual LINE/arrow/TEXT path for composite callouts and legacy dimensions. Native and visual representation remain separate acceptance claims.
5. Do not infer a Fillet dimension from a declared radius or torus alone, a feature spacing from a solid bounding box, or a pocket/slot depth from an outline. Keep unsupported or non-observable cases explicit until source-linked view evidence exists.
6. Do not change formal STEP lowering, Project v1/v2 dispatch, CAD Skills, or Workbench product UI in this phase.

## Consequences

- The tested circular Extrude ADD diameter is associative across a source Sketch radius edit and suppression. Existing Hole callouts continue to rebuild from their formal feature and validated cylindrical evidence.
- Generic operational measurement builders for all typed reference variants, Fillet radius, arbitrary reference spacing, and generic blind-cut depth remain incomplete. Feature-specific Hole and Pattern callouts are not substitutes for those generic capabilities.
- The Workbench does not expose a bindable parameter slot for a v2 `ParametricFeature::Hole`; a browser attempt to bind `diameter` is rejected with `SCC-TOOL-010`. Browser parameter-edit acceptance is therefore partial even though direct EngineeringModel source edits rebuild the tested circular-boss annotation.
- Native DIMENSION verification applies only to the simple supported dimension kinds. Composite hole/pattern callouts remain visual DrawingModel annotations.
- AICAD-COMPLEX-001 full support stays BLOCKED by the unresolved C1 chamfer edge set.

## Verification

See [Phase G6A.2 acceptance report](../testing/phase-g6a2-associative-dimensions/acceptance-report.md). The verified slice includes Cargo workspace tests and strict Clippy, UI Node tests, CadQuery/OCP and Rust→OCP integration, the Project v1/v2 formal STEP regression, independent DXF parsing, and isolated Playwright runs using a fake Provider. Real DeepSeek remains NOT_RUN.
