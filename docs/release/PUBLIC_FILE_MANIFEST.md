# Public file manifest

Snapshot version: `0.4.0-alpha.1`.
Private baseline: `0568fac256178ab9b344995c1eab91861c292a7d`.

The file list is the prep tree minus `.git/`, `target/`, and `node_modules/`.
A machine-readable exclusion diff is in `absence-reasons.json` in this
directory. That JSON was generated before the UI specification `.docx` was
restored; the prose manifests are the ones to trust for that file.

## Kept from the baseline

- `crates/` — all 18 workspace members, sources, and tests
- `apps/scc-ui/` — server, Workbench, CAD worker, unit tests, e2e scripts
- `skills/cad/` — six skills
- `examples/` — existing proposals plus `mounting-plate.dsl`
- `docs/adr/` — ADR-001 through ADR-076 and the index
- `docs/specs/`, `docs/testing/` — technical audits and historical acceptance evidence
- `.github/workflows/` — CI and release workflows
- `compose.yaml`, `Dockerfile` path `apps/scc-ui/Dockerfile`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`
- Root milestone Markdown reports

## Added for the public tree

- `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md`
- `README.md` (rewritten), `README.zh-CN.md`
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`
- `assets/brand/logo.svg`
- `docs/architecture/`, `docs/getting-started/`, `docs/development/`, `docs/api/`, `docs/examples/`, `docs/limitations/`, `docs/roadmap/`, `docs/release/`
- `.github/ISSUE_TEMPLATE/`, `.github/PULL_REQUEST_TEMPLATE.md`
- `examples/mounting-plate.dsl`
- `SCC_UI_UX_Design_Specification.docx` (test fixture, restored)

## Sanitization

Text files were scanned after the rewrite. A later rescan of 550 text files
found no host home-directory prefix and no personal mailbox marker. Image
links that pointed at an absolute Workbench screenshot now use a relative path.
