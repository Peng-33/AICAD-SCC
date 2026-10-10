# SCC UI Implementation Matrix

This matrix translates `SCC_UI_UX_Design_Specification.docx` and `SCC_Architecture_Specification.docx` into verifiable implementation requirements.

| Requirement | Implementation evidence | Verification evidence |
|---|---|---|
| Welcome and project start | Welcome dialog with role selection, blank start, and verified quick templates | Chromium welcome interaction and Firefox pixel inspection |
| Three-column workspace | Intent/documents left rail, mode viewport, inspector right rail, toolbar and bottom composer | Chromium layout audit at 1440, 1024, 768, and 320 px |
| Natural-language intent | Chat history plus UI-configurable OpenAI-compatible/SCC provider, exact Proposal/M3 capability contract, current-Proposal context, real M1→M2→M3 preflight, one bounded OpenAI correction attempt, and persistent diagnostics | Provider correction/diagnostic tests, real unsupported-On preflight test, and unavailable-provider browser test |
| Generic natural-language 3D intent | Data-only CadDocument/FeatureGraph contract, stable parameters/features, explicit requirement dispositions, one bounded model repair, and no executable generated code | FeatureGraph contract tests plus flange/bracket real-kernel tests |
| Requirement accountability | Build manifest records every feature result and complete/partial requirement coverage; unsupported or ambiguous intent stays visible | Manifest boundary test and browser coverage assertion |
| Editable 3D CAD | Shared OCCT feature executor, source-hash rebuilds, interactive GLB preview, and JSON/STEP/BREP/STL/GLB/SVG artifacts | Real flange parameter edit 160→170, STEP download, and legacy gearbox regression |
| PDF, DOCX, Markdown upload | Bounded extraction endpoint; extracted text remains local unless intent forwarding is configured | Markdown/DOCX unit tests and browser upload test; PDF utility path is explicit |
| Proposal viewer/editor | Categorized tree, raw JSON editor, format/apply actions | Proposal edit and invalid JSON tests |
| M1 compile and conflicts | Real `scc compile` adapter, stage state, plain-language conflict panel | Real Polygon pipeline and Conflict template browser tests |
| ConstraintIR graph | Deterministic node-edge SVG and raw IR viewer | Browser graph-mode test |
| M2 execution and mutation | Real `scc execute`, four MutationPatch operations, mutation list, and raw GraphState | Mutation, undo/redo, keyboard-run, and real execution browser tests |
| Runtime trace | Snapshot timeline with keyboard/click selection and raw artifact views | Trace-mode browser test |
| M3 geometry | Real `scc solve`, SVG preview, pointer/keyboard pan, zoom, and fit | Polygon integration plus pan/zoom/fit browser tests |
| CAD export | Real deterministic SVG and DXF download | Integration content assertions and browser download events |
| Property inspector | Proposal/graph/trace selection plus runtime parameter-to-mutation actions | Proposal selection and Mutation browser tests |
| History | Local artifact snapshots with timestamp labels and restore | History restore and reload persistence browser tests |
| User/debug modes | Simplified default view and raw Proposal/IR/State/Trace/log tabs | Mode-toggle browser test |
| Settings | Project role/name, language, reduced motion, and runtime intent provider/endpoint/model/key management | Browser settings test plus API secret-redaction and same-origin contract tests |
| Chinese and English | Browser-locale default, desktop EN/中文 switch, settings fallback, translated dynamic status, and persisted locale | Locale unit tests plus Chromium switch/reload assertions |
| Linux and Windows | Platform-neutral Node adapter, native CLI path resolution, PowerShell instructions, and container option | UI/CLI integration matrix on `ubuntu-latest` and `windows-latest`; Linux local/container acceptance |
| Notifications and stage indicator | Accessible live-region messages and Intent→M1→M2→M3 states | Browser stage/status assertions and source semantics |
| Responsive behavior | Persistent desktop rails and collapsible tablet/mobile sheets | No-overflow browser audit at four breakpoints |
| Accessibility | Skip link, landmarks, semantic controls, accessible names, visible focus, keyboard paths, reduced motion, non-color status | Browser landmark/name/keyboard assertions and checklist review |
| Architecture boundary | UI calls the CLI adapter and contains no compiler/solver rules | ADR-012 and source review |

Verification commands:

```bash
SCC_BIN="$PWD/target/release/scc" npm test --prefix apps/scc-ui
docker compose run --rm e2e
```

Every row has direct source plus runtime or test evidence as of 2026-08-09. Windows runtime evidence is produced by the checked-in CI matrix after publication; local acceptance covers Linux and the fixed container environment. A rendered shell without working pipeline actions does not satisfy this matrix.
