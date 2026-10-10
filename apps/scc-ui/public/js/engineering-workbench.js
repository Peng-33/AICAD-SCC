import { tr, describeEngineeringError, getEngineeringLocale } from "./engineering-i18n.js";
import { fingerprintModel } from "./engineering-project.js";

// AICAD Engineering Workbench (Phase 7, ADR-021).
//
// The workbench is a thin view over the derived `EngineeringWorkbenchView`
// JSON produced by the Rust/CLI. It never computes derived values, never
// parses formulas, never reads entity-id prefixes, and never mutates the
// EngineeringModel directly — every edit is a tool call through the backend
// (`/api/engineering/tool`), and every NL edit goes through Phase 6
// (`/api/engineering/intent`).
//
// The pure mapping functions at the top are unit-tested in Node without a
// browser (test/engineering-workbench.test.mjs).

const MAX_HISTORY = 50;

// Panel headings (Phase 8.10 naming isolation: the authoritative drawing
// preview vs. the raw entity debug view).
export const DRAWING_PANEL_TITLE = "Engineering Drawing";
export const GEOMETRY_DEBUG_PANEL_TITLE = "Geometry Debug";

// ---------- pure panel-data builders ----------

export function buildFeatureTree(workbench) {
  const root = { id: workbench?.part?.id ?? "part", label: workbench?.part?.name ?? "Part", type: "part", children: [] };
  for (const feature of workbench?.features ?? []) {
    root.children.push({ id: feature.id, label: feature.id, type: feature.feature_type, slots: feature.slots });
  }
  return root;
}

export function buildProperties(feature) {
  if (!feature) return [];
  return (feature.slots ?? []).map((slot) => ({
    slot: slot.slot,
    label: humanize(slot.label),
    value: slot.display_value,
    source: slot.source,
    parameter: slot.parameter,
    editable: slot.editable,
    unit: slot.unit ?? null,
    parameterReference: slot.parameter_reference ?? null,
    unresolved: slot.display_value === "UNRESOLVED",
  }));
}

export function buildRelations(workbench) {
  return (workbench?.parameters ?? [])
    .filter((parameter) => parameter.origin === "derived" || parameter.expression)
    .map((parameter) => ({
      id: parameter.id,
      value: parameter.value,
      expression: parameter.expression,
      dependencies: parameter.dependencies ?? [],
      editable: parameter.editable,
    }));
}

export function buildConstraints(workbench) {
  return (workbench?.constraints ?? []).map((constraint) => ({
    id: constraint.id,
    kind: constraint.kind,
    source: constraint.source,
    target: constraint.target,
  }));
}

export function buildReview(workbench) {
  const summary = workbench?.summary ?? { confirmed: 0, derived: 0, default: 0, unresolved: 0, warnings: 0 };
  const unresolved = (workbench?.unresolved ?? []).map((id) => ({ id }));
  // Find which features consume each unresolved parameter via a bound slot.
  for (const item of unresolved) {
    item.usedBy = [];
    for (const feature of workbench?.features ?? []) {
      for (const slot of feature.slots ?? []) {
        if (slot.parameter === item.id) item.usedBy.push(`${feature.id}.${slot.label}`);
      }
    }
  }
  return { summary, unresolved };
}

export function humanize(snake) {
  return snake.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());
}

// ---------- engineering drawing panel (Phase 8.10 / Phase 9.8) ----------
//
// The drawing preview is a thin transport over the formal backend endpoints:
// the SVG comes verbatim from scc-drawing via the CLI; this module never parses
// engineering semantics, computes geometry, or injects SVG markup. Rendering
// remains Blob → object URL → <img>, with strict revoke.
//
// The selected mode is presentation state only. It is deliberately not part
// of the EngineeringModel, command protocol, dirty state, or undo history.

export const DRAWING_MODE_PRINCIPAL = "principal";
export const DRAWING_MODE_THREE_VIEW = "three-view";
export const DRAWING_MODE_SECTION_AA = "section-a-a";
const DRAWING_MODES = new Set([DRAWING_MODE_PRINCIPAL, DRAWING_MODE_THREE_VIEW, DRAWING_MODE_SECTION_AA]);

export function drawingModeLabel(mode) {
  return tr(mode === DRAWING_MODE_THREE_VIEW ? "Three-view" : mode === DRAWING_MODE_SECTION_AA ? "Section A-A" : "Principal");
}

