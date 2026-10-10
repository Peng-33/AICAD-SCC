# Mounting plate

[examples/mounting-plate.dsl](../../examples/mounting-plate.dsl) is the DSL
stored in `FIXTURE_DSL` inside `apps/scc-ui/public/js/engineering-page.js`.
The Workbench button **载入内置安装板** submits that text. The Node test
`PLATE_DSL` in `apps/scc-ui/test/engineering-intent.test.mjs` is a shorter
sibling used for a geometry assertion. The file in `examples/` matches the
button, including thickness, third-angle drawing, and a through hole extent.

The part is a 120 mm by 80 mm rectangle, 10 mm thick, with four Ø10 corner
holes inset 15 mm. `hole_diameter` is bound to `plate_height / 8`, which is
10 mm for the default height.

No provider is involved if you load the fixture or run:

```bash
./target/release/scc engineering dsl examples/mounting-plate.dsl > /tmp/mounting-plate.model.json
./target/release/scc engineering drawing-three-view /tmp/mounting-plate.model.json
```

STEP:

```bash
./target/release/scc engineering export-step /tmp/mounting-plate.model.json > /tmp/mounting-plate.step
```

That second command is the v1 rectangle-and-hole manufacturing path. It needs
CadQuery. It is not a Sweep or Loft example. Section A-A on this model fails
closed as `SCC-DRW-024` because formal sections require an EngineeringModel v2
parametric rebuild.

For Sweep and Loft, read the bounded contracts in
[ADR-075](../adr/adr-075-general-parametric-fixed-section-sweep.md) and
[ADR-076](../adr/adr-076-general-parametric-loft.md), and the historical
acceptance notes under `docs/testing/phase-g7a-parametric-sweep/` and
`docs/testing/phase-g7b-loft/`. Those notes belong to ancestor runs. They are
not a re-run of snapshot 0.4.0-alpha.1.
