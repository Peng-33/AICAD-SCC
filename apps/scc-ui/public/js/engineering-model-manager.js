import { tr, describeEngineeringError, getEngineeringLocale } from "./engineering-i18n.js";

const STATUS = {
  unconfigured: ["未配置", "Not configured"],
  saved: ["已保存", "Saved"],
  testing: ["连接测试中", "Testing"],
  connected: ["连接成功", "Connected"],
  auth_failed: ["鉴权失败", "Authentication failed"],
  unreachable: ["网络不可达", "Network unreachable"],
  timeout: ["请求超时", "Timed out"],
  model_unavailable: ["模型不可用", "Model unavailable"],
  protocol_failed: ["工程协议校验失败", "Engineering protocol failed"],
};
const label = (zh, en) => getEngineeringLocale() === "zh-CN" ? zh : en;
const statusLabel = (status) => (STATUS[status] ?? STATUS.unconfigured)[getEngineeringLocale() === "zh-CN" ? 0 : 1];
const node = (tag, className = "", text = "") => {
  const element = document.createElement(tag);
  if (className) element.className = className;
  element.textContent = text;
  return element;
};
const button = (text, action, className = "") => {
  const item = node("button", className, text);
  item.type = "button";
  item.addEventListener("click", action);
  return item;
};