function projectionMethodLabel(method) {
  const value = String(method).replaceAll("_", "-");
  return value ? tr(value[0].toUpperCase() + value.slice(1)) : value;
}

/** Monotonic request-generation counter for stale-response rejection. */
export function createGenerationGuard() {
  let generation = 0;
  return {
    begin: () => ++generation,
    isActive: (generation_) => generation_ === generation,
  };
}

/** Human-readable sheet metadata line. Values are backend-derived. */
export function formatMeta(payload, mode = null) {
  const resolvedMode = payload?.drawing_mode === "three_view" ? DRAWING_MODE_THREE_VIEW
    : payload?.drawing_mode === "three_view_section" ? DRAWING_MODE_SECTION_AA : mode;
  const parts = [];
  if (resolvedMode) parts.push(drawingModeLabel(resolvedMode));
  if (payload?.projection_method) parts.push(projectionMethodLabel(payload.projection_method));
  if (payload?.sheet_width_mm !== undefined && payload?.sheet_height_mm !== undefined) {
    parts.push(tr("Sheet") + " " + payload.sheet_width_mm + "×" + payload.sheet_height_mm + " mm");
  }
  if (payload?.scale !== undefined) parts.push(tr("Scale") + " " + payload.scale);
  if (payload?.part_title) parts.push(payload.part_title);
  return parts.join(" · ");
}

function defaultBlobDeps() {
  return {
    Blob: globalThis.Blob,
    createObjectURL: (blob) => URL.createObjectURL(blob),
    revokeObjectURL: (url) => URL.revokeObjectURL(url),
  };
}

function validateDrawingPayload(payload) {
  if (!payload || typeof payload.svg !== "string" || !payload.svg.trim().startsWith("<svg")) {
    throw Object.assign(new Error("The drawing renderer returned an invalid SVG payload"), {
      code: "SCC-UI-DRAWING-002",
    });
  }
}

/** Pure reducer: success → a fresh object URL. The previous URL is revoked
 * exactly once, and only after the new URL has been created. */
export function applyDrawingResult(state, payload, blobDeps = defaultBlobDeps(), mode = null) {
  validateDrawingPayload(payload);
  const blob = new blobDeps.Blob([payload.svg], { type: "image/svg+xml" });
  const url = blobDeps.createObjectURL(blob);
  if (state.url) blobDeps.revokeObjectURL(state.url);
  return {
    ...state,
    url,
    meta: formatMeta(payload, mode),
    error: null,
    payload,
    loading: false,
  };
}

/** Pure reducer: failure → keep the old image/meta, record the structured
 * error, and leave the selected mode unchanged. */
export function showDrawingError(state, err, mode = null) {
  const error = {
    code: err?.code ?? err?.error?.code ?? "SCC-UI-DRAWING-001",
    message: err?.message ?? err?.error?.message ?? "Drawing request failed",
    ...((err?.details ?? err?.error?.details) ? { details: err?.details ?? err?.error?.details } : {}),
  };
  if (mode) error.mode = mode;
  return {
    ...state,
    loading: false,
    error,
  };
}

/** Paint a drawing state into explicit DOM elements (no SVG parsing or
 * post-processing). */
function paintDrawingDom(elements, state, makeEl = el) {
  const { error: errorBox, stage, meta } = elements;
  if (meta) meta.textContent = state.meta ?? "";
  if (errorBox) {
    const activeMode = state.error?.mode ?? state.mode;
    const prefix = activeMode
      ? getEngineeringLocale() === "zh-CN" ? `${drawingModeLabel(activeMode)} · ` : `${drawingModeLabel(activeMode)} drawing unavailable — `
      : "";
    errorBox.textContent = state.error
      ? prefix + describeEngineeringError(state.error)
      : "";
    errorBox.hidden = !state.error;
  }
  if (!stage) return;
  stage.replaceChildren();
  if (state.url) {
    const img = makeEl("img", "wb-drawing-img");
    img.alt = tr("Engineering drawing");
    img.src = state.url;
    stage.appendChild(img);
  } else if (!state.loading) {
    stage.appendChild(makeEl("div", "wb-note", tr("No drawing yet.")));
  }
  if (state.loading) {
    stage.appendChild(makeEl("div", "wb-note", tr("Generating drawing…")));
  }
}

/** Controller owning the drawing-panel state machine. DOM access goes through
 * the injected dom.query and createEl (defaults to document.querySelector /
 * createElement) so tests can supply fakes; blob handling is injectable via
 * blobDeps for Node tests. */
