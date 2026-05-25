const state = {
  data: null,
  selectedTask: 'all',
  indexes: null,
  highlightedDelivery: null,
};

const fallback = {
  schema_version: 1,
  generated_at: new Date().toISOString(),
  title: 'Agent Mail Triad Dogfood',
  summary: 'No fixture has been generated yet.',
  runtime: {},
  agents: [],
  tasks: [],
  mail: [],
};

async function loadFixture() {
  try {
    const response = await fetch('./fixtures/latest.json', { cache: 'no-store' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return await response.json();
  } catch {
    return fallback;
  }
}

function text(value) {
  return value == null ? '' : String(value);
}

function initials(name) {
  return text(name)
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0].toUpperCase())
    .join('');
}

function safeId(value) {
  return text(value).replace(/[^a-zA-Z0-9_-]/g, '-');
}

function shortId(value) {
  return text(value)
    .replace(/^delivery_/, 'del ')
    .replace(/^notif_/, 'notif ')
    .replace(/^talkmsg_/, 'msg ')
    .replace(/^mail_injection_/, 'inject ')
    .replace(/^mail_control_/, 'control ')
    .replace(/^sha256:/, 'sha ');
}

function agentUri(agent) {
  return agent.agent_id ? `agent://codex/${agent.agent_id}` : '';
}

function compactList(values, fallback = 'not recorded') {
  const unique = [...new Set(values.filter(Boolean).map(text))];
  return unique.length ? unique.join(', ') : fallback;
}

function formatCode(value) {
  return text(value).replace(/_/g, ' ');
}

function executionLabel(value) {
  const mode = text(value);
  if (!mode) return 'execution mode not recorded';
  if (mode === 'simulated_record_only') return 'simulated record-only app-server';
  return formatCode(mode);
}

function timeLabel(value) {
  if (!value) return '';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit', second: '2-digit' });
}

function buildIndexes(data) {
  const actors = new Map();
  const mailByDelivery = new Map();
  const mailByMessage = new Map();
  const mailByTask = new Map();

  actors.set('system://dogfood.mission-control', {
    name: 'Mission Control',
    role: 'Task Source',
    short: 'Mission Control',
  });

  for (const agent of data.agents || []) {
    const actor = {
      name: agent.name || agent.agent_id || agent.role_address,
      role: agent.role || 'Agent',
      short: agent.name || agent.role || agent.role_address,
      accent: agent.accent,
      agent,
    };
    if (agent.role_address) actors.set(agent.role_address, actor);
    if (agent.agent_id) actors.set(agentUri(agent), actor);
  }

  for (const mail of data.mail || []) {
    if (!mailByTask.has(mail.task_id)) mailByTask.set(mail.task_id, []);
    mailByTask.get(mail.task_id).push(mail);
    if (mail.delivery_id) mailByDelivery.set(mail.delivery_id, mail);
    if (mail.message_id) mailByMessage.set(mail.message_id, mail);
  }

  for (const entries of mailByTask.values()) {
    entries.forEach((mail, index) => {
      mail.__taskSequence = index + 1;
      mail.__taskTotal = entries.length;
    });
  }

  return { actors, mailByDelivery, mailByMessage, mailByTask };
}

function actorFor(address) {
  const actor = state.indexes?.actors.get(address);
  if (actor) return actor;
  return {
    name: text(address) || 'Unknown',
    role: '',
    short: text(address) || 'Unknown',
  };
}

function actorName(address) {
  const actor = actorFor(address);
  return actor.role ? `${actor.name} (${actor.role})` : actor.name;
}

function taskTitle(taskId) {
  if (taskId === 'all') return 'All tasks';
  return state.data.tasks.find((task) => task.id === taskId)?.title || taskId;
}

function selectedMail() {
  const mail = state.data.mail || [];
  if (state.selectedTask === 'all') return mail;
  return mail.filter((item) => item.task_id === state.selectedTask);
}

function selectedAgentCount() {
  if (state.selectedTask === 'all') return state.data.agents.length;
  return state.data.agents.filter((agent) =>
    (agent.session || []).some((event) => event.task_id === state.selectedTask)
  ).length;
}

function selectedTaskCount() {
  return state.selectedTask === 'all' ? state.data.tasks.length : Math.min(state.data.tasks.length, 1);
}