export function createModelManager({ api, dialog, onActivated, onError }) {
  let data = { activeId: null, profiles: [] };
  let presets = [];
  let editingId = null;
  let legacy = null;
  let busy = false;
  let feedback = "";

  const showError = (error) => {
    feedback = describeEngineeringError(error);
    onError?.(feedback);
    render();
  };
  async function reload() {
    try {
      [data, presets] = await Promise.all([api.modelConfigs(), api.modelPresets()]);
      legacy = null;
    } catch (error) {
      // A pre-upgrade service may still own an in-memory key. Keep it alive
      // and expose its existing configuration rather than restarting it.
      if (error.status !== 404 && error.status !== 405) throw error;
      legacy = await api.intentConfig();
      data = { activeId: null, profiles: [] };
      presets = [];
    }
    render();
  }
  async function run(action) {
    if (busy) return;
    busy = true;
    render();
    try {
      await action();
      feedback = "";
      await reload();
    } catch (error) { showError(error); }
    finally { busy = false; render(); }
  }
  function field(form, text, name, type = "text", value = "") {
    const row = node("label", "model-field", text);
    const input = node("input");
    input.name = name;
    input.type = type;
    input.value = value;
    input.autocomplete = name === "apiKey" ? "new-password" : "off";
    input.spellcheck = false;
    row.appendChild(input);
    form.appendChild(row);
    return input;
  }
  function selectField(form, text, name, options, selected) {
    const row = node("label", "model-field", text);
    const select = node("select");
    select.name = name;
    for (const [value, title] of options) {
      const option = node("option", "", title);
      option.value = value;
      select.appendChild(option);
    }
    select.value = selected;
    row.appendChild(select);
    form.appendChild(row);
    return select;
  }
  function renderLegacy(body) {
    const note = node("p", "model-note", label(
      "当前服务仍运行旧版本；DeepSeek 配置保留在服务进程内存中。可在此更新当前配置。多配置功能在服务重启并重新输入密钥后可用。",
      "The current service is still running the previous version. Its in-memory provider is preserved. You can update it here. Named profiles become available after restart and key re-entry.",
    ));
    body.appendChild(note);
    const form = node("form", "model-form");
    const provider = selectField(form, tr("Provider"), "provider", [
      ["disabled", tr("Disabled")], ["openai-compatible", tr("OpenAI-compatible")], ["scc", tr("SCC Intent API")],
    ], legacy.provider);
    const endpoint = field(form, tr("Endpoint"), "endpoint", "url", legacy.endpoint);
    const model = field(form, tr("Model ID"), "model", "text", legacy.model);
    const apiKey = field(form, tr("API Key"), "apiKey", "password");
    apiKey.placeholder = tr("Leave blank to retain the existing key");
    const safe = node("p", "model-note", `${legacy.hasApiKey ? tr("Key stored in server memory") : tr("No key stored")}. ${tr("Configuration is held in server memory and is lost on service restart.")}`);
    form.appendChild(safe);
    form.appendChild(button(tr("Update provider"), () => run(async () => {
      const result = await api.updateIntentConfig({ provider: provider.value, endpoint: endpoint.value.trim(), model: model.value.trim(), ...(apiKey.value ? { apiKey: apiKey.value } : {}) });
      apiKey.value = "";
      legacy = result;
      onActivated?.();
    }), "model-primary"));
    body.appendChild(form);
  }
  function renderList(body) {
    const list = node("div", "model-list");
    for (const profile of data.profiles) {
      const card = node("article", `model-card${profile.active ? " is-active" : ""}`);
      const heading = node("div", "model-card-heading");
      heading.append(node("strong", "", profile.name), node("span", "model-card-type", profile.type));
      if (profile.active) heading.appendChild(node("span", "model-active", label("当前启用", "Active")));
      card.appendChild(heading);
      card.appendChild(node("div", "model-card-meta", `${profile.endpoint || "—"} · ${profile.model || "—"}`));
      const status = node("div", "model-card-status");
      status.append(
        node("span", "", `${label("配置", "Config")}: ${statusLabel(profile.status)}`),
        node("span", "", `${label("连接", "Connection")}: ${statusLabel(profile.connection.status)}${profile.connection.code ? ` (${profile.connection.code})` : ""}`),
        node("span", "", `${label("工程协议", "Engineering")}: ${statusLabel(profile.engineering.status)}${profile.engineering.code ? ` (${profile.engineering.code})` : ""}`),
        node("span", "", profile.hasApiKey ? tr("Key stored in server memory") : tr("No key stored")),
      );
      card.appendChild(status);
      if (profile.connection.code) card.appendChild(node("p", "model-note", describeEngineeringError({ code: profile.connection.code, message: profile.connection.code })));
      if (profile.engineering.code) card.appendChild(node("p", "model-note", describeEngineeringError({ code: profile.engineering.code, message: profile.engineering.code })));
      const actions = node("div", "model-actions");
      if (!profile.active) actions.appendChild(button(label("一键启用", "Activate"), () => run(async () => {
        await api.activateModelConfig(profile.id);
        onActivated?.();
      }), "model-primary"));
      actions.appendChild(button(label("编辑", "Edit"), () => { editingId = profile.id; render(); }));
      actions.appendChild(button(tr("Test connection"), () => {
        if (!window.confirm(label("连接测试会向此模型发送一条短请求，可能产生 API 用量。继续？", "The connection test sends one short request and may incur API usage. Continue?"))) return;
        profile.connection = { status: "testing" };
        render();
        void run(() => api.testModelConnection(profile.id));
      }));
      actions.appendChild(button(label("工程命令测试", "Test engineering command"), () => {
        if (!window.confirm(label("工程协议测试会发送固定的简单安装板需求，最多进行两次模型调用，可能产生 API 用量。继续？", "The engineering test sends a fixed mounting-plate request and may use up to two model calls. Continue?"))) return;
        profile.engineering = { status: "testing" };
        render();
        void run(() => api.testModelEngineering(profile.id));
      }));
      actions.appendChild(button(label("删除", "Delete"), () => {
        if (!window.confirm(label(`确定删除“${profile.name}”？此操作无法撤销。`, `Delete “${profile.name}”? This cannot be undone.`))) return;
        void run(async () => { await api.deleteModelConfig(profile.id); if (profile.active) onActivated?.(); });
      }, "model-danger"));
      card.appendChild(actions);
      list.appendChild(card);
    }
    body.appendChild(list);
  }
  function renderEditor(body) {
    const current = data.profiles.find((profile) => profile.id === editingId);
    const form = node("form", "model-form");
    form.addEventListener("submit", (event) => { event.preventDefault(); submit(); });
    const name = field(form, label("配置名称", "Configuration name"), "name", "text", current?.name ?? "");
    name.required = true;
    const type = selectField(form, tr("Provider"), "type", presets.map((preset) => [preset.type, preset.name]), current?.type ?? presets[0]?.type);
    const endpoint = field(form, tr("Endpoint"), "endpoint", "url", current?.endpoint ?? presets[0]?.endpoint ?? "");
    const model = field(form, tr("Model ID"), "model", "text", current?.model ?? "");
    const authMode = selectField(form, label("鉴权方式", "Authentication"), "authMode", [["bearer", "Bearer API Key"], ["none", label("无鉴权", "None")]], current?.authMode ?? "bearer");
    const apiKey = field(form, tr("API Key"), "apiKey", "password");
    apiKey.placeholder = tr("Leave blank to retain the existing key");
    const clearRow = node("label", "model-checkbox", label("清除已保存密钥", "Clear stored key"));
    const clear = node("input"); clear.type = "checkbox"; clear.name = "clearApiKey";
    clearRow.prepend(clear); form.appendChild(clearRow);
    form.appendChild(node("p", "model-note", `${current?.hasApiKey ? tr("Key stored in server memory") : tr("No key stored")}. ${tr("Configuration is held in server memory and is lost on service restart.")}`));
    type.addEventListener("change", () => {
      const preset = presets.find((item) => item.type === type.value);
      endpoint.value = preset?.endpoint ?? "";
      authMode.value = type.value === "local" ? "none" : "bearer";
    });
    const actions = node("div", "model-actions");
    const save = node("button", "model-primary", label("保存配置", "Save configuration"));
    save.type = "submit";
    actions.append(save, button(tr("Cancel"), () => { editingId = null; render(); }));
    form.appendChild(actions);
    body.appendChild(form);
    function submit() {
      void run(async () => {
        const payload = { name: name.value.trim(), type: type.value, endpoint: endpoint.value.trim(), model: model.value.trim(), authMode: authMode.value,
          ...(apiKey.value ? { apiKey: apiKey.value } : {}), clearApiKey: clear.checked };
        if (editingId) await api.editModelConfig(editingId, payload);
        else await api.addModelConfig(payload);
        apiKey.value = "";
        editingId = null;
        if (current?.active) onActivated?.();
      });
    }
  }
  function render() {
    dialog.replaceChildren();
    const heading = node("header", "model-dialog-header");
    heading.append(node("h2", "", tr("Provider settings")), button("×", () => dialog.close(), "model-close"));
    dialog.appendChild(heading);
    const body = node("div", "model-dialog-body");
    if (feedback) body.appendChild(node("p", "model-error", feedback));
    if (legacy) renderLegacy(body);
    else {
      body.appendChild(node("p", "model-note", tr("Configuration is held in server memory and is lost on service restart.")));
      renderList(body);
      if (editingId === null) body.appendChild(button(label("添加配置", "Add configuration"), () => { editingId = ""; render(); }, "model-primary"));
      else renderEditor(body);
    }
    if (busy) body.appendChild(node("p", "model-note", label("正在处理…", "Working…")));
    dialog.appendChild(body);
  }
  return {
    async open() { feedback = ""; await reload(); dialog.showModal(); },
    async reload() { await reload(); },
    render,
  };
}