export function createDrawingPanelController({ api, dom, blobDeps, createEl } = {}) {
  const resolveDom = dom ?? { query: (selector) => globalThis.document?.querySelector(selector) ?? null };
  const makeEl = createEl ?? el;
  const blobs = blobDeps ?? defaultBlobDeps();
  const guard = createGenerationGuard();
  let mode = DRAWING_MODE_PRINCIPAL;
  let sectionPlane = { axis: "z", offset_mm: 5, view_side: "positive" };
  let lastModel;
  let lastRequestContext = {};
  let state = { url: null, meta: null, error: null, mode, loading: false, modelFingerprint: null };

  function queryPanel() {
    return {
      error: resolveDom.query(".wb-drawing-error"),
      stage: resolveDom.query(".wb-drawing-stage"),
      meta: resolveDom.query(".wb-drawing-meta"),
    };
  }

  function patch() {
    paintDrawingDom(queryPanel(), state, makeEl);
  }

  function apiForMode(selectedMode) {
    const method = selectedMode === DRAWING_MODE_THREE_VIEW
      ? "engineeringThreeViewDrawing"
      : selectedMode === DRAWING_MODE_SECTION_AA ? "engineeringSectionDrawing" : "engineeringDrawing";
    if (typeof api?.[method] !== "function") {
      throw Object.assign(new Error("Drawing API method " + method + " is unavailable"), {
        code: "SCC-UI-DRAWING-003",
      });
    }
    return api[method];
  }

  async function refresh(model, requestContext = lastRequestContext) {
    lastModel = model;
    lastRequestContext = requestContext ?? {};
    const requestMode = mode;
    const generation = guard.begin();
    state = { ...state, mode: requestMode, modelFingerprint: fingerprintModel(model), loading: true, error: null, payload: null };
    patch();
    try {
      const payload = requestMode === DRAWING_MODE_SECTION_AA
        ? await apiForMode(requestMode).call(api, model, sectionPlane)
        : await apiForMode(requestMode).call(api, model, lastRequestContext.userIntent ?? null);
      if (!guard.isActive(generation)) return;
      state = applyDrawingResult(state, payload, blobs, requestMode);
      patch();
    } catch (err) {
      if (!guard.isActive(generation)) return;
      state = showDrawingError(state, err, requestMode);
      patch();
    }
  }

  function setMode(nextMode) {
    if (!DRAWING_MODES.has(nextMode)) {
      throw new TypeError("Unsupported drawing mode: " + nextMode);
    }
    if (mode === nextMode && lastModel !== undefined) return Promise.resolve();
    mode = nextMode;
    if (lastModel === undefined) {
      guard.begin(); // invalidate any pre-model request, defensively
      state = { ...state, mode, loading: false, error: null };
      patch();
      return Promise.resolve();
    }
    return refresh(lastModel, lastRequestContext);
  }

  function setSectionPlane(plane) {
    if (!plane || !["x", "y", "z"].includes(plane.axis)
      || !Number.isFinite(Number(plane.offset_mm))
      || !["positive", "negative"].includes(plane.view_side)) {
      throw new TypeError("Invalid Section A-A plane");
    }
    sectionPlane = { axis: plane.axis, offset_mm: Number(plane.offset_mm), view_side: plane.view_side };
    mode = DRAWING_MODE_SECTION_AA;
    return lastModel === undefined ? Promise.resolve() : refresh(lastModel, lastRequestContext);
  }

  function dispose() {
    guard.begin(); // late success/error must not touch disposed DOM or URLs
    if (state.url) blobs.revokeObjectURL(state.url);
    lastModel = undefined;
    lastRequestContext = {};
    state = { url: null, meta: null, error: null, mode, loading: false };
  }

  return {
    refresh,
    setMode,
    setSectionPlane,
    getMode: () => mode,
    getSectionPlane: () => ({ ...sectionPlane }),
    dispose,
    getState: () => state,
  };
}

//// ---------- DOM rendering ----------

export function renderWorkbench(container, snapshot, handlers = {}) {
  const view = snapshot?.workbench ?? { features: [], parameters: [], constraints: [], unresolved: [], summary: {} };
  container.replaceChildren();

  const root = el("div", "wb-root");
  root.appendChild(renderTopBar(snapshot, handlers));
  const columns = el("div", "wb-columns");
  columns.appendChild(renderFeatureTree(buildFeatureTree(view), handlers.onSelectFeature));
  columns.appendChild(renderCenter(snapshot, handlers));
  columns.appendChild(renderInspector(snapshot, handlers));
  root.appendChild(columns);
  container.appendChild(root);
}

