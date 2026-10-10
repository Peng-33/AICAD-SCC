# SCC v0.1.0 Release Acceptance Report

**Release:** v0.1.0  
**Release Date:** 2026-07-12  
**Release Commit:** `b801306fcdcfd5be67bbb576c33d19f7a227604f`  
**Report Generated:** 2026-07-12  
**Status:** ✅ Accepted

---

## 1. Executive Summary

This report records the post-release evidence audit and acceptance decision for SCC (Semantic Constraint Compiler) v0.1.0.

v0.1.0 delivers the **M1 semantic compiler frontend** and the **M2 deterministic symbolic runtime**. The release was built from commit `b801306fcdcfd5be67bbb576c33d19f7a227604f`, tagged `v0.1.0`, and published on GitHub with Windows and Linux release archives. All required CI jobs, release jobs, and cross-platform smoke tests completed successfully.

Numerical geometry solving, CAD entity construction, and DXF/SVG export remain explicitly out of scope for v0.1.0 and are scheduled for the future M3 milestone.

---

## 2. Release Metadata

| Item | Value |
|---|---|
| Product | SCC — Semantic Constraint Compiler |
| Version | `0.1.0` |
| Git Tag | `v0.1.0` |
| Tagged Commit | `b801306fcdcfd5be67bbb576c33d19f7a227604f` |
| Release Date | 2026-07-12 |
| Cargo Version (`scc-cli`) | `0.1.0` (`crates/scc-cli/Cargo.toml:3`) |
| CLI Version Output | `scc 0.1.0` |
| GitHub Release | https://github.com/Peng-33/AICAD/releases/tag/v0.1.0 |
| Release Type | Official (not Draft, not Prerelease) |

### Version Consistency

All version identifiers are strictly aligned:

- Git Tag: `v0.1.0`
- Cargo package version: `0.1.0`
- `scc --version` output: `scc 0.1.0`
- GitHub Release name: `v0.1.0`
- BUILDINFO in both archives: `version: 0.1.0`

---

## 3. Evidence Audit

### 3.1 Commit and Tag Evidence

| Check | Result | Evidence |
|---|---|---|
| Local `HEAD` | ✅ | `b801306fcdcfd5be67bbb576c33d19f7a227604f` |
| `origin/master` | ✅ | `b801306fcdcfd5be67bbb576c33d19f7a227604f` |
| Working tree clean | ✅ | `git status --short` returned empty |
| Local tag `v0.1.0` dereference | ✅ | `git rev-list -n 1 v0.1.0` = `b801306...` |
| Remote tag `v0.1.0^{}` dereference | ✅ | `git ls-remote --tags origin refs/tags/v0.1.0^{}` = `b801306...` |

### 3.2 Continuous Integration Evidence

#### SCC CI Run — Release Baseline

- **Run ID:** 29192934156
- **Commit:** `b801306fcdcfd5be67bbb576c33d19f7a227604f`
- **URL:** https://github.com/Peng-33/AICAD/actions/runs/29192934156
- **Status:** `completed` / `success`

| Job | Status |
|---|---|
| Format | ✅ success |
| Clippy | ✅ success |
| Test (ubuntu-latest) | ✅ success |
| Test (windows-latest) | ✅ success |
| Coverage | ✅ success |

#### SCC CI Run — Smoke Test Integration

- **Run ID:** 29194067039
- **Commit:** `6e7fb7b8c5c2ed4f366f88da642b4ae9a0e2f350`
- **URL:** https://github.com/Peng-33/AICAD/actions/runs/29194067039
- **Status:** `completed` / `success`

| Job | Status |
|---|---|
| Format | ✅ success |
| Clippy | ✅ success |
| Test (ubuntu-latest) | ✅ success |
| Test (windows-latest) | ✅ success |
| Coverage | ✅ success |
| Smoke (ubuntu-latest) | ✅ success |
| Smoke (windows-latest) | ✅ success |

### 3.3 Release Workflow Evidence

- **Run ID:** 29193133514
- **Commit:** `b801306fcdcfd5be67bbb576c33d19f7a227604f`
- **URL:** https://github.com/Peng-33/AICAD/actions/runs/29193133514
- **Status:** `completed` / `success`

