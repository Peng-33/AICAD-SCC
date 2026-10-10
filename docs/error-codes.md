# SCC Error Code Registry

This file is the single source of truth for stable `SCC-{STAGE}-{NNN}` error codes used across the SCC workspace. Downstream tooling should branch on `code`, never on the human-readable `message` text.

## Format

- `SCC-PROP-NNN` — Proposal parsing and static validation
- `SCC-ANAL-NNN` — Semantic analysis
- `SCC-LOW-NNN` — Lowering from Semantic IR to Constraint IR
- `SCC-EXEC-NNN` — Execution, runtime, mutation, and GraphState validation
- `SCC-GEOM-NNN` — M3 geometry validation, construction, and solving
- `SCC-CLI-NNN` — CLI, I/O, atomic writes, and schema envelope handling
- `SCC-ENG-NNN` — Engineering validation, spatial intent, and source map
- `SCC-DSL-NNN` — DSL surface parsing and checks
- `SCC-TOOL-NNN` — Tool / workbench command layer
- `SCC-DRW-NNN` — Drawing rendering and sheet layout
- `SCC-SPAT-NNN` — 2.5-D spatial lowering
- `SCC-PROJ-NNN` — Orthographic projection model
- SCC-VIS-NNN — Hidden-line visibility classification

Codes are assigned sequentially within each stage. Do not reuse or duplicate a code.

## Proposal errors (`SCC-PROP-*`)

| Code           | Stage variant          | Meaning                                                                                               |
| -------------- | ---------------------- | ----------------------------------------------------------------------------------------------------- |
| `SCC-PROP-001` | `InvalidJson`          | JSON parsing or deserialization failure.                                                              |
| `SCC-PROP-002` | `MissingRequiredField` | A required field is missing or empty.                                                                 |
| `SCC-PROP-003` | _(reserved)_           | Input exceeds size limit. **Reserved but not implemented** until an input-size threshold is approved. |
| `SCC-PROP-004` | `EmptyVersion`         | `Proposal.version` is empty.                                                                          |
| `SCC-PROP-005` | `EmptyEntityId`        | An entity ID is empty.                                                                                |
| `SCC-PROP-006` | `DuplicateEntityId`    | An entity ID appears more than once.                                                                  |
| `SCC-PROP-007` | `MissingDistanceValue` | A `distance` relation is missing its `value`.                                                         |
| `SCC-PROP-008` | `NonFiniteValue`       | A numeric value is `NaN`, `+inf`, or `-inf`.                                                          |
| `SCC-PROP-009` | _(internal)_           | An internal proposal validation invariant failed (empty diagnostic set).                              |

## Analysis errors (`SCC-ANAL-*`)

| Code           | Stage variant             | Meaning                                                                |
| -------------- | ------------------------- | ---------------------------------------------------------------------- |
| `SCC-ANAL-001` | `MissingEntityReference`  | A relation or dimension references a non-existent entity.              |
| `SCC-ANAL-002` | `TypeMismatch`            | Entities are used in a relation that is incompatible with their kinds. |
| `SCC-ANAL-003` | `CircularDependency`      | The declaration graph contains a cycle.                                |
| `SCC-ANAL-004` | `InvalidDeclarationGraph` | The built declaration graph is structurally invalid.                   |
| `SCC-ANAL-005` | `InvalidArrayReference`   | An array references a non-existent element entity.                     |
| `SCC-ANAL-006` | `InvalidAnchorReference`  | An anchor references a non-existent entity.                            |
| `SCC-ANAL-007` | _(internal)_              | An internal analysis invariant failed (empty diagnostic set).          |

## Lowering errors (`SCC-LOW-*`)

| Code          | Stage variant                | Meaning                                                  |
| ------------- | ---------------------------- | -------------------------------------------------------- |
| `SCC-LOW-001` | `UnsupportedEntityKind`      | An entity kind cannot be lowered.                        |
| `SCC-LOW-002` | `UnsupportedRelation`        | A relation kind cannot be lowered.                       |
| `SCC-LOW-003` | `UnsupportedDimension`       | A dimension kind cannot be lowered.                      |
| `SCC-LOW-004` | `InvalidEntityReference`     | A lowered constraint references a missing entity.        |
| `SCC-LOW-005` | `ConstraintIdCollision`      | Two constraints would receive the same deterministic ID. |
| `SCC-LOW-006` | `NonFiniteParameter`         | A numeric parameter in a constraint is not finite.       |
| `SCC-LOW-007` | `InvalidAuthorityAssignment` | A constraint has an inconsistent authority level.        |
| `SCC-LOW-008` | `AnchorLoweringFailure`      | An anchor could not be lowered.                          |

