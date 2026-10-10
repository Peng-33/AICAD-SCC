# ADR-045 — Solved snapshot, revision, and derived artifact cache

Status: Accepted for Phase 10.6

## Context

Before Phase 10.6, the Node API assembled Workbench snapshots by applying an empty tool batch with `solve=true` after another operation had already solved the same model. DSL edits, tool edits, NL preview/confirm, drawing, and STEP therefore had avoidable repeated solve or re-derivation boundaries. Rust CLI execution is one-shot per request, so an in-memory Rust cache cannot survive a request.

## Decision

`EngineeringModel` remains the only editable and authoritative engineering state. Rust exposes `SolvedEngineeringSnapshot`, whose model clone, lowered proposal, geometry, source map, and diagnostics are produced by one `lower_and_solve` call. The CLI transport returns that complete snapshot, and downstream consumers in the same operation use its geometry/source map rather than solving again. Empty-batch solving is not used to assemble snapshots.

Revision and fingerprint are separate:

- `revision` is monotonic session ordering for committed transitions and stale-response context. Preview, cancel, drawing-mode changes, save, and STEP export do not increment it; commits, open, undo, and redo do.
- `ModelFingerprint` is the canonical SHA-256 identity of canonical `EngineeringModel` content. It excludes UI mode, project filename, undo history, revision, timestamps, previews, and derived artifacts. Undo may create a new revision with an old fingerprint. Phase 10.4/10.5/10.6 use the same server implementation.

Node owns a bounded, process-lifetime `DerivedArtifactCache` containing only opaque deterministic principal-drawing and three-view-drawing responses. Keys are `(serverCanonicalModelFingerprint, typedArtifactKind)`. Entries are defensive-cloned, LRU-evicted, and deletable without affecting correctness. Node does not inspect geometry, derive semantic keys, reconstruct models, or cache STEP bytes, CadQuery/OCP solids, provider responses, clarification state, proposals, errors, or paths.

All response cache hits carry the model fingerprint, typed artifact kind, and current session revision. Existing drawing/intent/STEP generation guards remain active; a cache hit cannot bypass stale checks.

## Rejected alternatives

- A persistent/SQLite/cloud cache: outside the ephemeral optimization scope and risks becoming a second truth.
- A long-lived Rust daemon: unnecessary for the current one-shot CLI architecture.
- Incremental solving or partial dependency rebuild: explicitly deferred.
- Caching STEP/CadQuery/BRep objects: external runtime identity and lifecycle make this unsafe in Phase 10.6.
- Trusting a client-provided fingerprint: it is not an authorization token or cache trust boundary.
- `Map<Fingerprint, Everything>`: unbounded and mixes artifact kinds.

## Impact

A single DSL/tool/NL commit now solves once and returns a complete snapshot. NL preview solves the candidate once; confirmation reapplies the exact plan and solves the committed model once. Drawing and three-view repeated requests can hit bounded opaque memory cache without changing output. Save and STEP continue to operate on committed source state and STEP remains uncached.

## Compatibility and migration

The existing JSON shape is preserved with additive `revision`, `model_fingerprint`, `artifact_kind`, and `cache_status` metadata. The new `engineering snapshot` CLI command is an internal transport boundary. `AicadProject` remains source-only and contains no snapshot, revision, cache, or derived artifacts.

## Verification

Rust snapshot tests observe one solve at the formal boundary and verify consistency. Node tests cover bounded LRU behavior, artifact-kind separation, defensive copies, cache misses/hits, fingerprint changes, revisions, and no client-trusted cache key. Existing project, drawing, STEP, NL preview/confirm, and full workspace regressions remain required.
