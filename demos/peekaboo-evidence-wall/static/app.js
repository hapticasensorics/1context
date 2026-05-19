const state = {
  frozen: false,
  events: [],
  feed: null,
  refreshQueued: false,
};

const el = (id) => document.getElementById(id);

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function text(value, fallback = "waiting") {
  if (value === undefined || value === null || value === "") return fallback;
  return String(value);
}

function fmtTime(ts) {
  if (!ts) return "";
  const date = new Date(ts);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleTimeString([], { hour12: false });
}

function fmtBytes(value) {
  const bytes = Number(value || 0);
  if (bytes > 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes > 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

function setPill(node, value, kind = "ok") {
  if (!node) return;
  node.textContent = value;
  node.classList.remove("ok", "warn", "fail", "muted");
  node.classList.add(kind);
}

function renderMetrics(panel, frame) {
  const metrics = panel.querySelector(".metrics");
  const segment = frame.last_segment || frame.active_segment || frame;
  const rows = [
    ["time", fmtTime(frame.ts || frame.updated_at)],
    ["source", frame.source_label || frame.lane_id],
    ["frames", frame.frames_seen ?? 0],
    ["segments", frame.segments_seen ?? 0],
    ["size", frame.dimensions ? `${frame.dimensions.width}x${frame.dimensions.height}` : fmtBytes(frame.size_bytes)],
    ["fps", `${text(frame.active_fps, "?")} active / ${text(frame.idle_fps, "?")} idle`],
    ["hash", frame.sha256 ? frame.sha256.slice(0, 12) : "n/a"],
    ["contact", segment.contact_sheet_url ? "ready" : "pending"],
  ];
  metrics.innerHTML = rows
    .map(([k, v]) => `<div><dt>${escapeHtml(k)}</dt><dd title="${escapeHtml(v)}">${escapeHtml(v)}</dd></div>`)
    .join("");
}

function renderFrame(panelId, frame) {
  const panel = el(panelId);
  if (!panel) return;
  const title = panel.querySelector("[data-field=title]");
  const img = panel.querySelector("[data-field=image]");
  const status = panel.querySelector("[data-field=status]");
  if (!frame) {
    if (title) title.textContent = panelId === "codexWindow" ? "Codex Window Stream" : "Work Screen";
    setPill(status, "missing", "warn");
    return;
  }
  const sourceLabel = frame.source_label || frame.lane_id || panelId;
  if (title) title.textContent = sourceLabel;
  if (frame.image_url) {
    img.src = `${frame.image_url}?v=${encodeURIComponent(frame.sha256 || frame.ts || Date.now())}`;
    img.alt = `${sourceLabel} evidence frame`;
    img.classList.add("ready");
  }
  const statusKind = frame.status === "error" ? "fail" : frame.status === "capturing" ? "warn" : "ok";
  setPill(status, frame.status || "live", statusKind);
  renderMetrics(panel, frame);
}

function renderHeader(feed) {
  const run = feed.run || {};
  const metrics = feed.metrics || {};
  el("runMeta").textContent = `${run.id || "run"} | ${run.evidence_root || ""}`;
  el("profileName").textContent = run.profile || "--";
  const total = Number(metrics.total_cpu_percent || 0);
  el("cpuTotal").textContent = metrics.metrics_ts ? `${total.toFixed(1)}%` : "--";
  el("cpuTotal").className = total <= 5 ? "good" : total <= 10 ? "warn-text" : "bad";
  el("cpuTarget").textContent = `${run.target_cpu_percent || 5}%`;
}

function renderLanes(feed) {
  const lanes = feed.lanes || {};
  const screenIds = Object.keys(lanes).filter((id) => id.startsWith("screen-")).sort((left, right) => {
    return Number(left.split("-")[1]) - Number(right.split("-")[1]);
  });
  renderFrame("screenA", lanes[screenIds[0]]);
  renderFrame("screenB", lanes[screenIds[1]]);
  renderFrame("codexWindow", lanes["codex-window"]);
}

function featureKind(status) {
  if (["sampled", "streaming", "available"].includes(status)) return "ok";
  if (["pending", "starting", "partial", "planned", "opt-in", "not-cli"].includes(status)) return "warn";
  if (["disabled"].includes(status)) return "muted";
  return "fail";
}

function renderFeatures(feed) {
  const features = Object.values(feed.features || {});
  const sampled = features.filter((feature) => ["sampled", "streaming", "available"].includes(feature.status)).length;
  el("featureCount").textContent = `${sampled}/${features.length}`;
  el("featureGrid").innerHTML = features.map((feature) => {
    const kind = featureKind(feature.status);
    const links = [
      feature.artifact_url ? `<a href="${escapeHtml(feature.artifact_url)}" target="_blank" rel="noreferrer">artifact</a>` : "",
      feature.raw_json_url ? `<a href="${escapeHtml(feature.raw_json_url)}" target="_blank" rel="noreferrer">raw</a>` : "",
    ].filter(Boolean).join("");
    return `<article class="feature-card ${kind}">
      <div><strong>${escapeHtml(feature.name || feature.id)}</strong><span>${escapeHtml(feature.category || "")}</span></div>
      <p>${escapeHtml(feature.summary || "")}</p>
      <footer><code>${escapeHtml(feature.status || "pending")}</code><span class="inline-links">${links}</span></footer>
    </article>`;
  }).join("");
}

function renderContract(feed) {
  const contract = feed.agent_contract || {};
  const endpoints = contract.endpoints || {};
  const ledgers = contract.ledgers || {};
  const included = contract.included_now || [];
  const excluded = contract.not_included_by_default || [];
  setPill(el("contractStatus"), `${included.length} included`, "ok");
  el("contractGrid").innerHTML = `
    <div class="tech-block wide"><strong>Current Contract</strong><p>${escapeHtml(contract.headline || "")}</p></div>
    <div class="tech-block"><strong>Endpoints</strong>${Object.entries(endpoints).map(([key, url]) => linkRow(key, url)).join("")}</div>
    <div class="tech-block"><strong>Ledgers</strong>${Object.entries(ledgers).filter(([key]) => key.endsWith("_url")).map(([key, url]) => linkRow(key, url)).join("")}</div>
    <div class="tech-block"><strong>Included Now</strong>${included.map((item) => `<div class="bullet ok-dot">${escapeHtml(item)}</div>`).join("")}</div>
    <div class="tech-block"><strong>Not Default</strong>${excluded.map((item) => `<div class="bullet warn-dot">${escapeHtml(item)}</div>`).join("")}</div>
  `;
}

function linkRow(label, url) {
  if (!url) return `<div class="kv-row"><span>${escapeHtml(label)}</span><code>n/a</code></div>`;
  return `<div class="kv-row"><span>${escapeHtml(label)}</span><a href="${escapeHtml(url)}" target="_blank" rel="noreferrer">${escapeHtml(url)}</a></div>`;
}

function parsePermissionLines(value) {
  return String(value || "").split(/\n+/).map((line) => {
    const [name, ...rest] = line.split(":");
    return { name: name?.trim() || line, value: rest.join(":").trim() };
  }).filter((item) => item.name);
}

function permissionKind(value) {
  if (/granted|ok|true/i.test(value)) return "ok";
  if (/denied|missing|false|error/i.test(value)) return "fail";
  return "warn";
}

function renderPermissions(feed) {
  const cap = feed.capabilities || {};
  const run = feed.run || {};
  const permissions = parsePermissionLines(cap.permissions);
  const allGranted = permissions.length && permissions.every((item) => permissionKind(item.value) === "ok" || item.name === "Source");
  setPill(el("permissionStatus"), allGranted ? "granted" : "check", allGranted ? "ok" : "warn");
  const runtimeRows = [
    ["Peekaboo", cap.peekaboo_version || "unknown"],
    ["Bridge", statusText(cap.bridge_status)],
    ["Daemon", statusText(cap.daemon_status)],
    ["Mode", run.mode || "unknown"],
    ["Profile", run.profile || "unknown"],
    ["Sample see", run.capture?.sample_see ? "enabled" : "disabled"],
  ];
  el("permissionGrid").innerHTML = `
    <div class="tech-block"><strong>TCC / Permissions</strong>
      ${permissions.map((item) => `<div class="perm-row ${permissionKind(item.value)}"><span>${escapeHtml(item.name)}</span><code>${escapeHtml(item.value || "present")}</code></div>`).join("")}
    </div>
    <div class="tech-block"><strong>Runtime</strong>
      ${runtimeRows.map(([key, value]) => `<div class="kv-row"><span>${escapeHtml(key)}</span><code>${escapeHtml(value)}</code></div>`).join("")}
    </div>
  `;
}

function statusText(value) {
  if (!value) return "unknown";
  if (typeof value === "string") return value;
  if (value.success !== undefined) return value.success ? "ok" : "error";
  if (value.data?.status) return value.data.status;
  if (value.error) return value.error;
  return "sampled";
}

function countAt(value, path) {
  let cursor = value;
  for (const key of path) cursor = cursor?.[key];
  return Array.isArray(cursor) ? cursor.length : Number(cursor?.count ?? cursor ?? 0) || 0;
}

function renderInventory(feed) {
  const cap = feed.capabilities || {};
  const files = cap.inventory_files || {};
  const spaces = cap.space_list?.data?.spaces || cap.space_list?.data || [];
  const rows = [
    ["Screens", countAt(cap.screens, ["data", "screens"])],
    ["Apps", countAt(cap.apps, ["data", "applications"])],
    ["Codex windows", countAt(cap.codex_windows, ["data", "windows"])],
    ["Menu bar items", cap.menubar?.data?.count ?? countAt(cap.menubar, ["data", "items"])],
    ["Tools", cap.tools?.data?.count ?? countAt(cap.tools, ["data", "tools"])],
    ["Spaces", Array.isArray(spaces) ? spaces.length : "sampled"],
  ];
  setPill(el("inventoryStatus"), cap.inventory_ts ? "sampled" : "waiting", cap.inventory_ts ? "ok" : "warn");
  const fileRows = Object.entries(files).slice(0, 14).map(([key, url]) => linkRow(key, url)).join("");
  el("inventoryGrid").innerHTML = `
    <div class="tech-block"><strong>Surface Counts</strong>${rows.map(([key, value]) => `<div class="kv-row"><span>${escapeHtml(key)}</span><code>${escapeHtml(value)}</code></div>`).join("")}</div>
    <div class="tech-block"><strong>Raw Inventory Files</strong>${fileRows || "<p>No inventory files yet.</p>"}</div>
  `;
}

function renderOcr(feed) {
  const ocr = feed.ocr || {};
  setPill(el("ocrStatus"), ocr.status || "deferred", "warn");
  const sections = [
    ...(ocr.peekaboo_builtin || []),
    ...(ocr.demo_plan || []),
  ];
  el("ocrPlan").innerHTML = `
    <p>${escapeHtml(ocr.headline || "")}</p>
    ${sections.map((item) => `<div class="note-row">${escapeHtml(item)}</div>`).join("")}
  `;
}

function renderUi(feed) {
  const ui = feed.latest_ui;
  if (!ui) {
    el("uiCount").textContent = "available";
    el("uiElements").innerHTML = `<div class="item"><strong>Accessibility probe not sampled</strong><code>Run with --sample-see to collect peekaboo see --annotate / --menubar. Permissions are still inventoried above.</code></div>`;
    return;
  }
  el("uiCount").textContent = `${ui.element_count || 0} elements`;
  const image = el("uiImage");
  if (ui.screenshot_url) {
    image.src = `${ui.screenshot_url}?v=${encodeURIComponent(ui.ts || Date.now())}`;
    image.classList.add("ready");
  }
  el("uiElements").innerHTML = (ui.text_elements || []).slice(0, 18).map((item) => {
    const name = item.label || item.title || item.value || item.role || "element";
    const meta = [item.role, item.id].filter(Boolean).join(" | ");
    return `<div class="item"><strong>${escapeHtml(name)}</strong><code>${escapeHtml(meta)}</code></div>`;
  }).join("");
}

function renderProvenance(feed) {
  const lanes = Object.values(feed.lanes || {});
  const keyframes = Object.values(feed.latest_keyframes || {});
  const rows = [
    ...lanes.map((lane) => provenanceLaneRow(lane)),
    ...keyframes.slice(0, 6).map((frame) => provenanceSampleRow(frame)),
  ].join("");
  setPill(el("provenanceStatus"), `${lanes.length} lanes`, lanes.length ? "ok" : "warn");
  el("provenanceList").innerHTML = rows || "<div class=\"item\"><strong>No provenance yet</strong><code>waiting for first frame</code></div>";
}

function provenanceLaneRow(lane) {
  const command = Array.isArray(lane.command) ? lane.command.join(" ") : "";
  const links = [
    lane.image_url ? `<a href="${escapeHtml(lane.image_url)}" target="_blank" rel="noreferrer">frame</a>` : "",
    lane.command_output_url ? `<a href="${escapeHtml(lane.command_output_url)}" target="_blank" rel="noreferrer">command JSON</a>` : "",
  ].filter(Boolean).join("");
  return `<article class="provenance-row">
    <div><strong>${escapeHtml(lane.source_label || lane.lane_id)}</strong><span>${escapeHtml(lane.capture_kind || lane.feature_id || "")}</span></div>
    <code>${escapeHtml(lane.artifact_path || lane.image_url || "no artifact")}</code>
    <div class="mini-metrics"><span>${escapeHtml(lane.dimensions ? `${lane.dimensions.width}x${lane.dimensions.height}` : "n/a")}</span><span>${escapeHtml(fmtBytes(lane.size_bytes))}</span><span>${escapeHtml((lane.sha256 || "").slice(0, 16))}</span></div>
    ${command ? `<pre>${escapeHtml(command)}</pre>` : ""}
    <footer>${links}</footer>
  </article>`;
}

function provenanceSampleRow(frame) {
  const links = [
    frame.artifact_url ? `<a href="${escapeHtml(frame.artifact_url)}" target="_blank" rel="noreferrer">artifact</a>` : "",
    frame.raw_json_url ? `<a href="${escapeHtml(frame.raw_json_url)}" target="_blank" rel="noreferrer">raw</a>` : "",
  ].filter(Boolean).join("");
  return `<article class="provenance-row sample">
    <div><strong>${escapeHtml(frame.summary || frame.feature_id)}</strong><span>${escapeHtml(frame.feature_id || "sample")}</span></div>
    <code>${escapeHtml(frame.artifact_path || frame.artifact_url || "sample artifact")}</code>
    <footer>${links}</footer>
  </article>`;
}

function renderKeyframes(feed) {
  const frames = Object.values(feed.latest_keyframes || {}).filter((frame) => frame.artifact_url);
  el("keyframeCount").textContent = frames.length ? `${frames.length} samples` : "pending";
  el("keyframes").innerHTML = frames.slice(0, 8).map((frame) => `
    <a class="keyframe-card" href="${escapeHtml(frame.artifact_url)}" target="_blank" rel="noreferrer">
      <img src="${escapeHtml(frame.artifact_url)}?v=${encodeURIComponent(frame.updated_at || frame.summary || "")}" alt="${escapeHtml(frame.summary || frame.feature_id)}" />
      <span>${escapeHtml(frame.summary || frame.feature_id)}</span>
    </a>
  `).join("");
}

function summarizeCapabilities(cap) {
  return {
    peekaboo_version: cap.peekaboo_version,
    permissions: cap.permissions,
    screen_count: cap.screens?.data?.screens?.length ?? cap.screens?.screens?.length,
    app_count: cap.apps?.data?.applications?.length,
    codex_window_count: cap.codex_windows?.data?.windows?.length,
    menubar_count: cap.menubar?.data?.count,
    tool_count: cap.tools?.data?.tools?.length ?? cap.tools?.data?.count,
    bridge: cap.bridge_status?.success ?? cap.bridge_status?.data?.status,
    daemon: cap.daemon_status?.success ?? cap.daemon_status?.data?.status,
  };
}

function renderAgentFeed(feed) {
  const display = {
    run: feed.run,
    metrics: feed.metrics,
    lanes: Object.fromEntries(Object.entries(feed.lanes || {}).map(([id, lane]) => [id, {
      status: lane.status,
      source_label: lane.source_label,
      image_url: lane.image_url,
      artifact_path: lane.artifact_path,
      sha256: lane.sha256,
      dimensions: lane.dimensions,
      size_bytes: lane.size_bytes,
      command: lane.command,
      command_output_url: lane.command_output_url,
      frames_seen: lane.frames_seen,
      active_segment: lane.active_segment?.segment_path,
      last_segment: lane.last_segment?.segment_path,
    }])),
    features: Object.fromEntries(Object.entries(feed.features || {}).map(([id, feature]) => [id, {
      status: feature.status,
      artifact_url: feature.artifact_url,
      summary: feature.summary,
    }])),
    agent_contract: feed.agent_contract,
    ocr: feed.ocr,
    capabilities: summarizeCapabilities(feed.capabilities || {}),
  };
  el("agentFeed").textContent = JSON.stringify(display, null, 2);
  el("agentEventCount").textContent = `${state.events.length} events`;
}

function renderEvents() {
  const rows = state.events.slice(-90).reverse().map((event) => {
    const payload = event.payload || {};
    const detail = payload.lane_id || payload.feature_id || payload.summary || payload.error || "";
    const typeClass = event.type.includes("error") ? "fail-text" : "";
    return `<div class="event-row ${typeClass}"><span>${escapeHtml(fmtTime(event.ts))}</span><span>${escapeHtml(event.type)}</span><span>${escapeHtml(detail)}</span></div>`;
  }).join("");
  el("eventLog").innerHTML = rows;
  el("eventClock").textContent = fmtTime(new Date().toISOString());
}

function render(feed) {
  if (!feed || state.frozen) return;
  state.feed = feed;
  renderHeader(feed);
  renderLanes(feed);
  renderFeatures(feed);
  renderContract(feed);
  renderPermissions(feed);
  renderInventory(feed);
  renderProvenance(feed);
  renderOcr(feed);
  renderUi(feed);
  renderKeyframes(feed);
  renderAgentFeed(feed);
  renderEvents();
}

async function refresh() {
  if (state.frozen) return;
  const res = await fetch("/agent-feed/latest");
  render(await res.json());
}

function queueRefresh() {
  if (state.refreshQueued) return;
  state.refreshQueued = true;
  window.setTimeout(async () => {
    state.refreshQueued = false;
    await refresh();
  }, 300);
}

function connectEvents() {
  const source = new EventSource("/events");
  source.addEventListener("message", (message) => {
    const event = JSON.parse(message.data);
    state.events.push(event);
    if (state.events.length > 400) state.events.shift();
    renderEvents();
    queueRefresh();
  });
  source.addEventListener("error", () => {
    el("runMeta").textContent = "event stream reconnecting";
  });
}

el("freezeToggle").addEventListener("change", (event) => {
  state.frozen = event.target.checked;
});
el("refreshButton").addEventListener("click", refresh);

refresh();
connectEvents();
window.setInterval(refresh, 1500);
