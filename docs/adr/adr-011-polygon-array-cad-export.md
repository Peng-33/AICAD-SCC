# ADR-011: Polygon, Array, and CAD Export Semantics

- Status: Accepted for v0.3.0
- Date: 2026-08-09
- Owners: SCC architecture

## Context

ADR-004 reserved Polygon construction, array-derived geometry, and CAD export for later M3 slices. The symbolic M1/M2 pipeline already represented arrays as paired `ArrayRelation` and `InstanceOf` constraints, but the pattern values were not preserved in GraphState. GeometryModel already declared a Polygon variant but did not define how vertices reached it. SVG and DXF also needed deterministic, testable subsets before they could be exposed through the CLI.

## Decision

### Polygon parameters

A Polygon RuntimeEntity uses a numeric `vertex_count` and zero-based coordinate pairs named `x_0`/`y_0`, `x_1`/`y_1`, and so on. `vertex_count` must be an integer from 3 through 100,000. Every declared coordinate must be finite, vertices must be unique within the solver tolerance, and the ordered loop must enclose nonzero signed area. Vertex order is preserved in GeometryModel and both exporters.

### Array parameter bridge

Lowering stores the selected pattern and its numeric values in the `ArrayRelation` parameter map. `InstanceOf` remains the inverse ownership relation and has no duplicated pattern data. The Geometry solver accepts an array only when both active relations form a matching pair.

The source entity is a template. It is replaced in GeometryModel by stable derived IDs of the form `array_id[index]`:

- Linear arrays contain `count` instances. Index 0 is the untranslated template and later instances use `index * spacing` on x or y.
- Grid arrays contain `rows * cols` instances in row-major order. Column spacing moves x and row spacing moves y.
- Circular arrays contain `count` translated instances. Their offsets are equally spaced around a circle centered on the template position. Geometry orientation is preserved; this first subset does not rotate instances.

Circular coordinates are normalized to 12 decimal places to keep JSON stable across supported platforms. Derived ID collisions and incomplete relation pairs are hard errors rather than partial output.

### SVG subset

`scc export MODEL --format svg` supports Point, Line, Circle, Arc, Rectangle, and Polygon. Output uses a deterministic viewBox with one coordinate unit of padding, a Cartesian-to-SVG y-axis conversion, stable entity-ID ordering, XML-escaped IDs, and numbers normalized to at most 12 fractional digits. Arc angles are radians in GeometryModel; their absolute span must be greater than zero and less than one full turn. Full turns use Circle instead.

### DXF subset

`scc export MODEL --format dxf` emits deterministic ASCII DXF with `$ACADVER=AC1027`. The supported mapping is:

| Geometry primitive | DXF entity |
|---|---|
| Point | `POINT` |
| Line | `LINE` |
| Circle | `CIRCLE` |
| Arc | `ARC` with radian-to-degree conversion |
| Rectangle | closed `LWPOLYLINE` |
| Polygon | closed `LWPOLYLINE` |

All entities use layer `0` and world x/y coordinates with z=0. SCC coordinates remain unitless; callers choose the real-world unit consistently for GraphState, GeometryModel, SVG, and DXF. Because no unit conversion occurs, a new schema envelope is not required for v0.3.0.

### CLI and determinism

The export command accepts legacy or schema-1.x GeometryModel JSON and writes atomically with the existing `--force` protection. SVG and DXF Golden fixtures are compared byte-for-byte. Repeated exports of the same GeometryModel must produce identical bytes.

## Compatibility

- Existing GraphState documents with empty array parameters remain readable but cannot be solved as arrays; they return the existing `SCC-GEOM-005` unsupported-constraint diagnostic.
- ConstraintIR and GraphState generated from array proposals now include array parameters, intentionally changing their deterministic root and Golden snapshots.
- Existing GeometryModel JSON remains compatible because no fields were added to the model schema.
- The new CLI command is additive.

## Rejected alternatives

- Keeping the source template alongside all instances: duplicates index 0 in exports and makes `count` ambiguous.
- Encoding array patterns only in GeometryModel: violates the read-only GraphState boundary.
- Locale-dependent or shortest-platform float output: weakens cross-platform byte determinism.
- Adding a general CAD library for the first subset: increases the dependency and supply-chain surface without improving the required primitive coverage.
- Emitting unit metadata without performing conversion: creates an appearance of stronger dimensional semantics than the current unitless pipeline provides.
