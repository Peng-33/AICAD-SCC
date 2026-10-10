# GitHub publication status

Recorded on 2026-10-10 before the first push. This file does not by itself
prove that the remote repository exists. The push target is a new public
repository, `Peng-33/AICAD-SCC`. `Peng-33/AICAD` and
`Peng-33/AICAD-w11-backup-20261003` stay private.

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
- description `Open-source semantic constraint compiler for verifiable CAD.`
- branch protection after the repository exists and the owner asks for it

Topics suggested for that future repository, and not applied now:

`rust`, `cad`, `parametric-cad`, `engineering`, `step`, `cadquery`, `open-source`

Discussions, stars, download counts, and CI badges are not invented here.

## Owner grant

1. The owner confirmed the rights to publish the original source and adopted Apache-2.0, including commercial use, modification, and redistribution. See `NOTICE`.
2. The approved new repository name is `Peng-33/AICAD-SCC`.
3. `Peng-33/AICAD` stays an existing private repository and is not the push target.
4. The local test report is `docs/release/TEST_REPORT.md`. Rust fmt, clippy, and workspace tests passed, with two ignored performance baselines. The release CLI was rebuilt and prints `scc 0.4.0-alpha.1`. The gated Node run passed 191, failed 0, and skipped 0. Both gated G7C cases passed inside the narrow Sweep and Loft domains. Playwright on the rebuilt binary passed G3, G4, G5, G6B, G7A, and G7B with the fake provider; G6A.2 and G6A.3 failed; `browser-g6a.mjs` was not run. Docker registry access still timed out, and the image was not built or pushed. None of this is a GitHub Actions result, and it does not make a public tag accepted.
5. The CadQuery and OCP wheel license files were opened. The OCP wheel ships Apache-2.0 text for the binding and does not ship LGPL text, the OCCT exception, or license expressions for the bundled native libraries. Source publication does not vendor those binaries. Do not publish the Docker image until those notices are added. See `THIRD_PARTY_NOTICES.md`.
