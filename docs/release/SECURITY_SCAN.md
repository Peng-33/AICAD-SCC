# Security scan

Date: 2026-10-10. Scope: the public-prep tree only. The private repository
was not uploaded to an external scanner, and its git objects were not copied
into this snapshot.

## Method

A local scan walked text files under the prep tree, skipping `.git`,
`target`, and `node_modules`. It counted matches for private-key blocks,
AWS access-key ids, GitHub token prefixes, Slack token prefixes, long
`sk-` tokens, and assignments that look like `api_key`, `secret`,
`password`, or `token`. It also counted a host home-directory prefix and two
mailbox markers that had appeared in the private history. Matched secret
values are not recorded in this file, and this note does not repeat those
markers.

The UI specification `.docx` was scanned as XML text for the same home path,
email, private-key, and key-id markers.

## Results

| Check | Result |
| --- | --- |
| Text files read | 550 on the rescan after the release notes, skipping `.git`, `target`, and `node_modules` |
| Private-key blocks | 0 |
| AWS-style access key ids | 0 |
| GitHub token prefixes | 0 |
| Slack token prefixes | 0 |
| Long `sk-` tokens | 0 |
| Host home-directory prefix | 0 outside this scan's own earlier wording. Those wording hits were removed. |
| Personal mailbox markers from the private history | 0 after the same wording cleanup |
| `compose.yaml` `SCC_INTENT_API_KEY` | Empty default interpolation `${SCC_INTENT_API_KEY:-}`. No stored key. |
| Node tests that assign `apiKey` | Placeholder strings in tests (`secret-value`, `local-secret`, `never-return-this`, `existing-secret`). They are not credentials. |
| DOCX fixture XML | 0 of the markers above |
| G7B Workbench PNG used in the README | Engineering drawing and status text. No key field is filled. The frame is an ancestor acceptance shot, not a new capture. |

## Not done

- No scan of the private 134-commit history. That history is not in this snapshot. Publishing the backup repository would be a different and rejected action.
- No OCR of every PNG. The README uses one reviewed frame.
- `cargo-audit` was not run. The Rust advisory database was not installed. `npm audit` during `npm ci` reported 0 vulnerabilities for the UI lockfile (the UI package and `three`).
- CadQuery 2.8.0 and cadquery-ocp 7.9.3.1.1 license files and the wheel SBOM were opened after this note was first written. The result is in `THIRD_PARTY_NOTICES.md`. The wheel does not contain an OCCT license text. That blocks image redistribution. It does not block publishing AICAD source, which does not vendor those binaries.
- A pre-commit scan of 557 text files found no private-key block, AWS-style key id, GitHub token prefix, Slack token prefix, long `sk-` token, host home-directory prefix, or personal mailbox marker.
- `strings` on the staged PNG files found none of those markers. This is not an OCR of the pixels.
- The local commit author is a placeholder. It is not a confirmed legal maintainer, and it is not taken from the private history.

## Publication gate

This scan does not authorize the first public push. The copyright holder is
still unconfirmed, and `Peng-33/AICAD` already exists as a private repository.
