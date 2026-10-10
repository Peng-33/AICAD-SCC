# ADR-032: Engineering spatial semantics

- Status: Accepted
- Date: 2026-08-26
- Related: ADR-016 (engineering model as single source of truth), ADR-017
  (typed feature-parameter slots), `crates/scc-engineering/src/spatial.rs`,
  `docs/error-codes.md` (`SCC-ENG-031`, `SCC-ENG-032`)

## Background

Through Phase 8, the engineering model stored intent for _which_ features
exist, their parameters, bindings, constraints, and drawing presentation — but
not _how deep features go through material_. A hole knew its diameter but not
whether it pierces the plate; nothing recorded plate thickness at all. Downstream
spatial consumers (material removal, mass properties, section views,
manufacturing annotations) cannot be built on that vocabulary. Phase 9.1 adds a
minimal, data-only spatial semantics layer to the frozen engineering core:
plate thickness and per-hole extents.

## Decisions

1. **New module, new types.** `scc-engineering` gains a `spatial` module with
   `SourcedLength` (literal or parameter-driven), `HoleExtent`
   (`ThroughAll` | `Blind { start_face, depth }`), `HoleStartFace`
   (`Base` | `Opposite`), and `PartSpatialDefinition` (thickness + a
   `BTreeMap<FeatureId, HoleExtent>`). All are plain serde data types; no
   geometry, no lowering.
2. **One field on the authoritative model.** `EngineeringModel` gains exactly
   one persisted field: `pub spatial: PartSpatialDefinition`. Spatial state is
   part of the single source of truth — never derived, never cached elsewhere.
3. **Commands, not direct writes.** Two new commands enter the existing
   protocol: `SetPartThickness { source }` and `SetHoleExtent { target,
extent }`, applied by `CommandExecutor` with validate-before-write
   atomicity like every other command. `Set…None` clears; there is no
   delete-spatial command.
4. **Fail-closed readiness check.**
   `EngineeringModel::validate_spatial_readiness()` returns a deterministically
   ordered `SpatialReadiness { ready, issues }`: missing/invalid/unresolvable
   thickness; every `Hole` / `HolePattern` must declare an extent
   (`MissingHoleExtent`); blind depths must resolve and stay strictly below
   the thickness (`InvalidBlindDepth`, equality rejected); extent entries may
   only target existing hole-bearing features (`UnknownHoleExtentTarget`);
   parameters referenced by sourced lengths must resolve to finite positive
   numbers (`UnresolvableParameter`); and every `Slot` / `Fillet` present in
   the model is reported as `UnsupportedForSpatial`.
5. **Eager rejection only where both sides are literal, in both command
   directions.** The executor atomically rejects a literal blind depth ≥
   literal thickness with `SCC-ENG-032 DepthExceedsThickness` — both when
   `SetHoleExtent` writes a literal depth against a literal thickness and
   when `SetPartThickness` writes a literal thickness against any existing
   literal blind depth. Both directions call one shared helper
   (`spatial::check_blind_depth_against_thickness`), so the strict-`<` rule
   of Decision 4 has exactly one eager owner. For `SetPartThickness` the
   reported offender is the first conflicting feature in `BTreeMap` order
   (deterministic, locked by test). Parameter-mediated conflicts are _not_
   guessed at write time in either direction (a parameter can change later);
   they surface through the readiness check instead. Error precedence for
   `SetPartThickness{Literal}`: `PartMissing` → non-finite/non-positive →
   `ParameterNotFound` → `SCC-ENG-032` → write.
6. **Removal keeps the invariant.** `RemoveFeature` also deletes the removed
   feature's spatial extent entry; no dangling keys ever persist.
7. **Two structured error codes.** `SCC-ENG-031 InvalidSpatialTarget`
   (extent targeted a non-hole-bearing feature) and `SCC-ENG-032
DepthExceedsThickness`, mapped into `StructuredError` under
   `ErrorStage::Engineering`, recoverable, registered in `docs/error-codes.md`.
