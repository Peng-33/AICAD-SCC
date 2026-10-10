# ADR-014: Generic FeatureGraph replaces part-family-only CAD execution

- Status: Accepted
- Date: 2026-08-10

ADR-013 proved the safe data-only Intent → CadSpec → OCCT path with one gearbox family, but the resulting interface duplicated the complete part template in validation, prompting, geometry, UI, and tests. AICAD will keep that family adapter for compatibility while making `feature_graph_v1` the default general CadDocument: stable ordered features reference named driving parameters, every requirement has an explicit coverage disposition, and one deterministic KernelAdapter rebuilds unrelated part classes through the same interface. Model-generated executable code remains forbidden.

## Consequences

- New part classes should be expressed as FeatureGraphs before adding a family-specific adapter.
- A successful B-Rep does not imply complete design intent; RequirementCoverage is separately reported.
- Unsupported feature types are rejected or explicitly represented as unsupported requirements, never silently ignored.
- The first generic slice supports additive/cutting boxes and cylinders plus circular hole patterns; sketches, revolves, shells, fillets, chamfers, ribs, and richer dependency expressions extend the same interface incrementally.
