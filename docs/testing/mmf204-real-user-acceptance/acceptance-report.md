# AICAD MMF-204 Real User Acceptance

Date: 2026-09-19

## Baseline and environment

- Repository: `<repo>`
- Branch: `codex/aicad-pre-rollback-recovery`
- HEAD: `dcdb3f50d479ef0735a0f7c1dc755bd41cab54cc`
- Phase 13.5: confirmed frozen before testing.
- UI URL: `http://127.0.0.1:4173/`
- Engineering Workbench URL: `http://127.0.0.1:4173/engineering.html`
- Runtime: Docker Compose `ui`, healthy; `/api/health` returned `{"status":"ready","version":"scc 0.3.0","intentConfigured":false}`.
- Browser: project Playwright 1.60.0 Chromium container, 1440×1024 viewport.
- Browser page errors: none.

The exact MMF-204 text supplied by the user was saved as
`mmf204-input.txt` and entered into the main page textarea without editing.
Evidence hash: `bd62337d68c356e43674741a164c676670101f78460368df38ebbde4f033f1e7`.

## AICAD actual test result

```text
FULL_MMF204_INPUT = FAIL
SIMPLE_CONTROL_INPUT = FAIL
```

The failure occurred at the natural-language provider boundary, before
parsing or engineering generation. It was not a geometry-kernel assertion.

## Web page actual behavior

On the main AICAD page, the full MMF-204 text was visibly added to the
conversation. After the real Send button was clicked, the page showed:

```text
SCC-UI-INTENT-001
No intent compiler is configured
未配置可选意图模型。你的指令已保留——请选择已验证模板，或直接编辑 Proposal JSON。
```

The Intent stage changed to an error state. M1 Compile, M2 Execute, and M3
Solve remained `尚未开始`. The canvas remained the empty deterministic
workspace; no MMF-204 geometry or drawing appeared. The SVG/DXF labels were
present in the project panel, but no generated artifact existed.

The simple control text was then entered and submitted through the same real
textarea and Send button. It produced the same `SCC-UI-INTENT-001` result.

## Stage results — full MMF-204 main-page input

| Stage | Result | Evidence |
|---|---|---|
| `USER_INPUT_ACCEPTED` | PASS | Exact textarea round-trip; full text was visible in conversation |
| `INTENT_PARSED` | FAIL | HTTP 501 `/api/intent`, `SCC-UI-INTENT-001` |
| `ENGINEERING_COMMANDS_GENERATED` | NOT_REACHED | Intent provider failure |
| `ENGINEERING_MODEL_CREATED` | NOT_REACHED | No proposal/model returned |
| `SOLVE_COMPLETED` | NOT_REACHED | M3 remained not started |
| `DRAWING_MODEL_CREATED` | NOT_REACHED | No engineering model existed |
| `FRONT_VIEW_RENDERED` | NOT_REACHED | No MMF-204 drawing |
| `TOP_VIEW_RENDERED` | NOT_REACHED | No MMF-204 drawing |
| `LEFT_VIEW_RENDERED` | NOT_REACHED | No MMF-204 drawing |
| `EXPORT_AVAILABLE` | NOT_REACHED | No generated SVG/DXF |

## Three-view result

The separate Engineering Workbench page loaded its built-in mounting-plate
fixture. This is not an MMF-204 result and is reported only as a product
surface check.

- The workbench loaded and showed the fixture's principal drawing.
- Selecting `Three-view` produced the real page error:

  `Three-view drawing unavailable — SCC-SPAT-001: 2 engineering spatial readiness issue(s) block lowering`

- The workbench's natural-language edit field is a single-line input and did
  not preserve the full multiline MMF-204 text exactly (`fullInputAccepted =
  false`). Clicking `Apply` nevertheless reached the real endpoint and showed:

  `Intent: error — No intent provider is configured for engineering intent`

Therefore no MMF-204 Front/Top/Left views were generated. The fixture result
cannot be used to claim MMF-204 success.

## MMF-204 geometry verification

No candidate MMF-204 model was created, so every geometric requirement is
unverifiable rather than inferred from a similar shape.

| Requirement | Result |
|---|---|
| 120×120 mm base plate | UNVERIFIABLE — generation not reached |
| 10 mm base thickness | UNVERIFIABLE — generation not reached |
| Four outer R15 corners | UNVERIFIABLE — generation not reached |
| Center boss Ø70 × 15 mm; total height 25 mm | UNVERIFIABLE — generation not reached |
| 1° boss draft | UNVERIFIABLE — generation not reached |
| Center Ø38 H7 through hole | UNVERIFIABLE — generation not reached |
| 1×45° top and bottom chamfers | UNVERIFIABLE — generation not reached |
| 4× Ø5.5 countersunk holes, PCD Ø54, Ø10×5.5 | UNVERIFIABLE — generation not reached |
| 4× Ø8.5 mounting holes at 15 mm offsets | UNVERIFIABLE — generation not reached |
| Ø15×2 bottom counterbores | UNVERIFIABLE — generation not reached |
| Four 4 mm × 12 mm ribs at 45° | UNVERIFIABLE — generation not reached |
| 0.5° rib draft | UNVERIFIABLE — generation not reached |
| R2 rib root fillets | UNVERIFIABLE — generation not reached |
| Surface roughness and anodizing requirements | UNVERIFIABLE — generation not reached |
| C0.2–C0.5 edge deburring | UNVERIFIABLE — generation not reached |
| Flatness, perpendicularity, position and coaxiality controls | UNVERIFIABLE — generation not reached |

## 2D CAD export

No SVG or DXF was generated because the intent stage failed and no geometry
artifact existed. No prebuilt or independently generated file was used.

## Errors and blockers

Observed API responses:

- `501 /api/intent` → `SCC-UI-INTENT-001` for full MMF-204 input.
- `501 /api/intent` → `SCC-UI-INTENT-001` for simple control input.
- `422 /api/engineering/drawing/three-view` → `SCC-SPAT-001` for the built-in
  mounting-plate fixture.
- `501 /api/engineering/intent` → no intent provider configured.

The primary acceptance blocker is that the Docker deployment starts with
`intentConfigured: false`; it has no configured natural-language provider.
The separate fixture check also exposed a spatial readiness failure for its
Three-view request.

## Evidence files

- [Exact MMF-204 input](mmf204-input.txt)
- [Browser result JSON](browser-result.json)
- [Environment and health log](environment.txt)
- [Docker UI log](docker-ui.log)
- [Initial main page screenshot](00-main-initial.png)
- [Full input before submit](01-main-full-input-filled.png)
- [Full input result](02-main-full-input-result.png)
- [Simple control result](04-main-simple-control-result.png)
- [Engineering Workbench initial page](05-engineering-workbench-initial.png)
- [Fixture Three-view result](06-engineering-fixture-three-view.png)
- [Engineering full-input result](07-engineering-full-input-result.png)

The corresponding `.txt` files contain the captured visible page text for each
screenshot. No generated SVG/DXF exists because the UI never reached export.

## Follow-up tasks — not implemented in this test

1. Provide a real, authorized SCC/OpenAI-compatible intent provider in the
   Docker runtime and repeat the exact MMF-204 test.
2. Preserve the full natural-language input path through Engineering Workbench
   without reducing it to a single-line field.
3. Diagnose and fix the observed `SCC-SPAT-001` Three-view readiness failure
   on the built-in mounting-plate fixture.
4. Add a real acceptance path that distinguishes unsupported draft,
   countersink/counterbore, ribs, fillets, chamfers, GD&T, roughness, and
   surface-treatment requirements instead of claiming them as geometry.

No source code or manufacturing code was modified. No push was performed.
