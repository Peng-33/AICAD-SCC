import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  createGenerationGuard,
  applyDrawingResult,
  showDrawingError,
  formatMeta,
  createDrawingPanelController,
  DRAWING_MODE_PRINCIPAL,
  DRAWING_MODE_THREE_VIEW,
  DRAWING_PANEL_TITLE,
  GEOMETRY_DEBUG_PANEL_TITLE,
} from "../public/js/engineering-workbench.js";

const workbenchSource = readFileSync(
  new URL("../public/js/engineering-workbench.js", import.meta.url),
  "utf8",
);

const PAYLOAD = (svg = "<svg>ok</svg>") => ({
  svg,
  sheet_width_mm: 150,
  sheet_height_mm: 110,
  scale: 1,
  part_title: "Mounting Plate",
});

/** Blob/objectURL fakes that record every call, in order. */
function fakeBlobDeps() {
  const calls = [];
  let next = 1;
  return {
    calls,
    Blob: class {
      constructor(parts) { this.parts = parts; }
    },
    createObjectURL: (blob) => {
      const url = `blob:fake-${next++}`;
      calls.push(["create", url, blob]);
      return url;
    },
    revokeObjectURL: (url) => calls.push(["revoke", url]),
  };
}

// ---------- generation guard ----------

test("generation guard: latest begin wins, stale generations are inactive", () => {
  const guard = createGenerationGuard();
  const a = guard.begin();
  const b = guard.begin();
  assert.equal(guard.isActive(b), true);
  assert.equal(guard.isActive(a), false);
});

// ---------- pure reducers ----------

test("applyDrawingResult creates a URL from the payload and revokes the previous one", () => {
  const deps = fakeBlobDeps();
  const first = applyDrawingResult({ url: null, meta: null, error: null }, PAYLOAD(), deps);
  assert.equal(first.url, "blob:fake-1");
  assert.equal(first.error, null);
  assert.deepEqual(
    deps.calls.map(([kind, url]) => [kind, url]),
    [["create", "blob:fake-1"]],
  );

  const second = applyDrawingResult(first, PAYLOAD("<svg>v2</svg>"), deps);
  assert.equal(second.url, "blob:fake-2");
  assert.deepEqual(
    deps.calls.map(([kind, url]) => [kind, url]),
    [["create", "blob:fake-1"], ["create", "blob:fake-2"], ["revoke", "blob:fake-1"]],
  );
  // Input state is never mutated.
  assert.equal(first.url, "blob:fake-1");
});

test("showDrawingError keeps the old url/meta and records the error", () => {
  const deps = fakeBlobDeps();
  const ok = applyDrawingResult({ url: null, meta: null, error: null }, PAYLOAD(), deps);
  const failed = showDrawingError(ok, { code: "SCC-DRW-005", message: "empty drawing" });
  assert.equal(failed.url, ok.url, "old image is preserved");
  assert.equal(failed.meta, ok.meta);
  assert.deepEqual(failed.error, { code: "SCC-DRW-005", message: "empty drawing" });
});

test("formatMeta renders Sheet · Scale · Title", () => {
  assert.equal(formatMeta(PAYLOAD()), "Sheet 150×110 mm · Scale 1 · Mounting Plate");
  assert.equal(formatMeta({ ...PAYLOAD(), part_title: null }), "Sheet 150×110 mm · Scale 1");
});

test("formatMeta displays backend-provided three-view mode and projection method", () => {
  assert.equal(
    formatMeta({ ...PAYLOAD(), drawing_mode: "three_view", projection_method: "third_angle" }),
    "Three-view · Third-angle · Sheet 150×110 mm · Scale 1 · Mounting Plate",
  );
});


// ---------- controller ----------

function fakeDom() {
  const elements = new Map();
  const makeNode = () => ({
    textContent: "",
    hidden: false,
    src: undefined,
    alt: undefined,
    children: [],
    replaceChildren() { this.children.length = 0; },
    appendChild(child) { this.children.push(child); },
  });
  return {
    dom: {
      query: (selector) => {
        if (!elements.has(selector)) elements.set(selector, makeNode());
        return elements.get(selector);
      },
    },
    createEl: (tag, className, text) => {
      const node = makeNode();
      node.tagName = tag;
      node.className = className;
      node.textContent = text ?? "";
      return node;
    },
  };
}

function fakeApi() {
  let shouldFail = false;
  return {
    setShouldFail: (value) => { shouldFail = value; },
    api: {
      engineeringDrawing: async (model) => {
        if (shouldFail) throw Object.assign(new Error("boom"), { code: "SCC-DRW-005" });
        return PAYLOAD();
      },
    },
  };
}