## Execution / runtime errors (`SCC-EXEC-*`)

| Code           | Stage variant            | Meaning                                                                                   |
| -------------- | ------------------------ | ----------------------------------------------------------------------------------------- |
| `SCC-EXEC-001` | `MissingEntity`          | An entity, runtime lookup target, or mutation target was not found in the GraphState.     |
| `SCC-EXEC-002` | _(mutation/runtime)_     | A mutation or runtime overlay was rejected because the target is a frozen L0 constraint.  |
| `SCC-EXEC-003` | `OrphanEntity`           | `validate_final` detected an entity not referenced by any constraint.                     |
| `SCC-EXEC-004` | `MissingConstraint`      | A reference points to a constraint that does not exist.                                   |
| `SCC-EXEC-005` | _(runtime)_              | An overlay target (entity or constraint) was not found in the GraphState.                 |
| `SCC-EXEC-006` | _(runtime)_              | The declaration graph contains a cycle and cannot be topologically ordered.               |
| `SCC-EXEC-007` | _(runtime)_              | An execution conflict was detected while applying a runtime command.                      |
| `SCC-EXEC-008` | _(reserved)_             | Reserved for future trace-generation failures; not currently emitted.                     |
| `SCC-EXEC-009` | _(runtime)_              | A runtime validation check failed.                                                        |
| `SCC-EXEC-010` | _(mutation)_             | A mutation validation check failed.                                                       |
| `SCC-EXEC-011` | _(mutation/runtime)_     | Cannot promote a constraint to L0 authority.                                              |
| `SCC-EXEC-012` | _(mutation)_             | Negative parameter value is not allowed.                                                  |
| `SCC-EXEC-013` | `CanonicalizationFailed` | Base IR or overlay serialization failed while computing the deterministic execution root. |

## Geometry errors (`SCC-GEOM-*`)

| Code           | Stage variant           | Meaning                                                                                                     |
| -------------- | ----------------------- | ----------------------------------------------------------------------------------------------------------- |
| `SCC-GEOM-001` | `InvalidGraph`          | GraphState failed structural validation or contains a circular geometry dependency.                         |
| `SCC-GEOM-002` | `MissingParameter`      | An entity lacks a required geometry parameter.                                                              |
| `SCC-GEOM-003` | `InvalidParameter`      | A geometry parameter is non-finite, non-positive where required, or otherwise invalid.                      |
| `SCC-GEOM-004` | `UnsupportedEntityKind` | The current solver does not implement the entity kind.                                                      |
| `SCC-GEOM-005` | `UnsupportedConstraint` | The current solver does not implement an active constraint kind or supported axis selection.                |
| `SCC-GEOM-006` | `ConflictingParameters` | Multiple parameters or positioning constraints describe incompatible geometry.                              |
| `SCC-GEOM-007` | `Underconstrained`      | A supported constraint component retains unresolved positional degrees of freedom and cannot emit geometry. |

## CLI errors (`SCC-CLI-*`)

| Code          | Meaning                            |
| ------------- | ---------------------------------- |
| `SCC-CLI-001` | `AlreadyExists`                    | Output file already exists (use `--force` to overwrite).                                                                   |
| `SCC-CLI-002` | `Io` / `Serialization`             | Input file not found or unreadable, or JSON serialization/deserialization failed.                                          |
| `SCC-CLI-003` | `AtomicWriteFailed`                | Atomic write failed, including rollback information for paired outputs.                                                    |
| `SCC-CLI-004` | `DuplicateOutputPath`              | Trace and state output paths must not be the same.                                                                         |
| `SCC-CLI-005` | `EnvelopeDocumentTypeMismatch`     | Envelope `document_type` does not match the expected payload type.                                                         |
| `SCC-CLI-006` | `EnvelopeUnsupportedSchemaVersion` | Envelope schema major version is not supported.                                                                            |
| `SCC-CLI-007` | `EnvelopeMissingPayload`           | Envelope is missing `payload`.                                                                                             |
| `SCC-CLI-008` | `EnvelopePayloadDeserialize`       | Envelope metadata is malformed or `payload` failed to deserialize as the expected type.                                    |
| `SCC-CLI-009` | `ConflictReport`                   | A business conflict report was produced (used when normalizing legacy `Conflict` errors and internal contract violations). |
| `SCC-CLI-010` | `ArgumentParseFailed`              | CLI arguments do not satisfy the command contract.                                                                         |

## Engineering errors (`SCC-ENG-*`)