function renderTopBar(snapshot, handlers) {
  const bar = el("div", "wb-topbar");
  const title = el("span", "wb-title", `${tr("Part")}: ${snapshot?.workbench?.part?.name ?? "—"}`);
  const undo = el("button", "wb-btn", tr("Undo"));
  undo.addEventListener("click", () => handlers.onUndo?.());
  const redo = el("button", "wb-btn", tr("Redo"));
  redo.addEventListener("click", () => handlers.onRedo?.());
  const input = el("textarea", "wb-nl");
  input.rows = 3;
  input.placeholder = tr("Natural language engineering request… e.g. 把板宽改成160");
  const apply = el("button", "wb-btn", tr("Apply"));
  apply.addEventListener("click", () => handlers.onNaturalLanguage?.(input.value));
  bar.append(title, undo, redo, input, apply);
  return bar;
}

function renderFeatureTree(tree, onSelect) {
  const column = el("div", "wb-column wb-feature-tree");
  column.appendChild(el("h3", "wb-panel-title", tr("Features")));
  const list = el("ul", "wb-tree");
  for (const feature of tree.children) {
    const item = el("li", "wb-tree-item", `${feature.label}`);
    const type = el("span", "wb-tree-type", ` ${tr(feature.type)}`);
    item.appendChild(type);
    item.addEventListener("click", () => onSelect?.(feature.id));
    list.appendChild(item);
  }
  column.appendChild(list);
  return column;
}

function renderCenter(snapshot, handlers) {
  const column = el("div", "wb-column wb-center");
  column.appendChild(renderDrawingSection(handlers.drawingController, handlers.onDrawingModeChange, handlers.onSectionChange));
  // Raw solved-geometry debug view: entity click-to-inspect only. This is NOT
  // the engineering drawing preview (see the section above).
  const debug = el("details", "wb-debug");
  debug.appendChild(el("summary", "wb-panel-title", tr(GEOMETRY_DEBUG_PANEL_TITLE)));
  const geometry = snapshot?.geometry;
  if (geometry) {
    const viewport = el("div", "wb-viewport");
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "-100 -100 200 200");
    for (const entity of Object.values(geometry.entities ?? {})) {
      svg.appendChild(renderPrimitive(entity, handlers.onSelectEntity));
    }
    viewport.appendChild(svg);
    debug.appendChild(viewport);
  } else {
    debug.appendChild(el("div", "wb-note", snapshot?.solve_error ? describeEngineeringError(snapshot.solve_error) : tr("No geometry (unresolved or unsolved).")));
  }
  column.appendChild(debug);
  return column;
}

/** The authoritative drawing preview. State comes from the controller (no
 * fetch here); the controller patches the live DOM when a refresh lands. */