function boundaryEvidence(data) {
  const laneBodies = (data.agents || []).flatMap((agent) => agent.session || []);
  const laneEvidence = laneBodies.some((event) =>
    /thread\/inject_items|mail\.open|opened through/i.test(`${event.body || ''} ${(event.meta || []).join(' ')}`)
  );
  const ledgerEvidence = (data.mail || []).some((mail) =>
    /thread\/inject_items/i.test(`${mail.content_delivery_method || ''}`)
  );
  return laneEvidence || ledgerEvidence;
}

function assertionSummary() {
  const assertions = state.data.assertions || [];
  if (!assertions.length) return { passed: 0, failed: 0, other: 0, total: 0, label: 'No assertions recorded' };
  const passed = assertions.filter((item) => item.status === 'passed').length;
  const failed = assertions.filter((item) => item.status === 'failed').length;
  const other = assertions.length - passed - failed;
  const label = failed
    ? `${failed} failed of ${assertions.length}`
    : `${passed}/${assertions.length} passed${other ? `, ${other} other` : ''}`;
  return { passed, failed, other, total: assertions.length, label };
}

function coverageSummary() {
  const coverage = state.data.protocol_coverage || {};
  const surfaces = coverage.surfaces || [];
  const gaps = coverage.gaps || surfaces.filter((surface) => surface.status !== 'passed');
  const simulatedSurfaces = surfaces.filter((surface) => surface.app_server_execution);
  const simulatedDetail = simulatedSurfaces.length
    ? `; ${formatCode(simulatedSurfaces[0].id)} used ${executionLabel(simulatedSurfaces[0].app_server_execution)}`
    : '';
  if (!surfaces.length) {
    return {
      status: coverage.status || 'not recorded',
      surfaces,
      gaps,
      label: 'Coverage not recorded',
      detail: 'No protocol coverage summary was included in this fixture',
    };
  }
  const passed = surfaces.filter((surface) => surface.status === 'passed').length;
  const label = gaps.length ? `${gaps.length} protocol gaps` : `${passed}/${surfaces.length} surfaces`;
  const detail = gaps.length
    ? gaps.map((gap) => `${formatCode(gap.id)} ${gap.observed}/${gap.expected}`).slice(0, 3).join(', ')
    : `Project page ${coverage.project_page?.route || 'not recorded'}, ${coverage.expected_mail_count || '?'} expected mail records${simulatedDetail}`;
  return { status: coverage.status || (gaps.length ? 'gaps' : 'passed'), surfaces, gaps, label, detail };
}

function selectedDeliveryMethods() {
  return compactList(selectedMail().map((mail) => mail.content_delivery_method), 'delivery method not recorded');
}

function selectedExecutionModes() {
  return compactList(selectedMail().map((mail) => executionLabel(mail.app_server_execution)), 'execution mode not recorded');
}

function summaryText() {
  if (!state.data.mail?.length) return state.data.summary || 'No mail fixture has been generated yet.';
  const scope = state.selectedTask === 'all' ? 'All tasks' : taskTitle(state.selectedTask);
  const proof = assertionSummary();
  const coverage = coverageSummary();
  const setupNote = 'Prompt and harness cards are setup evidence, not mail bodies.';
  const simulatedNote = selectedMail().some((mail) => mail.app_server_execution === 'simulated_record_only')
    ? 'Injection receipt recording is host-only with simulated app-server success.'
    : '';
  const bodyNote = boundaryEvidence(state.data)
    ? `Bodies opened through ${selectedDeliveryMethods()}. ${simulatedNote}`
    : 'Mail body boundary evidence was not found in this fixture.';
  return `${scope}: ${selectedMail().length} ledger messages, ${proof.label}, ${coverage.label}. ${setupNote} ${bodyNote}`;
}

function renderStats() {
  const generated = new Date(state.data.generated_at).toLocaleString();
  document.getElementById('runSummary').textContent = `${summaryText()} Generated ${generated}.`;
  document.getElementById('agentCount').textContent = selectedAgentCount();
  document.getElementById('mailCount').textContent = selectedMail().length;
  document.getElementById('taskCount').textContent = selectedTaskCount();
  document.getElementById('ledgerScope').textContent =
    state.selectedTask === 'all' ? 'showing all task mail' : `showing ${taskTitle(state.selectedTask)}`;
  document.getElementById('evidencePath').textContent =
    state.data.runtime?.evidence_dir || 'evidence not available';
}