test("controller: success updates state.url and paints the img + meta", async () => {
  const { dom, createEl } = fakeDom();
  const deps = fakeBlobDeps();
  const { api } = fakeApi();
  const controller = createDrawingPanelController({ api, dom, blobDeps: deps, createEl });
  await controller.refresh({ part: {} });
  const state = controller.getState();
  assert.equal(state.url, "blob:fake-1");
  assert.ok(state.meta.includes("Mounting Plate"));
  const stage = dom.query(".wb-drawing-stage");
  assert.equal(stage.children.length, 1);
  assert.equal(stage.children[0].tagName, "img");
  assert.equal(stage.children[0].src, "blob:fake-1");
  assert.match(dom.query(".wb-drawing-meta").textContent, /Sheet 150×110 mm/);
  assert.equal(dom.query(".wb-drawing-error").hidden, true);
});

test("controller: ApiError shows the banner with code+message and keeps the old image", async () => {
  const { dom, createEl } = fakeDom();
  const deps = fakeBlobDeps();
  const backend = fakeApi();
  const controller = createDrawingPanelController({ api: backend.api, dom, blobDeps: deps, createEl });
  await controller.refresh({});
  const goodUrl = controller.getState().url;
  assert.ok(goodUrl);

  backend.setShouldFail(true);
  await controller.refresh({});
  const errorBox = dom.query(".wb-drawing-error");
  assert.equal(errorBox.hidden, false);
  assert.match(errorBox.textContent, /SCC-DRW-005: boom/);
  assert.equal(controller.getState().url, goodUrl, "failed refresh must keep the old image");
  const stage = dom.query(".wb-drawing-stage");
  assert.equal(stage.children[0].src, goodUrl, "stage still shows the previous drawing");
});

test("controller: stale responses are dropped without touching state or URLs", async () => {
  const { dom, createEl } = fakeDom();
  const deps = fakeBlobDeps();
  let releaseA;
  const gate = new Promise((resolve) => { releaseA = resolve; });
  const slowApi = {
    engineeringDrawing: async (model) => {
      if (model.tag === "a") {
        await gate;
        return PAYLOAD("<svg>A</svg>");
      }
      return PAYLOAD("<svg>B</svg>");
    },
  };
  const controller = createDrawingPanelController({ api: slowApi, dom, blobDeps: deps, createEl });
  const promiseA = controller.refresh({ tag: "a" }); // starts first, resolves last
  await controller.refresh({ tag: "b" });           // fast, becomes current
  const stateB = controller.getState();
  assert.equal(stateB.url, "blob:fake-1");

  releaseA();
  await promiseA;
  assert.equal(controller.getState().url, stateB.url, "late A must not replace B");
  const createdUrls = deps.calls.filter(([kind]) => kind === "create").length;
  assert.equal(createdUrls, 1, "stale response must never allocate a URL");
});

test("controller: dispose revokes the current URL exactly once", async () => {
  const { dom, createEl } = fakeDom();
  const deps = fakeBlobDeps();
  const controller = createDrawingPanelController({ api: fakeApi().api, dom, blobDeps: deps, createEl });
  await controller.refresh({});
  const url = controller.getState().url;
  controller.dispose();
  assert.deepEqual(deps.calls.at(-1), ["revoke", url]);
  assert.equal(controller.getState().url, null);
  controller.dispose(); // second dispose is a no-op
  assert.equal(deps.calls.filter(([kind]) => kind === "revoke").length, 1);
});

test("controller: refresh forwards the model payload to the API", async () => {
  const { dom, createEl } = fakeDom();
  const seen = [];
  const controller = createDrawingPanelController({
    api: { engineeringDrawing: async (model) => { seen.push(model); return PAYLOAD(); } },
    dom,
    blobDeps: fakeBlobDeps(),
    createEl,
  });
  const model = { part: { id: "p" } };
  await controller.refresh(model);
  assert.deepEqual(seen, [model]);
});


test("controller defaults to Principal and switches endpoints without mutating the model", async () => {
  const { dom, createEl } = fakeDom();
  const deps = fakeBlobDeps();
  const calls = [];
  const api = {
    engineeringDrawing: async (model) => {
      calls.push(["principal", model]);
      return PAYLOAD("<svg>principal</svg>");
    },
    engineeringThreeViewDrawing: async (model) => {
      calls.push(["three-view", model]);
      return {
        ...PAYLOAD("<svg>three-view</svg>"),
        drawing_mode: "three_view",
        projection_method: "third_angle",
      };
    },
  };
  const controller = createDrawingPanelController({ api, dom, blobDeps: deps, createEl });
  const model = { part: { id: "p" } };
  assert.equal(controller.getMode(), DRAWING_MODE_PRINCIPAL);
  await controller.refresh(model);
  await controller.setMode(DRAWING_MODE_THREE_VIEW);
  assert.equal(controller.getMode(), DRAWING_MODE_THREE_VIEW);
  assert.deepEqual(calls.map(([mode]) => mode), ["principal", "three-view"]);
  assert.deepEqual(calls[0][1], model);
  assert.deepEqual(calls[1][1], model);
  assert.match(controller.getState().meta, /^Three-view · Third-angle/);
  assert.deepEqual(model, { part: { id: "p" } }, "mode is not EngineeringModel state");
  await controller.setMode(DRAWING_MODE_PRINCIPAL);
  assert.deepEqual(calls.map(([mode]) => mode), ["principal", "three-view", "principal"]);
});

