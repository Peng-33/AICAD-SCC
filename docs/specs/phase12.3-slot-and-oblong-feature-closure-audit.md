# Phase 12.3 — Slot, Oblong Hole & Reusable Profile Feature Semantics Closure Audit

- Date: 2026-09-16
- Task type: LARGE_FEATURE
- Decision: PASS_WITH_NOTES
- Freeze: FREEZE
- Status: closed in this audit

## Phase 12.3 Status

`PASS_WITH_NOTES`. The only note is `BROWSER_E2E = NOT_EXECUTED`.

`FULL_REAUDIT = NO`. This task did not re-audit Phase 12.2. ADR-053 is the frozen architecture contract.

## Architecture

One engineering truth: `EngineeringFeature::Slot`. Oblong hole and long hole are UI/NL synonyms only. Slot is a through cut feature, never `BaseProfile::Slot`, and never inferred from Composite geometry.

Canonical dimensions: `overall_length` (persisted as `length`), `width`, `center`, `orientation_degrees`. Center-distance is derived. `L == W` is rejected; a round hole remains `Hole`. Orientation is canonicalized to `[0°, 180°)`. Extent is Through only.

## Provenance

```text
SLOT_INFERRED_FROM_GEOMETRY = NO
SEMANTIC_ID_PARSING = NO
SLOT_BBOX_CUT = NO
SECOND_LAYOUT_ENGINE = NO
SLOT_MANUFACTURING = DEFERRED
NL_SLOT = DEFERRED
```

Front uses `SpatialElementRef::SlotSegment { id, index }`. Top/Left use `SpatialElementRef::SlotCut { id }`. Visibility owns hidden geometry.

## Goldens

Unchanged: Phase 11, 12.1, 12.2, rectangle, circle, rounded, composite.

New: `phase123_slot_horizontal.svg/.dxf`, `phase123_slot_rotated.svg/.dxf`.

## Verification

Workspace tests: 1077 passed, 0 failed, 2 ignored. Clippy `-D warnings` green. `cargo fmt --all -- --check` green. `git diff --check` green (pre-existing `.gitignore` CRLF warning only). UI: 125 passed, 0 failed, 1 skipped. Docker CadQuery 31/31. Docker Rust→STEP 1/1. Browser E2E not executed (compose e2e timeout 124).

## 2026-09-16 Verification Delta

- Added a real project legacy-JSON regression: removing Slot `center` and `orientation_degrees` from a serialized Project v1 document reopens as the intentional defaults `(0,0)` and `0°`.
- Added an independent `BaseProfile::Composite + Slot` product-chain fixture, a 135° native-slot fixture, and an inside/touch/cross/outside containment matrix for Rectangle, Circle, RoundedRectangle, and Composite.
- Focused Phase 12.3 crates: 725 passed, 0 failed, 0 ignored. The workspace rerun is the authoritative aggregate above.
- Current release CLI UI test: 125 passed, 0 failed, 1 skipped. `cad-test`: 31/31. `cad-rust-test`: 1/1.
- Browser E2E was attempted once; the UI became healthy but `timeout 25 docker compose run --rm e2e` exited 124. It remains `NOT_EXECUTED`, not a fabricated pass.
