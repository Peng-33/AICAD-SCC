# SCC v0.2.0 Post-Review Optimization and Local Acceptance Report

**Branch:** `worktree-v0.2.0-cg-base`

**Committed base HEAD:** `707c719027a17145a5d5d1987a881f640fea28ef`

**Change state:** staged local changes; no new commit, push, merge, tag, or release

**Date:** 2026-07-12

**Workspace version:** `0.2.0`

## 1. Executive conclusion

The SCC v0.2.0 post-review optimization set passes every local engineering quality gate and is ready for human code review. The implementation now distinguishes unsafe analysis input from business conflicts, preserves multi-error diagnostics, strengthens runtime integrity controls, rejects malformed schema envelopes, hardens atomic output behavior, and closes CI trigger gaps.

**Local verdict: PASSED.** The staged change set is buildable, deterministic under the covered scenarios, and locally runnable through the complete Proposal → Semantic analysis → Constraint IR → Execution → Trace/GraphState path.

**Release/merge verdict: NOT YET AUTHORIZED.** The changes are not committed or pushed, and therefore have no matching remote Linux/Windows CI evidence. PR approval, current-head remote CI, and maintainer review remain required before merge, Tag, or Release.

## 2. Implemented optimization scope

### 2.1 Analysis and error-contract routing

- `scc compile` now uses `analyze_detailed()` instead of the legacy `analyze()` compatibility wrapper.
- Missing references, invalid types, cycles, and invalid declaration graphs return `SCC-ANAL-*` errors and exit code `1`.
- Genuine MCS contradictions remain ConflictReports and retain exit code `0`, or `2` with `--fail-on-conflict`.
- `SccError::StructuredBatch` preserves every deterministic proposal/analysis diagnostic; JSON output no longer truncates to the first error.
- Analysis relation paths retain the original Proposal index even when unsupported relations are filtered from Semantic IR.
- `StructuredError::validate_contract()` now rejects present-but-empty path, entity ID, constraint ID, and suggestion context.

### 2.2 Runtime integrity and execution API boundary

- Missing constraint source/target references return `SCC-EXEC-001` with entity and constraint context.
- Canonicalization serialization failure returns `SCC-EXEC-013`; it cannot silently hash empty bytes.
- Overlay application independently rejects L0 suppress/unsuppress/re-authorize, promotion to L0, and negative parameter mutations before state mutation.
- Internal execution-plan, patcher, mutation, canonicalizer, plan-generator, and runtime modules are crate-private.
- The supported external execution path remains `driver::execute`; `RuntimeOutput` is re-exported at the crate root.
- Unused `MutationProposal` and unreachable `Decision::Skip` code were removed.

### 2.3 Schema-envelope contract

- Envelope versions must be exactly two numeric components (`MAJOR.MINOR`).
- Only the supported major (`1.x`) is accepted; malformed, `0.x`, and `2.x` versions are rejected with `SCC-CLI-006`.
- Any top-level Envelope marker triggers complete Envelope validation; partial envelopes cannot fall back to legacy parsing.
- Missing payload remains `SCC-CLI-007`; malformed metadata or payload is `SCC-CLI-008`.
- Envelope errors now carry stable `/schema_version`, `/document_type`, or `/payload` paths.
- v0.2.0 scope is explicit: Proposal input and ConflictReport remain legacy JSON; Envelope read support is for Constraint IR, and Envelope output covers Constraint IR, GraphState, and ExecutionTrace.

### 2.4 CLI and atomic output behavior

- `--json-errors` covers command-line argument parsing failures through `SCC-CLI-010`.
- `--json-errors --fail-on-conflict` writes the ConflictReport, returns exit code `2`, emits `SCC-CLI-009` on stderr, and keeps stdout clean.
- An impossible empty compile result is treated as an internal contract failure instead of false success.
- Bare relative output paths use the current directory correctly.
- Pair writes preflight no-clobber checks before publishing either output.
- Single-file writes no longer load existing target contents into memory solely for unused rollback data.
- Pair backups are captured at transaction preparation; second-publish failure triggers an atomic restore of the first target.
- Publish-before-directory-sync failures are distinguished so best-effort rollback can be attempted without deleting a competing writer's file after a pre-publish failure.

