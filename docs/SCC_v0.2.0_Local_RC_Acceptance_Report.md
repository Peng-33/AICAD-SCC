# SCC v0.2.0 Local Release Candidate Acceptance Report

> Historical snapshot: this report records the earlier local RC state only. It is not current acceptance evidence after the post-review optimization changes; use `SCC_v0.2.0_PostReview_Optimization_Report.md` for the latest local result.

**Branch:** `worktree-v0.2.0-cg-base`
**Date:** 2026-07-12
**Reporter:** Claude Code (automated local RC build)
**Workspace version:** `0.2.0`

## 1. Scope

This report records the local acceptance verification for the SCC v0.2.0 contract-governance release candidate. The RC covers:

- Finalization of the `SCC-{STAGE}-{NNN}` structured error contract.
- Structured proposal parsing/validation (`SCC-PROP-*`).
- Structured semantic analysis diagnostics (`SCC-ANAL-*`).
- Structured lowering failures and `try_lower` (`SCC-LOW-*`).
- Structured execution/runtime/GraphState errors (`SCC-EXEC-*`).
- CLI `--json-errors`, `--fail-on-conflict`, stable exit codes, `scc validate`, and `scc inspect`.
- ADR-003 schema envelope with v0.1.0 legacy JSON compatibility (opt-in `--schema-version 1.0`).
- Workspace version bump to `0.2.0`.

## 2. Verification Matrix

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Structured error contract enforced | `StructuredError::validate_contract()` called before JSON emission; unit tests in `scc-core/src/error.rs` | PASS |
| Proposal validation emits stable codes | `scc-proposal/src/error.rs` maps `ProposalValidationError` to `SCC-PROP-001/002/004-008` | PASS |
| Analysis diagnostics accumulate | `scc-analysis/src/error.rs` maps `AnalysisError` to `SCC-ANAL-001-006` | PASS |
| Lowering errors are structured | `scc-lowering/src/error.rs` maps `LoweringError` to `SCC-LOW-001-008`; `try_lower` returns `Result` | PASS |
| Execution/runtime errors propagate | `scc-execution/src/runtime/runtime_error.rs`, `mutation_error.rs`, `scc-core/src/runtime.rs` map to `SCC-EXEC-001-008` | PASS |
| CLI JSON error protocol | `scc-cli/src/output.rs` implements `write_json_errors` / `write_human_errors` / `exit_code_for_errors` | PASS |
| Stable exit codes | `scc-cli/src/main.rs`: `0` success, `1` invalid input/CLI, `2` conflict with `--fail-on-conflict`, `3` execution errors | PASS |
| Schema envelope (ADR-003) | `scc-core/src/schema_envelope.rs`; `--schema-version 1.0` opt-in; legacy read/write preserved | PASS |
| `scc validate` subcommand | `scc-cli/src/validate.rs`; returns exit code `1` on invalid proposal | PASS |
| `scc inspect` subcommand | `scc-cli/src/inspect.rs`; accepts envelope and legacy IR | PASS |
| Workspace version `0.2.0` | All `Cargo.toml` files updated | PASS |
| Documentation updated | `docs/error-codes.md`, `docs/adr/adr-003-schema-versioning.md` synchronized with code | PASS |

## 3. Test Execution

Command:

```bash
cargo test --workspace
```

Result:

```text
   Compiling scc-core v0.2.0
   Compiling scc-proposal v0.2.0
   Compiling scc-execution v0.2.0
   Compiling scc-lowering v0.2.0
   Compiling scc-analysis v0.2.0
   Compiling scc-cli v0.2.0
    Finished `test` profile [unoptimized + debuginfo] target(s) in 6.72s
     Running unittests src\lib.rs (scc_analysis)
     Running unittests src\lib.rs (scc_cli)
     Running unittests src\main.rs (scc)
     Running tests\compile_integration_tests.rs
     Running tests\end_to_end_tests.rs
     Running tests\execute_integration_tests.rs
     Running tests\golden_tests.rs
     Running tests\schema_envelope_integration_tests.rs
     Running unittests src\lib.rs (scc_core)
     Running unittests src\lib.rs (scc_execution)
     Running unittests src\lib.rs (scc_lowering)
     Running unittests src\lib.rs (scc_proposal)

result: ok. 233 passed; 0 failed; 0 ignored
```

Breakdown by crate/test suite:

- `scc_analysis`: 32 passed
- `scc_cli` (unit): 22 passed
- `scc` (binary unit): 0 passed
- `compile_integration_tests`: 5 passed
- `end_to_end_tests`: 15 passed
- `execute_integration_tests`: 2 passed
- `golden_tests`: 5 passed
- `schema_envelope_integration_tests`: 7 passed
- `scc_core`: 48 passed
- `scc_execution`: 47 passed
- `scc_lowering`: 25 passed
- `scc_proposal`: 25 passed

Total: **233 passed; 0 failed; 0 ignored**. The earlier 227/218 figures were documentation/transcription errors.

## 4. Manual CLI Spot Checks

The following behaviours were exercised by the end-to-end test suite:

- `scc --version` reports `scc 0.2.0`.
- `scc compile examples/bracket.proposal.json` succeeds and writes legacy IR.
- `scc compile ... --schema-version 1.0` writes an ADR-003 envelope.
- `scc execute` reads both envelope and legacy IR.
- `scc validate` succeeds on valid proposals and exits `1` on invalid proposals.
- `scc inspect` prints a summary for both envelope and legacy IR.
- `--fail-on-conflict` returns exit code `2`.
- Execution-stage errors (dangling references) return exit code `3`.
- `--json-errors` emits a valid JSON error document.

## 5. Known Limitations / Follow-up Work

- The schema envelope is opt-in only; default output remains `legacy-unversioned` to preserve v0.1.0 consumers. A future release may switch the default after a deprecation window.
- `SCC-CLI-009` is reserved for legacy `Conflict` normalization and internal contract-violation reports; it is not emitted for normal user-facing CLI failures.
- No `msgpack` or additional output formats are implemented yet; the `--output-format` extension point is documented in ADR-003 for future work.

## 6. Sign-off

Local RC acceptance criteria are satisfied. The branch is ready for final review before merge into `master` pending remote CI and code review.

**Status:** ACCEPTED (local RC, conditional on remote CI and PR review)
