# Directions

Phase numbers in the ADR index are historical labels. This page is the
public-facing order of work. Nothing here is a commitment date.

## In progress

- Keep one EngineeringModel and one formal STEP checker while the v2 feature set stays bounded.
- Keep G7C inside the gated Sweep and Loft cases. A rim-consuming sweep hole stays fail-closed.
- Make the public docs, the Workbench README, and the feature table describe the same bounds.

## Planned

- Parametric modeling: more sweep and loft cases, each with a fail-closed test and a named rejection.
- Semantic topology: only add a selector when rebuilds re-resolve it to one edge or face.
- Drawings: the associative dimension kinds that ADR-071 and ADR-072 still call partial.
- Interchange: open foreign STEP in a clean experiment and publish the mismatch list.
- Intent: record pass or fail for one named provider without putting that provider on the default demo path.
- Workbench: error text that quotes the `SCC-*` code and the field that failed.

## Not scheduled as current work

- Assembly product model
- Full NURBS editor
- Full PMI and standard drawing frames
- Replacing Open CASCADE with an AICAD kernel
- A hosted multi-tenant service

## Release engineering

- Confirm the copyright holder and the Apache-2.0 grant before the first public push.
- Read the CadQuery 2.8.0 wheel's license files before publishing an image.
- Keep the private backup repository private.
- Tag `v0.4.0-alpha.1` only when `scc-cli` is that version, which it is in this snapshot, and only when the owner asks for the tag.
