const state = {
  frozen: false,
  latest: {},
  events: [],
  feed: {},
};

const el = (id) => document.getElementById(id);

function text(value, fallback = "waiting") {
  if (value === undefined || value === null || value === "") return fallback;
  return String(value);
}

function fmtTime(ts) {
  if (!ts) return "";
  const date = new Date(ts);
  return date.toLocaleTimeString([], { hour12: false });
}

function setPill(node, value, kind = "ok") {
  node.textContent = value;
  node.classList.remove("ok", "warn", "fail");
  node.classList.add(kind);
}

function renderMetrics(panel, frame) {
  const metrics = panel.querySelector(".metrics");
  const rows = [
    ["time", fmtTime(frame.ts)],
    ["source", frame.source_label || frame.lane_id],
    ["size", frame.dimensions ? `${frame.dimensions.width}x${frame.dimensions.height}` : "n/a"],
    ["hash", frame.sha256 ? frame.sha256.slice(0, 12) : "n/a"],
    ["changed", frame.changed ? "yes" : "no"],
    ["latency", frame.elapsed_ms ? `${Math.round(frame.elapsed_ms)} ms` : "n/a"],
  ];
  metrics.innerHTML = rows
    .map(([k, v]) => `<div><dt>${k}</dt><dd title="${escapeHtml(v)}">${escapeHtml(v)}</dd></div>`)
    .join("");
}

function renderFrame(panelId, frame) {
  const panel = el(panelId);
  if (!panel || !frame) return;
  const img = panel.querySelector("[data-field=image]");
  const status = panel.querySelector("[data-field=status]");
  if (frame.image_url) img.src = `${frame.image_url}?v=${encodeURIComponent(frame.sha256 || frame.ts)}`;
  setPill(status, frame.ok ? "live" : "error", frame.ok ? "ok" : "fail");
  renderMetrics(panel, frame);
}

function renderTerminal(feed) {
  const lines = feed.terminal_lines || [];
  el("terminalLines").innerHTML = lines.slice(0, 24).map((entry) => {
    const age = entry.visible_ms ? `${Math.round(entry.visible_ms / 1000)}s` : "new";
    return `<div class="line-row"><span>${escapeHtml(age)}</span><span>${escapeHtml(entry.line)}</span></div>`;
  }).join("");
}

function renderAgentFeed(feed) {
  const display = {
    run: feed.run,
    lanes: feed.lanes,
    terminal_lines: (feed.terminal_lines || []).slice(0, 16),
    latest_ui: feed.latest_ui,
    latest_live_burst: feed.latest_live_burst,
    capabilities: summarizeCapabilities(feed.capabilities || {}),
  };
  el("agentFeed").textContent = JSON.stringify(display, null, 2);
  el("agentEventCount").textContent = `${state.events.length} events`;
}

function summarizeCapabilities(cap) {
  return {
    peekaboo_version: cap.peekaboo_version,
    permissions: cap.permissions,
    screen_count: cap.screens?.data?.screens?.length ?? cap.screens?.screens?.length,
    app_count: cap.apps?.data?.applications?.length,
    codex_window_count: cap.codex_windows?.data?.windows?.length,
    menubar_count: cap.menubar?.data?.count,
    tool_count: cap.tools?.data?.count,
  };
}

function renderUi(feed) {
  const ui = feed.latest_ui;
  if (!ui) return;
  el("uiCount").textContent = `${ui.element_count || 0} elements`;
  el("uiElements").innerHTML = (ui.text_elements || []).slice(0, 18).map((item) => {
    const name = item.label || item.title || item.value || item.role || "element";
    const meta = [item.role, item.id].filter(Boolean).join(" | ");
    return `<div class="item"><strong>${escapeHtml(name)}</strong><code>${escapeHtml(meta)}</code></div>`;
  }).join("");
}

function renderBurst(feed) {
  const burst = feed.latest_live_burst;
  if (!burst) return;
  el("burstStats").textContent = `${burst.frames_kept || 0} kept`;
  const img = el("burstImage");
  if (burst.contact_sheet_url) img.src = `${burst.contact_sheet_url}?v=${encodeURIComponent(burst.ts)}`;
  el("burstFrames").innerHTML = (burst.frames || []).slice(0, 10).map((frame) => {
    return `<div class="item"><strong>${escapeHtml(frame.file || frame.path || "frame")}</strong><code>${escapeHtml(frame.reason || "")} ${escapeHtml(String(frame.changePercent ?? ""))}%</code></div>`;
  }).join("");
}

function renderCapabilities(feed) {
  const cap = feed.capabilities || {};
  const summary = summarizeCapabilities(cap);
  const tiles = [
    ["Peekaboo", summary.peekaboo_version || "unknown"],
    ["Permissions", permissionText(summary.permissions)],
    ["Screens", text(summary.screen_count, "0")],
    ["Apps", text(summary.app_count, "0")],
    ["Codex Windows", text(summary.codex_window_count, "0")],
    ["Menu Bar", text(summary.menubar_count, "0")],
    ["MCP Tools", text(summary.tool_count, "0")],
    ["Mode", feed.run?.mode || "unknown"],
  ];
  el("capabilities").innerHTML = tiles.map(([k, v]) => `<div class="cap-tile"><b>${escapeHtml(k)}</b><span>${escapeHtml(v)}</span></div>`).join("");
  setPill(el("capabilityStatus"), summary.peekaboo_version ? "ready" : "mock", summary.peekaboo_version ? "ok" : "warn");
}

function permissionText(value) {
  if (!value) return "unknown";
  if (typeof value === "string") return value.replace(/\n/g, " | ");
  return JSON.stringify(value);
}

function renderEvents() {
  const rows = state.events.slice(-80).reverse().map((event) => {
    const detail = event.payload?.lane_id || event.payload?.source_label || event.payload?.summary || event.payload?.message || "";
    return `<div class="event-row"><span>${escapeHtml(fmtTime(event.ts))}</span><span>${escapeHtml(event.type)}</span><span>${escapeHtml(detail)}</span></div>`;
  }).join("");
  el("eventLog").innerHTML = rows;
  el("eventClock").textContent = fmtTime(new Date().toISOString());
}

function render(feed) {
  if (!feed || state.frozen) return;
  state.feed = feed;
  el("runMeta").textContent = `${feed.run?.id || "run"} | ${feed.run?.evidence_root || ""}`;
  const lanes = feed.lanes || {};
  const laneIds = Object.keys(lanes);
  renderFrame("screenA", lanes[laneIds.find((id) => id.startsWith("screen-"))] || lanes["screen-0"]);
  renderFrame("screenB", lanes[laneIds.filter((id) => id.startsWith("screen-"))[1]] || lanes["screen-1"]);
  renderFrame("codexTerminal", lanes["codex-terminal"]);
  renderTerminal(feed);
  renderAgentFeed(feed);
  renderUi(feed);
  renderBurst(feed);
  renderCapabilities(feed);
  renderEvents();
}

async function refresh() {
  const res = await fetch("/agent-feed/latest");
  render(await res.json());
}

function connectEvents() {
  const source = new EventSource("/events");
  source.addEventListener("message", (message) => {
    const event = JSON.parse(message.data);
    state.events.push(event);
    if (state.events.length > 300) state.events.shift();
    if (event.feed) render(event.feed);
  });
  source.addEventListener("error", () => {
    el("runMeta").textContent = "event stream reconnecting";
  });
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

el("freezeToggle").addEventListener("change", (event) => {
  state.frozen = event.target.checked;
});
el("refreshButton").addEventListener("click", refresh);

refresh();
connectEvents();
