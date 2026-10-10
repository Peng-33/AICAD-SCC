# Phase G5 acceptance report — Project v2 formal STEP

Date: 2026-09-26
Branch: codex/aicad-pre-rollback-recovery
Baseline HEAD: 5da9aa44119d90be029dd83058e70a7825d1057c

## Result

**PHASE_G5 = PARTIAL.** Bounded Project v2 histories now reach the existing canonical STEP API and CLI, share the ordered CadQuery/OCP rebuild executor with drawing, and pass M1–M16 plus real Chromium acceptance. The implementation does not claim general complex-part support.

The remaining acceptance gaps are explicit: X2–X10 have lower-level Rust/OCP validation evidence but were not each exercised through the formal HTTP API; the complete AICAD-COMPLEX-001 specification was not exported as STEP; and Drawing↔STEP analytic dimensions were not cross-compared for every operation in the full machining fixture.

## Acceptance matrix

| Area | Result | Evidence |
|---|---|---|
| Bounded Project v2 manufacturing lowering | PASS | Validated immutable, ordered manufacturing request derived from EngineeringModel v2; no second editable tree or drawing-derived geometry |
| Canonical STEP authority | PASS | Existing POST /api/engineering/export/step and scc engineering export-step accept Project v1 and Project v2; no second endpoint/exporter |
| Project v1 compatibility | PASS | Existing formal STEP regression in the Rust→CadQuery/OCP integration run |
| EngineeringModel v2 M1–M16 | PASS | 16/16 real formal STEP CLI cases; every case reported manufacturing/build/reimport/equivalence passed and delivered hash/size matched the exact CLI bytes |
| Project v2 API positive path | PASS | Real API request for the complete generic machining fixture returned the canonical validated STEP artifact and stage headers |
| API no-partial-artifact failure | PASS, bounded | Detached ADD / multi-body failure returned JSON, no STEP body, and no SHA/size artifact headers |
| X1–X11 formal API failure matrix | PARTIAL | X1 detached ADD and its multi-body outcome were exercised at the API. X2–X10 were not each posted through the HTTP API; lower-level Rust/OCP validation exists for no-intersection/full-body cuts, counterbore and pattern seed, ambiguous semantic edge, fillet radius, and Revolve axis/profile failures |
| STEP export, reimport, geometric/material equivalence | PASS for tested fixtures | Worker validates one valid solid, bounds, volume, area, center of mass, material probes, and analytic feature descriptors on the same exported STEP artifact |
| Exact validated-byte delivery | PASS | Rust and API verified SHA-256 and byte length. Chromium-saved STEP bytes matched the canonical API validation hash |
| Drawing↔STEP conformance | PARTIAL | Shared operation contract/executor; Project v2 worker test compares bounds and volume for the plate/through-hole witness. Full machining fixture analytic dimensions were not all independently cross-compared |
| Browser acceptance | PASS | Isolated Chromium/Playwright with fake Provider exercised the shared home/compatibility entry, Chinese Engineering Workbench, machining and Revolve histories, three views, SVG/DXF download, and canonical STEP download |
| Full AICAD-COMPLEX-001 STEP | NOT_RUN | No named-part builder was added. Existing bounded G3 complex subset is not evidence of the full complex target |
| Real Provider connection | NOT_RUN | No paid Provider request; no key read or changed |

## Formal STEP matrix

Each case entered EngineeringModel v2, lowered to the versioned manufacturing request, ran through the canonical Rust CLI and real CadQuery/OCP STEP exporter, reimported the exported artifact, passed geometric equivalence, and verified SHA-256/size for the delivered bytes.

| Case | Description | STEP bytes |
|---|---|---:|
| M1 | Rectangle Sketch → Extrude ADD | 15,501 |
| M2 | Extrude ADD → THROUGH_ALL Extrude CUT | 30,370 |
| M3 | Circle ADD / cylindrical boss | 19,402 |
| M4 | Blind cut | 33,111 |
| M5 | Through cut | 55,430 |
| M6 | Multi-axis blind Hole | 51,593 |
| M7 | Counterbore | 63,319 |
| M8 | Circular Hole Pattern | 79,033 |
| M9 | Linear Hole Pattern | 90,611 |
| M10 | Semantic Fillet | 93,361 |
| M11 | Semantic Chamfer | 96,061 |
| M12 | Revolve ADD | 13,261 |
| M13 | Revolve CUT | 23,736 |
| M14 | Mixed Extrude and Revolve history | 28,074 |
| M15 | Revolve + Hole + Pattern | 65,439 |
| M16 | ADD → CUT → ADD → Hole → Pattern → Fillet → Chamfer | 96,061 |