function renderTasks() {
  const strip = document.querySelector('.task-strip');
  const existing = [...strip.querySelectorAll('[data-generated="true"]')];
  existing.forEach((node) => node.remove());
  const all = strip.querySelector('[data-task="all"]');
  if (all) {
    all.textContent = `All (${state.data.mail.length})`;
    all.setAttribute('aria-label', `Show all ${state.data.mail.length} mail records`);
  }
  for (const task of state.data.tasks) {
    const button = document.createElement('button');
    button.type = 'button';
    button.setAttribute('role', 'tab');
    button.className = 'task-tab';
    button.dataset.task = task.id;
    button.dataset.generated = 'true';
    const count = (state.indexes.mailByTask.get(task.id) || []).length;
    button.textContent = `${task.title} (${count})`;
    button.setAttribute('aria-label', `Show ${count} mail records for ${task.title}`);
    strip.append(button);
  }
  strip.querySelectorAll('.task-tab').forEach((button) => {
    const active = button.dataset.task === state.selectedTask;
    button.classList.toggle('active', active);
    button.setAttribute('aria-selected', active ? 'true' : 'false');
    button.onclick = () => {
      state.selectedTask = button.dataset.task;
      state.highlightedDelivery = null;
      render();
    };
  });
}

function idsFromMeta(event) {
  const ids = {};
  for (const item of event.meta || []) {
    const value = text(item);
    if (value.startsWith('delivery_')) ids.deliveryId = value;
    if (value.startsWith('notif_')) ids.notificationId = value;
    if (value.startsWith('talkmsg_')) ids.messageId = value;
  }
  return ids;
}

function normalizedBody(value) {
  return text(value).replace(/\s+/g, ' ').slice(0, 90).toLowerCase();
}

function matchesBody(event, mail) {
  const eventBody = normalizedBody(event.body);
  const mailBody = normalizedBody(mail.body_excerpt);
  return eventBody && mailBody && (eventBody.includes(mailBody.slice(0, 40)) || mailBody.includes(eventBody.slice(0, 40)));
}

function explicitObjects(event) {
  return [event.mail, event.route, event.proof, event.proof?.mail, event.proof?.route].filter(Boolean);
}

function explicitIds(event) {
  const ids = {};
  for (const explicit of explicitObjects(event)) {
    ids.deliveryId ||= explicit.delivery_id || explicit.deliveryId;
    ids.messageId ||= explicit.message_id || explicit.messageId || explicit.receipt_id || explicit.receiptId;
    ids.notificationId ||= explicit.notification_id || explicit.notificationId;
  }
  return ids;
}

function explicitMail(event) {
  const { deliveryId, messageId } = explicitIds(event);
  if (deliveryId && state.indexes.mailByDelivery.has(deliveryId)) return state.indexes.mailByDelivery.get(deliveryId);
  if (messageId && state.indexes.mailByMessage.has(messageId)) return state.indexes.mailByMessage.get(messageId);
  return null;
}

function explicitRoute(event) {
  const route = explicitObjects(event).find((item) =>
    item.from || item.sender || item.sender_address || item.to || item.recipient || item.recipient_address
  );
  if (!route) return null;
  return {
    direction: route.direction,
    from: route.from || route.sender || route.sender_address,
    to: route.to || route.recipient || route.recipient_address,
    sequence: route.sequence || route.step || route.step_index,
    total: route.total || route.step_total,
    next: route.next_handoff || route.next || route.next_recipient,
    deliveryId: route.delivery_id || route.deliveryId,
  };
}

function eventMail(agent, event) {
  const direct = explicitMail(event);
  if (direct) return direct;

  const ids = idsFromMeta(event);
  if (ids.deliveryId && state.indexes.mailByDelivery.has(ids.deliveryId)) {
    return state.indexes.mailByDelivery.get(ids.deliveryId);
  }
  if (ids.messageId && state.indexes.mailByMessage.has(ids.messageId)) {
    return state.indexes.mailByMessage.get(ids.messageId);
  }

  if (event.kind !== 'mail-done') return null;

  const entries = state.indexes.mailByTask.get(event.task_id) || [];
  const sender = agentUri(agent);
  return entries.find((mail) => mail.from === sender && matchesBody(event, mail)) || null;
}

function nextMail(mail) {
  const entries = state.indexes.mailByTask.get(mail.task_id) || [];
  return entries[mail.__taskSequence] || null;
}

function eventDirection(agent, mail) {
  if (!mail) return '';
  const uri = agentUri(agent);
  if (mail.to === agent.role_address || mail.to === uri) return 'incoming';
  if (mail.from === uri || mail.from === agent.role_address) return 'outgoing';
  return 'related';
}

