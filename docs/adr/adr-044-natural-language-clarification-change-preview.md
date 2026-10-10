# ADR-044 — Natural-language clarification and change preview workflow

Status: Accepted for Phase 10.5

## Context

The Engineering Workbench already had a provider-neutral natural-language adapter, but its product route returned a candidate model that the browser could apply immediately. That made an ambiguous request, an LLM proposal, and a committed `EngineeringModel` too easy to conflate. Phase 10.5 requires a reviewable workflow while preserving the existing Rust command layer as the only mutation authority.

## Decision

The product workflow is `Ask → Clarify → Preview → Confirm → Commit`. The provider may return only a typed clarification or a bounded formal command plan. It never receives mutation authority and never writes `EngineeringModel`.

- `EngineeringCommand` remains the sole mutation boundary. Provider tool calls are validated by the Rust manifest and applied atomically to a model clone for preview.
- Clarifications use a narrow typed contract: `length`, `integer`, `boolean`, `enum`, `feature_reference`, `parameter_reference`, `point2`, and `text`. Missing critical diameter, extent, blind face, and target references must be requested; no silent critical defaults are permitted.
- A clarification is ephemeral UI state. It is bound to the original intent and the base model fingerprint, is not persisted in `AicadProject`, and is invalidated by New/Open/manual edit/undo/redo.
- `ChangePreview` is structured data containing proposal ID, base/candidate fingerprints, source expressions and resolved values, feature added/modified/removed/unchanged classification, diagnostics, downstream readiness, and the exact formal command plan. LLM prose is never diff truth.
- Preview applies to a clone only. It does not change the committed model, dirty state, undo history, drawing generation, STEP generation, or project persistence.
- Confirm does not call the provider again. It replays the exact previewed plan against the current base, rejects stale fingerprints, requires candidate fingerprint equality, and records one undo transaction.
- UI intent generations are independent from drawing and STEP generations. Drawing mode changes do not invalidate a proposal; source/model changes do.
- The supported NL edit surface follows the registered EngineeringCommand/tool manifest. Fillet and Slot are unsupported and fail closed. No arbitrary JSON patch, code, `eval`, `exec`, shell, file path, or new CAD feature is introduced.

## Rejected alternatives

- Applying the candidate model directly from the LLM response: violates the command authority and prevents a trustworthy review.
- Treating clarification as an editable project object: pollutes source persistence and undo semantics with transient conversation state.
- Recalling the intent during Confirm: allows provider drift and makes the reviewed plan non-authoritative.
- Generic JSON Schema or arbitrary patch/code output: expands the attack surface and bypasses the typed command registry.
- Building a parallel NL-specific fingerprint: risks races with project, drawing, and STEP identity.

## Impact

The browser gains a formal clarification form and structured preview with Apply/Cancel. Save and validated STEP export continue to operate on the committed base while a preview is open. Existing low-level NewDesign/EditDesign orchestration remains available for compatibility tests, while the product Phase 10.5 route is preview-only for existing EngineeringModel edits.

## Compatibility and migration

The existing `/api/engineering/intent` product route now returns `preview_ready` or `needs_clarification`; clients must call `/api/engineering/intent/confirm` with the returned proposal. Existing direct `runIntent` callers retain provider-neutral validation behavior. The existing command manifest remains the single capability source for edit execution.

## Verification

Focused tests cover typed clarification parsing, non-mutating structured preview, source/derived parameter reporting, feature classification, stale confirmation, no-provider confirm, non-formal command rejection, CSP integrity, and the existing UI/Rust regression suite.