### 2.5 CI and documentation governance

- CI path filters include `scripts/**` and `examples/**`, so smoke scripts and fixtures trigger the quality workflow.
- README, CHANGELOG, ADR-002, ADR-003, and the error-code registry match actual behavior.
- Earlier RC reports are explicitly marked as historical snapshots; their old test counts and CI SHAs are not reused as evidence for this staged change set.

## 3. Local acceptance evidence

| Gate | Command | Result |
|---|---|---|
| Full tests | `cargo test --workspace --all-targets` | **254 passed; 0 failed; 0 ignored** |
| Documentation tests | `cargo test --workspace --doc` | 0 doctests; command passed |
| Static analysis | `cargo clippy --workspace --all-targets -- -D warnings` | Passed; zero warnings |
| Formatting | `cargo fmt --all -- --check` | Passed |
| Release build | `cargo build --release -p scc-cli` | Passed |
| End-to-end smoke | `python scripts/smoke-test.py` | All smoke tests passed |
| Binary version | `target/release/scc.exe --version` | `scc 0.2.0` |
| Patch hygiene | `git diff --check` before staging | Passed |

The 254-test result is the current command result for this exact staged change set. Historical reports with 218, 227, or 233 tests remain snapshots of earlier revisions and are not current acceptance evidence.

## 4. Core flow verification

The verified main flow is:

1. Parse legacy Proposal JSON.
2. Collect proposal validation diagnostics in deterministic order.
3. Build Semantic IR and collect structured analysis errors.
4. Separate unsafe analysis errors from genuine business MCS conflicts.
5. Lower a valid Semantic IR into Constraint IR with authority/provenance metadata.
6. Serialize legacy IR or schema `1.0` Envelope output.
7. Execute Constraint IR through mutation guard, canonicalization, plan generation, decision evaluation, command dispatch, overlay application, and GraphState validation.
8. Atomically publish ExecutionTrace and GraphState as a best-effort pair.

Success is proven when the smoke test completes compile and execute, the emitted trace/state parse successfully, deterministic golden tests match, no integrity error is raised, and every quality gate above remains green.

## 5. Human re-verification required

The following items intentionally remain human or remote-review gates:

1. **Public API review:** confirm the v0.2.0 RC is allowed to make internal execution modules crate-private and add `SccError::StructuredBatch` to a public enum.
2. **Remote cross-platform CI:** push only after authorization, then require Test (Ubuntu/Windows), Smoke (Ubuntu/Windows), Clippy, Format, and Coverage to pass on the exact pushed SHA.
3. **Atomic-write platform review:** Windows tests pass locally; Linux parent-directory sync and rollback paths still require current-head Linux CI evidence.
4. **Schema-scope product review:** confirm Proposal Envelope input and versioned ConflictReport remain outside v0.2.0, as now documented.
5. **PR review:** inspect analysis routing, error-code compatibility, output-file semantics, and API visibility changes before approving PR #1.
6. **Repository governance:** verify branch protection, required checks, base branch, and PR head SHA after any push.

## 6. Known non-blocking limits

- Trace/State publication is a best-effort two-file transaction, not a filesystem-wide atomic transaction.
- Windows `std` has no stable portable parent-directory sync API; file data is synced, but directory durability relies on platform behavior.
- Rollback restores content, not all original filesystem metadata.
- Orphan entities remain allowed by M2 policy; strict final-state callers must invoke `GraphState::validate_final()`.
- Formal performance thresholds, fuzzing, coverage thresholds, SBOM generation, Trace Replay, and M3 geometry/CAD solving remain future work.
- Proposal missing-field paths are stable for covered fields, but deeply nested serde path precision is not yet guaranteed without a path-aware deserializer.

## 7. Final recommendation

The staged SCC v0.2.0 changes meet the local definition of done and are ready for maintainer review. Do not merge, tag, or release until a local commit is created, pushed with explicit authorization, and remote CI is green for that exact commit.
