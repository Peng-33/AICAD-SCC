# ADR-012: UI Application Boundary and Local Service Adapter

- Status: Accepted for UI implementation
- Date: 2026-08-09

## Context

The frozen architecture defines the UI as an independent interaction and visualization layer. It may orchestrate the LLM intent compiler, M1, M2, and M3, but it must not reproduce compiler, runtime, or geometry business rules. The current implementation exposes those stages through the `scc` CLI rather than an HTTP service.

The UI/UX specification requires an operational workspace covering Proposal editing, compilation, conflict resolution, execution, mutation overlays, geometry solving, CAD export, history, document context, and developer inspection.

## Decision

The first UI application lives in `apps/scc-ui` and uses browser-native HTML, CSS, and ES modules. A dependency-free Node service adapter:

1. serves the UI only on the loopback interface by default;
2. accepts bounded JSON requests;
3. invokes the existing `scc` binary without a shell in a request-scoped temporary directory;
4. returns structured artifacts and diagnostics to the browser;
5. extracts local document text as UI context without sending it externally; and
6. forwards natural-language intent only to an explicitly configured OpenAI-compatible or SCC Intent endpoint; and
7. keeps UI-entered API secrets only in service memory and never returns them to the browser.

Project snapshots and UI preferences are stored locally in the browser. They are presentation state, not GraphState or compiler authority. The service adapter does not mutate SCC internals and does not infer Proposal, ConstraintIR, GraphState, or geometry.

The same application and API contract run on Linux and Windows. Platform-specific behavior is limited to resolving the native `scc`/`scc.exe` path and optional PDF utility. Interface copy is available in English and Simplified Chinese; locale selection is presentation state and never changes compiler input or semantics.

## API surface

- `GET /api/health`
- `GET /api/intent-config`
- `PATCH /api/intent-config`
- `GET /api/examples`
- `GET /api/examples/:id`
- `POST /api/intent`
- `POST /api/compile`
- `POST /api/execute`
- `POST /api/inspect`
- `POST /api/solve`
- `POST /api/export`
- `POST /api/documents/extract`

## Security and operational constraints

- Listen on `127.0.0.1` unless the operator explicitly changes the host.
- Apply a request-body limit and reject unsupported content types.
- Use allowlisted example identifiers and normalized static paths.
- Invoke binaries with argument arrays and `shell: false`.
- Remove every request-scoped temporary directory.
- Runtime endpoint changes require same-origin JSON requests and accept only bounded HTTP(S) URLs without embedded credentials.
- API keys require HTTPS except for loopback targets, remain in process memory, and are redacted from every response and log.
- Treat provider responses as untrusted: validate JSON and response shape, then preflight the Proposal and Mutation through the authoritative SCC M1→M2→M3 pipeline before returning them to the UI. Conflict outcomes remain reviewable; other pipeline failures may trigger one bounded OpenAI-compatible correction attempt containing sanitized diagnostics. The browser does not reproduce these rules.
- Preserve provider and M1 rejection details in the conversation and bounded Debug log so the accessible transient notification is not the only failure evidence.
- Bound the number and size of document excerpts forwarded to a model.
- Do not persist uploaded documents on disk after extraction.

## Consequences

- The complete deterministic SCC pipeline is usable without duplicating domain logic in JavaScript.
- The application has no npm runtime dependencies and can be audited or packaged easily.
- An LLM provider remains optional and replaceable; structured Proposal editing and examples keep the application fully usable without one.
- Users can configure a provider without restarting the local UI, while environment variables remain the managed-deployment default after restart.
- PDF extraction requires `pdftotext`; Markdown, text, JSON, and DOCX extraction remain available without npm packages.
- Native Linux and Windows operation share one source tree, while the CI matrix guards path and process-spawn portability.