The `engineering` stage is emitted by the AICAD engineering command layer
(`crates/scc-engineering`). Phase 1 (`001`..`008`) validates a single
`EngineeringCommand` in isolation. Phase 2 (`009`..`024`) reports model-level
failures raised while applying a command to an `EngineeringModel` or while
validating / evaluating the model. Geometric / cross-feature conflicts (hole
inside plate, overconstraint) belong to a later semantic/lowering stage.

| Code          | Stage variant                    | Meaning                                                                                                                                                                                     |
| ------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SCC-ENG-001` | `EmptyId`                        | An engineering id (part, feature, parameter, constraint) is empty.                                                                                                                          |
| `SCC-ENG-002` | `InvalidId`                      | An engineering id violates the deterministic rule `[a-z][a-z0-9_-]{0,63}`.                                                                                                                  |
| `SCC-ENG-003` | `EmptyName`                      | A name is empty or has surrounding whitespace.                                                                                                                                              |
| `SCC-ENG-004` | `NonFiniteValue`                 | A numeric value (or an expression result after overflow) is `NaN`, `+inf`, or `-inf`.                                                                                                       |
| `SCC-ENG-005` | `NonPositiveLength`              | A dimension, diameter, radius, or scale must be strictly positive.                                                                                                                          |
| `SCC-ENG-006` | `ZeroCount`                      | A hole-pattern count (or grid rows/columns) is zero.                                                                                                                                        |
| `SCC-ENG-007` | `DivisionByConstantZero`         | An expression divides by a constant zero.                                                                                                                                                   |
| `SCC-ENG-008` | `EmptyChoice`                    | An enumerated parameter choice value is empty.                                                                                                                                              |
| `SCC-ENG-009` | `DuplicatePart`                  | `CreatePart` applied while a part already exists (never silently replaced).                                                                                                                 |
| `SCC-ENG-010` | `PartMissing`                    | A command requires a part that does not exist yet.                                                                                                                                          |
| `SCC-ENG-011` | `DuplicateFeature`               | A feature with the same id already exists.                                                                                                                                                  |
| `SCC-ENG-012` | `FeatureNotFound`                | A referenced feature does not exist.                                                                                                                                                        |
| `SCC-ENG-013` | `FeatureInUse`                   | `RemoveFeature` rejected because a constraint references the feature.                                                                                                                       |
| `SCC-ENG-014` | `DuplicateConstraint`            | A constraint with the same id already exists.                                                                                                                                               |
| `SCC-ENG-015` | `ConstraintNotFound`             | A constraint to remove does not exist.                                                                                                                                                      |
| `SCC-ENG-016` | `DuplicateParameter`             | A parameter with the same id already exists.                                                                                                                                                |
| `SCC-ENG-017` | `ParameterNotFound`              | A parameter does not exist.                                                                                                                                                                 |
| `SCC-ENG-018` | `DanglingReference`              | A model reference (constraint feature or feature→parameter binding) points to a missing target.                                                                                             |
| `SCC-ENG-019` | `ExpressionReferenceMissing`     | An expression references a parameter that does not exist.                                                                                                                                   |
| `SCC-ENG-020` | `ExpressionCycle`                | The parameter dependency graph contains a cycle.                                                                                                                                            |
| `SCC-ENG-021` | `ExpressionDivideByZero`         | An expression evaluates to division by zero (resolved divisor is zero).                                                                                                                     |
| `SCC-ENG-022` | `ExpressionTypeMismatch`         | A boolean or choice value is used in arithmetic.                                                                                                                                            |
| `SCC-ENG-023` | `UnsupportedMove`                | `MoveFeature` targets a feature without a movable position/center.                                                                                                                          |
| `SCC-ENG-024` | `ModelValidationFailed`          | `validate_model` found an inconsistency (e.g. missing part, key/id mismatch).                                                                                                               |
| `SCC-ENG-025` | `UnsupportedFeatureLowering`     | A feature (e.g. `Slot`, `Fillet`) cannot be lowered; fail closed, never ignored.                                                                                                            |
| `SCC-ENG-026` | `UnsupportedConstraintLowering`  | A constraint (e.g. `Concentric`) has no correct SCC mapping; fail closed.                                                                                                                   |
| `SCC-ENG-027` | `UnresolvedRequiredParameter`    | A bound parameter required for lowering is `Unresolved` (feature + slot + parameter reported).                                                                                              |
| `SCC-ENG-028` | `InvalidFeatureParameterBinding` | A feature→slot→parameter binding is invalid (e.g. slot not valid for the feature type).                                                                                                     |
| `SCC-ENG-029` | `MissingFeatureParameterBinding` | A required slot has no binding and no literal.                                                                                                                                              |
| `SCC-ENG-030` | `LoweringFailed`                 | Engineering lowering failed for another reason (e.g. corner pattern without a base profile).                                                                                                |
| `SCC-ENG-031` | `InvalidSpatialTarget`           | A spatial extent command targeted a feature that is not a hole-bearing feature (`Hole` or `HolePattern`); fail-closed.                                                                      |
| `SCC-ENG-032` | `DepthExceedsThickness`          | A literal blind depth reaches or exceeds the literal part thickness; the command is rejected atomically (parameter-mediated conflicts are caught by the Phase 9.1 readiness check instead). |
| `SCC-ENG-033` | `InvalidDimensionalSpecification` | A typed tolerance/precision target or value is invalid, non-canonical, unsupported for the target, or violates a ThroughAll/quantity boundary. |

## DSL errors (`SCC-DSL-*`)

The `dsl` stage is emitted by the AICAD Engineering DSL (`crates/scc-dsl`,
Phase 4): lexing, parsing, and command building. DSL errors carry a
line/column position and never panic.

| Code          | Stage variant               | Meaning                                                                |
| ------------- | --------------------------- | ---------------------------------------------------------------------- |
| `SCC-DSL-001` | `UnexpectedToken`           | A token was found where another was expected.                          |
| `SCC-DSL-002` | `UnexpectedEof`             | The document ended before a statement completed.                       |
| `SCC-DSL-003` | `UnknownKeyword`            | An uppercase token is not a known DSL keyword.                         |
| `SCC-DSL-004` | `InvalidIdentifier`         | An identifier violates `[a-z][a-z0-9_-]{0,63}`.                        |
| `SCC-DSL-005` | `InvalidNumber`             | A number token is malformed or non-finite.                             |
| `SCC-DSL-006` | `DuplicateField`            | A field is repeated in one feature block (silent overwrite forbidden). |
| `SCC-DSL-007` | `MissingRequiredField`      | A required feature field is absent.                                    |
| `SCC-DSL-008` | `InvalidFeatureField`       | A field is not valid for the feature type in v1.                       |
| `SCC-DSL-009` | `UnsupportedDslFeature`     | A known-but-disabled v1 keyword (FILLET, SLOT, CONCENTRIC).            |
| `SCC-DSL-010` | `UnsupportedVersion`        | The document version is not 1.                                         |
| `SCC-DSL-011` | `UnsupportedUnit`           | A unit other than `mm` was requested.                                  |
| `SCC-DSL-012` | `InvalidExpression`         | An expression cannot be parsed.                                        |
| `SCC-DSL-013` | `UnknownParameterReference` | An expression or BIND references an undeclared parameter.              |
| `SCC-DSL-014` | `InvalidBinding`            | A BIND / BASE / constraint feature reference is invalid or undeclared. |
| `SCC-DSL-015` | `CommandBuildFailed`        | A command could not be built (e.g. non-integer count).                 |
| `SCC-DSL-016` | `UnterminatedString`        | A quoted string is not closed.                                         |
| `SCC-DSL-017` | `MissingHeader`             | The document has no `AICAD 1` (or `VERSION 1`) header.                 |

## Tool errors (`SCC-TOOL-*`)

The `tool` stage is emitted by the provider-neutral tool layer
(`crates/scc-tools`, Phase 5): tool-call validation and adaptation. Tool errors
report the tool name and the offending argument; engineering/SCC errors are
preserved (as the structured error `source` chain) rather than flattened.

| Code           | Stage variant              | Meaning                                                                                                                                          |
| -------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `SCC-TOOL-001` | `UnknownTool`              | The tool name is not registered.                                                                                                                 |
| `SCC-TOOL-002` | `MissingArgument`          | A required tool argument is absent.                                                                                                              |
| `SCC-TOOL-003` | `UnexpectedArgument`       | An argument is not in the tool's schema (never ignored).                                                                                         |
| `SCC-TOOL-004` | `InvalidArgumentType`      | An argument value has the wrong type (e.g. `count = 8.5`).                                                                                       |
| `SCC-TOOL-005` | `InvalidArgumentValue`     | A value violates schema constraints (e.g. negative diameter, `count = 0`, NaN/Inf).                                                              |
| `SCC-TOOL-006` | `DuplicateArgument`        | A duplicate argument name (structurally prevented by the map, rejected on pair construction).                                                    |
| `SCC-TOOL-007` | `UnsupportedTool`          | A known-but-disabled tool (e.g. duplicate registration).                                                                                         |
| `SCC-TOOL-008` | `AdapterFailed`            | The tool adapter could not produce commands.                                                                                                     |
| `SCC-TOOL-009` | `ExpressionParseFailed`    | A `set_expression` string failed the DSL expression parser (never eval).                                                                         |
| `SCC-TOOL-010` | `CommandApplicationFailed` | `apply_all` rejected the adapted commands; the original engineering error is preserved as the source.                                            |
| `SCC-ANAL-008` | `SemanticConflict`         | A lowered model produced business-level semantic conflicts (reported by the engineering intent bridge when DSL/tools yield a conflicting model). |

## Drawing errors (`SCC-DRW-*`)

Phase 8 `scc-drawing` converts a solved `GeometryModel` into a derived,
read-only `DrawingModel`. These errors are additive; the legacy
proposal→geometry pipeline never emits them. `DrawingError` → `StructuredError`
with `ErrorStage::Drawing`.

| Code          | Variant                        | Meaning                                                                                                                                                                     |
| ------------- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SCC-DRW-001` | `UnsupportedGeometryPrimitive` | A solved geometry primitive has no drawing representation (fail-closed; reserved for future geometry primitives — all current kinds are supported losslessly).              |
| `SCC-DRW-002` | `InvalidGeometryValue`         | A solved geometry value is not a valid drawing value (e.g. non-finite/degenerate), never silently dropped.                                                                  |
| `SCC-DRW-003` | `MissingSourceMapping`         | A solved geometry entity has no engineering feature mapping (fail-closed; never silently treated as synthetic).                                                             |
| `SCC-DRW-004` | `InvalidDrawingScale`          | The drawing scale is not finite and strictly positive.                                                                                                                      |
| `SCC-DRW-005` | `EmptyDrawing`                 | The geometry model has no solved entities; no meaningful sheet can be built.                                                                                                |
| `SCC-DRW-006` | `InvalidSourceIntegrity`       | A source-integrity validation failed: an orphan primitive, an entity missing from geometry, a feature missing from the engineering model, or an inconsistent source map.    |
| `SCC-DRW-007` | `InvalidDrawingTransform`      | A view transform is not a valid deterministic placement (non-finite / non-positive scale, or non-finite translation).                                                       |
| `SCC-DRW-008` | `InvalidDrawingAnnotation`     | A drawing annotation is invalid (control characters in resolved text, non-finite position, or non-positive text height), or fails annotation integrity validation.          |
| `SCC-DRW-009` | `InvalidDrawingDimension`      | A drawing dimension is invalid: kind/value mismatch, a source that does not re-resolve to its cached value, a slot not valid for its feature, or a missing feature/literal. |
| `SCC-DRW-010` | `UnresolvedDimensionValue`     | A dimension's bound parameter is unresolved; never falls back to a feature literal or zero (defensive gate — lowering normally fails earlier with `SCC-ENG-027`).           |
| `SCC-DRW-011` | `InvalidDimensionTarget`       | A dimension targets an engineering feature with no represented primitive in the current drawing presentation (fail-closed).                                                 |
| `SCC-DRW-012` | `InvalidDimensionLayout`       | A dimension layout is invalid: orphan layout (references an unknown dimension), non-finite coordinate, or degenerate dimension line.                                        |
| `SCC-DRW-013` | `MissingDimensionLayout`       | A dimension has no layout; every dimension must be laid out exactly once.                                                                                                   |
| `SCC-DRW-014` | `InvalidDimensionWitness`      | A dimension's engineering value does not match its solved geometry witness — the two truth chains disagree (fail-closed).                                                   |
| `SCC-DRW-015` | `InvalidCenterGeometry`        | A center mark/centerline is invalid: non-finite coordinates, degenerate segment, non-cross mark, or a source feature that is not a circular hole feature.                   |
| `SCC-DRW-016` | `MissingCenterSource`          | A center element has no resolvable provenance/feature, or its sourced feature has no represented circle in the presentation.                                                |
| `SCC-DRW-017` | `InvalidSvgDrawing`            | The SVG renderer rejected the drawing: missing sheet/view, invalid annotation, non-finite bounds, or a non-finite coordinate reached emission.                              |
| `SCC-DRW-018` | `InvalidDxfDrawing`            | The DXF renderer rejected the drawing: same gates as SVG plus entity-outside-sheet-bounds containment; also wraps shared low-level DXF writer failures.                     |
| `SCC-DRW-019` | `MissingProjectionMethod`      | The formal three-view builder has no explicit FirstAngle/ThirdAngle drawing property.                                                                                       |
| `SCC-DRW-020` | `UnsupportedProjectedGeometry` | A projection primitive is not representable by the additive Segment/Circle drawing adapter.                                                                                |
| `SCC-DRW-021` | `InvalidThreeViewLayout`       | A formal view placement or scaled sheet bound is non-finite or structurally invalid.                                                                                       |
| `SCC-DRW-022` | `MissingProjectionSource`      | A formal drawing primitive has no matching opaque ProjectionPrimitive provenance, or the projected source is orphaned.                                                     |
| `SCC-DRW-023` | `InvalidVisibilityMapping`     | A projected primitive has no corresponding VisibilityModel decision.                                                                                                       |
| `SCC-DRW-024` | `SectionParametricRebuildRequired` | Formal Section A-A was requested for a model that is not an EngineeringModel v2 parametric history. Fail closed. The semantic cause `SECTION_PARAMETRIC_REBUILD_REQUIRED` is in `details.parametric_code`. |
| `SCC-DRW-025` | `SectionGeometryMissing`       | A parametric section rebuild returned no section geometry. Fail closed. The semantic cause `SECTION_GEOMETRY_MISSING` is in `details.parametric_code`.                     |