test("controller drops stale Principal success after a Three-view mode switch", async () => {
  const { dom, createEl } = fakeDom();
  const deps = fakeBlobDeps();
  let releasePrincipal;
  const principalGate = new Promise((resolve) => { releasePrincipal = resolve; });
  const api = {
    engineeringDrawing: async () => {
      await principalGate;
      return PAYLOAD("<svg>principal-late</svg>");
    },
    engineeringThreeViewDrawing: async () => ({
      ...PAYLOAD("<svg>three-current</svg>"),
      drawing_mode: "three_view",
      projection_method: "third_angle",
    }),
  };
  const controller = createDrawingPanelController({ api, dom, blobDeps: deps, createEl });
  const principal = controller.refresh({ revision: "a" });
  await controller.setMode(DRAWING_MODE_THREE_VIEW);
  const current = controller.getState();
  releasePrincipal();
  await principal;
  assert.equal(controller.getState().url, current.url);
  assert.match(controller.getState().meta, /^Three-view/);
  assert.equal(deps.calls.filter(([kind]) => kind === "create").length, 1);
});

test("controller drops stale error after a Three-view success", async () => {
  const { dom, createEl } = fakeDom();
  const deps = fakeBlobDeps();
  let rejectPrincipal;
  const principalGate = new Promise((_resolve, reject) => { rejectPrincipal = reject; });
  const api = {
    engineeringDrawing: async () => {
      await principalGate;
      throw Object.assign(new Error("late Principal failure"), { code: "SCC-DRW-005" });
    },
    engineeringThreeViewDrawing: async () => ({
      ...PAYLOAD("<svg>three-current</svg>"),
      drawing_mode: "three_view",
      projection_method: "first_angle",
    }),
  };
  const controller = createDrawingPanelController({ api, dom, blobDeps: deps, createEl });
  const principal = controller.refresh({ revision: "a" });
  await controller.setMode(DRAWING_MODE_THREE_VIEW);
  rejectPrincipal();
  await principal;
  assert.equal(controller.getState().error, null);
  assert.match(controller.getState().meta, /First-angle/);
  assert.equal(dom.query(".wb-drawing-error").hidden, true);
});

test("Three-view failure keeps the current mode and never silently calls Principal", async () => {
  const { dom, createEl } = fakeDom();
  const deps = fakeBlobDeps();
  let principalCalls = 0;
  let threeCalls = 0;
  const api = {
    engineeringDrawing: async () => {
      principalCalls += 1;
      return PAYLOAD("<svg>principal</svg>");
    },
    engineeringThreeViewDrawing: async () => {
      threeCalls += 1;
      throw Object.assign(new Error("missing thickness"), { code: "SCC-SPAT-001" });
    },
  };
  const controller = createDrawingPanelController({ api, dom, blobDeps: deps, createEl });
  await controller.refresh({});
  const principalUrl = controller.getState().url;
  await controller.setMode(DRAWING_MODE_THREE_VIEW);
  assert.equal(principalCalls, 1);
  assert.equal(threeCalls, 1);
  assert.equal(controller.getMode(), DRAWING_MODE_THREE_VIEW);
  assert.equal(controller.getState().url, principalUrl);
  assert.match(dom.query(".wb-drawing-error").textContent, /Three-view drawing unavailable/);
  assert.match(dom.query(".wb-drawing-error").textContent, /SCC-SPAT-001/);
});

test("invalid renderer payload becomes an active structured UI error", async () => {
  const { dom, createEl } = fakeDom();
  const controller = createDrawingPanelController({
    api: { engineeringDrawing: async () => ({ svg: "" }) },
    dom,
    blobDeps: fakeBlobDeps(),
    createEl,
  });
  await controller.refresh({});
  assert.equal(controller.getState().error.code, "SCC-UI-DRAWING-002");
  assert.match(dom.query(".wb-drawing-error").textContent, /SCC-UI-DRAWING-002/);
});

// ---------- naming isolation (static assertion; renderCenter needs a DOM) ----------

test("panel titles keep Geometry Debug and Engineering Drawing isolated", () => {
  assert.equal(DRAWING_PANEL_TITLE, "Engineering Drawing");
  assert.equal(GEOMETRY_DEBUG_PANEL_TITLE, "Geometry Debug");
  assert.match(workbenchSource, /tr\(GEOMETRY_DEBUG_PANEL_TITLE\)\)\);/, "renderCenter uses the renamed title");
  assert.doesNotMatch(workbenchSource, /Drawing View \(2D\)/);
});

test("drawing preview source has no SVG injection or semantic post-processing", () => {
  assert.doesNotMatch(workbenchSource, /innerHTML|DOMParser/);
  assert.doesNotMatch(workbenchSource, /querySelector\(["'](line|circle|path)["']\)/);
  assert.equal(workbenchSource.includes("engineeringThreeViewDrawing"), true);
  assert.equal(DRAWING_MODE_THREE_VIEW, "three-view");
});