function renderDrawingSection(controller, onModeChange, onSectionChange) {
  const wrap = el("div", "wb-drawing");
  const heading = el("div", "wb-drawing-heading");
  heading.appendChild(el("h3", "wb-panel-title", tr(DRAWING_PANEL_TITLE)));
  const mode = document.createElement("select");
  mode.className = "wb-drawing-mode";
  mode.setAttribute("aria-label", tr("Drawing mode"));
  for (const option of [
    [DRAWING_MODE_PRINCIPAL, tr("Principal")],
    [DRAWING_MODE_THREE_VIEW, tr("Three-view")],
    [DRAWING_MODE_SECTION_AA, tr("Section A-A")],
  ]) {
    const item = document.createElement("option");
    item.value = option[0];
    item.textContent = option[1];
    mode.appendChild(item);
  }
  mode.value = controller?.getMode?.() ?? DRAWING_MODE_PRINCIPAL;
  mode.addEventListener("change", () => onModeChange?.(mode.value));
  heading.appendChild(mode);
  wrap.appendChild(heading);
  if (mode.value === DRAWING_MODE_THREE_VIEW) wrap.appendChild(el("p", "wb-view-caption", tr("Front / Top / Left")));
  if (mode.value === DRAWING_MODE_SECTION_AA) wrap.appendChild(el("p", "wb-view-caption", tr("Front / Top / Left / Section A-A")));

  const sectionControls = el("div", "wb-section-controls");
  const axisLabel = el("label", "wb-section-label", tr("Section plane"));
  const axis = document.createElement("select"); axis.setAttribute("aria-label", tr("Section axis"));
  for (const value of ["x", "y", "z"]) { const option = document.createElement("option"); option.value = value; option.textContent = value.toUpperCase(); axis.appendChild(option); }
  const selectedPlane = controller?.getSectionPlane?.() ?? { axis: "z", offset_mm: 5, view_side: "positive" };
  axis.value = selectedPlane.axis;
  const offset = document.createElement("input"); offset.type = "number"; offset.step = "any"; offset.value = String(selectedPlane.offset_mm); offset.setAttribute("aria-label", tr("Section offset mm"));
  const side = document.createElement("select"); side.setAttribute("aria-label", tr("Viewing direction"));
  for (const [value, label] of [["positive", tr("Positive direction")], ["negative", tr("Negative direction")]]) { const option = document.createElement("option"); option.value = value; option.textContent = label; side.appendChild(option); }
  side.value = selectedPlane.view_side;
  const generateSection = el("button", "wb-btn", tr("Generate Section A-A"));
  generateSection.addEventListener("click", () => { mode.value = DRAWING_MODE_SECTION_AA; onSectionChange?.({ axis: axis.value, offset_mm: offset.valueAsNumber, view_side: side.value }); });
  axisLabel.append(axis, offset, side);
  sectionControls.append(axisLabel, generateSection);
  wrap.appendChild(sectionControls);

  const errorBox = el("div", "wb-drawing-error");
  errorBox.hidden = true;
  const stage = el("div", "wb-drawing-stage");
  const meta = el("div", "wb-drawing-meta");
  wrap.append(errorBox, stage, meta);
  paintDrawingDom(
    { error: errorBox, stage, meta },
    controller?.getState?.() ?? {
      url: null,
      meta: null,
      error: null,
      mode: DRAWING_MODE_PRINCIPAL,
      loading: false,
    },
  );
  return wrap;
}

function renderPrimitive(entity, onSelectEntity) {
  const g = document.createElementNS("http://www.w3.org/2000/svg", "g");
  g.setAttribute("data-entity-id", entity.id);
  g.addEventListener("click", () => onSelectEntity?.(entity.id));
  const p = entity.primitive;
  if (p.kind === "circle") {
    const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
    circle.setAttribute("cx", p.center.x);
    circle.setAttribute("cy", p.center.y);
    circle.setAttribute("r", p.radius);
    circle.setAttribute("stroke", "black");
    circle.setAttribute("fill", "none");
    g.appendChild(circle);
  } else if (p.kind === "rectangle") {
    const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    rect.setAttribute("x", p.origin.x);
    rect.setAttribute("y", p.origin.y);
    rect.setAttribute("width", p.width);
    rect.setAttribute("height", p.height);
    rect.setAttribute("stroke", "black");
    rect.setAttribute("fill", "none");
    g.appendChild(rect);
  }
  return g;
}

