# ADR-066: General Machining Features and Reference Geometry

- Status: Accepted for Phase G2
- Decision owners: SCC architecture
- Scope: generic Hole and Pattern features and semantic planar-face support in EngineeringModel v2

## Context

ADR-064 defines EngineeringModel as the editable design truth. ADR-065 adds a stateless ordered-history rebuild through the formal CadQuery/OCP backend and the existing Spatial → Projection → Visibility → Drawing pipeline. G2 adds machining semantics without introducing another solid model or routing derived kernel shapes to manufacturing.

A persistent CadQuery/OCP face index is not stable across rebuilds. Holes and patterns also need to preserve their source relationships so parameter edits rebuild dependent geometry deterministically.

## Decision

### 1. EngineeringModel feature semantics

- Add typed `Hole` and `Pattern` values to the existing ordered parametric feature history. Stable feature IDs and `feature_order` remain authoritative.
- A Hole retains its support reference, 2D placement in that support frame, diameter, direction, termination, and typed kind. A Counterbore retains its larger diameter and depth as one compound Hole feature.
- A Pattern retains `seed_feature_id` and a typed Linear or Circular definition. It does not store expanded instances as editable features. The current patternable subset includes Hole and generic Extrude CUT operations; pattern planning is not Hole-specific.
- A changed seed triggers a complete deterministic rebuild of the ordered history. Missing, suppressed, forward, or unsupported seeds fail before geometry is returned. Pattern instance booleans must each intersect the one-body solid and preserve a valid single solid.

### 2. Stable planar references

- `PlaneReference` supports principal planes, EngineeringModel datum planes, and semantic planar-face references. Face references carry the source feature ID, a semantic selector, and geometric qualifiers including expected normal, plane offset, and tolerance.
- CadQuery/OCP face ordinals are transient implementation details and never enter EngineeringModel. The backend resolves candidates against source-feature provenance and qualified planar geometry; zero or multiple matches fail closed with stable reference diagnostics.
- The supported semantic selector set is Top, Bottom, Positive/Negative X, and Positive/Negative Y. Arbitrary custom frames, cylinders, and persistent topology indices are not inferred.
- Hole direction is derived from its support frame plus its explicit reversal. Real tests cover both signs of X/Y/Z directions. Through-all depth is derived from measured body bounds; blind cuts use the requested exact depth.

### 3. Rebuild and drawing boundary

- Rust lowers validated feature history to a versioned transient operation sequence. The worker performs exact CadQuery/OCP Boolean operations in order and returns measured body, provenance, analytic curve/face, and HLR evidence. Rebuild failure returns no partial model or drawing.
- The same formal Spatial → Projection → Visibility → Drawing → SVG/DXF path consumes that evidence. A model reply or a 2D approximation is never accepted as a completed drawing.
- Hole and Pattern are represented in the EngineeringModel and tool registry; the existing CAD Skills plan exposes only the supported generic operation set.
- Legacy Project v1 and the established STEP authority remain unchanged. Parametric rebuild shapes do not enter STEP delivery.

### 4. Explicit capability limits

- Simple through/blind Hole and exact coaxial Counterbore geometry are supported and tested with real B-Rep operations.
- Countersink schema and parameter validation exist, but exact conical cutting geometry is deferred. It is not substituted with a cylinder and must not be described as implemented geometry.
- Explicit-placement patterns, `OffsetFromFace`, custom datum frames, multiple bodies, and parametric STEP remain deferred. Linear and Circular patterns are the supported definitions.
- Fillet, Chamfer, Revolve, Sweep, Loft, NURBS, assemblies, and general multi-body modeling remain out of scope. Passing the G2 fixture or AICAD-COMPLEX-001 subset does not establish general complex-part capability.

## Rejected alternatives

- Persisting topology indices would make face references unstable across rebuilds.
- Expanding patterns into copied feature JSON would lose seed identity and create multiple editable truths.
- Approximating a Countersink with a cylindrical cut would change the requested geometry.
- Routing a transient drawing rebuild shape to STEP would bypass the existing manufacturing validation boundary.

## Verification contract and evidence

- The approved isolated CadQuery/OCP suite passed 71/71 tests.
- Rust → CadQuery/OCP integration passed 22/22 tests, including the G1B generic history matrix, G2 Hole/Counterbore/semantic face/Linear and Circular pattern fixture, the tested AICAD-COMPLEX-001 subset, formal three-view SVG/DXF output, and legacy STEP regressions.
- Measured G2 rebuilds: generic machining fixture kernel 1.408 s; complex subset kernel 1.357 s. Both requests also completed spatial lowering, all three views, SVG, and DXF in the same pipeline.
- Browser acceptance with the isolated fake Provider passed for both `/` and `/engineering.html`, two independently named configurations and switching, multiline Chinese Engineering Intent, EngineeringModel v2 Hole/Pattern, Front/Top/Left drawing, SVG/DXF downloads, and Chinese/English UI. It did not call a paid or real Provider.
- A new-design CLI adapter regression was fixed so a `null` model snapshot is not written as a file; real G2 Engineering Intent integration now exercises that case. CadQuery worker process output is drained concurrently with stderr before wait, preventing a large validated result from blocking the subprocess.
- The parametric STEP route remains deferred; legacy STEP coverage in the 22-test integration suite is a regression check only.