function setHighlight(deliveryId, scrollTarget = 'event') {
  state.highlightedDelivery = state.highlightedDelivery === deliveryId ? null : deliveryId;
  document.querySelectorAll('[data-delivery-id]').forEach((node) => {
    node.classList.toggle('linked-highlight', node.dataset.deliveryId === state.highlightedDelivery);
  });
  if (state.highlightedDelivery) {
    const key = safeId(state.highlightedDelivery);
    const candidates = [...document.querySelectorAll(`[data-delivery-key="${key}"]`)].filter(
      (node) => !node.classList.contains('hidden') && node.offsetParent !== null
    );
    const target =
      scrollTarget === 'ledger'
        ? candidates.find((node) => node.classList.contains('mail-card'))
        : candidates.find((node) => node.classList.contains('event-card') && node.dataset.direction === 'incoming') ||
          candidates.find((node) => node.classList.contains('event-card'));
    target?.scrollIntoView({
      block: 'nearest',
      behavior: 'smooth',
    });
  }
}

function routeRows(agent, event, mail) {
  const fallbackRoute = explicitRoute(event);
  if (!mail && !fallbackRoute) return [];
  if (!mail) {
    return [
      ['Direction', fallbackRoute.direction || event.kind || 'mail'],
      ['From', actorName(fallbackRoute.from)],
      ['To', actorName(fallbackRoute.to)],
      ['Sequence', fallbackRoute.sequence ? `Step ${fallbackRoute.sequence}${fallbackRoute.total ? ` of ${fallbackRoute.total}` : ''}` : 'not recorded'],
      ['Next', fallbackRoute.next ? text(fallbackRoute.next) : 'not recorded'],
    ];
  }
  const direction = eventDirection(agent, mail);
  const upcoming = nextMail(mail);
  return [
    ['Direction', direction || 'mail'],
    ['From', actorName(mail.from)],
    ['To', actorName(mail.to)],
    ['Sequence', `Step ${mail.__taskSequence || '?'} of ${mail.__taskTotal || '?'}`],
    ['Next', upcoming ? `${actorFor(upcoming.from).name} -> ${actorFor(upcoming.to).name}` : 'Task loop closed'],
  ];
}

function appendRoute(article, agent, event, mail) {
  const rows = routeRows(agent, event, mail);
  if (!rows.length) return;

  const route = document.createElement('dl');
  route.className = 'handoff-grid';
  for (const [label, value] of rows) {
    const dt = document.createElement('dt');
    dt.textContent = label;
    const dd = document.createElement('dd');
    dd.textContent = value;
    route.append(dt, dd);
  }
  article.append(route);

  const deliveryId = mail?.delivery_id || explicitRoute(event)?.deliveryId;
  if (deliveryId) {
    const link = document.createElement('button');
    link.type = 'button';
    link.className = 'ledger-link';
    link.textContent = `Ledger ${shortId(deliveryId)}`;
    link.setAttribute('aria-label', `Highlight ledger card for delivery ${deliveryId}`);
    link.addEventListener('click', () => setHighlight(deliveryId, 'ledger'));
    article.append(link);
  }
}

function visibleEvent(event) {
  return state.selectedTask === 'all' || event.task_id === 'all' || event.task_id === state.selectedTask;
}

function eventCard(agent, event) {
  const mail = eventMail(agent, event);
  const article = document.createElement('article');
  article.className = 'event-card';
  article.dataset.kind = event.kind;
  article.dataset.task = event.task_id || 'all';
  article.style.setProperty('--accent', agent.accent || '#2f6c9f');
  article.classList.toggle('hidden', !visibleEvent(event));
  const routeDeliveryId = mail?.delivery_id || explicitRoute(event)?.deliveryId;
  if (routeDeliveryId) {
    article.dataset.deliveryId = routeDeliveryId;
    article.dataset.deliveryKey = safeId(routeDeliveryId);
    article.dataset.direction = eventDirection(agent, mail);
    article.classList.toggle('linked-highlight', routeDeliveryId === state.highlightedDelivery);
  }

  const top = document.createElement('div');
  top.className = 'event-top';
  const kind = document.createElement('span');
  kind.className = 'event-kind';
  kind.textContent = event.kind || 'event';
  const topMeta = document.createElement('span');
  topMeta.className = 'event-task';
  topMeta.textContent = compactList([taskTitle(event.task_id || 'all'), timeLabel(event.at)]);
  top.append(kind, topMeta);

  const title = document.createElement('div');
  title.className = 'event-title';
  title.textContent = event.title || mail?.subject || 'Untitled event';

  const body = document.createElement('div');
  body.className = 'event-body';
  body.textContent = event.body || '';

  const meta = document.createElement('div');
  meta.className = 'event-meta';
  const chips = (event.meta || [])
    .filter((item) => !/^bodyless notification$/i.test(text(item)))
    .slice(0, 5);
  for (const item of chips) {
    const chip = document.createElement('span');
    chip.textContent = shortId(item);
    meta.append(chip);
  }

  article.append(top, title);
  appendRoute(article, agent, event, mail);
  article.append(body);
  if (meta.childNodes.length) article.append(meta);
  return article;
}

