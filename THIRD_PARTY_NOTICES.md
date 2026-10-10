# Third-party notices

Checked on 2026-10-10 for the local 0.4.0-alpha.1 prep tree. This is an
inventory of files and metadata that were actually read. It is not a legal
opinion. The AICAD grant is in `NOTICE`. This file does not relicense
the third-party components below.

## Rust lockfile

`cargo metadata --locked --format-version 1` listed 68 packages.

The 18 workspace crates are `scc-analysis`, `scc-cad-backend`, `scc-cli`,
`scc-core`, `scc-drawing`, `scc-dsl`, `scc-engineering`, `scc-execution`,
`scc-geometry`, `scc-lowering`, `scc-manufacturing`, `scc-project`,
`scc-projection`, `scc-projection-consistency`, `scc-proposal`,
`scc-spatial`, `scc-tools`, and `scc-visibility`. Each is version
`0.4.0-alpha.1` with the Cargo `license` field `Apache-2.0`. That field
matches the Apache-2.0 grant recorded in `NOTICE`. Third-party crates
below are not covered by that grant.

The other 50 packages are third-party crates. None had an empty license
field. Counts:

| Packages | SPDX field in cargo metadata |
| --- | --- |
| 39 | `MIT OR Apache-2.0` |
| 3 | `MIT` |
| 2 | `Apache-2.0 OR MIT` (`fastrand` 2.4.1, `utf8parse` 0.2.2) |
| 2 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` (`linux-raw-sys` 0.12.1, `rustix` 1.1.4) |
| 1 | `Unlicense OR MIT` (`memchr` 2.8.3) |
| 1 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` (`r-efi` 6.0.0) |
| 1 | `(MIT OR Apache-2.0) AND Unicode-3.0` (`unicode-ident` 1.0.24) |
| 1 | `MIT/Apache-2.0` (`version_check` 0.9.5) |

`r-efi` offers MIT or Apache-2.0 as alternatives to LGPL-2.1-or-later. This
notice does not hide that third alternative. This tree does not vendor crate
sources. A build fetches them from the lockfile.

## Node

`apps/scc-ui/package-lock.json` records `three` 0.184.0 with `"license": "MIT"`.
The tarball was not unpacked. `npm ci` for that lockfile reported 0
vulnerabilities. Playwright 1.60.0 is an e2e dependency only. `.dockerignore`
excludes the e2e directory from the server image.

## CadQuery, OCP, and Open CASCADE

`apps/scc-ui/cad/requirements.txt` pins `cadquery==2.8.0` and nothing else.
A local virtual environment installed that pin on 2026-10-10. The environment
was created from the host Python 3.12.3, with system site packages disabled.
The Docker image pins Python 3.12.11. This host check is not that image.

Packages that pip installed for this pin:

| Package | Version | License evidence opened |
| --- | --- | --- |
| cadquery | 2.8.0 | Metadata says "Apache Public License 2.0". `cadquery-2.8.0.dist-info/licenses/LICENSE` is the Apache-2.0 text and states that CadQuery may be redistributed under Apache Public License v2.0. `Requires-Dist` includes `cadquery-ocp<8.0,>=7.9.3.1`. |
| cadquery-ocp | 7.9.3.1.1 | Metadata `License: Apache-2.0` and the Apache classifier. `cadquery_ocp/LICENSE` is the Apache-2.0 text. Its appendix says "Copyright 2020 OCP contributors". |
| cadquery-ocp-proxy | 7.9.3.1.1 | Metadata `License: Apache-2.0`. The package file list has no license file. |

`cadquery_ocp.libs` contains 47 `libTK*.so.7.9.3` libraries. A STEP file
exported by `scc engineering export-step` on this tree names
`Open CASCADE STEP processor 7.9` in the header. The wheel SBOM
`cadquery_ocp-7.9.3.1.1.dist-info/sboms/auditwheel.cdx.json` also lists
bundled Ubuntu 20.04 libraries, including freetype, freeimage, webp,
openexr, tiff, png, lcms, openjpeg, jxr, fontconfig, jbig, libraw,
jpeg-turbo, and ilmbase. That SBOM has no license expressions.

No LGPL text, no `OCCT-exception-1.0`, and no notices for those bundled
libraries were found beside the `.so` files. The only license file inside
`cadquery-ocp` is the Apache-2.0 text for the binding.

Consequences:

- Publishing this source tree does not redistribute those binaries.
- `apps/scc-ui/Dockerfile` runs `pip install` of `cadquery==2.8.0` while
  building an image. That step downloads the wheel and copies the native
  libraries into the image. The wheel, as installed here, does not carry
  the notices required to redistribute Open CASCADE or the bundled system
  libraries. Do not publish that image until those notices are added.
- The intended Apache-2.0 terms for AICAD source do not relicense Open
  CASCADE or the libraries bundled in the wheel.

## Container base images

This session did not pull `rust:1.96.0-bookworm`,
`python:3.12.11-slim-bookworm`, or `node:24.18.1-bookworm-slim`. A Docker
daemon pull of the Rust image timed out. The image was not built here.
Those base-image licenses apply to anyone who builds or publishes the image.

## Names

This file does not grant trademark rights in "Open CASCADE", "CadQuery",
or "AICAD". Whether "AICAD" conflicts with an existing mark was not decided.