8. **Canonical Part Local Frame.** All spatial data is expressed in one
   canonical part-local coordinate frame, formally defined here as a **domain
   contract**:
   - **+X** follows the existing `EngineeringModel` / `GeometryModel` +X
     direction (right); **+Y** follows the existing +Y (up).
   - **+Z** is perpendicular to the primary sketch plane, pointing out of the
     material.
   - The **primary sketch plane is XY @ z = 0**, which _is_ the **Base face**
     (`HoleStartFace::Base`). The **Opposite face** lies at
     `z = part_thickness` (`HoleStartFace::Opposite`).
   - The plate solid is the `BaseProfile` extruded along **+Z** over
     `[0, thickness]`; the extrusion direction is fixed (a 2.5-D domain
     contract). A future Projection Engine may only consume this convention —
     it must never redefine it.
   - The terms **Base/Opposite** are deliberately chosen over Front/Back to
     avoid collision with future projection-view names.

### Adjudication notes (on-variant vetoes)

- _`UnsupportedForSpatial` for extent entries targeting non-hole features?_
  **Vetoed.** That variant is reserved for `Slot` / `Fillet` features present
  in the model itself. An extent entry whose target is absent or not a hole
  feature reports `UnknownHoleExtentTarget`.
- _Hand-written `Ord` on `SpatialIssue` to allow `BTreeSet`?_ **Vetoed.** The
  `f64` payload fields (`value`, `depth`) make `Eq`/`Ord` derives unsound and
  hand-written orderings risk drifting from `PartialEq`. Issues collect into a
  `Vec` sorted stably by `(discriminant, Option<&FeatureId>)`; collection order
  is already deterministic (BTree iteration), so identical inputs yield
  identical outputs.
- _Expanding hole patterns into per-hole extents?_ **Vetoed** — patterns stay
  parametric and unexpanded everywhere (existing invariant); one extent entry
  covers the whole pattern.

## Alternatives considered

- **Storing thickness/extents on each feature variant**: would require touching
  the frozen `feature.rs` shape and re-litigating every consumer; a separate
  definition keyed by feature id keeps Phase-8 data untouched.
- **A separate spatial sidecar document**: splits the single source of truth
  and invites divergence between model and spatial state during undo/redo.
- **Full readiness enforcement inside `validate_model`**: would make every
  existing model with holes invalid overnight; readiness is an opt-in gate for
  spatial consumers, not a global invariant yet.
- **Rejecting parameter-driven depths eagerly via evaluation snapshot**: gives
  false safety (parameters change after the write) and duplicates evaluator
  failures inside the executor; deferred to readiness by design. This remains
  true after the symmetric literal-vs-literal amendment: the eager path never
  invokes the evaluator.

## Compatibility

- This is the **first `#[serde(default)]` field on `EngineeringModel`**, paired
  with `skip_serializing_if = "PartSpatialDefinition::is_empty"`:
  - Phase-8 documents (no `spatial` key) deserialize unchanged;
  - any model whose spatial definition is empty serializes byte-for-byte
    identically to Phase 8 output.
- Both properties are locked by tests: an inline legacy JSON round-trip in
  `model.rs` and `tests/back_compat.rs` +
  `tests/fixtures/model_phase8_legacy.json` (×100 byte-stability loops).
- Older binaries reading a Phase-9 document will ignore the unknown `spatial`
  key (serde default behavior), so the change is forward-compatible within the
  same major document generation.

### Serialization contract

Observed and locked deserialization behaviors for the `spatial` field:

| Input                                                                         | Behavior                                                                                                 |
| ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Missing `spatial` key                                                         | Accepted; defaults to empty (locked by `tests/back_compat.rs`)                                           |
| `"spatial": null`                                                             | Rejected — serde `default` covers absence, not explicit null                                             |
| `"spatial": {}`                                                               | Rejected — `hole_extents` is required; `thickness` (`Option`) defaults when omitted                      |
| Unknown struct fields (inside `PartSpatialDefinition` and at model top level) | Ignored — no `deny_unknown_fields`, deliberate forward compatibility within the same document generation |
| Unknown enum variants (`HoleExtent` type tag, `HoleStartFace`)                | Rejected, fail-closed                                                                                    |
| JSON non-finite (NaN serializes to `null`)                                    | Rejected at deserialization (`f64` rejects null), so non-finite values cannot be persisted               |

