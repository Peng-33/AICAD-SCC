      import { api } from "/js/api.js";
      import {
        renderWorkbench,
        applyToolEdit,
        createModelHistory,
        createDrawingPanelController,
      } from "/js/engineering-workbench.js";
      import { fingerprintModel, safeProjectFilename, safeStepFilename } from "/js/engineering-project.js";
      import { setEngineeringLocale, getEngineeringLocale, tr, describeEngineeringError } from "/js/engineering-i18n.js";
      import { createModelManager } from "/js/engineering-model-manager.js";

      const FIXTURE_DSL = `AICAD 1
PART mounting_plate "Mounting Plate"
PARAM plate_width = 120
PARAM plate_height = 80
PARAM edge_offset = 15
PARAM hole_diameter := plate_height / 8
BASE_RECT main
  WIDTH 120
  HEIGHT 80
THICKNESS 10
BIND main WIDTH <- plate_width
BIND main HEIGHT <- plate_height
HOLE_PATTERN mounting CORNER
  BASE main
  OFFSET_X 15
  OFFSET_Y 15
  DIA 10
BIND mounting OFFSET_X <- edge_offset
BIND mounting OFFSET_Y <- edge_offset
DRAWING PROJECTION_METHOD THIRD_ANGLE
BIND mounting DIAMETER <- hole_diameter
EXTENT mounting THRU
`;

      const storedLocale = localStorage.getItem("aicad.language");
      setEngineeringLocale(storedLocale === "en" ? "en" : "zh-CN");
      document.getElementById("language-select").value = getEngineeringLocale();
      const root = document.getElementById("workbench-root");
      const status = document.getElementById("status");
      const history = createModelHistory();
      const drawingController = createDrawingPanelController({ api });
      void drawingController.setMode("three-view");
      let snapshot = null;
      let selectedFeatureId = null;
      let projectName = "aicad-project";
      let savedFingerprint = null;
      let currentFingerprint = null;
      let stepExportGeneration = 0;
      let editInFlight = false;
      let revision = 0;
      let intentGeneration = 0;
      let intentWorkflow = { status: "idle" };
      let skillRun = null;
      let currentSkillIntent = null;
      let cadSkillDefinitions = [];
      void api.cadSkills().then((payload) => { cadSkillDefinitions = payload.skills ?? []; renderCadSkillStatus(); }).catch(() => {});

      window.addEventListener("pagehide", () => drawingController.dispose());
      window.addEventListener("beforeunload", () =>
        drawingController.dispose(),
      );

      function setStatus(text) {
        status.textContent = tr(text);
      }

      function translateStatic() {
        document.documentElement.lang = getEngineeringLocale();
        for (const element of document.querySelectorAll("[data-i18n]")) element.textContent = tr(element.dataset.i18n);
        for (const element of document.querySelectorAll("[data-i18n-placeholder]")) element.placeholder = tr(element.dataset.i18nPlaceholder);
      }

      async function refreshModelStatus() {
        try {
          const config = await api.intentConfig();
          document.getElementById("model-status").textContent = config.configured
            ? `${config.model} · ${config.hasApiKey ? tr("Key stored in server memory") : tr("No key stored")}`
            : tr("Provider not configured");
        } catch (error) { document.getElementById("model-status").textContent = describeEngineeringError(error); }
      }

      const modelManager = createModelManager({
        api, dialog: document.getElementById("model-manager-dialog"),
        onActivated: () => { clearIntentWorkflow(); void refreshModelStatus(); setStatus(tr("Configuration saved")); },
        onError: setStatus,
      });
      document.getElementById("model-manager-open").addEventListener("click", () => void modelManager.open().catch((error) => setStatus(describeEngineeringError(error))));
      document.getElementById("language-select").addEventListener("change", (event) => {
        setEngineeringLocale(event.target.value);
        localStorage.setItem("aicad.language", getEngineeringLocale());
        translateStatic();
        if (snapshot) {
          render();
          void drawingController.refresh(snapshot.model).then(updateDrawingExportState);
        }
        modelManager.render();
        void refreshModelStatus();
      });
      translateStatic();
      void refreshModelStatus();

      function updateDrawingExportState() {
        const drawing = drawingController.getState();
        const ready = Boolean(drawing.payload && !drawing.loading && !drawing.error && snapshot);
        const report = drawing.payload?.skills_report;
        const fingerprintMatches = !drawing.payload?.model_fingerprint || drawing.payload.model_fingerprint === currentModelFingerprint();
        const formalDrawingMode = drawing.mode === "three-view" || drawing.mode === "section-a-a";
        const svgValidated = !formalDrawingMode || report?.SVG_EXPORT?.status === "PASS";
        const dxfValidated = !formalDrawingMode || report?.DXF_EXPORT?.status === "PASS";
        document.getElementById("export-svg").disabled = !ready || !fingerprintMatches || !svgValidated;
        document.getElementById("export-dxf").disabled = !ready || !fingerprintMatches || !dxfValidated || typeof drawing.payload?.dxf !== "string";
        document.getElementById("drawing-status").textContent = drawing.loading
          ? tr("Generating drawing…")
          : drawing.error ? describeEngineeringError(drawing.error)
          : ready ? tr(drawing.mode === "three-view"
            ? (report?.status === "PASS" ? "Three-view drawing ready" : "Three-view quality checks failed")
            : drawing.mode === "section-a-a"
              ? (report?.status !== "PASS" ? "Section A-A quality checks failed" : report?.DRAWING_COMPLETENESS?.status === "PARTIAL" ? "Section A-A exports validated; parametric dimensions are incomplete" : "Section A-A drawing ready")
              : "Principal drawing ready") : "";
        renderCadSkillStatus();
        if (snapshot) render();
      }

      function downloadDrawing(format) {
        const drawing = drawingController.getState();
        if (!drawing.payload || drawing.loading || drawing.error || !snapshot) return;
        const fingerprint = drawing.payload.model_fingerprint;
        if (fingerprint && fingerprint !== currentModelFingerprint()) { setStatus(tr("Drawing is for a different model revision")); return; }
        const reportStatus = drawing.payload.skills_report?.[format === "dxf" ? "DXF_EXPORT" : "SVG_EXPORT"]?.status;
        if (["three-view", "section-a-a"].includes(drawing.mode) && reportStatus !== "PASS") { setStatus(tr("Export quality check did not pass")); return; }
        const content = format === "dxf" ? drawing.payload.dxf : drawing.payload.svg;
        if (typeof content !== "string" || !content) return;
        const blob = new Blob([content], { type: format === "dxf" ? "application/dxf" : "image/svg+xml" });
        const url = URL.createObjectURL(blob);
        const link = document.createElement("a");
        link.href = url;
        link.download = safeProjectFilename(projectName).replace(/\.aicad\.json$/, `.${format}`);
        link.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        setStatus(tr(format === "dxf" ? "Exported DXF" : "Exported SVG"));
      }
      document.getElementById("export-svg").addEventListener("click", () => downloadDrawing("svg"));
      document.getElementById("export-dxf").addEventListener("click", () => downloadDrawing("dxf"));

      function updateProjectState() {
        currentFingerprint = snapshot ? (snapshot.fingerprint ?? fingerprintModel(snapshot.model)) : null;
        document.getElementById("project-state").textContent =
          tr(currentFingerprint && currentFingerprint === savedFingerprint ? "Saved" : "Unsaved");
      }

      function invalidateStepExports() {
        stepExportGeneration += 1;
      }

      function selectedFeature() {
        const view = snapshot?.workbench;
        return (
          view?.features?.find((feature) => feature.id === selectedFeatureId) ??
          null
        );
      }

      function clearIntentWorkflow() {
        intentGeneration += 1;
        intentWorkflow = { status: "idle" };
        renderIntentWorkflow();
      }

      function currentModelFingerprint() {
        return snapshot?.fingerprint ?? fingerprintModel(snapshot?.model ?? {});
      }

      function onEngineeringStateCommitted(model, committedRevision = undefined, skillContext = null) {
        if (Number.isInteger(committedRevision)) revision = committedRevision;
        else revision += 1;
        clearIntentWorkflow();
        if (skillContext) {
          skillRun = skillContext.skillRun ?? null;
          currentSkillIntent = skillContext.userIntent ?? null;
        } else {
          skillRun = null;
          currentSkillIntent = null;
        }
        invalidateStepExports();
        updateProjectState();
        const drawingRequest = drawingController.refresh(model, { userIntent: currentSkillIntent });
        updateDrawingExportState();
        void drawingRequest.then(updateDrawingExportState);
      }

      function beginIntentWorkflow(text) {
        if (!text.trim()) { setStatus(tr("Enter a request first")); return; }
        const mode = snapshot?.model?.part ? "edit" : "new";
        const generation = ++intentGeneration;
        const baseFingerprint = mode === "edit" ? currentModelFingerprint() : null;
        currentSkillIntent = text;
        skillRun = { skill_states: [
          { id: "mechanical-part-analysis", state: "IN_PROGRESS" },
          { id: "engineering-model-planning", state: "PENDING" },
          { id: "orthographic-drawing", state: "PENDING" },
          { id: "drawing-annotation", state: "PENDING" },
          { id: "cad-export-validation", state: "PENDING" },
        ] };
        renderCadSkillStatus();
        intentWorkflow = { status: "understanding", generation, mode, userIntent: text };
        renderIntentWorkflow();
        void (async () => {
          const planned = await api.engineeringSkillPlan(mode, text, mode === "edit" ? snapshot.model : undefined, baseFingerprint);
          if (generation !== intentGeneration) return;
          skillRun = planned.plan;
          renderCadSkillStatus();
          const result = await api.engineeringIntentPreview(mode, text, mode === "edit" ? snapshot.model : undefined, baseFingerprint, undefined, planned.plan_id);
          if (generation !== intentGeneration) return;
          skillRun = result.skill_run ?? skillRun;
          if (result.status === "new_ready") {
            if (snapshot) history.push(snapshot);
            snapshot = result;
            selectedFeatureId = null;
            render();
            onEngineeringStateCommitted(snapshot.model, result.revision, { skillRun: result.skill_run, userIntent: text });
            setStatus("Created EngineeringModel from natural language");
            return;
          }
          if (result.status === "needs_clarification") {
            intentWorkflow = { status: "needs_clarification", generation, request: result, mode, skillRun, userIntent: text };
          } else if (result.status === "preview_ready") {
            intentWorkflow = { status: "preview_ready", generation, proposal: result.proposal, mode, skillRun, userIntent: text };
          } else {
            intentWorkflow = { status: "error", generation, error: { ...(result.error ?? { message: "Intent was rejected" }), details: { diagnostic: result.diagnostic } }, skillRun, userIntent: text };
          }
          renderIntentWorkflow();
          renderCadSkillStatus();
        })().catch((error) => {
          if (generation !== intentGeneration) return;
          skillRun = error?.details?.skill_run ?? skillRun;
          intentWorkflow = { status: "error", generation, error, skillRun, userIntent: text };
          renderIntentWorkflow();
          renderCadSkillStatus();
        });
      }

      async function continueIntent(answers) {
        const request = intentWorkflow.request;
        if (!request) return;
        const generation = ++intentGeneration;
        const mode = request.mode ?? intentWorkflow.mode ?? "edit";
        const userIntent = request.original_intent ?? intentWorkflow.userIntent ?? "";
        const clarification = {
          request_id: request.request_id,
          base_model_fingerprint: request.base_model_fingerprint,
          answers,
        };
        intentWorkflow = { status: "understanding", generation, mode, userIntent };
        if (skillRun) skillRun = { ...skillRun, skill_states: (skillRun.skill_states ?? []).map((item) => ({ ...item, state: item.id === "mechanical-part-analysis" ? "IN_PROGRESS" : item.state })) };
        renderIntentWorkflow();
        renderCadSkillStatus();
        try {
          const baseFingerprint = mode === "edit" ? currentModelFingerprint() : null;
          const planned = await api.engineeringSkillPlan(mode, userIntent, mode === "edit" ? snapshot?.model : undefined, baseFingerprint, clarification);
          if (generation !== intentGeneration) return;
          skillRun = planned.plan;
          renderCadSkillStatus();
          const result = await api.engineeringIntentPreview(
            mode, userIntent, mode === "edit" ? snapshot?.model : undefined, baseFingerprint, clarification, planned.plan_id,
          );
          if (generation !== intentGeneration) return;
          skillRun = result.skill_run ?? skillRun;
          if (result.status === "new_ready") {
            if (snapshot) history.push(snapshot);
            snapshot = result;
            selectedFeatureId = null;
            render();
            onEngineeringStateCommitted(snapshot.model, result.revision, { skillRun: result.skill_run, userIntent });
            setStatus("Created EngineeringModel from natural language");
            return;
          }
          if (result.status === "needs_clarification") intentWorkflow = { status: "needs_clarification", generation, request: result, mode, skillRun, userIntent };
          else if (result.status === "preview_ready") intentWorkflow = { status: "preview_ready", generation, proposal: result.proposal, mode, skillRun, userIntent };
          else intentWorkflow = { status: "error", generation, error: { ...(result.error ?? { message: "Intent was rejected" }), details: { diagnostic: result.diagnostic } }, skillRun, userIntent };
          renderIntentWorkflow();
          renderCadSkillStatus();
        } catch (error) {
          if (generation !== intentGeneration) return;
          skillRun = error?.details?.skill_run ?? skillRun;
          intentWorkflow = { status: "error", generation, error, skillRun, userIntent };
          renderIntentWorkflow();
          renderCadSkillStatus();
        }
      }

      async function confirmIntent() {
        if (intentWorkflow.status !== "preview_ready" || !snapshot) return;
        const generation = ++intentGeneration;
        intentWorkflow = { ...intentWorkflow, status: "committing", generation };
        renderIntentWorkflow();
        try {
          const result = await api.engineeringIntentConfirm(snapshot.model, intentWorkflow.proposal);
          if (generation !== intentGeneration) return;
          history.push(snapshot);
          snapshot = result;
          selectedFeatureId = null;
          render();
          onEngineeringStateCommitted(snapshot.model, result.revision, { skillRun: intentWorkflow.skillRun, userIntent: intentWorkflow.userIntent });
          setStatus("Applied confirmed engineering change");
        } catch (error) {
          if (generation !== intentGeneration) return;
          intentWorkflow = { status: "error", generation, error };
          renderIntentWorkflow();
        }
      }

      function renderIntentWorkflow() {
        const panel = document.getElementById("intent-workflow");
        if (!panel) return;
        panel.replaceChildren();
        if (intentWorkflow.status === "idle") { panel.hidden = true; return; }
        panel.hidden = false;
        const title = document.createElement("h2");
        title.textContent = intentWorkflow.status === "preview_ready" ? tr("Proposed changes") : `Engineering Intent: ${intentWorkflow.status}`;
        panel.appendChild(title);
        if (intentWorkflow.status === "understanding" || intentWorkflow.status === "committing") {
          panel.appendChild(document.createTextNode(tr("Working through the formal engineering workflow…")));
          return;
        }
        if (intentWorkflow.status === "error") {
          const error = document.createElement("div"); error.className = "wb-intent-warning"; error.textContent = describeEngineeringError(intentWorkflow.error); panel.appendChild(error);
          const diagnostic = intentWorkflow.error?.details?.diagnostic ?? intentWorkflow.error?.error?.details?.diagnostic;
          if (diagnostic) {
            const suggestion = document.createElement("p");
            suggestion.textContent = `${diagnostic.code}: ${getEngineeringLocale() === "zh-CN" ? diagnostic.recovery?.["zh-CN"] : diagnostic.recovery?.en}`;
            panel.appendChild(suggestion);
          }
          addWorkflowButton(panel, "Cancel", clearIntentWorkflow);
          return;
        }
        if (intentWorkflow.status === "needs_clarification") {
          panel.appendChild(document.createTextNode(intentWorkflow.request.reason ?? "I need a few details:"));
          const controls = [];
          for (const field of intentWorkflow.request.fields ?? []) {
            const row = document.createElement("div"); row.className = "wb-intent-row";
            const label = document.createElement("label"); label.textContent = field.prompt; row.appendChild(label);
            const selectKind = field.kind === "enum" || field.kind === "feature_reference" || field.kind === "boolean";
            const control = document.createElement(selectKind ? "select" : "input");
            control.dataset.key = field.key;
            if (field.kind === "length") control.type = "number";
            if (field.kind === "integer") { control.type = "number"; control.step = "1"; }
            if (control.tagName === "SELECT") {
              const placeholder = document.createElement("option");
              placeholder.value = ""; placeholder.textContent = tr("Select a value…"); placeholder.disabled = true; placeholder.selected = true;
              control.appendChild(placeholder);
              const options = field.options ?? (field.kind === "feature_reference" ? (snapshot?.workbench?.features ?? []).map((f) => f.id) : field.kind === "boolean" ? ["true", "false"] : ["through_all", "blind_base", "blind_opposite"]);
              for (const option of options) { const item = document.createElement("option"); item.value = option; item.textContent = option; control.appendChild(item); }
            }
            row.appendChild(control); panel.appendChild(row); controls.push([field, control]);
          }
          addWorkflowButton(panel, "Continue", () => {
            const missing = controls.filter(([field, control]) => field.required !== false && control.value === "");
            if (missing.length > 0) {
              setStatus(`${getEngineeringLocale() === "zh-CN" ? "请填写" : "Please answer"}: ${missing.map(([field]) => field.prompt).join(", ")}`);
              return;
            }
            const answers = Object.fromEntries(controls.map(([field, control]) => [field.key, field.kind === "boolean" ? control.value === "true" : control.value]));
            void continueIntent(answers);
          });
          addWorkflowButton(panel, "Cancel", clearIntentWorkflow);
          return;
        }
        const proposal = intentWorkflow.proposal;
        panel.appendChild(document.createTextNode(proposal.intent_summary ?? tr("Review the formal model changes before applying.")));
        const parameters = document.createElement("div"); parameters.innerHTML = `<strong>${tr("Parameters")}</strong>`;
        for (const change of proposal.parameter_changes ?? []) {
          const row = document.createElement("div"); row.className = "wb-intent-diff"; row.textContent = `${change.display_name}: ${formatChangeValue(change.before)} → ${formatChangeValue(change.after)}${change.change_kind === "derived" ? " (derived)" : ""}`; parameters.appendChild(row);
        }
        panel.appendChild(parameters);
        const features = document.createElement("div"); features.innerHTML = `<strong>${tr("Features")}</strong>`;
        for (const change of proposal.feature_changes ?? []) {
          const row = document.createElement("div"); row.className = change.kind === "unchanged" ? "wb-intent-unchanged" : "wb-intent-diff"; row.textContent = `${change.id}: ${change.kind}`; features.appendChild(row);
        }
        panel.appendChild(features);
        if (proposal.diagnostics?.length) { const diag = document.createElement("div"); diag.className = "wb-intent-warning"; diag.textContent = proposal.diagnostics.map((d) => d.message ?? d.code).join("; "); panel.appendChild(diag); }
        if (intentWorkflow.status === "preview_ready") { addWorkflowButton(panel, "Apply Changes", () => void confirmIntent()); addWorkflowButton(panel, "Cancel", clearIntentWorkflow); }
      }

      function renderCadSkillStatus() {
        const panel = document.getElementById("cad-skill-status");
        if (!panel) return;
        const drawing = drawingController.getState();
        const drawingRun = drawing.payload?.skill_run;
        const baseRun = skillRun ?? drawingRun;
        if (!baseRun && !drawingRun && !drawing.error?.details?.diagnostic) { panel.hidden = true; return; }
        panel.hidden = false;
        panel.replaceChildren();
        const title = document.createElement("h2");
        title.textContent = tr("CAD Skills execution");
        panel.appendChild(title);
        const states = new Map((baseRun?.skill_states ?? []).map((item) => [item.id, item.state]));
        for (const item of drawingRun?.skill_states ?? []) states.set(item.id, item.state);
        const diagnostic = baseRun?.diagnostic ?? drawing.payload?.skills_report?.diagnostic ?? drawing.error?.details?.diagnostic;
        if (diagnostic) states.set("engineering-diagnostics", "COMPLETE");
        for (const id of ["mechanical-part-analysis", "engineering-model-planning", "orthographic-drawing", "drawing-annotation", "cad-export-validation", "engineering-diagnostics"]) {
          const state = states.get(id);
          if (!state) continue;
          const definition = cadSkillDefinitions.find((entry) => entry.id === id);
          const name = definition?.name?.[getEngineeringLocale() === "zh-CN" ? "zh-CN" : "en"] ?? tr(id);
          const row = document.createElement("div");
          row.className = "wb-cad-skill-row";
          const label = document.createElement("span"); label.textContent = name;
          const result = document.createElement("strong"); result.textContent = tr(state);
          row.append(label, result); panel.appendChild(row);
        }
        const report = drawing.payload?.skills_report;
        if (report) {
          for (const key of ["SVG_EXPORT", "DXF_EXPORT", "DRAWING_CONSISTENCY", "GEOMETRY_VALIDATION"]) {
            const evidence = report[key];
            if (!evidence) continue;
            const row = document.createElement("div"); row.className = "wb-cad-skill-row";
            const label = document.createElement("span"); label.textContent = tr(key);
            const result = document.createElement("strong"); result.textContent = tr(evidence.status);
            row.append(label, result); panel.appendChild(row);
          }
          const annotations = report.DRAWING_ANNOTATION;
          if (annotations) {
            const summary = document.createElement("p");
            summary.textContent = `${tr("Formal dimensions")}: ${annotations.dimensions?.length ?? 0} · ${tr("Center geometry")}: ${annotations.center_geometry?.length ?? 0}`;
            panel.appendChild(summary);
            for (const item of annotations.unsupported ?? []) appendEvidenceRow(panel, "UNSUPPORTED", item.evidence, item.type);
            for (const item of annotations.ambiguous ?? []) appendEvidenceRow(panel, "AMBIGUOUS", item.evidence, item.type);
            for (const item of annotations.recognized_but_not_rendered ?? []) appendEvidenceRow(panel, "RECOGNIZED_BUT_NOT_RENDERED", item.evidence, item.requirement);
          }
        }
        if (diagnostic) {
          const note = document.createElement("p"); note.className = "wb-intent-warning";
          const explanation = diagnostic.explanation?.[getEngineeringLocale() === "zh-CN" ? "zh-CN" : "en"];
          const recovery = diagnostic.recovery?.[getEngineeringLocale() === "zh-CN" ? "zh-CN" : "en"];
          note.textContent = `${diagnostic.code}: ${explanation ?? diagnostic.original_message} ${recovery ?? ""}`;
          panel.appendChild(note);
        }
        const analysis = baseRun?.request_analysis;
        if (analysis) {
          const details = document.createElement("details");
          const summary = document.createElement("summary"); summary.textContent = tr("Analysis evidence"); details.appendChild(summary);
          for (const [key, items] of [["EXPLICIT", analysis.explicit], ["DERIVED", analysis.derived], ["AMBIGUOUS", analysis.ambiguous], ["CONFLICTING", analysis.conflicting], ["UNSUPPORTED", analysis.unsupported], ["MISSING", analysis.missing]]) {
            for (const item of items ?? []) appendEvidenceRow(details, key, item.evidence ?? item.message, item.kind ?? item.type ?? item.id);
          }
          panel.appendChild(details);
        }
        if (baseRun?.engineering_commands?.length) {
          const detail = document.createElement("details");
          const summary = document.createElement("summary"); summary.textContent = `${tr("Validated EngineeringCommand")} (${baseRun.engineering_commands.length})`; detail.appendChild(summary);
          const list = document.createElement("pre"); list.textContent = JSON.stringify(baseRun.engineering_commands, null, 2); detail.appendChild(list); panel.appendChild(detail);
        }
      }

      function appendEvidenceRow(parent, category, evidence, name) {
        const row = document.createElement("div"); row.className = "wb-cad-evidence";
        row.textContent = `${tr(category)} · ${name ?? ""}${evidence ? `: ${evidence}` : ""}`;
        parent.appendChild(row);
      }

      function formatChangeValue(value) {
        if (!value) return "—";
        if (value.source_expression) return `${value.resolved_value ?? "?"} [${JSON.stringify(value.source_expression)}]`;
        return String(value.resolved_value ?? "—");
      }

      function addWorkflowButton(parent, label, handler) {
        const button = document.createElement("button"); button.textContent = tr(label); button.addEventListener("click", handler); parent.appendChild(button);
      }

      function render() {
        const draft = root.querySelector(".wb-nl")?.value ?? "";
        const drawingState = drawingController.getState();
        const diagnostics = drawingState.error?.details?.diagnostics ?? drawingState.error?.error?.details?.diagnostics ?? [];
        const parametricDiagnostic = diagnostics.find((item) => item?.details?.parametric_code);
        const topologyResolution = {
          matchesCurrentModel: Boolean(snapshot && drawingState.modelFingerprint === fingerprintModel(snapshot.model)),
          loading: drawingState.loading,
          succeeded: Boolean(drawingState.payload && !drawingState.error),
          failed: Boolean(drawingState.error),
          code: parametricDiagnostic?.details?.parametric_code ?? drawingState.error?.details?.diagnostic?.code ?? drawingState.error?.code ?? drawingState.error?.error?.code ?? null,
          featureId: parametricDiagnostic?.details?.feature_id ?? parametricDiagnostic?.details?.operation_id ?? drawingState.error?.details?.feature_id ?? null,
        };
        renderWorkbench(
          root,
          { ...snapshot, selectedFeature: selectedFeature(), topologyResolution },
          {
            drawingController,
            onSectionChange: (plane) => {
              try {
                const request = drawingController.setSectionPlane(plane);
                updateDrawingExportState();
                void request.then(updateDrawingExportState).catch((error) => setStatus(describeEngineeringError(error)));
              } catch (error) { setStatus(describeEngineeringError(error)); }
            },
            onDrawingModeChange: (mode) => {
              try {
                const request = drawingController.setMode(mode);
                updateDrawingExportState();
                void request.then(updateDrawingExportState).catch((error) => setStatus(describeEngineeringError(error)));
              } catch (error) {
                setStatus(describeEngineeringError(error));
              }
            },
            onSelectFeature: (id) => {
              selectedFeatureId = id;
              render();
            },
            onSelectEntity: (entityId) => {
              const featureId = snapshot?.entity_to_feature?.[entityId];
              if (featureId) {
                selectedFeatureId = featureId;
                render();
              }
            },
            onEdit: async (toolCall) => {
              if (editInFlight) return;
              editInFlight = true;
              try {
                const next = await applyToolEdit(snapshot, toolCall, api);
                history.push(snapshot);
                snapshot = next;
                render();
                onEngineeringStateCommitted(snapshot.model, next.revision);
                setStatus(`${getEngineeringLocale() === "zh-CN" ? "已应用" : "Applied"} ${toolCall.name}`);
              } catch (error) {
                render();
                setStatus(describeEngineeringError(error));
              } finally {
                editInFlight = false;
              }
            },
            onUndo: () => {
              const previous = history.undo(snapshot);
              if (previous) {
                snapshot = previous;
                render();
                onEngineeringStateCommitted(snapshot.model);
              }
            },
            onRedo: () => {
              const next = history.redo(snapshot);
              if (next) {
                snapshot = next;
                render();
                onEngineeringStateCommitted(snapshot.model);
              }
            },
            onNaturalLanguage: async (text) => {
              beginIntentWorkflow(text);
            },
          },
        );
        const input = root.querySelector(".wb-nl");
        if (input) input.value = draft;
      }

      async function loadSnapshot(payload) {
        snapshot = payload;
        selectedFeatureId = null;
        history.push(null); // reset history baseline
        savedFingerprint = null;
        invalidateStepExports();
        render();
        onEngineeringStateCommitted(snapshot.model, payload.revision);
      }

      document.getElementById("project-new").addEventListener("click", () => {
        projectName = "aicad-project";
        const input = root.querySelector(".wb-nl");
        if (input) input.value = "";
        snapshot = null;
        selectedFeatureId = null;
        savedFingerprint = null;
        drawingController.dispose();
        clearIntentWorkflow();
        skillRun = null;
        currentSkillIntent = null;
        renderCadSkillStatus();
        invalidateStepExports();
        render();
        updateProjectState();
        updateDrawingExportState();
        setStatus("New project");
      });

      document.getElementById("project-save").addEventListener("click", async () => {
        if (!snapshot) { setStatus(tr("Enter a request first")); return; }
        const name = window.prompt(tr("Project name"), projectName) ?? projectName;
        if (!name.trim()) return;
        try {
          const result = await api.engineeringProjectSave(snapshot.model, name.trim());
          projectName = result.project.metadata.name;
          savedFingerprint = result.fingerprint;
          updateProjectState();
          const blob = new Blob([JSON.stringify(result.project, null, 2) + "\n"], { type: "application/json" });
          const url = URL.createObjectURL(blob);
          const link = document.createElement("a");
          link.href = url; link.download = safeProjectFilename(projectName); link.click();
          URL.revokeObjectURL(url);
          setStatus("Project saved");
        } catch (error) { setStatus(describeEngineeringError(error)); }
      });

      document.getElementById("project-open").addEventListener("click", () => document.getElementById("project-file").click());
      document.getElementById("project-file").addEventListener("change", async (event) => {
        const file = event.target.files?.[0];
        if (!file) return;
        try {
          const opened = await api.engineeringProjectOpen(JSON.parse(await file.text()));
          projectName = opened.project.metadata.name;
          await loadSnapshot({ ...(await api.engineeringWorkbench(opened.model)), fingerprint: opened.fingerprint, revision: opened.revision });
          savedFingerprint = opened.fingerprint;
          updateProjectState();
          setStatus("Project opened");
        } catch (error) { setStatus(describeEngineeringError(error)); }
        event.target.value = "";
      });

      document.getElementById("project-export-step").addEventListener("click", async () => {
        if (!snapshot) return;
        const generation = stepExportGeneration;
        const fingerprint = currentModelFingerprint();
        const model = snapshot.model;
        setStatus("Validating STEP…");
        try {
          const result = await api.engineeringExportStep(model, projectName);
          if (generation !== stepExportGeneration || fingerprint !== currentModelFingerprint()) {
            setStatus("Project changed while STEP was generated. Export again.");
            return;
          }
          const url = URL.createObjectURL(result.blob);
          const link = document.createElement("a");
          link.href = url; link.download = safeStepFilename(projectName); link.click();
          URL.revokeObjectURL(url);
          const drawing = drawingController.getState();
          const drawingReady = Boolean(drawing.payload && !drawing.loading && !drawing.error
            && (!drawing.payload.model_fingerprint || drawing.payload.model_fingerprint === fingerprint));
          setStatus(drawingReady
            ? "EngineeringModel valid; three-view drawing ready; STEP lowering, build, reimport, and equivalence passed"
            : "EngineeringModel valid; STEP lowering, build, reimport, and equivalence passed; three-view drawing not ready");
        } catch (error) { setStatus(describeEngineeringError(error)); }
      });

      document
        .getElementById("load-fixture")
        .addEventListener("click", async () => {
          try {
            await loadSnapshot(await api.engineeringDsl(FIXTURE_DSL));
          } catch (error) {
            setStatus(describeEngineeringError(error));
          }
        });
      document
        .getElementById("dsl-apply")
        .addEventListener("click", async () => {
          const dsl = document.getElementById("dsl-input").value.trim();
          if (!dsl) return;
          try {
            await loadSnapshot(await api.engineeringDsl(dsl));
          } catch (error) {
            setStatus(describeEngineeringError(error));
          }
        });

      loadFixture();
      async function loadFixture() {
        try {
          await loadSnapshot(await api.engineeringDsl(FIXTURE_DSL));
        } catch (error) {
          setStatus(describeEngineeringError(error));
        }
      }