| Job | Status |
|---|---|
| Verify release metadata | ✅ success |
| Build (ubuntu-latest) | ✅ success |
| Build (windows-latest) | ✅ success |
| Publish GitHub release | ✅ success |

The Verify release metadata job confirmed that the Git Tag, Cargo version, and `scc --version` all agree on `0.1.0`.

### 3.4 Release Asset Verification

Assets were downloaded via `gh release download v0.1.0` and SHA-256 checksums were verified.

| Asset | Size | SHA-256 (from release) | Verification |
|---|---|---|---|
| `scc-linux-x86_64-v0.1.0.tar.gz` | 765,730 bytes | `db797b77d268d0dd3580af3f067f97df2e0a856f97545a407fe65b90385674df` | ✅ |
| `scc-linux-x86_64-v0.1.0.tar.gz.sha256` | 97 bytes | `7dcf204707a892dedda12ab95f6840186db93155a57f09200e8775781828ba60` | ✅ |
| `scc-windows-x86_64-v0.1.0.zip` | 599,063 bytes | `d9d00a53f7a8e4a1cce003ab882912ba0d48977ca092afe92f956c94d73865e1` | ✅ |
| `scc-windows-x86_64-v0.1.0.zip.sha256` | 97 bytes | `e1a78e2697fc22caceda68719c69e01eede1bd8ce8931c028ef0b312dec5a623` | ✅ |

#### Archive Contents

Both archives contain:

- `scc` / `scc.exe` — release binary
- `README.md`
- `CHANGELOG.md`
- `Cargo.lock`
- `BUILDINFO`
- `cargo-metadata.json`

`BUILDINFO` in both archives records:

```
name: scc
version: 0.1.0
commit: b801306fcdcfd5be67bbb576c33d19f7a227604f
toolchain: rustc 1.96.0 (ac68faa20 2026-05-25)
platform: ubuntu-latest / windows-latest
```

---

## 4. Documentation Consistency

After the v0.1.0 release, documentation was audited and synchronized in commit `8bde52b`.

### Changes Applied

| File | Change |
|---|---|
| `CHANGELOG.md` | Merged duplicate `0.1.0` sections into a single `## [0.1.0] - 2026-07-12` section; removed `Release Candidate` wording; added release evidence links. |
| `tasks/todo.md` | Marked CI, Tag, Release, and asset-verification tasks as completed with Run ID and Commit references. |
| `README.md` | Removed hardcoded "146 automated tests" and replaced it with a reference to the Windows/Linux CI baseline. |
| `tasks/plan.md` | Updated test-baseline task to reference the latest successful CI run. |
| `SCC_M2_Stabilization_Report.md` | Bound the historical `146` test count to the specific commit, date, and SCC CI Run ID. |

### Architecture Decision Records

| ADR | Status | Scope |
|---|---|---|
| ADR-001 — Orphan Entity Validation | Accepted | M2 runtime validation boundary |
| ADR-002 — Structured Error Model | Accepted for v0.2.0 | Design only for v0.1.0 |
| ADR-003 — Schema Versioning | Accepted for v0.2.0 | Design only for v0.1.0 |

ADR-002 and ADR-003 are correctly documented as design decisions for v0.2.0 and are not implemented in v0.1.0.

### M3 Scope

All reviewed documents correctly state that numerical geometry solving, CAD entity construction, and DXF/SVG export are **not** part of v0.1.0 and are deferred to M3.

---

## 5. Automated Smoke Tests

A cross-platform smoke test suite was added in commit `6e7fb7b`.

- **Script:** `scripts/smoke-test.py`
- **CI Job:** `Smoke` in `.github/workflows/scc-ci.yml`
- **Platforms:** `ubuntu-latest`, `windows-latest`

### Coverage

The smoke tests validate the following end-to-end checkpoints:

1. `scc --version` returns exactly `scc 0.1.0`.
2. A valid Proposal compiles into a ConstraintIR with non-empty entities.
3. Recompiling the same Proposal produces byte-identical IR.
4. Overwriting an existing output without `--force` fails and leaves the original file intact.
5. Overwriting with `--force` succeeds and produces valid JSON.
6. A valid ConstraintIR executes and produces both Trace and State files.
7. Re-executing the same IR produces byte-identical Trace and State.
8. A conflict Proposal produces a report with `status: conflict` and non-empty conflicts.
9. No temporary files are left behind after the tests.