## Spatial lowering errors (`SCC-SPAT-*`)

Phase 9.2 `scc-spatial` lowers a spatial-ready `EngineeringModel`, the solved
`GeometryModel`, and the `EngineeringSourceMap` into a derived, read-only
2.5-D `SpatialModel` (ADR-033). These errors are additive; the engineering,
geometry, and drawing stages never emit them. `SpatialError` →
`StructuredError` with `ErrorStage::Spatial`; every variant is fail-closed (`recoverable: false`) —
no spatial model is ever partially assembled.

| Code           | Variant                                | Meaning                                                                                                                                                                                      |
| -------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SCC-SPAT-001` | `EngineeringNotSpatiallyReady`         | The Phase 9.1 readiness gate rejected the model; the deterministically ordered `SpatialIssue` list is carried verbatim (single-owner gate, never re-implemented in the lowering).            |
| `SCC-SPAT-002` | `MissingBaseRepresentation`            | No `BaseProfile` feature exists, so the spatial body has no representation.                                                                                                                  |
| `SCC-SPAT-003` | `InvalidBaseRepresentation`            | The base profile cannot become the spatial body: no solved entity, more than one solved entity, solved primitive not a rectangle, non-finite, or non-positive footprint.                     |
| `SCC-SPAT-004` | `MissingSpatialSource`                 | A hole has no mapped solved geometry entity (fail-closed; never treated as synthetic).                                                                                                       |
| `SCC-SPAT-005` | `InvalidSpatialSource`                 | A mapped entity cannot be a spatial source: absent from the solved geometry, or its inverse provenance resolves to a different engineering feature.                                          |
| `SCC-SPAT-006` | `InvalidSpatialGeometry`               | A solved primitive is not a valid spatial cut: wrong circle kind/center/radius, or a through slot that leaves or intersects the body outline.                                                |
| `SCC-SPAT-007` | `SpatialWitnessMismatch`               | The engineering diameter and the solved geometry witness (`2 × radius`) disagree beyond the 1 nm tolerance — the two truth chains disagree (fail-closed).                                    |
| `SCC-SPAT-008` | `PatternCardinalityMismatch`           | A hole-bearing feature declares a different instance count (count, rows × columns, or 4 corners) than the number of solved circles.                                                          |
| `SCC-SPAT-009` | `UnsupportedSpatialFeature`            | A feature kind (`Fillet`) has no spatial meaning in this phase; reaching the lowering is defensive (the readiness gate normally reports it first). Slot is a supported through cut.           |
| `SCC-SPAT-010` | `InvalidBlindDepthDefensive`           | A blind depth reached or exceeded the plate thickness while lowering; never silently converted to through-all.                                                                               |
| `SCC-SPAT-011` | `InvalidThicknessDefensive`            | The plate thickness is missing or unresolvable while lowering (defensive — readiness normally rejects earlier).                                                                              |
| `SCC-SPAT-012` | `InvalidBlindDepthResolutionDefensive` | A blind depth cannot be resolved to a finite positive length while lowering (defensive — readiness normally rejects earlier).                                                                |
| `SCC-SPAT-013` | `InvalidSpatialInterval`               | A Z interval is invalid: non-finite, empty (`max ≤ min`), or starting below the base plane.                                                                                                  |
| `SCC-SPAT-014` | `InvalidSpatialBounds`                 | A 3-D bounds envelope is invalid: non-finite or non-positive axis extent.                                                                                                                    |
| `SCC-SPAT-015` | `UnmappedGeometryEntity`               | A solved geometry entity is claimed by no spatial element (body, cut, or construction circle) — fail-closed claim integrity.                                                                 |
| `SCC-SPAT-016` | `InvalidHoleDiameterDefensive`         | A hole-bearing feature's diameter (literal or bound) cannot be resolved to a finite positive length while lowering (defensive — the readiness gate checks extents/thickness, not diameters). |

## Projection errors (`SCC-PROJ-*`)

Phase 9.3 `scc-projection` derives an orthographic projection model from a
`SpatialModel` (ADR-034). These errors are additive; the engineering,
geometry, drawing, and spatial stages never emit them. `ProjectionError` →
`StructuredError` with `ErrorStage::Projection`; every variant is fail-closed
(`recoverable: false`) — no projection model is ever partially assembled or
silently repaired.

| Code           | Variant                        | Meaning                                                                                                                                          |
| -------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `SCC-PROJ-001` | `MissingProjectionView`        | A primitive was staged for a view that was never added (`add_view` first); the first failing builder gate.                                        |
| `SCC-PROJ-002` | `InvalidProjectionGeometry`    | A projected primitive is geometrically invalid: non-finite coordinate, zero-length segment (exact `==`), or non-positive radius.                   |
| `SCC-PROJ-003` | `InvalidProjectionDepthRange`  | A depth interval is non-finite or `min > max`; `min == max` (edge-on face) is legal and never rejected.                                            |
| `SCC-PROJ-004` | `InvalidProjectionBounds`      | A view bounds envelope is invalid (non-finite or inverted on an axis).                                                                             |
| `SCC-PROJ-005` | `InvalidProjectionSource`      | A primitive references a spatial element (`Cut` id) absent from the referenced `SpatialModel` — fail-closed source integrity, never a synthetic fallback. |
| `SCC-PROJ-006` | `DuplicateProjectionPrimitive` | A caller-supplied primitive id collides with an existing primitive in the model (including across views).                                      |
| `SCC-PROJ-007` | `InvalidSpatialModel`         | The Phase 9.4 input `SpatialModel` is malformed or internally inconsistent; projection fails before emitting a partial model.                    |

## Visibility errors (SCC-VIS-*)

Phase 9.5 scc-visibility classifies each frozen projection candidate as
Visible, Hidden, or Occluded. These errors are additive; this layer accepts
only SpatialModel + ProjectionModel, fails closed for unsupported topology, and
never mutates projection geometry. VisibilityError converts to StructuredError
with ErrorStage::Visibility; every variant is recoverable: false.

| Code          | Variant                              | Meaning                                                                                                      |
| ------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| SCC-VIS-001   | InvalidVisibilityInput / ProjectionInput | A spatial/projection model, role/source relation, primitive shape, or numeric comparison is invalid.        |
| SCC-VIS-002   | UnsupportedVisibilityConfiguration | The analytic MVP cannot classify the topology without clipping/splitting/Boolean (outside, tangent, overlap, or unsupported Z topology). |
| SCC-VIS-003   | AmbiguousCoincidentCandidates      | Coincident raw candidates have no unique nearest depth winner.                                             |
| SCC-VIS-004   | MissingVisibilityDecision          | A visibility sidecar is missing a decision for a projection primitive.                                      |
| SCC-VIS-005   | InvalidVisibilityDecision          | A sidecar has the wrong view/decision coverage or an orphan decision.                                      |

## Intent UI errors (`SCC-UI-INTENT-*`)

Node-side intent-orchestrator errors (Phase 6) continue the existing
`SCC-UI-INTENT-NNN` series from `apps/scc-ui/lib/intent-service.mjs`:

| Code                  | Meaning                                                                             |
| --------------------- | ----------------------------------------------------------------------------------- |
| `SCC-UI-INTENT-008`   | Intent mode or text is malformed / required text missing.                           |
| `SCC-UI-INTENT-009`   | Intent text exceeds the input size limit.                                           |
| `SCC-UI-INTENT-010`   | Wrong output mode (new design produced tool calls, or edit produced free text/DSL). |
| `SCC-UI-INTENT-011`   | Too many tool calls in one edit request (limit 32).                                 |
| `SCC-UI-INTENT-012`   | DSL/tool output exceeds the size limit.                                             |
| `SCC-UI-INTENT-013`   | Intent output remained invalid after the single allowed repair.                     |
| `SCC-UI-PROVIDER-001` | The intent provider could not be reached or returned a non-OK HTTP status.          |
| `SCC-UI-PROVIDER-002` | The intent provider returned unparseable/malformed output.                          |
| `SCC-UI-INTENT-015`   | Phase 10.5 product preview requires an existing EngineeringModel edit.              |
| `SCC-UI-INTENT-CLARIFICATION-001` | Clarification answers are not a typed object.                              |
| `SCC-UI-INTENT-CLARIFICATION-002` | A clarification continuation contains no answers.                           |
| `SCC-UI-INTENT-STALE-001` | The model fingerprint changed after the intent or preview was created.             |
| `SCC-UI-INTENT-PREVIEW-001` | The proposal is malformed or contains non-formal commands.                         |
| `SCC-UI-INTENT-PREVIEW-002` | Confirmation did not reproduce the preview candidate fingerprint.                  |

## Adding or changing codes

1. Update this registry first.
2. Assign the next unused number in the relevant stage.
3. Add a corresponding variant and `From` conversion in the crate that owns the error.
4. Add a unit test that asserts the code passes `StructuredError::validate_contract()`.
5. Update ADR-002 and the changelog if the change affects compatibility.

## Projection consistency errors (`SCC-PCV-*`)

Phase 9.6 `scc-projection-consistency` validates the frozen SpatialModel,
ProjectionModel, and VisibilityModel boundary. It is fail-closed and emits
`ErrorStage::ProjectionConsistency`; it never re-projects or re-classifies.

| Code | Meaning |
| --- | --- |
| `SCC-PCV-001` | An input SpatialModel, ProjectionModel, or VisibilityModel is structurally invalid. |
| `SCC-PCV-002` | The input uses topology outside the frozen Phase 9.5 supported domain. |
| `SCC-PCV-003` | Required principal view or primitive/decision coverage is missing or extra. |
| `SCC-PCV-004` | Source-map provenance or body/cut role does not agree with the source model. |
| `SCC-PCV-005` | Stored projected geometry does not match the source-derived canonical geometry. |
| `SCC-PCV-006` | Stored projected depth does not match the source-derived depth evidence. |
| `SCC-PCV-007` | A VisibilityModel class violates the frozen coincidence/occlusion invariant. |
| `SCC-PCV-008` | A non-finite or numerically unsafe state was encountered. |


## Manufacturing errors (`SCC-MFG-*`)

| Code | Variant | Meaning |
| --- | --- | --- |
| `SCC-MFG-001` | `InvalidManufacturingInput` | The resolved SpatialModel or SolidBuildSpec structure is invalid. |
| `SCC-MFG-002` | `UnsupportedManufacturingFeature` | A spatial representation is outside the Phase 10.1 manufacturing vocabulary. |
| `SCC-MFG-003` | `InvalidManufacturingSource` | Typed body/cut provenance is missing, duplicated, or inconsistent. |
| `SCC-MFG-004` | `InvalidManufacturingGeometry` | A body, bound, cut, interval, or numeric value is invalid or non-finite. |

All Phase 10.1 manufacturing errors are non-recoverable and use `ErrorStage::Manufacturing`.


## CadQuery backend protocol errors

The Phase 10.2 stateless backend uses a JSON error envelope with `stage: cad_backend`.
These codes are transport/backend-specific and do not add a Rust `ErrorStage`:

- `CAD-BACKEND-001` — schema version missing or unsupported
- `CAD-BACKEND-002` — unsupported body, operation, or axis variant
- `CAD-BACKEND-003` — body bounds do not match the resolved footprint
- `CAD-BACKEND-004` — required field, numeric, interval, or JSON validation failure
- `CAD-BACKEND-005` — source-map or opaque operation identity failure
- `CAD-BACKEND-006` — CadQuery/OCP operation failure
- `CAD-BACKEND-007` — resulting solid validity or geometry-witness failure

## Phase 10.3 STEP delivery

| Code | Meaning |
|---|---|
| `CAD-STEP-001` | Source SolidBuildSpec/CadQuery build or source-solid validation failed. |
| `CAD-STEP-002` | STEP export, artifact inspection, or private artifact I/O failed. |
| `CAD-STEP-003` | STEP reimport or imported-solid validation failed. |
| `CAD-STEP-004` | Source/imported bbox, volume, or material witness equivalence failed. |
| `CAD-STEP-005` | Unexpected STEP delivery protocol failure; details are sanitized and bounded. |

| `SCC-CLI-012` | Project persistence validation | `.aicad.json` source project is invalid or has an unsupported schema. |
| `SCC-CLI-013` | Validated STEP backend | The configured CadQuery/reimport backend failed or returned mismatched artifact identity. |
| `SCC-UI-STEP-002` | STEP byte identity mismatch | Worker metadata does not match the bytes returned to the UI adapter. |
| `SCC-UI-STEP-003` | STEP artifact response invalid | The HTTP delivery boundary rejected empty or mismatched artifact bytes. |