## Chromium evidence

The isolated fake-Provider browser run visited / and /engineering.html and confirmed both render the same Simplified Chinese Engineering Workbench, with the English switch available. It entered multiline Chinese engineering intent, generated a generic machining Project v2 fixture and a separate stepped Revolve fixture, viewed Front/Top/Left, downloaded SVG and DXF, and requested STEP through the canonical API. The browser test verified the STEP response stage headers and compared Chromium-downloaded bytes with the API body and SHA-256.

- Machining fixture: SVG 17,616 bytes; DXF 8,029 bytes; STEP 75,479 bytes; SHA-256 3eac6ca5201d85db4ec03b62bb1bf1dd4990b118e3050cf10d67cc4fcfeeb2b6.
- Revolve fixture: STEP 32,726 bytes; SHA-256 be080c6692a651f0ba89ca2ec1b9985a67c4740c6d08ab4264b6307db598a74f.
- Screenshot: aicad-g5-engineering-workbench.png, 1440×1677, SHA-256 5536c3a23e5d72438bbe9b67846ee25609afcf1049683f9e62311874d8309533.
- Real Provider connectivity and protocol behavior were not tested.

## Equivalence and tolerances

ADR-069 records the formal comparison tolerances:

- Bounds and center of mass: 1e-6 mm.
- Volume: 1e-5 mm³ + 1e-9 × max absolute source/import volume.
- Surface area: 1e-5 mm² + 1e-9 × max absolute source/import area.
- Analytic descriptor scalars: 1e-5.
- Material isInside probe tolerance: 1e-7.

Project v2 does not require raw edge/face/vertex count equality as model identity. It compares stable analytic edge/surface descriptors and material witnesses instead.

## Timing sample

One M16 formal STEP observation in the isolated OCP test image:

| Stage | Time |
|---|---:|
| Manufacturing lowering | 0.155 ms |
| CadQuery/OCP build | 371.150 ms |
| STEP serialization | 6.499 ms |
| STEP reimport | 101.756 ms |
| Equivalence | 35.932 ms |
| Worker total | 515.648 ms |
| CLI wall time including process overhead | 1,716 ms |

Project v1 STEP worker sample: lowering 0.137 ms, build 4.957 ms, serialization 2.098 ms, reimport 6.295 ms, equivalence 0.009 ms, total 13.598 ms. These are test-machine observations, not a performance SLA.

## Regression and tool results

| Check | Result |
|---|---|
| cargo fmt --all -- --check | PASS |
| cargo check --workspace --all-targets | PASS |
| cargo test --workspace --all-targets | PASS |
| cargo clippy --workspace --all-targets -- -D warnings | PASS |
| npm test --prefix apps/scc-ui | PASS: 172 total, 155 passed, 0 failed, 17 environment-gated skips |
| Isolated CadQuery/OCP suite | PASS: 102 tests |
| Rust→CadQuery/OCP + formal STEP integration | PASS: 30 tests, including Project v1 and Project v2 M1–M16 |
| Chromium/Playwright browser acceptance | PASS |
| JavaScript syntax checks | PASS |
| Python worker py_compile | PASS |
| docker compose config --quiet | PASS |
| git diff --check | PASS |

The host Node skips are environment-gated OCP/CLI integration cases; real OCP/STEP tests were run separately in the isolated Docker image. Browser acceptance used an internal-only network, no host port mapping, and a fake Provider.

## Deferred and unsupported

Countersink geometry, ExplicitPlacementPattern, Sweep, Loft, NURBS, assemblies and multi-body manufacturing remain deferred or unsupported. The formal worker enforces a single-solid result. The full AICAD-COMPLEX-001 STEP target remains untested; the generic fixtures and bounded historical subset do not establish arbitrary complex mechanical-part support. No compatibility CadDocument STEP path, second API authority, part-specific builder, Drawing Shape cache, or persisted B-Rep was introduced.

The user-owned docs/testing/mmf204-real-user-acceptance/ directory was left untouched and excluded from staging. No API key was read, no Provider was called, and no push was performed.
