# Spec: Generic CadDocument FeatureGraph v1

- Status: Accepted, first implementation slice
- Date: 2026-08-10

## Purpose

Move AICAD from a part-family template interface to a general, data-only feature interface. A flange, mounting bracket, shaft, or housing must be representable by the same CadDocument and rebuilt by the same KernelAdapter. Family adapters may remain as compatibility translators, but they are not the default architecture.

## Document interface

```json
{
  "version": "1.0",
  "kind": "feature_graph_v1",
  "name": "six-hole-flange",
  "units": "mm",
  "parameters": {
    "outer_diameter": { "value": 160, "source": "user" }
  },
  "features": [],
  "requirements": [],
  "manufacturing": {
    "fits": [], "tolerances": [], "datums": [],
    "geometric_controls": [], "surface_finishes": [], "threads": []
  },
  "assumptions": [],
  "unresolved": []
}
```

All top-level fields are required except the derived `source_hash`. Unknown fields are rejected. IDs use `[a-z][a-z0-9_-]{0,63}` and are unique in their collection. Units are millimetres in this version.

## Parameters and scalar references

A parameter contains a finite `value`, a `source` of `user`, `derived`, or `assumption`, and an optional description. Feature dimensions and placements accept either a finite number or `{ "parameter": "parameter_id" }`. Every reference must resolve. Updating a parameter value and rebuilding must change every referencing feature deterministically and produce a new source hash.

## Ordered features in the first slice

Every feature contains `id`, `type`, `operation`, and optional `suppressed`. Operations are evaluated in document order.

- `box`: `size: {x,y,z}` and `origin: {x,y,z}`; operation `add` or `cut`.
- `cylinder`: `diameter`, `length`, `axis`, and `origin`; operation `add` or `cut`.
- `circular_hole_pattern`: `diameter`, `length`, `count`, `pitch_diameter`, optional `start_angle`, `axis`, and `origin`; operation must be `cut`.

The first active feature must add material. Every later active feature must change the current B-Rep. Additive features must remain a single connected solid. The final result must be exactly one valid non-empty solid.

## Requirement coverage

Every source requirement has a stable ID, original text, and exactly one disposition:

- `geometry`: one or more existing, non-suppressed feature IDs provide geometric evidence.
- `manufacturing`: the requirement is retained as a manufacturing semantic but is not shape geometry.
- `assumption`: an explicit assumption is required.
- `unresolved`: an explicit unresolved item is required; critical items block the kernel.
- `unsupported`: the capability is not implemented and a reason is required.

The build manifest returns per-feature evidence and coverage totals. `complete` means every requirement is either geometric or manufacturing; `partial` means at least one requirement is an assumption or unsupported. A model with critical unresolved decisions is not built.

## Acceptance

- One FeatureGraph builds a six-hole flange as a valid 160×160×18 mm B-Rep and re-importable STEP.
- A different FeatureGraph builds a mounting bracket through the same KernelAdapter.
- Editing the flange outer-diameter parameter changes its B-Rep bounds and source hash.
- Unknown feature types, unresolved parameter references, duplicate IDs, no-op cuts, disconnected additions, and silent requirement omissions fail deterministically.
- The existing gearbox family and 2D SCC workflow remain compatible during migration.
