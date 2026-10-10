# ADR-073 — General Parametric Feature Parameter Binding and Editing

- Status: Accepted for the bounded G6A.3 Hole parameter capability
- Date: 2026-09-27
- Scope: EngineeringModel v2 feature parameter identity, edits, rebuild, derived DrawingModel callouts, and the existing formal STEP path

## Context

ADR-064–069 established the generic v2 feature history and manufacturing rebuild path. ADR-070–072 established geometry-backed sections and associative drawing dimensions. G6A.2 browser evidence found that a v2 `ParametricFeature::Hole` had no editable Workbench slots: `FeatureParameterSlot::valid_for` returned an empty set for Parametric features, while the v2 OCP lowering and Hole annotations read literal fields. The generic `set_parameter` target identified only a parameter ID and did not identify a Feature capability.

## Decision

1. Add a formal `FeatureParameterReference` containing `part_id`, `feature_id`, typed `slot`, stable `parameter_id`, and `expected_type`. Its identity is derived from the persistent part/feature IDs and slot, never from UI ordering or OCP topology. An ID collision or stale reference fails closed.
2. Keep an explicit capability schema. G6A.3 enables only v2 Hole `diameter`, `blind_depth` for a Blind termination, and `counterbore_diameter`/`counterbore_depth` for a supported Counterbore kind. These slots currently require a positive finite scalar in millimetres. Through-all Holes do not gain a blind-depth slot. Other feature slots remain unavailable.
3. Add `EngineeringCommand::SetFeatureParameter` and the registered `set_feature_parameter` Tool Calling operation. The executor resolves the full reference, checks the capability and value type, applies a candidate copy, validates all bound values and Hole/Counterbore relationships, then commits. Legacy `SetParameter` and `SetExpression` cannot bypass bound Hole checks.
4. On first edit, bind a deterministic parameter identity to the existing feature slot; later edits update that same parameter. `EngineeringModel` and its existing `feature_parameters` map remain the only editable engineering truth. No independent parameter store, arbitrary JSON setter, or second edit executor is introduced.
5. Resolve the bound value through the shared v2 feature history for OCP geometry, associative Hole/Counterbore dimension sources, and the Workbench view. The existing candidate `solve_payload` path must successfully rebuild before the API returns an updated model; otherwise the browser retains the prior model and drawing. Formal STEP continues through its existing build, serialization, reimport, and equivalence authority.
6. Expose only supported v2 Hole slots in the existing Workbench. The selected Feature and millimetre unit are visible; edits go through the formal tool route, and the drawing is regenerated from the returned candidate model. Chinese guidance preserves original machine error codes, including the distinction between `SCC-TOOL-005` input-schema refusal, `SCC-TOOL-010` command-application envelope, `SCC-ENG-028` engineering binding validation, and nested OCP rebuild diagnostics.
7. Do not change Project v1 serialization, the Project v2 schema version, geometry Feature kinds, DrawingModel authority, or STEP authority. General Extrude, Fillet, Chamfer, Revolve, Pattern, and Sketch parameter editing remains out of scope.

## Consequences

- v2 Hole diameter and applicable Blind depth can be edited against stable typed targets without recreating the Feature. Supported Counterbore slots use the same capability and consumer resolution path.
- A failed command or candidate rebuild returns no updated EngineeringModel. The Workbench restores the last committed values and retains its prior DrawingModel image; later valid edits remain possible.
- The verified end-to-end case covers a 120 × 90 × 20 mm plate and a blind Hole. Counterbore parameter editing is in the bounded schema but was not exercised in the real-browser fixture.
- G6A.2's deferred general spacing, generic blind-cut depth, and Fillet radius associative dimensions remain deferred. This decision does not claim universal parameter editing or complete complex-part support.

## Verification

See [Phase G6A.3 acceptance report](../testing/phase-g6a3-parametric-binding/acceptance-report.md) and the real Chromium screenshot at `../testing/phase-g6a3-parametric-binding/g6a3-browser-acceptance.png`. Real DeepSeek remains `NOT_RUN`; no API key was accessed.