function setupDetails(agent, setupEvents) {
  const details = document.createElement('details');
  details.className = 'setup-details';
  details.open = state.selectedTask === 'all';

  const summary = document.createElement('summary');
  summary.textContent = `Setup evidence (${setupEvents.length})`;
  details.append(summary);

  const list = document.createElement('div');
  list.className = 'setup-list';
  for (const event of setupEvents) list.append(eventCard(agent, event));
  details.append(list);
  return details;
}

function agentLane(agent) {
  const section = document.createElement('section');
  section.className = 'agent-lane';

  const head = document.createElement('div');
  head.className = 'agent-head';
  const avatar = document.createElement('div');
  avatar.className = 'avatar';
  avatar.setAttribute('aria-hidden', 'true');
  avatar.style.background = agent.accent || '#2f6c9f';
  avatar.textContent = initials(agent.name);
  const titleBlock = document.createElement('div');
  const name = document.createElement('div');
  name.className = 'agent-name';
  name.textContent = agent.name;
  const role = document.createElement('div');
  role.className = 'agent-role';
  role.textContent = agent.role;
  const address = document.createElement('div');
  address.className = 'agent-address';
  address.textContent = agent.role_address;
  titleBlock.append(name, role, address);
  head.append(avatar, titleBlock);

  const metrics = document.createElement('div');
  metrics.className = 'agent-metrics';
  const agentTaskMail = selectedMail().filter((mail) => {
    const uri = agentUri(agent);
    return mail.from === uri || mail.to === uri || mail.to === agent.role_address || mail.from === agent.role_address;
  });
  for (const [label, value] of [
    ['Turns', `${agent.turns_completed || 0}/${agent.turns_started || 0}`],
    ['Task mail', agentTaskMail.length],
    ['Agent', agent.agent_id || '--'],
  ]) {
    const tile = document.createElement('div');
    const span = document.createElement('span');
    span.textContent = label;
    const strong = document.createElement('strong');
    strong.textContent = value;
    tile.append(span, strong);
    metrics.append(tile);
  }

  const list = document.createElement('div');
  list.className = 'session-list';
  const setupEvents = [];
  const taskEvents = [];
  for (const event of agent.session || []) {
    if (event.task_id === 'all' || event.kind === 'prompt' || event.kind === 'harness') setupEvents.push(event);
    else taskEvents.push(event);
  }
  if (setupEvents.length) list.append(setupDetails(agent, setupEvents));
  for (const event of taskEvents) list.append(eventCard(agent, event));

  section.append(head, metrics, list);
  return section;
}

function renderAgents() {
  const grid = document.getElementById('agentGrid');
  grid.replaceChildren(...state.data.agents.map(agentLane));
}

function proofCards() {
  const assertions = assertionSummary();
  const coverage = coverageSummary();
  const failed = (state.data.assertions || []).filter((item) => item.status === 'failed');
  const hostTools = state.data.runtime?.host_only_tools || [];
  const mcpTools = state.data.runtime?.mcp_tools || [];
  const commandCount = state.data.runtime?.command_count;
  const hostInjection = hostTools.includes('wiki.mail.record_injection');
  return [
    ['Run', state.data.runtime?.run_id || 'fixture fallback', `${commandCount ?? '--'} commands captured`],
    [
      'Boundary',
      selectedDeliveryMethods(),
      `${selectedExecutionModes()}${hostInjection ? '; host-only injection receipt recorder' : ''}`,
    ],
    ['Coverage', coverage.label, coverage.detail],
    ['Assertions', assertions.label, failed.length ? failed.map((item) => formatCode(item.code)).slice(0, 3).join(', ') : 'No failed checks'],
    ['Tools', `${mcpTools.length} MCP tools`, hostTools.length ? `Host-only: ${hostTools.join(', ')}` : 'No host-only tools recorded'],
  ];
}

