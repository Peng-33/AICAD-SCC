# ADR-063: Canonical STEP Authority Consolidation

- Status: Accepted for Phase 13.5A
- Decision owners: SCC architecture
- Scope: formal STEP manufacturing and delivery authority

## Context

The Phase 13.5 audit found two reachable CAD construction paths. The Engineering
route produces a source-only `EngineeringModel`, lowers through `SpatialModel`
and `SolidBuildSpec`, and delivers only the STEP bytes that passed CadQuery/OCP
reimport, geometric equivalence, and artifact identity checks. The compatibility
route accepts a mutable `CadDocument` / `feature_graph_v1`, builds a B-Rep in
Python, and previously exposed a direct STEP artifact without that canonical
contract. Its Python feature graph also expanded circular patterns itself.

The compatibility route is still used for editable generic CAD state, OCCT
preview, BREP/STL/GLB/SVG derivatives, source JSON, and requirement-coverage
inspection. Removing the whole route would regress those non-STEP capabilities.
There is no small, lossless, semantically unambiguous CadDocument-to-
EngineeringModel conversion contract, so inventing one would create a false
authority bridge rather than consolidate authority.

## Decision

Select **Strategy A: retire formal STEP delivery from the compatibility route**.

1. `POST /api/cad/build` remains available for compatibility CAD editing and
   non-STEP derivatives. `CadDocument` remains explicitly a compatibility
   editing state, not the product's EngineeringModel truth.
2. The compatibility Python worker no longer exports or declares a STEP
   artifact. Its retained outputs are source JSON, BREP, STL, GLB, and SVG;
   the in-memory SVG preview and manifest metrics remain available.
3. The compatibility artifact endpoint therefore cannot serve `step`, and the
   compatibility UI does not offer a STEP download. A CadDocument is never
   reverse-inferred into an EngineeringModel to satisfy a STEP request.
4. Formal STEP creation and download use only
   `EngineeringModel → solve/GeometryModel → SpatialModel → SolidBuildSpec →
   Rust CadBackendClient → stateless CadQuery/OCP → STEP export → STEP
   reimport/equivalence → artifact identity → exact validated-byte delivery`.
5. Compatibility circular-pattern expansion may remain for retained non-STEP
   B-Rep/preview behavior, but it has no path to a formal STEP artifact. The
   canonical Engineering path remains the sole formal pattern authority.

## Rejected alternative

Strategy B, migrating the compatibility input into the canonical authority, is
not selected because the repository has no existing lossless bridge and a safe
bridge would require rebuilding the CadDocument/FeatureGraph semantic system.
That would exceed this consolidation phase and risk changing established
compatibility editing behavior.

## Route inventory at decision time

| Entry | Caller/use | Input authority | STEP result |
|---|---|---|---|
| `POST /api/engineering/export/step` | Engineering UI and API clients | source-only EngineeringModel | canonical validated bytes |
| `POST /api/cad/build` | 3D compatibility UI and CAD clients | CadDocument / FeatureGraph | no STEP; retained non-STEP derivatives |
| `GET /api/cad/artifacts/:jobId/:name` | compatibility artifact downloads | bounded CAD job manifest | `step` is absent and returns not found |
| `aicad_cad.worker` | compatibility worker/tests | CadDocument / legacy CadSpec | no STEP export |

The engineering CLI adapter and project export route remain on the canonical
path. No alias, response format, or direct worker output is allowed to re-open
the retired compatibility STEP route.

## Consequences

- The product has one formal STEP authority and one validated-byte delivery
  contract.
- Existing compatibility editing, B-Rep, preview, mesh, GLB/SVG, source JSON,
  and coverage features remain available.
- Users who need formal STEP must use an EngineeringModel project and the
  Engineering export route; a generic CadDocument cannot be silently converted.
- The compatibility CadDocument remains a second product-level editable state
  for its bounded legacy/compatibility workspace. This ADR does not claim that
  all product editing truth has become EngineeringModel.
- Phase 13.5 Manufacturing Domain Closure must be re-audited separately after
  this implementation; this ADR alone does not freeze Phase 13.5 or Phase 14.

## Verification contract

The implementation must prove, with real server-side tests, that compatibility
builds retain non-STEP artifacts while `step` is absent, compatibility artifact
downloads reject `step`, and canonical Engineering STEP delivery continues to
return the exact adapter bytes. The CAD worker tests must also prove that the
compatibility worker does not write a STEP file.