### Non-finite value contract

Every entry path for a non-finite value is fail-closed:

| Entry path                                                              | Behavior                                                                                                                  |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Command literal write                                                   | Rejected (`NonFiniteValue` via `positive`)                                                                                |
| Parameter value write (`SetParameter` / `DeclareParameter`)             | Rejected (`ParameterValue::validate`)                                                                                     |
| Expression evaluation                                                   | Never resolves to non-finite (`evaluator::finish` rejects); resolve failures surface as readiness `UnresolvableParameter` |
| JSON non-finite / null                                                  | Deserialization rejected                                                                                                  |
| Direct in-process construction of a non-finite literal (test-only path) | Not caught eagerly; reported defensively by readiness as `InvalidThickness` / `InvalidBlindDepth`                         |

### Undo/redo and atomicity

The engineering mutation layer has no undo/redo;
`CommandExecutor::apply` / `apply_all` are validate-before-write with
clone-and-commit atomicity: a rejected command — including the symmetric
`SCC-ENG-032` check — leaves the model `PartialEq`-identical and its
serialization byte-identical. Workbench-level undo/redo is a client snapshot
concern and inherits spatial state for free (see the existing Consequences
bullet on snapshots copying the whole model including `spatial`).

### Parameter drift policy

`SetParameter` / `SetExpression` never scan or validate `spatial` — generic
parameter mutations stay spatial-agnostic.
`validate_spatial_readiness` re-derives every parameter value on each call
and caches nothing, so any parameter-mediated drift is detected at the next
readiness call regardless of mutation order.

## Consequences

- Spatial consumers gain a complete, validated vocabulary: every hole declares
  an extent, and a `ready` report guarantees thickness/depth consistency.
- Readiness is deterministic and cheap (no geometry): it can gate UI badges,
  CLI subcommands, or future lowering stages without cost concerns.
- `Slot` / `Fillet` models can never be spatial-ready until later phases give
  them semantics — intentional fail-closed pressure, not an accident.
- Undo/redo inherits correct spatial behavior for free: snapshots copy the
  whole model including `spatial`, and batch commits roll back atomically.

## Not doing (Phase 9.1 explicit non-goals)

1. No changes to `feature.rs`, `lowering.rs`, `binding.rs`, or `workbench.rs`.
2. No DSL (`scc-dsl`) syntax for thickness or extents.
3. No tool-calling (`scc-tools`) surface for the new commands.
4. No execution-layer (`scc-execution`) consumption of spatial data.
5. No drawing (`scc-drawing`) changes: no section views, no hidden lines.
6. No geometry (`scc-geometry`) changes; no solid modeling of depth.
7. No Projection or Solid type introductions anywhere.
8. No UI work: no server.mjs, public/**, workbench panel, or API route changes.
9. No golden SVG/DXF regeneration; renderer output is untouched.
10. No CadQuery/OCCT worker interaction with thickness or extents.
11. No text-metrics or annotation-layout work of any kind.
12. No expansion of hole patterns into concrete holes for spatial purposes.
13. No spatial meaning assigned to `ConstructionCircle` (reference-only).
14. No `Slot` / `Fillet` spatial semantics (reported unsupported instead).
15. No pocket, counterbore, countersink, thread, or taper extent variants.
16. No position/tolerance/finish metadata on spatial definitions.
17. No multi-body or multi-part thickness (one part, one thickness).
18. No auto-defaulting thickness when missing (readiness reports it).
19. No eager rejection of parameter-mediated depth conflicts at write time.
20. No persistence format version bump or migration machinery.
21. No workbench view fields exposing spatial rows (later slice decides).
22. No deprecation of any existing error code, command, or field.