function renderProof() {
  const grid = document.getElementById('proofGrid');
  if (!grid) return;
  grid.replaceChildren(
    ...proofCards().map(([label, value, detail]) => {
      const card = document.createElement('article');
      card.className = 'proof-card';
      if (/simulated|host-only/i.test(`${value} ${detail}`)) card.classList.add('proof-card--caveat');
      const title = document.createElement('span');
      title.textContent = label;
      const strong = document.createElement('strong');
      strong.textContent = value;
      const small = document.createElement('small');
      small.textContent = detail;
      card.append(title, strong, small);
      return card;
    })
  );
}

function idRows(mail) {
  return [
    ['Delivery', mail.delivery_id],
    ['Notification', mail.notification_id || 'bodyless notification'],
    ['Receipt', mail.message_id],
    ['Reply to', mail.reply_to || 'new task'],
    ['Page', mail.wiki_route ? `${mail.wiki_page || 'page'} ${mail.wiki_route}` : mail.wiki_page || 'not recorded'],
    ['Talk', mail.talk_thread_id || mail.talk_source || 'not recorded'],
    ['Claim', `${mail.claim_state || 'not recorded'}${mail.claimed_by ? ` by ${shortId(mail.claimed_by)}` : ''}`],
    ['Mark / Ack', `${mail.mark_state || 'not recorded'} / ${mail.notification_ack_state || 'not recorded'}`],
    ['Method', mail.content_delivery_method || 'not recorded'],
    ['Injection', mail.injection_id || 'not recorded'],
    ['Control', mail.control_event_id || 'not recorded'],
    ['Operation', mail.operation_id || 'not recorded'],
    ['Body SHA', mail.body_sha256 || 'not recorded'],
  ];
}

function mailCard(mail) {
  const article = document.createElement('article');
  article.className = 'mail-card';
  article.dataset.task = mail.task_id;
  if (mail.delivery_id) {
    article.id = `ledger-${safeId(mail.delivery_id)}`;
    article.dataset.deliveryId = mail.delivery_id;
    article.dataset.deliveryKey = safeId(mail.delivery_id);
    article.classList.toggle('linked-highlight', mail.delivery_id === state.highlightedDelivery);
  }
  article.classList.toggle('hidden', state.selectedTask !== 'all' && mail.task_id !== state.selectedTask);

  const top = document.createElement('div');
  top.className = 'mail-topline';
  const sequence = document.createElement('span');
  sequence.className = 'mail-sequence';
  sequence.textContent = `${mail.__taskSequence || '?'} / ${mail.__taskTotal || '?'}`;
  const statePill = document.createElement('span');
  statePill.className = 'mail-state';
  statePill.textContent = mail.state || 'unknown';
  top.append(sequence, statePill);

  const subject = document.createElement('div');
  subject.className = 'mail-subject';
  subject.textContent = mail.subject;

  const route = document.createElement('div');
  route.className = 'mail-route';
  route.textContent = `${actorName(mail.from)} -> ${actorName(mail.to)}`;

  const body = document.createElement('div');
  body.className = 'mail-excerpt';
  body.textContent = mail.body_excerpt;

  const ids = document.createElement('dl');
  ids.className = 'mail-ids';
  for (const [label, value] of idRows(mail)) {
    const dt = document.createElement('dt');
    dt.textContent = label;
    const dd = document.createElement('dd');
    dd.textContent = shortId(value);
    ids.append(dt, dd);
  }

  const lane = document.createElement('button');
  lane.type = 'button';
  lane.className = 'lane-link';
  lane.textContent = 'Highlight handoff';
  lane.setAttribute('aria-label', `Highlight lane event for delivery ${mail.delivery_id}`);
  lane.addEventListener('click', () => setHighlight(mail.delivery_id, 'event'));

  article.append(top, subject, route, body, ids, lane);
  return article;
}

function renderMail() {
  document.getElementById('mailLedger').replaceChildren(...selectedMail().map(mailCard));
}

function render() {
  renderStats();
  renderTasks();
  renderProof();
  renderAgents();
  renderMail();
}

loadFixture().then((data) => {
  state.data = data;
  state.indexes = buildIndexes(data);
  render();
});