### CI Results

Both Smoke jobs passed in SCC CI Run 29194067039.

| Platform | Status |
|---|---|
| Smoke (ubuntu-latest) | ✅ success |
| Smoke (windows-latest) | ✅ success |

---

## 6. Acceptance Criteria Evaluation

| Criterion | Result | Evidence |
|---|---|---|
| M1/M2 modules function correctly | ✅ | All workspace tests pass (146/146 on `b801306...`, Run 29192934156) |
| Windows & Linux CI all green | ✅ | SCC CI Run 29192934156 and 29194067039 |
| P0/P1 defects = 0 | ✅ | No blocking defects identified in audit |
| Tag / Cargo / CLI / Release versions aligned | ✅ | All `0.1.0` |
| Release archives with SHA-256 | ✅ | 4 assets verified |
| Archives pass smoke tests | ✅ | Smoke jobs on both platforms (Run 29194067039) |
| Documentation consistent with release | ✅ | Doc fixes in commit `8bde52b` |
| ADR-002/003 not claimed as v0.1.0 implemented | ✅ | ADR README and docs correctly scoped |
| M3 not in v0.1.0 deliverables | ✅ | README, plan, todo, ADR-003 explicitly exclude M3 |
| Master branch protection enabled | ⚠️ | Evidence gap — private repository requires GitHub Pro or public visibility to query branch protection via API |

---

## 7. Known Limitations & Deferred Work

### Known Limitations (v0.1.0)

From `CHANGELOG.md`:

- Trace/State are committed as a best-effort two-file transaction, not a strict atomic transaction.
- Windows does not provide a stable std API for syncing the parent directory after rename.
- Rollback restores file contents but does not preserve original permissions or timestamps.
- Release artifacts include Cargo metadata, not a formal SPDX/CycloneDX SBOM.
- Action SHA pinning is currently manual; an automated update mechanism is planned.

### Deferred to v0.2.0

- Implementation of structured cross-crate error contracts (ADR-002).
- Implementation of Schema Envelope versioning (ADR-003).
- `--json-errors`, `--quiet`, `validate`, and `inspect` CLI features.
- Complete public API rustdoc and `missing_docs` enforcement.
- Coverage and performance thresholds.
- Formal SPDX/CycloneDX SBOM generation.

### Deferred to M3

- Geometry solver architecture.
- CAD entity construction.
- DXF/SVG export.

---

## 8. Evidence Gaps

| Gap | Severity | Notes |
|---|---|---|
| Branch protection configuration | P2 | GitHub API returns `403` for private repositories on the current plan. The repository owner should confirm manually or make the repository public to enable verifiable branch protection. |

No other evidence gaps were identified.

---

## 9. Conclusion and Sign-off

SCC v0.1.0 is accepted as a successful release.

- The release Tag, Cargo version, CLI version, and GitHub Release are all consistent at `0.1.0`.
- The release Commit `b801306fcdcfd5be67bbb576c33d19f7a227604f` is immutable and traceable.
- All CI and release workflow jobs completed successfully.
- Release archives were downloaded and SHA-256 verified.
- Cross-platform smoke tests passed on both Ubuntu and Windows.
- Documentation inconsistencies identified during audit have been corrected.
- ADR-002 and ADR-003 are correctly scoped as v0.2.0 design work.
- M3 capabilities remain excluded from v0.1.0.

The only remaining item is manual confirmation of branch protection settings, which is blocked by the current private-repository API limitations and does not affect the functional acceptance of the release.

| Role | Signature / Evidence |
|---|---|
| Release Commit | `b801306fcdcfd5be67bbb576c33d19f7a227604f` |
| Release Tag | `v0.1.0` |
| SCC CI Evidence | Run 29192934156, Run 29194067039 |
| SCC Release Evidence | Run 29193133514 |
| GitHub Release | https://github.com/Peng-33/AICAD/releases/tag/v0.1.0 |
| Smoke Test Script | `scripts/smoke-test.py` |

---

*Report generated by automated audit pipeline. Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>*
