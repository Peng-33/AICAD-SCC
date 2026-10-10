# Current limitations

This page is the current product bound for snapshot 0.4.0-alpha.1.
Older ADRs and acceptance reports sometimes say Sweep, Loft, Revolve, or
parametric STEP were still deferred. That was true when those documents were
written. It is not the current source bound. The current source bound is
also not "fully supported".

## Implemented, with a domain

| Area | Allowed | Rejected |
| --- | --- | --- |
| v1 profiles | Rectangle, circle, rounded rectangle, polygon, one Line/Arc composite loop, holes, hole patterns, slots | A second solid, a freeform profile |
| v2 history | Sketch, datum plane, datum axis, path, extrude, revolve, hole, pattern, constant-radius fillet, equal-distance chamfer, fixed-binormal sweep, ruled aligned loft | Variable fillet, distance-angle chamfer, closed or sharp or twisted or variable sweep, smooth or holed or non-parallel loft |
| Drawings | Front, Top, Left; principal Section A-A after a v2 rebuild; a subset of associative dimensions and hole callouts | Title blocks, standard sheet frames, full GD&T/PMI. A v1 model fails closed as `SCC-DRW-024` instead of inventing a section. |
| STEP | Formal EngineeringModel route after re-import and equivalence | Any STEP response from `/api/cad/build` |
| Intent | DSL for new work, typed tools for edits, keys only in process memory | A model that returns CadQuery, Python, or mesh script as the part |

G7C selectors for sweep and loft caps and lateral faces are in
`crates/scc-engineering` and in `apps/scc-ui/test/rust-parametric.integration.test.mjs`.
On this snapshot those tests were run with
`AICAD_RUST_PARAMETRIC_INTEGRATION=1`. The Loft case passed, including formal
STEP re-import. The Sweep case failed closed with `TOPOLOGY_REFERENCE_STALE`
after a hole of the same diameter as the tube end removed the referenced full
rim. That failure is not accepted, and the selector set is not accepted.
Skipped is not accepted. The six CAD skills describe that selector contract
so a planner does not invent topology ids. Skill text is not a test pass.
See [../release/TEST_REPORT.md](../release/TEST_REPORT.md).

## Not in the product

- General assemblies and multi-body parts
- Complete NURBS or a freeform surface modeler
- Threads and general shell
- A guarantee that an arbitrary mechanical part prompt will build
- Verified interchange with FreeCAD or other CAD systems
- A production operations claim, a service SLA, or a hosted API

## Evidence

Ancestor reports under `docs/testing/` are historical. They may show
`scc 0.3.0` or an image tag `0.4.0` from the day they were written.
Snapshot 0.4.0-alpha.1 evidence is only [../release/TEST_REPORT.md](../release/TEST_REPORT.md).

Real DeepSeek and Qwen providers are not part of the required demo. The
mounting-plate fixture and `examples/mounting-plate.dsl` do not call them.

## Licensing limit

Apache-2.0 is the intended license and is not yet an authorized public grant.
OCCT inside a CadQuery install stays under LGPL-2.1 plus the Open CASCADE
exception. See [../../NOTICE](../../NOTICE) and [../../THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md).
