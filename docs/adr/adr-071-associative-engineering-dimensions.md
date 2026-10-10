# ADR-071 — Associative Engineering Dimensions on the Derived DrawingModel

- Status: Accepted for the bounded G6A.1 slice; remaining dimension kinds stay explicitly partial
- Date: 2026-09-26
- Scope: EngineeringModel v2 DrawingModel/SVG/DXF annotation; no geometry Feature or STEP authority change

## Context

ADR-064–069 keep EngineeringModel as editable truth and formalize the validated rebuild/manufacturing boundary. ADR-070 adds Section A-A to the same derived DrawingModel. G6A still had no general parametric feature-linked dimensions. Existing legacy dimension types and renderers could represent lines, arrows, radius/diameter marks, and text, but the parametric rebuild path needed a typed, source-linked association and evidence contract.

ParametricSpatialEvidence contains canonical exact face/edge descriptions, source-feature provenance, a rebuild digest, and orthographic projection results. Projection/Visibility/Drawing consume derived evidence; they do not expose persistent OCP topology ordinals. The current Section A-A result is sampled and does not provide a unique feature-linked circular arc for every Fillet.

## Decision

1. Add a derived `associative_dimensions` index to DrawingModel. Each record carries a stable annotation ID, kind, semantic feature/body reference, view, nominal value/unit, source list, declared-versus-geometry-derived provenance, observability, status, diagnostics, and optional geometry witness.
2. A geometry witness is bound to the current EngineeringModel rebuild digest and canonical exact face evidence IDs. It stores B-Rep measurement, optional drawing measurement, component measurements, and a millimetre tolerance. Persistent references never contain OCP Face/Edge indices.
3. Build and validate associations only after the current EngineeringModel v2 rebuild and its Spatial/Projection/Visibility evidence have passed existing gates. Rebuilds regenerate associations. Suppressed features are omitted; missing, suppressed, incompatible, or ambiguous references fail closed and are not rebound by proximity.
4. Support body envelope X/Y dimensions, an exact source-attributed base Extrude thickness, and Hole/Counterbore callouts where exact cylindrical B-Rep and projected Front-view circle/arc evidence agree. Hole parameters remain the engineering declaration; B-Rep and Drawing values are independently observed evidence.
5. Use existing DrawingDimension/layout/rendering for resolved associations and preserve explicit unsupported records where the requested feature/view cannot be uniquely proven. A declared blind depth may be expressed as a linked Hole callout; it is not described as directly observed from the ordinary outline.
6. Render SVG/DXF visual annotation from the same DrawingModel. DXF visual LINE/arrow/TEXT geometry is not a native `DIMENSION` entity. Native DXF DIMENSION is deferred.

## Consequences

- The change adds no second editable geometry model, CAD kernel, production Feature, STEP route, or project-specific builder.
- Supported scope is deliberately bounded: overall X/Y, first active additive base Extrude thickness, tested Z-axis Hole/Counterbore/pattern callouts, edit refresh, and suppression cleanup.
- Parametric Fillet radius remains unsupported until an exact source-linked circular drawing/section witness and legal layout exist. The mere presence of a torus/cylinder surface or declared radius does not create a rendered radius dimension.
- General feature-to-feature distances, additive-cylinder diameter dimensions, and non-Hole depth callouts (pocket/slot/Revolve) remain unsupported.
- Native DXF `DIMENSION` entities remain deferred; SVG/DXF visual export is validated separately.
- AICAD-COMPLEX-001 full-part support remains blocked by the unresolved C1 target edge set; only the previously defined generic supported subset is exercised.
- G6A section cases for blind slots, counterbores, and revolved bodies remain NOT_RUN.

## Verification

See the [Phase G6A.1 acceptance report](../testing/phase-g6a1-associative-dimensions/acceptance-report.md) for D1–D13, OCP, Rust→OCP, Project v1/v2 STEP, fail-closed, fixture, and browser/provider evidence.
