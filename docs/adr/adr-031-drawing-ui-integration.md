# ADR-031: Minimal drawing UI integration

- Status: Accepted
- Date: 2026-08-26
- Related: ADR-016, ADR-021..030, `crates/scc-cli/src/engineering.rs`, `apps/scc-ui/` (`lib/scc-cli.mjs`, `server.mjs`, `public/js/api.js`, `public/js/engineering-workbench.js`)

## Background

Phase 8 froze the presentation engine (`scc-drawing`: `DrawingModel`,
`drawing_to_svg`) but no UI surface consumed it: the workbench only showed the
raw solved-geometry debug view. Phase 8.10 wires the authoritative drawing
preview into the engineering workbench through a minimal, strictly layered
path. The scc-drawing / scc-engineering / scc-geometry engines remain frozen;
the only new dependency edge is `scc-cli → scc-drawing`.

## Decisions

1. **Single preview source of truth.** The official drawing preview is
   `EngineeringModel → lower_and_solve → DrawingInputs → build_drawing →
drawing_to_svg`, delivered as JSON over one new CLI subcommand
   (`scc engineering drawing`). No second rendering path exists.
2. **The frontend never draws engineering geometry for the preview.** It
   displays the SVG verbatim inside `<img>`; all geometry, dimension, and
   layout computation stays in `scc-drawing`.
3. **Frozen engine boundary preserved.** The only new crate dependency is
   `scc-cli → scc-drawing`; golden files and engine crates are untouched.
4. **CLI stdout contract.** On success the subcommand prints exactly one JSON
   document `{svg, sheet_width_mm, sheet_height_mm, scale, part_title}` on
   stdout; on failure it emits structured errors on stderr with exit code 1.
5. **The Node server is a thin transport.** `/api/engineering/drawing` maps
   request → CLI spawn → parsed stdout; missing `model` is a 400, CLI
   diagnostics map to 422 preserving their `SCC-DRW-*` codes. The server never
   parses engineering semantics.
6. **Failure semantics are three-way.** Command failure (transport): do not
   refresh and keep the old image, surfacing the error banner (stricter than a
   silent keep). Drawing failure (SCC-DRW): show an error
   banner with code + message, keep the old image — never silently fall back
   to the naive geometry view. In-flight refreshes show loading text and are
   guarded against staleness.
7. **Stale-response guard.** Every refresh takes a generation from a monotonic
   counter; late responses whose generation is no longer current are dropped
   before any URL allocation or DOM patch (no leak, no flicker).
8. **Blob discipline.** SVG bytes → `Blob` → `URL.createObjectURL` → `<img>`,
   with the previous URL revoked exactly once on replacement and on dispose;
   mutation-success paths trigger exactly one refresh each. `innerHTML` SVG
   injection is forbidden.
9. **Naming isolation.** "Geometry Debug" names the raw solved-entity debug
   view (entity click-to-inspect retained); "Engineering Drawing" names the
   authoritative preview. The two panels are visually and conceptually distinct.
10. **UI zoom ≠ drawing scale.** Any future view zoom control is a UI concern
    over the `<img>`; `DrawingScale` remains engineering truth from
    `DrawingState`. This phase ships no zoom control.
11. **Explicitly not in this phase:** DXF in the UI, download buttons, PDF,
    multi-view/third-angle projections, click-to-select inside the SVG,
    write-back APIs to the drawing layer, Phase 9 vocabulary.
12. **Interactive SVG selection is deferred** to a later phase; the current
    `<img>` transport is deliberately the simplest correct boundary.

## Alternatives rejected

- **Bare-SVG HTTP response** (server returns `image/svg+xml`): loses the
  structured metadata (sheet size, scale, title) and the uniform JSON error
  channel.
- **Piggybacking the SVG onto the snapshot response**: couples two failure
  domains (solve vs. draw) and forces redraw cost on every panel re-render.
- **Frontend renders the SVG DOM** (parse and inject nodes): invites geometry
  manipulation in JS, violates the transport-only rule, and breaks the CSP /
  innerHTML discipline.
- **FFI into scc-drawing from Node**: adds a build/platform matrix for zero
  benefit when a CLI subprocess boundary already exists.

## Consequences

- The preview always agrees byte-for-byte with the frozen renderer semantics
  (golden-tested) because it _is_ the renderer output.
- One extra CLI invocation per mutation success path; acceptable at current
  model sizes and isolated by the stale guard under rapid edits.
- Future interactive selection will likely replace `<img>` with inline
  `<svg>` served as a document (not via innerHTML), keeping every other
  boundary in this ADR intact.
