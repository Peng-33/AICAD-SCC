# GitHub publication status

No public repository was created or updated by this prep.

## Name check

`gh repo view Peng-33/AICAD` on 2026-10-10 returned:

- name: `AICAD`
- visibility: `PRIVATE`
- url: `https://github.com/Peng-33/AICAD`

The preferred name is already a private repository. This prep does not push
to it, change its visibility, or replace its contents. Choosing that
repository, or choosing another name, is an owner decision.

The backup repository `Peng-33/AICAD-w11-backup-20261003` was not modified
and was not made public.

## What a future public repository would need

These settings are not applied anywhere:

- default branch `main`
- the workflows already in `.github/workflows/`
- issue templates and the pull request template in this tree
- `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`
- description and topics only after the owner approves a new repository
- branch protection after the repository exists and the owner asks for it

Topics suggested for that future repository, and not applied now:

`rust`, `cad`, `parametric-cad`, `engineering`, `step`, `cadquery`, `open-source`

Discussions, stars, download counts, and CI badges are not invented here.

## Gates still closed

1. Copyright and the Apache-2.0 grant are not confirmed. See `NOTICE`.
2. The owner has not approved the first push or the license.
3. `Peng-33/AICAD` is occupied by an existing private repository.
4. The local test report is `docs/release/TEST_REPORT.md`. Rust workspace tests passed, with two ignored performance baselines. The gated Node run passed 189 and failed the two G7C tests. Docker and a browser pass were not run. G7C remains a P0 for calling this tag accepted. It does not by itself forbid preparing the source tree.
5. The CadQuery and OCP wheel license files were opened. The OCP wheel ships Apache-2.0 text for the binding and does not ship LGPL text, the OCCT exception, or license expressions for the bundled native libraries. Source publication does not vendor those binaries. Do not publish the Docker image until those notices are added. See `THIRD_PARTY_NOTICES.md`.