function renderInspector(snapshot, handlers) {
  const column = el("div", "wb-column wb-inspector");
  const selected = snapshot?.selectedFeature;
  column.appendChild(el("h3", "wb-panel-title", tr("Properties")));
  if (selected) {
    column.appendChild(el("div", "wb-selected-feature", `${tr("Selected feature")}: ${selected.id} · ${tr(selected.feature_type ?? "")}`));
    for (const prop of buildProperties(selected)) {
      const row = el("div", "wb-prop");
      const label = el("span", "wb-prop-label", tr(prop.label));
      const value = el("span", "wb-prop-value", prop.unresolved ? `${tr("Unresolved")} ⚠` : `${tr(String(prop.value))}${prop.unit ? ` ${prop.unit}` : ""}`);
      const badge = el("span", `wb-badge wb-${prop.source.startsWith("parameter") ? "bound" : "literal"}`, prop.source);
      row.append(label, value, badge);
      if (prop.editable) {
        const input = el("input", "wb-prop-input");
        input.type = "number";
        input.step = "any";
        input.min = "0";
        input.setAttribute("aria-label", `${tr("Edit")} ${tr(prop.label)} ${prop.unit ?? ""}`.trim());
        input.value = prop.unresolved ? "" : prop.value;
        input.addEventListener("change", () => {
          const reference = prop.parameterReference;
          if (!reference) return;
          const toolCall = { name: "set_feature_parameter", arguments: {
            part_id: reference.part_id,
            feature: reference.feature_id,
            slot: reference.slot,
            parameter: reference.parameter_id,
            expected_type: reference.expected_type,
            value: Number(input.value),
          } };
          handlers.onEdit?.(toolCall);
        });
        row.appendChild(input);
      }
      column.appendChild(row);
    }
    const topologyReferences = selected.topology_references ?? [];
    if (topologyReferences.length) {
      column.appendChild(el("h3", "wb-panel-title", tr("Semantic topology references")));
      const resolution = snapshot?.topologyResolution ?? {};
      const currentRevision = resolution.matchesCurrentModel === true;
      for (const reference of topologyReferences) {
        let status = "awaiting_rebuild";
        if (currentRevision && resolution.loading) status = "resolving";
        else if (currentRevision && resolution.succeeded) status = "resolved";
        else if (currentRevision && resolution.failed) {
          status = resolution.featureId === selected.id && String(resolution.code ?? "").includes("TOPOLOGY") ? "failed" : "not_verified";
        }
        const item = el("div", `wb-topology-reference wb-topology-${status}`);
        item.appendChild(el("div", "wb-topology-source", `${tr("Source feature")}: ${reference.source_feature_id}`));
        item.appendChild(el("div", "wb-topology-role", `${tr("Role")}: ${tr(reference.role)}${reference.identity ? ` · ${reference.identity}` : ""}`));
        item.appendChild(el("div", "wb-topology-relation", `${tr("Relation")}: ${tr(reference.relation)}`));
        item.appendChild(el("div", "wb-topology-status", `${tr("Status")}: ${tr(status)}`));
        const diagnostic = status === "failed" || status === "not_verified" ? resolution.code : null;
        if (diagnostic) item.appendChild(el("div", "wb-topology-diagnostic", `${tr("Diagnostic")}: ${diagnostic}`));
        column.appendChild(item);
      }
    }
  }
  column.appendChild(el("h3", "wb-panel-title", tr("Relations")));
  const relations = buildRelations(snapshot?.workbench);
  for (const relation of relations) {
    const item = el("div", "wb-relation");
    item.appendChild(el("div", "wb-relation-formula", `${relation.id} = ${relation.expression ?? relation.value}`));
    for (const dependency of relation.dependencies) item.appendChild(el("div", "wb-relation-dep", `${tr("depends on")} ${dependency}`));
    column.appendChild(item);
  }
  column.appendChild(el("h3", "wb-panel-title", tr("Constraints")));
  for (const constraint of buildConstraints(snapshot?.workbench)) {
    column.appendChild(el("div", "wb-constraint", `${constraint.kind} ${constraint.source} ↔ ${constraint.target}`));
  }
  column.appendChild(el("h3", "wb-panel-title", tr("Review")));
  const review = buildReview(snapshot?.workbench);
  column.appendChild(el("div", "wb-review-summary",
    `${tr("User")} ${review.summary.confirmed} · ${tr("Derived")} ${review.summary.derived} · ${tr("Unresolved")} ${review.summary.unresolved}`));
  for (const unresolved of review.unresolved) {
    const item = el("div", "wb-review-unresolved", `⚠ ${unresolved.id}`);
    for (const usage of unresolved.usedBy) item.appendChild(el("div", "wb-review-usage", `${tr("required by")} ${usage}`));
    column.appendChild(item);
  }
  return column;
}

// ---------- edit actions ----------

export async function applyToolEdit(snapshot, toolCall, api) {
  const response = await api.engineeringTool(snapshot.model, toolCall);
  return response;
}

export async function applyNaturalLanguageEdit(snapshot, text, api) {
  const response = await api.engineeringIntent("edit", text, snapshot.model);
  return response;
}

// ---------- tiny DOM helper ----------

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// ---------- undo/redo over model snapshots ----------

export function createModelHistory() {
  const undoStack = [];
  const redoStack = [];
  return {
    push(state) {
      undoStack.push(state);
      if (undoStack.length > MAX_HISTORY) undoStack.shift();
      redoStack.length = 0;
    },
    undo(current) {
      const target = undoStack.pop();
      if (!target) {
        // The stack holds "all history except the current state" plus a null
        // baseline sentinel pushed by loadSnapshot: popping the sentinel must
        // restore it so canUndo semantics stay intact.
        if (target === null) undoStack.push(null);
        return null;
      }
      // The state being left is not on any stack; hand it to redo.
      redoStack.push(current);
      return target;
    },
    redo(current) {
      const target = redoStack.pop();
      if (!target) return null;
      undoStack.push(current);
      return target;
    },
    canUndo: () => undoStack.length > 0,
    canRedo: () => redoStack.length > 0,
  };
}
