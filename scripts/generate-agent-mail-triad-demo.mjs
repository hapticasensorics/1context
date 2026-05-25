#!/usr/bin/env node
import { appendFile, cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { createInterface } from 'node:readline';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const args = new Set(process.argv.slice(2));
const shouldBuild = args.has('--build');
const keepRuntime = args.has('--keep-runtime');
const timestamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
const runNonce = randomBytes(4).toString('hex');
const runId = `${timestamp}-${runNonce}`;
const shortStamp = `${timestamp.replace(/^.*T(\d{6})Z$/, '$1').toLowerCase()}-${runNonce}`;
const evidenceDir = resolve(
  process.env.ONECONTEXT_AGENT_MAIL_TRIAD_EVIDENCE_DIR ||
    join(repoRoot, 'test-results', `agent-mail-triad-demo-${runId}`)
);
const requestDir = join(evidenceDir, 'requests');
const ledgerEvidenceDir = join(evidenceDir, 'ledgers');
const runtimeHome = resolve(
  process.env.ONECONTEXT_AGENT_MAIL_TRIAD_HOME || join('/tmp', `1context-agent-mail-triad-${shortStamp}`)
);
const runtimeRoot = join(runtimeHome, '1Context');
const fixturePath = resolve(
  process.env.ONECONTEXT_AGENT_MAIL_TRIAD_FIXTURE ||
    join(repoRoot, 'demos/agent-mail-triad/static/fixtures/latest.json')
);
const harnessBin = resolve(
  process.env.ONECONTEXT_AGENT_HARNESS_BIN || join(repoRoot, 'target/debug/onecontext-agent-harness')
);
const wikiBin = resolve(
  process.env.ONECONTEXT_WIKI_CORE_BIN || join(repoRoot, 'target/debug/onecontext-wiki')
);
const wikiMcpServerScript = resolve(repoRoot, 'scripts/onecontext-wiki-mcp-server.mjs');
const commandLog = join(evidenceDir, 'commands.jsonl');
const mcpProtocolLog = join(evidenceDir, 'mcp-protocol.jsonl');

const mcpToolsets = {
  mail: [
    'wiki.agent.identify',
    'wiki.agent.inbox',
    'wiki.mail.open',
    'wiki.mail.claim',
    'wiki.mail.mark',
    'wiki.notify.poll',
    'wiki.notify.ack',
  ],
  wiki: ['wiki.list', 'wiki.page.status', 'wiki.talk.append'],
};
const expectedMcpTools = [...mcpToolsets.mail, ...mcpToolsets.wiki];
const hiddenHostTools = ['wiki.notify.dispatch', 'wiki.mail.record_injection'];
const collaborationPage = 'projects';
let discoveredMcpTools = [];
let requestSequence = 0;
let mcp = null;
const operationCounts = {};
const hostOperationCounts = {};
const wikiProbe = {
  page_id: collaborationPage,
  list_seen: false,
  status_seen: false,
  route: null,
  talk_handle: null,
  published_handle: null,
};

const agents = [
  {
    key: 'pip',
    name: 'Pip Promptsmith',
    role: 'Mission Framer',
    unit_id: `agent-triad-pip-${shortStamp}`,
    role_address: 'role://triad.promptsmith',
    thread_id: `triad-pip-thread-${shortStamp}`,
    session_id: `triad-pip-session-${shortStamp}`,
    accent: '#e56b5d',
    prompt:
      'Turn vague wishes into crisp mission postcards. Add one odd constraint, one acceptance check, then mail the route-builder.',
  },
  {
    key: 'mira',
    name: 'Mira Mapmaker',
    role: 'Route Builder',
    unit_id: `agent-triad-mira-${shortStamp}`,
    role_address: 'role://triad.cartographer',
    thread_id: `triad-mira-thread-${shortStamp}`,
    session_id: `triad-mira-session-${shortStamp}`,
    accent: '#43a98f',
    prompt:
      'Convert mission postcards into dependency maps. Name the first two links to create and send the clean route onward.',
  },
  {
    key: 'nox',
    name: 'Nox Archivist',
    role: 'Receipt Closer',
    unit_id: `agent-triad-nox-${shortStamp}`,
    role_address: 'role://triad.archivist',
    thread_id: `triad-nox-thread-${shortStamp}`,
    session_id: `triad-nox-session-${shortStamp}`,
    accent: '#7c6df2',
    prompt:
      'Close loops. Turn routes into wiki-ready notes with receipts, unanswered questions, and a tiny harmless flourish.',
  },
];

const tasks = [
  {
    id: 'shortcut-museum',
    title: 'Tiny Museum Of Lost Keyboard Shortcuts',
    seed: 'Design a wiki page plan for a tiny museum that preserves lost keyboard shortcuts as if they were artifacts.',
    object: 'a glass case for Command-Option-Whatever',
    odd_constraint: 'no exhibit label may exceed seven words',
  },
  {
    id: 'moonbase-onboarding',
    title: 'Moonbase Onboarding For New Agents',
    seed: 'Create a first-day onboarding ritual for agents arriving at a wiki-powered moonbase.',
    object: 'a laminated airlock checklist',
    odd_constraint: 'every step must leave a durable receipt',
  },
  {
    id: 'snack-sprint',
    title: 'Snack-Powered Wiki Maintenance Sprint',
    seed: 'Plan a small maintenance sprint where wiki cleanup tasks are unlocked by increasingly specific snacks.',
    object: 'a crumb-proof task board',
    odd_constraint: 'no one can claim two crunchy tasks in a row',
  },
];
const expectedMailCount = tasks.length * 4;

const fixture = {
  schema_version: 1,
  generated_at: new Date().toISOString(),
  title: 'Agent Mail Triad Dogfood',
  summary:
    'Three harness-born agents use wiki mail through the MCP facade. Every visible body was opened through the mail injection boundary; injection receipt recording is host-only and simulates app-server success unless a real app-server is wired in.',
  runtime: {
    run_id: runId,
    evidence_dir: evidenceDir,
    runtime_root: runtimeRoot,
    command_log: commandLog,
    mcp_protocol_log: mcpProtocolLog,
    app_server_execution: 'simulated_record_only',
    host_only_tools: ['wiki.ensure', 'wiki.page.create_all', 'wiki.mail.record_injection'],
  },
  agents: [],
  tasks,
  mail: [],
  assertions: [],
  protocol_coverage: {
    status: 'pending',
    expected_mail_count: expectedMailCount,
    operation_counts: {},
    host_operation_counts: {},
    ledgers: {},
    surfaces: [],
    gaps: [],
  },
};

function usage() {
  console.log(`Usage: node scripts/generate-agent-mail-triad-demo.mjs [--build] [--keep-runtime]

Builds a disposable three-agent mail dogfood run and writes:
  demos/agent-mail-triad/static/fixtures/latest.json

Environment:
  ONECONTEXT_AGENT_HARNESS_BIN                 harness CLI binary
  ONECONTEXT_WIKI_CORE_BIN                     wiki CLI binary
  ONECONTEXT_AGENT_MAIL_TRIAD_EVIDENCE_DIR     evidence output directory
  ONECONTEXT_AGENT_MAIL_TRIAD_HOME             disposable fake home
  ONECONTEXT_AGENT_MAIL_TRIAD_FIXTURE          fixture output path
`);
}

if (args.has('--help') || args.has('-h')) {
  usage();
  process.exit(0);
}

function sha256(value) {
  return `sha256:${createHash('sha256').update(value, 'utf8').digest('hex')}`;
}

function fail(code, detail) {
  const error = new Error(`${code}: ${detail}`);
  error.code = code;
  throw error;
}

function assertInvariant(condition, code, detail) {
  if (!condition) fail(code, detail);
  fixture.assertions.push({ code, status: 'passed' });
}

function countOperation(counts, operation) {
  counts[operation] = (counts[operation] || 0) + 1;
}

function redactArgs(commandArgs) {
  const redacted = [];
  for (let index = 0; index < commandArgs.length; index += 1) {
    const arg = commandArgs[index];
    redacted.push(arg);
    if (arg === '--body' || arg === '--request-json') {
      index += 1;
      redacted.push('[redacted]');
    }
  }
  return redacted;
}

async function run(command, commandArgs, options = {}) {
  const started = Date.now();
  const child = spawn(command, commandArgs, {
    cwd: repoRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
    ...options,
  });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => {
    stdout += chunk.toString('utf8');
  });
  child.stderr.on('data', (chunk) => {
    stderr += chunk.toString('utf8');
  });
  const code = await new Promise((resolveClose, reject) => {
    child.on('error', reject);
    child.on('close', resolveClose);
  });
  return {
    command,
    args: commandArgs,
    code,
    stdout,
    stderr,
    ms: Date.now() - started,
  };
}

async function logCommand(entry) {
  await appendFile(commandLog, JSON.stringify(entry) + '\n');
}

async function logMcpProtocol(entry) {
  await appendFile(mcpProtocolLog, JSON.stringify(entry) + '\n');
}

function parseJson(stdout, label) {
  try {
    return JSON.parse(stdout);
  } catch (error) {
    fail('invalid_json', `${label} did not emit JSON: ${error.message}\n${stdout.slice(0, 500)}`);
  }
}

class McpClient {
  constructor() {
    this.command = 'node';
    this.args = [wikiMcpServerScript, '--root', runtimeRoot, '--wiki-bin', wikiBin];
    this.nextId = 1;
    this.pending = new Map();
    this.stderr = '';
    this.child = spawn(this.command, this.args, {
      cwd: repoRoot,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...process.env,
        ONECONTEXT_ROOT: runtimeRoot,
        ONECONTEXT_WIKI_CORE_BIN: wikiBin,
      },
    });
    this.lines = createInterface({ input: this.child.stdout, crlfDelay: Infinity });
    this.lines.on('line', (line) => this.handleLine(line));
    this.child.stderr.on('data', (chunk) => {
      this.stderr += chunk.toString('utf8');
    });
    this.child.on('close', (code) => {
      for (const pending of this.pending.values()) {
        pending.reject(new Error(`MCP server exited with code ${code}`));
      }
      this.pending.clear();
    });
  }

  handleLine(line) {
    if (!line.trim()) return;
    let message;
    try {
      message = JSON.parse(line);
    } catch (error) {
      for (const pending of this.pending.values()) {
        pending.reject(new Error(`MCP server emitted invalid JSON: ${error.message}: ${line}`));
      }
      this.pending.clear();
      return;
    }
    const pending = this.pending.get(message.id);
    if (pending) {
      clearTimeout(pending.timeout);
      this.pending.delete(message.id);
      pending.resolve(message);
    }
  }

  async send(method, params = {}, options = {}) {
    const id = this.nextId;
    this.nextId += 1;
    const message = { jsonrpc: '2.0', id, method, params };
    const requestPath = await requestFile(
      `mcp-${method.replace(/\W+/g, '-')}${params?.name ? `-${params.name.replace(/\W+/g, '-')}` : ''}`,
      redactMcpMessage(message)
    );
    const started = Date.now();
    const timeoutMs = options.timeoutMs || 120_000;
    const response = await new Promise((resolveResponse, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`MCP ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending.set(id, { resolve: resolveResponse, reject, timeout });
      this.child.stdin.write(`${JSON.stringify(message)}\n`, (error) => {
        if (!error) return;
        clearTimeout(timeout);
        this.pending.delete(id);
        reject(error);
      });
    });
    const elapsed = Date.now() - started;
    const isToolCall = method === 'tools/call';
    const operation = isToolCall ? params.name : method;
    countOperation(operationCounts, operation);
    const status = response.error
      ? 'error'
      : isToolCall
        ? response.result?.structuredContent?.status || (response.result?.isError ? 'error' : 'ok')
        : 'ok';
    await logMcpProtocol({
      at: new Date().toISOString(),
      server: 'onecontext',
      method,
      operation,
      request: redactMcpMessage(message),
      response: redactMcpMessage(response),
      ms: elapsed,
    });
    await logCommand({
      tool: 'mcp-jsonrpc',
      via: 'stdio',
      server: 'onecontext',
      method,
      operation,
      request_file: requestPath,
      code: response.error || response.result?.isError ? 1 : 0,
      status,
      error: response.error || null,
      ms: elapsed,
      stderr: this.stderr,
    });
    if (response.error) {
      fail('mcp_request_failed', `${method} failed: ${JSON.stringify(response.error)}`);
    }
    return response.result;
  }

  notify(method, params = {}) {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  close() {
    this.lines.close();
    this.child.stdin.end();
    this.child.kill();
  }
}

async function startMcp() {
  mcp = new McpClient();
  const initialized = await mcp.send('initialize', {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: { name: 'agent-mail-triad-demo', version: '1.0.0' },
  });
  fixture.runtime.mcp_initialize = {
    protocol_version: initialized.protocolVersion,
    server_info: initialized.serverInfo,
  };
  mcp.notify('notifications/initialized');
  const listed = await mcp.send('tools/list');
  discoveredMcpTools = (listed.tools || []).map((tool) => tool.name);
  fixture.runtime.mcp_tools = discoveredMcpTools;
  assertInvariant(
    sameStringSet(discoveredMcpTools, expectedMcpTools),
    'mcp_tools_match_declared_capabilities',
    JSON.stringify({ discoveredMcpTools, expectedMcpTools })
  );
}

async function exerciseWikiProjectSurface() {
  const inventory = await mcpTool('wiki.list', {});
  assertInvariant(inventory.status === 'ok', 'wiki_list_status_ok', JSON.stringify(inventory));
  const projectListEntry = (inventory.pages || []).find((page) => page.id === collaborationPage);
  assertInvariant(Boolean(projectListEntry), 'wiki_list_includes_projects_page', JSON.stringify(inventory));
  wikiProbe.list_seen = true;

  const status = await mcpTool('wiki.page.status', { page: collaborationPage });
  assertInvariant(status.status === 'ok', 'wiki_page_status_ok', JSON.stringify(status));
  assertInvariant(status.id === collaborationPage, 'wiki_page_status_projects', JSON.stringify(status));
  assertInvariant(status.route === '/projects', 'wiki_project_route_matches', JSON.stringify(status));
  assertInvariant(status.handles?.talk === `user-wiki://page/${collaborationPage}/talk`, 'wiki_project_talk_handle_present', JSON.stringify(status));
  wikiProbe.status_seen = true;
  wikiProbe.route = status.route;
  wikiProbe.talk_handle = status.handles?.talk || null;
  wikiProbe.published_handle = status.handles?.published || null;
  fixture.protocol_coverage.project_page = {
    page_id: status.id,
    route: status.route,
    talk_handle: status.handles?.talk || null,
    published_handle: status.handles?.published || null,
    list_entry_seen: Boolean(projectListEntry),
  };
}

async function mcpTool(name, argsObject) {
  if (!mcp) fail('mcp_not_started', 'MCP client was not initialized');
  const result = await mcp.send('tools/call', { name, arguments: argsObject });
  if (result?.isError) {
    fail('mcp_tool_failed', `${name} returned isError: ${JSON.stringify(result.structuredContent || result)}`);
  }
  return result.structuredContent || parseJson(result.content?.[0]?.text || '{}', name);
}

async function requestFile(label, payload) {
  await mkdir(requestDir, { recursive: true });
  requestSequence += 1;
  const path = join(requestDir, `${String(requestSequence).padStart(4, '0')}-${label}.json`);
  await writeFile(path, JSON.stringify(payload, null, 2) + '\n');
  return path;
}

function cloneJson(value) {
  return JSON.parse(JSON.stringify(value));
}

function redactMcpMessage(message) {
  const redacted = cloneJson(message);
  const toolArgs = redacted.params?.arguments;
  if (toolArgs && Object.prototype.hasOwnProperty.call(toolArgs, 'body')) {
    toolArgs.body = '[redacted]';
  }
  const contentDelivery = redacted.result?.structuredContent?.content_delivery;
  if (Array.isArray(contentDelivery?.items)) {
    contentDelivery.item_count = contentDelivery.items.length;
    contentDelivery.items = '[redacted injection item text]';
    if (Array.isArray(redacted.result.content)) {
      redacted.result.content = [
        {
          type: 'text',
          text: `[redacted tool output: content_delivery.item_count=${contentDelivery.item_count}]`,
        },
      ];
    }
  }
  const resultArgs = redacted.result?.structuredContent?.args;
  if (resultArgs && Object.prototype.hasOwnProperty.call(resultArgs, 'body')) {
    resultArgs.body = '[redacted]';
  }
  return redacted;
}

function sorted(values) {
  return [...values].sort();
}

function sameStringSet(left, right) {
  const a = sorted(left);
  const b = sorted(right);
  return a.length === b.length && a.every((value, index) => value === b[index]);
}

async function readJsonLinesIfExists(path) {
  if (!existsSync(path)) return [];
  const text = await readFile(path, 'utf8');
  return text
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function coverageSurface(id, observed, expected, detail = {}) {
  const status = observed >= expected ? 'passed' : 'gap';
  return {
    id,
    expected,
    observed,
    status,
    ...detail,
  };
}

function ledgerStateCounts(rows) {
  return rows.reduce((counts, row) => {
    const state = row.state || 'unknown';
    counts[state] = (counts[state] || 0) + 1;
    return counts;
  }, {});
}

function rowsBy(rows, key) {
  return rows.reduce((groups, row) => {
    const value = row[key];
    if (!value) return groups;
    const group = groups.get(value) || [];
    group.push(row);
    groups.set(value, group);
    return groups;
  }, new Map());
}

function bodyHashValue(hash) {
  return String(hash || '').replace(/^sha256:/, '');
}

function recipientAgent(mailRecord) {
  return agents.find((agent) => agent.role_address === mailRecord.to);
}

function hasReplyToWithinTask(mailRecord) {
  if (!mailRecord.reply_to) return true;
  return fixture.mail.some(
    (candidate) =>
      candidate.task_id === mailRecord.task_id &&
      candidate.message_id === mailRecord.reply_to
  );
}

function mcpLifecycleIndexes(mcpEntries, mailRecord) {
  const findAfter = (operation, afterIndex, predicate) => {
    for (let index = afterIndex + 1; index < mcpEntries.length; index += 1) {
      const entry = mcpEntries[index];
      if (
        entry.method === 'tools/call' &&
        entry.operation === operation &&
        predicate(entry.request?.params?.arguments || {}, entry.response?.result?.structuredContent || {})
      ) {
        return index;
      }
    }
    return -1;
  };
  const indexes = {
    delivered: findAfter('wiki.talk.append', -1, (_argsObject, content) =>
      (content.mail_delivery?.attempts || []).some(
        (attempt) => attempt.delivery_id === mailRecord.delivery_id
      )
    ),
  };
  indexes.poll = findAfter(
    'wiki.notify.poll',
    indexes.delivered,
    (argsObject, content) =>
      argsObject.agent_id === mailRecord.claimed_by &&
      (content.notifications || []).some(
        (notification) => notification.delivery_id === mailRecord.delivery_id
      )
  );
  indexes.inbox = findAfter(
    'wiki.agent.inbox',
    indexes.poll,
    (argsObject, content) =>
      argsObject.agent_id === mailRecord.claimed_by &&
      (content.deliveries || []).some((delivery) => delivery.delivery_id === mailRecord.delivery_id)
  );
  indexes.open = findAfter(
    'wiki.mail.open',
    indexes.inbox,
    (argsObject) =>
      argsObject.delivery_id === mailRecord.delivery_id &&
      argsObject.agent_id === mailRecord.claimed_by
  );
  indexes.claim = findAfter(
    'wiki.mail.claim',
    indexes.open,
    (argsObject) =>
      argsObject.delivery_id === mailRecord.delivery_id &&
      argsObject.agent_id === mailRecord.claimed_by
  );
  indexes.mark = findAfter(
    'wiki.mail.mark',
    indexes.claim,
    (argsObject) =>
      argsObject.delivery_id === mailRecord.delivery_id &&
      argsObject.agent_id === mailRecord.claimed_by &&
      argsObject.state === 'done'
  );
  indexes.ack = findAfter(
    'wiki.notify.ack',
    indexes.mark,
    (argsObject, content) =>
      argsObject.agent_id === mailRecord.claimed_by &&
      content.notification?.notification_id === mailRecord.notification_id &&
      content.notification?.delivery_id === mailRecord.delivery_id
  );
  return indexes;
}

function mcpLifecycleIsOrdered(indexes) {
  return (
    indexes.delivered >= 0 &&
    indexes.poll > indexes.delivered &&
    indexes.inbox > indexes.poll &&
    indexes.open > indexes.inbox &&
    indexes.claim > indexes.open &&
    indexes.mark > indexes.claim &&
    indexes.ack > indexes.mark
  );
}

async function harness(commandArgs, request = null) {
  const requestPath = request ? await requestFile(`harness-${commandArgs[0]}`, request) : null;
  const useBinary = existsSync(harnessBin);
  const command = useBinary ? harnessBin : 'cargo';
  const fullArgs = useBinary
    ? ['--root', runtimeRoot, ...commandArgs]
    : ['run', '-q', '-p', 'onecontext-agent-harness-daemon', '--', '--root', runtimeRoot, ...commandArgs];
  if (requestPath) fullArgs.push('--request-file', requestPath);
  const entry = await run(command, fullArgs);
  const json = parseJson(entry.stdout, `onecontext-agent-harness ${commandArgs.join(' ')}`);
  await logCommand({
    tool: 'onecontext-agent-harness',
    via: useBinary ? 'binary' : 'cargo-run',
    args: ['--root', runtimeRoot, ...commandArgs, ...(requestPath ? ['--request-file', requestPath] : [])],
    request_file: requestPath,
    code: entry.code,
    status: json.status,
    operation: json.operation,
    error: json.error || null,
    ms: entry.ms,
    stderr: entry.stderr,
  });
  if (entry.code !== 0 || !['ok', 'scaffold'].includes(json.status)) {
    fail('harness_command_failed', `${commandArgs.join(' ')} failed: ${JSON.stringify(json.error || json)}`);
  }
  return json;
}

async function wikiHost(commandArgs, hostScope = 'host-only') {
  const useBinary = existsSync(wikiBin);
  const command = useBinary ? wikiBin : 'cargo';
  const fullArgs = useBinary
    ? ['--root', runtimeRoot, ...commandArgs]
    : ['run', '-q', '-p', 'onecontext-wiki-daemon', '--', '--root', runtimeRoot, ...commandArgs];
  const entry = await run(command, fullArgs);
  const json = parseJson(entry.stdout, `onecontext-wiki ${commandArgs.join(' ')}`);
  if (json.operation) countOperation(hostOperationCounts, json.operation);
  await logCommand({
    tool: 'onecontext-wiki-host-only',
    via: useBinary ? 'host-only-binary' : 'host-only-cargo-run',
    host_scope: hostScope,
    args: ['--root', runtimeRoot, ...redactArgs(commandArgs)],
    code: entry.code,
    status: json.status,
    operation: json.operation,
    error: json.error || null,
    ms: entry.ms,
    stderr: entry.stderr,
  });
  if (entry.code !== 0) {
    fail('wiki_host_command_failed', `${commandArgs.join(' ')} failed: ${JSON.stringify(json.error || json)}`);
  }
  return json;
}

async function maybeBuild() {
  if (!shouldBuild) return;
  const build = await run('cargo', ['build', '-p', 'onecontext-agent-harness-daemon', '-p', 'onecontext-wiki-daemon']);
  await logCommand({
    tool: 'cargo',
    args: ['build', '-p', 'onecontext-agent-harness-daemon', '-p', 'onecontext-wiki-daemon'],
    code: build.code,
    ms: build.ms,
    stderr: build.stderr,
  });
  if (build.code !== 0) fail('cargo_build_failed', `${build.stdout}\n${build.stderr}`);
}

async function prepareRuntime() {
  if (!keepRuntime) await rm(runtimeHome, { recursive: true, force: true });
  await mkdir(evidenceDir, { recursive: true });
  await mkdir(runtimeHome, { recursive: true });
  if (!existsSync(runtimeRoot) && existsSync(join(repoRoot, 'runtime/1Context'))) {
    await cp(join(repoRoot, 'runtime/1Context'), runtimeRoot, { recursive: true });
  }
  await mkdir(runtimeRoot, { recursive: true });
}

function onecontextMcpServerConfig() {
  return {
    server_name: 'onecontext',
    uri: 'mcp://onecontext',
    transport: 'stdio',
    command: 'node',
    args: [wikiMcpServerScript, '--root', runtimeRoot, '--wiki-bin', wikiBin],
    env: {
      ONECONTEXT_ROOT: runtimeRoot,
      ONECONTEXT_WIKI_CORE_BIN: wikiBin,
    },
    visible_toolsets: ['toolset-mail', 'toolset-wiki'],
    hidden_host_tools: hiddenHostTools,
  };
}

function capabilityBinding(id, transport, toolNames, proofRequired = []) {
  const toolsetPath = id === 'toolset-mail' ? 'mail' : 'wiki';
  return {
    id,
    transport,
    tool_names: toolNames,
    config: {
      external_source: `mcp://onecontext/toolsets/${toolsetPath}`,
      external_owner: id === 'toolset-mail' ? '1Context wiki mail' : '1Context wiki',
      harness_owns_content: false,
      mcp_server: onecontextMcpServerConfig(),
      mcp_server_name: 'onecontext',
      mcp_tool_prefix: 'mcp__onecontext__',
    },
    policy: {
      declared_by: 'agent-mail-triad-demo',
      visible_to_agent: true,
      approval_mode: 'auto',
    },
    proof_required: proofRequired,
  };
}

function birthRequest(agent) {
  return {
    unit_id: agent.unit_id,
    role: agent.role,
    model: 'gpt-5-codex',
    identity: {
      display_name: agent.name,
      address: agent.role_address,
    },
    instructions: {
      system: agent.prompt,
    },
    runtime: {
      adapter: 'local_test',
      thread_id: agent.thread_id,
      session_id: agent.session_id,
      room_id: `room-${agent.key}-${shortStamp}`,
      mcp_servers: {
        onecontext: onecontextMcpServerConfig(),
      },
    },
    capabilities: [
      capabilityBinding(
        'toolset-mail',
        'mcp',
        mcpToolsets.mail,
        ['transport_identity', 'tool_conformance']
      ),
      capabilityBinding(
        'toolset-wiki',
        'mcp',
        mcpToolsets.wiki,
        ['transport_identity', 'tool_conformance']
      ),
    ],
    visibility: 'private',
    metadata: {
      title: `${agent.name} triad dogfood lane`,
      owner: 'agent-mail-triad-demo',
      tags: ['dogfood', 'mail', 'triad'],
    },
  };
}

function agentFixture(agent) {
  return {
    key: agent.key,
    name: agent.name,
    role: agent.role,
    accent: agent.accent,
    prompt: agent.prompt,
    role_address: agent.role_address,
    unit_id: agent.unit_id,
    agent_id: agent.agent_id,
    thread_id: agent.thread_id,
    session_id: agent.session_id,
    certificate_id: agent.certificate_id,
    mcp_server: onecontextMcpServerConfig(),
    turns_started: 0,
    turns_completed: 0,
    session: [
      {
        kind: 'prompt',
        task_id: 'all',
        title: 'standing prompt',
        body: agent.prompt,
        meta: ['birth prompt', agent.role_address],
      },
    ],
  };
}

function lane(agentKey) {
  return fixture.agents.find((agent) => agent.key === agentKey);
}

function pushEvent(agentKey, event) {
  const agent = lane(agentKey);
  agent.session.push({
    at: new Date().toISOString(),
    ...event,
  });
}

function bodyExcerpt(body) {
  return body.length > 220 ? `${body.slice(0, 217)}...` : body;
}

async function birthAgents() {
  for (const agent of agents) {
    const born = await harness(['birth'], birthRequest(agent));
    const unit = born.unit;
    agent.certificate_id = unit.certificate.certificate_id;
    const identified = await mcpTool('wiki.agent.identify', {
      thread_id: agent.thread_id,
      roles: [agent.role_address],
      capabilities: ['wiki.mail', 'wiki.talk'],
      ttl_seconds: 7200,
    });
    agent.agent_id = identified.agent.agent_id;
    fixture.agents.push(agentFixture(agent));
    await recordMcpBindingProof(agent, unit);
    pushEvent(agent.key, {
      kind: 'harness',
      task_id: 'all',
      title: 'born into harness',
      body: `Birth certificate ${agent.certificate_id}; wiki directory identity ${agent.agent_id}; MCP server onecontext mounted for toolset-mail and toolset-wiki.`,
      meta: ['harness.birth', 'wiki.agent.identify', 'mcp://onecontext'],
    });
  }
}

async function recordMcpBindingProof(agent, unit) {
  const turnId = `turn-${agent.key}-mcp-binding-${shortStamp}`;
  const generatedIds = unit.certificate?.generated_ids || {};
  await harness(['record-adapter-event'], {
    unit_id: agent.unit_id,
    adapter: 'mcp',
    kind: 'transport_identity_observed',
    status: 'observed',
    correlation: {
      thread_id: agent.thread_id,
      session_id: agent.session_id,
      turn_id: turnId,
      expected_turn_id: turnId,
      transport_attempt_id: `mcp-bind-${agent.key}-${shortStamp}`,
    },
    evidence: {
      mcp_server_name: 'onecontext',
      mcp_server_uri: 'mcp://onecontext',
      visible_toolsets: ['toolset-mail', 'toolset-wiki'],
      generated_ids: generatedIds,
      redacted: true,
    },
    redaction: {
      raw_prompts_redacted: true,
      mail_bodies_redacted: true,
      tool_outputs_redacted: true,
      secrets_redacted: true,
    },
  });
  await harness(['record-adapter-event'], {
    unit_id: agent.unit_id,
    adapter: 'mcp',
    kind: 'tool_allowlist_checked',
    status: 'accepted',
    correlation: {
      thread_id: agent.thread_id,
      session_id: agent.session_id,
      turn_id: turnId,
      expected_turn_id: turnId,
      transport_attempt_id: `mcp-allowlist-${agent.key}-${shortStamp}`,
    },
    evidence: {
      mcp_server_name: 'onecontext',
      allowed_tools: discoveredMcpTools,
      declared_toolsets: mcpToolsets,
      hidden_host_tools: hiddenHostTools,
      redacted: true,
    },
    redaction: {
      raw_prompts_redacted: true,
      mail_bodies_redacted: true,
      tool_outputs_redacted: true,
      secrets_redacted: true,
    },
  });
}

async function recordAdapterEvent(agent, { kind, status = 'observed', toolName, deliveryId, messageId, turnId }) {
  return harness(['record-adapter-event'], {
    unit_id: agent.unit_id,
    adapter: 'mcp',
    kind,
    status,
    correlation: {
      thread_id: agent.thread_id,
      session_id: agent.session_id,
      turn_id: turnId,
      expected_turn_id: turnId,
      delivery_id: deliveryId,
      message_id: messageId,
      tool_call_id: `${toolName}-${deliveryId || messageId || Date.now()}`,
    },
    evidence: {
      tool_name: toolName,
      redacted: true,
      body_sha256_present_elsewhere: true,
    },
    redaction: {
      raw_prompts_redacted: true,
      mail_bodies_redacted: true,
      tool_outputs_redacted: true,
      secrets_redacted: true,
    },
  });
}

async function sendMail({ from, to, task, subject, body, replyTo = null, kind = 'proposal' }) {
  const operationId = `triad-${shortStamp}-${task.id}-${fixture.mail.length + 1}`;
  const toolArgs = {
    page: collaborationPage,
    kind,
    subject,
    from,
    body,
    to: [to],
    operation_id: operationId,
    delivery_mode: 'mail',
  };
  if (replyTo) toolArgs.reply_to = replyTo;
  const result = await mcpTool('wiki.talk.append', toolArgs);
  assertInvariant(result.mail_delivery?.status === 'delivered', 'mail_delivered', JSON.stringify(result.mail_delivery));
  const attempt = result.mail_delivery.attempts[0];
  const mailRecord = {
    task_id: task.id,
    message_id: result.message_id,
    delivery_id: attempt.delivery_id,
    notification_id: attempt.notification_id || null,
    from,
    to,
    subject,
    wiki_page: result.page_id,
    wiki_route: result.route,
    talk_thread_id: result.thread_id,
    talk_source: result.source,
    body_excerpt: bodyExcerpt(body),
    body_sha256: sha256(body),
    state: 'delivered',
    operation_id: operationId,
    reply_to: replyTo,
    injection_id: null,
    control_event_id: null,
    injection_item_count: 0,
    app_server_execution: 'pending',
    claim_state: 'pending',
    claimed_by: null,
    mark_state: 'pending',
    notification_ack_state: 'pending',
  };
  Object.defineProperty(mailRecord, 'expected_body', { value: body, enumerable: false });
  fixture.mail.push(mailRecord);
  return mailRecord;
}

function injectionText(opened) {
  return (opened.content_delivery?.items || [])
    .flatMap((item) => item.content || [])
    .map((content) => content.text || '')
    .join('\n');
}

function assertOpenInjection(opened, { agent, mailRecord }) {
  assertInvariant(opened.delivery?.delivery_id === mailRecord.delivery_id, 'mail_open_delivery_matches', JSON.stringify(opened));
  assertInvariant(opened.message?.envelope?.message_id === mailRecord.message_id, 'mail_open_message_matches', JSON.stringify(opened));
  assertInvariant(
    opened.content_delivery?.transport === 'codex.thread.inject_items',
    'mail_open_transport_is_codex_inject_items',
    JSON.stringify(opened.content_delivery)
  );
  assertInvariant(
    opened.content_delivery?.method === 'thread/inject_items',
    'mail_open_uses_injection',
    JSON.stringify(opened.content_delivery)
  );
  assertInvariant(
    opened.content_delivery?.thread_id === agent.thread_id,
    'mail_open_thread_matches',
    JSON.stringify(opened.content_delivery)
  );
  const itemCount = opened.content_delivery.items?.length || 0;
  assertInvariant(itemCount > 0, 'mail_open_injection_item_count_positive', JSON.stringify(opened.content_delivery));
  const text = injectionText(opened);
  assertInvariant(text.includes(mailRecord.delivery_id), 'mail_open_item_has_delivery_id', text);
  assertInvariant(text.includes(mailRecord.message_id), 'mail_open_item_has_message_id', text);
  const escapedExpectedBody = JSON.stringify(mailRecord.expected_body).slice(1, -1);
  assertInvariant(
    text.includes(mailRecord.expected_body) || text.includes(escapedExpectedBody),
    'mail_open_item_has_expected_body',
    text
  );
  return { itemCount, text };
}

async function recordOpenInjection(opened, { agent, mailRecord }) {
  const { itemCount } = assertOpenInjection(opened, { agent, mailRecord });
  const recorded = await wikiHost(
    [
      'mail-record-injection',
      mailRecord.delivery_id,
      '--agent-id',
      agent.agent_id,
      '--thread-id',
      agent.thread_id,
      '--result',
      'ok',
      '--item-count',
      String(itemCount),
    ],
    'host-only simulated app-server injection receipt'
  );
  assertInvariant(recorded.operation === 'wiki.mail.record_injection', 'record_injection_operation_matches', JSON.stringify(recorded));
  assertInvariant(recorded.receipt?.delivery_id === mailRecord.delivery_id, 'record_injection_delivery_matches', JSON.stringify(recorded));
  assertInvariant(recorded.receipt?.message_id === mailRecord.message_id, 'record_injection_message_matches', JSON.stringify(recorded));
  assertInvariant(recorded.receipt?.agent_id === agent.agent_id, 'record_injection_agent_matches', JSON.stringify(recorded));
  assertInvariant(recorded.receipt?.thread_id === agent.thread_id, 'record_injection_thread_matches', JSON.stringify(recorded));
  assertInvariant(recorded.receipt?.app_server_method === 'thread/inject_items', 'record_injection_method_matches', JSON.stringify(recorded));
  assertInvariant(recorded.receipt?.app_server_result === 'ok', 'record_injection_result_ok', JSON.stringify(recorded));
  assertInvariant(recorded.receipt?.item_count === itemCount, 'record_injection_item_count_matches', JSON.stringify(recorded));
  assertInvariant(
    recorded.control_event?.mail_refs?.delivery_id === mailRecord.delivery_id,
    'record_injection_control_event_delivery_matches',
    JSON.stringify(recorded.control_event)
  );
  mailRecord.injection_id = recorded.receipt.injection_id;
  mailRecord.control_event_id = recorded.control_event.control_event_id;
  mailRecord.injection_item_count = itemCount;
  mailRecord.content_delivery_method = opened.content_delivery.method;
  mailRecord.app_server_execution = 'simulated_record_only';
  return recorded;
}

async function openWork(agent, mailRecord, task, title) {
  const turnId = `turn-${agent.key}-${task.id}-${mailRecord.delivery_id.slice(-6)}`;
  await harness(['start-turn'], {
    unit_id: agent.unit_id,
    turn_id: turnId,
    reason: `triad mail dogfood: ${title}`,
    expected_transport: 'mcp',
    context: {
      task_id: task.id,
      delivery_id: mailRecord.delivery_id,
      message_id: mailRecord.message_id,
    },
  });
  lane(agent.key).turns_started += 1;

  const poll = await mcpTool('wiki.notify.poll', { agent_id: agent.agent_id });
  const notification = poll.notifications.find((item) => item.delivery_id === mailRecord.delivery_id);
  assertInvariant(Boolean(notification), 'notification_polled', JSON.stringify({ agent: agent.agent_id, poll }));
  assertInvariant(!JSON.stringify(notification).includes(mailRecord.body_excerpt), 'notification_bodyless', JSON.stringify(notification));
  mailRecord.notification_id = notification.notification_id;

  const inbox = await mcpTool('wiki.agent.inbox', { agent_id: agent.agent_id });
  assertInvariant(
    inbox.deliveries.some((delivery) => delivery.delivery_id === mailRecord.delivery_id),
    'inbox_contains_delivery',
    JSON.stringify(inbox)
  );

  const opened = await mcpTool('wiki.mail.open', {
    delivery_id: mailRecord.delivery_id,
    agent_id: agent.agent_id,
  });
  const injectionRecord = await recordOpenInjection(opened, { agent, mailRecord });
  const claimed = await mcpTool('wiki.mail.claim', {
    delivery_id: mailRecord.delivery_id,
    agent_id: agent.agent_id,
  });
  assertInvariant(claimed.delivery?.state === 'claimed', 'mail_claim_state_claimed', JSON.stringify(claimed));
  assertInvariant(claimed.delivery?.claimed_by === agent.agent_id, 'mail_claim_agent_matches', JSON.stringify(claimed));
  mailRecord.claim_state = claimed.delivery.state;
  mailRecord.claimed_by = claimed.delivery.claimed_by;
  await recordAdapterEvent(agent, {
    kind: 'tool_call_observed',
    status: 'observed',
    toolName: 'wiki.mail.open',
    deliveryId: mailRecord.delivery_id,
    messageId: mailRecord.message_id,
    turnId,
  });

  pushEvent(agent.key, {
    kind: 'mail-in',
    task_id: task.id,
    title,
    body: `Opened ${mailRecord.subject}; body entered through thread/inject_items, then ${agent.name} claimed the delivery. Injection success is host-only simulated app-server recording for this demo run.`,
    notification_id: notification.notification_id,
    injection_id: injectionRecord.receipt.injection_id,
    control_event_id: injectionRecord.control_event.control_event_id,
    meta: [
      mailRecord.delivery_id,
      notification.notification_id,
      injectionRecord.receipt.injection_id,
      injectionRecord.control_event.control_event_id,
      'bodyless notification',
      'host-only wiki.mail.record_injection',
    ],
  });
  return { notification, injectionRecord, turnId };
}

async function closeWork(agent, mailRecord, task, context, title, body) {
  const marked = await mcpTool('wiki.mail.mark', {
    delivery_id: mailRecord.delivery_id,
    agent_id: agent.agent_id,
    state: 'done',
  });
  assertInvariant(marked.delivery?.state === 'done', 'mail_mark_done', JSON.stringify(marked));
  assertInvariant(marked.delivery?.claimed_by === agent.agent_id, 'mail_mark_agent_matches', JSON.stringify(marked));
  mailRecord.mark_state = marked.delivery.state;
  const acked = await mcpTool('wiki.notify.ack', {
    notification_id: context.notification.notification_id,
    agent_id: agent.agent_id,
  });
  assertInvariant(acked.notification?.state === 'acknowledged', 'notify_acknowledged', JSON.stringify(acked));
  mailRecord.notification_ack_state = acked.notification.state;
  await recordAdapterEvent(agent, {
    kind: 'tool_call_observed',
    status: 'accepted',
    toolName: 'wiki.mail.mark',
    deliveryId: mailRecord.delivery_id,
    messageId: mailRecord.message_id,
    turnId: context.turnId,
  });
  await harness(['complete-turn'], {
    unit_id: agent.unit_id,
    turn_id: context.turnId,
    outcome: 'completed',
    usage: {
      input_tokens: 180 + body.length,
      output_tokens: 120 + title.length,
      total_tokens: 300 + body.length + title.length,
    },
    duration_ms: 600 + body.length,
    metadata: {
      task_id: task.id,
      delivery_id: mailRecord.delivery_id,
      completion: title,
    },
  });
  lane(agent.key).turns_completed += 1;
  mailRecord.state = 'done';
  pushEvent(agent.key, {
    kind: 'mail-done',
    task_id: task.id,
    title,
    body,
    notification_id: context.notification.notification_id,
    injection_id: mailRecord.injection_id,
    control_event_id: mailRecord.control_event_id,
    meta: ['wiki.mail.mark(done)', 'wiki.notify.ack', context.notification.notification_id],
  });
}

function pipBody(task) {
  return [
    `Mission card for "${task.title}"`,
    `Odd constraint: ${task.odd_constraint}.`,
    `Acceptance check: the resulting wiki page has one summary, three links, and one open question.`,
    `Project link: [[projects#${task.id}]] should be the durable coordination anchor.`,
    `Please map the route around ${task.object}.`,
  ].join('\n');
}

function miraBody(task) {
  return [
    `Route map for "${task.title}"`,
    `First project link: [[projects#${task.id}-brief]] captures the mission card.`,
    `Second project link: [[projects#${task.id}-receipts]] stores the mail and injection receipts.`,
    `Dependency: Nox should close with a wiki-ready log and one unresolved follow-up.`,
  ].join('\n');
}

function noxBody(task) {
  return [
    `Archive closure for "${task.title}"`,
    `Ready project page shape: summary, route, receipts, follow-up.`,
    `Receipt note: all bodies were opened by mail.open and recorded as injection receipts.`,
    `Unresolved question: who curates ${task.object} after the first pass?`,
  ].join('\n');
}

async function runTask(task) {
  const pip = agents.find((agent) => agent.key === 'pip');
  const mira = agents.find((agent) => agent.key === 'mira');
  const nox = agents.find((agent) => agent.key === 'nox');

  const seed = await sendMail({
    from: 'system://dogfood.mission-control',
    to: pip.role_address,
    task,
    subject: `Frame task: ${task.title}`,
    body: task.seed,
    kind: 'question',
  });
  const pipContext = await openWork(pip, seed, task, 'Mission control mail');
  const pipReply = pipBody(task);
  const pipToMira = await sendMail({
    from: `agent://codex/${pip.agent_id}`,
    to: mira.role_address,
    task,
    subject: `Route request: ${task.title}`,
    body: pipReply,
    replyTo: seed.message_id,
  });
  await recordAdapterEvent(pip, {
    kind: 'tool_call_observed',
    status: 'accepted',
    toolName: 'wiki.talk.append',
    deliveryId: pipToMira.delivery_id,
    messageId: pipToMira.message_id,
    turnId: pipContext.turnId,
  });
  await closeWork(pip, seed, task, pipContext, 'Forwarded framed task', pipReply);

  const miraContext = await openWork(mira, pipToMira, task, 'Promptsmith handoff');
  const miraReply = miraBody(task);
  const miraToNox = await sendMail({
    from: `agent://codex/${mira.agent_id}`,
    to: nox.role_address,
    task,
    subject: `Archive request: ${task.title}`,
    body: miraReply,
    replyTo: pipToMira.message_id,
  });
  await recordAdapterEvent(mira, {
    kind: 'tool_call_observed',
    status: 'accepted',
    toolName: 'wiki.talk.append',
    deliveryId: miraToNox.delivery_id,
    messageId: miraToNox.message_id,
    turnId: miraContext.turnId,
  });
  await closeWork(mira, pipToMira, task, miraContext, 'Sent route map', miraReply);

  const noxContext = await openWork(nox, miraToNox, task, 'Mapmaker handoff');
  const noxReply = noxBody(task);
  const noxToPip = await sendMail({
    from: `agent://codex/${nox.agent_id}`,
    to: pip.role_address,
    task,
    subject: `Closure receipt: ${task.title}`,
    body: noxReply,
    replyTo: miraToNox.message_id,
  });
  await recordAdapterEvent(nox, {
    kind: 'tool_call_observed',
    status: 'accepted',
    toolName: 'wiki.talk.append',
    deliveryId: noxToPip.delivery_id,
    messageId: noxToPip.message_id,
    turnId: noxContext.turnId,
  });
  await closeWork(nox, miraToNox, task, noxContext, 'Sent closure receipt', noxReply);

  const finalContext = await openWork(pip, noxToPip, task, 'Archivist closure receipt');
  await closeWork(
    pip,
    noxToPip,
    task,
    finalContext,
    'Closed task loop',
    `Task loop closed for "${task.title}". Pip has the route, receipt note, and follow-up question.`
  );
}

async function buildProtocolCoverage(ledgerPaths) {
  const [
    deliveryRows,
    claimRows,
    injectionRows,
    notificationRows,
    controlRows,
    mcpEntries,
  ] = await Promise.all([
    readJsonLinesIfExists(ledgerPaths.mailDeliveriesPath),
    readJsonLinesIfExists(ledgerPaths.mailClaimsPath),
    readJsonLinesIfExists(ledgerPaths.injectionReceiptsPath),
    readJsonLinesIfExists(ledgerPaths.notificationsOutboxPath),
    readJsonLinesIfExists(ledgerPaths.controlEventsPath),
    readJsonLinesIfExists(mcpProtocolLog),
  ]);
  const claimedRows = claimRows.filter((row) => row.state === 'claimed');
  const doneRows = claimRows.filter((row) => row.state === 'done');
  const acknowledgedNotifications = notificationRows.filter((row) => row.state === 'acknowledged');
  const deliveryRowsByDelivery = rowsBy(deliveryRows, 'delivery_id');
  const claimRowsByDelivery = rowsBy(claimRows, 'delivery_id');
  const injectionRowsByDelivery = rowsBy(injectionRows, 'delivery_id');
  const notificationRowsByDelivery = rowsBy(notificationRows, 'delivery_id');
  const projectMailRows = fixture.mail.filter(
    (mailRecord) =>
      mailRecord.wiki_page === collaborationPage &&
      mailRecord.wiki_route === wikiProbe.route &&
      mailRecord.talk_source?.includes(`/${collaborationPage}/talk/`)
  );
  const deliveryStateJoinedRows = fixture.mail.filter((mailRecord) => {
    const rows = deliveryRowsByDelivery.get(mailRecord.delivery_id) || [];
    const stateRows = new Map(rows.map((row) => [row.state, row]));
    const unread = stateRows.get('unread');
    const claimed = stateRows.get('claimed');
    const done = stateRows.get('done');
    return (
      unread?.message_id === mailRecord.message_id &&
      unread?.recipient === mailRecord.to &&
      claimed?.message_id === mailRecord.message_id &&
      claimed?.recipient === mailRecord.to &&
      claimed?.claimed_by === mailRecord.claimed_by &&
      done?.message_id === mailRecord.message_id &&
      done?.recipient === mailRecord.to &&
      done?.claimed_by === mailRecord.claimed_by
    );
  });
  const claimLedgerJoinedRows = fixture.mail.filter((mailRecord) => {
    const rows = claimRowsByDelivery.get(mailRecord.delivery_id) || [];
    return (
      rows.some(
        (row) =>
          row.state === 'claimed' &&
          row.message_id === mailRecord.message_id &&
          row.recipient === mailRecord.to &&
          row.agent_id === mailRecord.claimed_by
      ) &&
      rows.some(
        (row) =>
          row.state === 'done' &&
          row.message_id === mailRecord.message_id &&
          row.recipient === mailRecord.to &&
          row.agent_id === mailRecord.claimed_by
      )
    );
  });
  const injectionReceiptJoinedRows = fixture.mail.filter((mailRecord) => {
    const agent = recipientAgent(mailRecord);
    const rows = injectionRowsByDelivery.get(mailRecord.delivery_id) || [];
    return rows.some(
      (row) =>
        row.injection_id === mailRecord.injection_id &&
        row.message_id === mailRecord.message_id &&
        row.agent_id === mailRecord.claimed_by &&
        row.thread_id === agent?.thread_id &&
        row.app_server_method === mailRecord.content_delivery_method &&
        row.app_server_result === 'ok' &&
        row.item_count === mailRecord.injection_item_count &&
        row.body_sha256 === bodyHashValue(mailRecord.body_sha256)
    );
  });
  const notificationLifecycleJoinedRows = fixture.mail.filter((mailRecord) => {
    const agent = recipientAgent(mailRecord);
    const rows = notificationRowsByDelivery.get(mailRecord.delivery_id) || [];
    return (
      rows.some(
        (row) =>
          row.notification_id === mailRecord.notification_id &&
          row.message_id === mailRecord.message_id &&
          row.agent_id === mailRecord.claimed_by &&
          row.transport_thread_id === agent?.thread_id &&
          row.state === 'pending'
      ) &&
      rows.some(
        (row) =>
          row.notification_id === mailRecord.notification_id &&
          row.message_id === mailRecord.message_id &&
          row.agent_id === mailRecord.claimed_by &&
          row.transport_thread_id === agent?.thread_id &&
          row.state === 'acknowledged'
      )
    );
  });
  const orderedMcpLifecycleRows = fixture.mail.filter((mailRecord) =>
    mcpLifecycleIsOrdered(mcpLifecycleIndexes(mcpEntries, mailRecord))
  );
  const nonSeedReplyRows = fixture.mail.filter((mailRecord) => mailRecord.reply_to);
  const replyChainRows = nonSeedReplyRows.filter(hasReplyToWithinTask);
  const projectAnchorRows = fixture.mail.filter((mailRecord) =>
    /\[\[projects#[^\]]+\]\]/.test(mailRecord.expected_body || '')
  );
  const expectedProjectAnchorCount = tasks.length * 2;
  const lifecycleReadyAgents = fixture.agents.filter(
    (agent) =>
      agent.lifecycle?.state === 'ready' &&
      agent.lifecycle?.turns_started === agent.turns_started &&
      agent.lifecycle?.turns_completed === agent.turns_completed &&
      agent.proof_status?.gate_status === 'satisfied'
  );
  const surfaces = [
    coverageSurface('mcp_tool_discovery', operationCounts['tools/list'] || 0, 1),
    coverageSurface('wiki_list_inventory', operationCounts['wiki.list'] || 0, 1),
    coverageSurface('wiki_page_status_projects', operationCounts['wiki.page.status'] || 0, 1, {
      page_id: collaborationPage,
      route: wikiProbe.route,
    }),
    coverageSurface('talk_append_mail_delivery', operationCounts['wiki.talk.append'] || 0, expectedMailCount, {
      page_id: collaborationPage,
    }),
    coverageSurface('mail_delivery_records', fixture.mail.length, expectedMailCount),
    coverageSurface('notify_poll', operationCounts['wiki.notify.poll'] || 0, expectedMailCount),
    coverageSurface('agent_inbox', operationCounts['wiki.agent.inbox'] || 0, expectedMailCount),
    coverageSurface('mail_open', operationCounts['wiki.mail.open'] || 0, expectedMailCount),
    coverageSurface('host_injection_receipts', hostOperationCounts['wiki.mail.record_injection'] || 0, expectedMailCount, {
      app_server_execution: fixture.runtime.app_server_execution,
    }),
    coverageSurface('mail_claim', operationCounts['wiki.mail.claim'] || 0, expectedMailCount),
    coverageSurface('mail_mark_done', operationCounts['wiki.mail.mark'] || 0, expectedMailCount),
    coverageSurface('notify_ack', operationCounts['wiki.notify.ack'] || 0, expectedMailCount),
    coverageSurface('project_talk_links', projectMailRows.length, expectedMailCount, {
      page_id: collaborationPage,
      route: wikiProbe.route,
      talk_handle: wikiProbe.talk_handle,
      published_handle: wikiProbe.published_handle,
    }),
    coverageSurface('ledger_claimed_events', claimedRows.length, expectedMailCount),
    coverageSurface('ledger_done_events', doneRows.length, expectedMailCount),
    coverageSurface('ledger_injection_receipts', injectionRows.length, expectedMailCount),
    coverageSurface('ledger_control_events', controlRows.length, expectedMailCount),
    coverageSurface('ledger_acknowledged_notifications', acknowledgedNotifications.length, expectedMailCount),
    coverageSurface('mail_delivery_state_chain_join', deliveryStateJoinedRows.length, expectedMailCount, {
      joins: ['delivery_id', 'message_id', 'recipient', 'claimed_by'],
      required_states: ['unread', 'claimed', 'done'],
    }),
    coverageSurface('mail_claim_ledger_agent_join', claimLedgerJoinedRows.length, expectedMailCount, {
      joins: ['delivery_id', 'message_id', 'recipient', 'agent_id'],
      required_states: ['claimed', 'done'],
    }),
    coverageSurface('injection_receipt_identity_join', injectionReceiptJoinedRows.length, expectedMailCount, {
      joins: ['delivery_id', 'message_id', 'agent_id', 'thread_id', 'body_sha256', 'item_count'],
      app_server_execution: fixture.runtime.app_server_execution,
    }),
    coverageSurface('notification_lifecycle_identity_join', notificationLifecycleJoinedRows.length, expectedMailCount, {
      joins: ['notification_id', 'delivery_id', 'message_id', 'agent_id', 'transport_thread_id'],
      required_states: ['pending', 'acknowledged'],
    }),
    coverageSurface('mcp_call_order_per_delivery', orderedMcpLifecycleRows.length, expectedMailCount, {
      order: ['talk.append', 'notify.poll', 'agent.inbox', 'mail.open', 'mail.claim', 'mail.mark(done)', 'notify.ack'],
    }),
    coverageSurface('reply_chain_edges_within_task', replyChainRows.length, nonSeedReplyRows.length, {
      expected_non_seed_replies: nonSeedReplyRows.length,
    }),
    coverageSurface('body_project_anchor_mentions', projectAnchorRows.length, expectedProjectAnchorCount, {
      note: 'Only Pip and Mira generated bodies are expected to mention wiki project anchors; seed and closure-only messages are not counted.',
    }),
    coverageSurface('agent_lifecycle_ready_status', lifecycleReadyAgents.length, agents.length, {
      joins: ['unit_id', 'turns_started', 'turns_completed', 'proof_status.gate_status'],
    }),
  ];
  const gaps = surfaces.filter((surface) => surface.status !== 'passed');
  fixture.protocol_coverage = {
    status: gaps.length === 0 ? 'passed' : 'gaps',
    expected_mail_count: expectedMailCount,
    operation_counts: operationCounts,
    host_operation_counts: hostOperationCounts,
    project_page: {
      page_id: collaborationPage,
      list_seen: wikiProbe.list_seen,
      status_seen: wikiProbe.status_seen,
      route: wikiProbe.route,
      talk_handle: wikiProbe.talk_handle,
      published_handle: wikiProbe.published_handle,
    },
    ledgers: {
      mail_deliveries: {
        rows: deliveryRows.length,
        states: ledgerStateCounts(deliveryRows),
      },
      mail_claims: {
        rows: claimRows.length,
        states: ledgerStateCounts(claimRows),
      },
      injection_receipts: {
        rows: injectionRows.length,
        ok: injectionRows.filter((row) => row.app_server_result === 'ok').length,
      },
      notifications_outbox: {
        rows: notificationRows.length,
        states: ledgerStateCounts(notificationRows),
      },
      mail_control_events: {
        rows: controlRows.length,
        record_only: controlRows.filter((row) => row.decision?.behavior === 'record_only').length,
      },
    },
    surfaces,
    gaps,
  };
  assertInvariant(gaps.length === 0, 'protocol_coverage_complete', JSON.stringify(gaps));
}

async function collectFinalStatus() {
  for (const agent of agents) {
    const status = await harness(['agent-status'], { unit_id: agent.unit_id });
    const target = lane(agent.key);
    target.lifecycle = status.lifecycle || status.unit?.lifecycle_state || 'ready';
    target.proof_status = status.proof_status || status.unit?.proof_status || null;
  }

  const controlEventsPath = join(runtimeRoot, 'context-engine/mail/control-events.jsonl');
  const injectionReceiptsPath = join(runtimeRoot, 'context-engine/mail/injection-receipts.jsonl');
  const mailDeliveriesPath = join(runtimeRoot, 'context-engine/mail/deliveries.jsonl');
  const mailClaimsPath = join(runtimeRoot, 'context-engine/mail/claims.jsonl');
  const notificationsOutboxPath = join(runtimeRoot, 'context-engine/notifications/outbox.jsonl');
  const ledgerPaths = {
    controlEventsPath,
    injectionReceiptsPath,
    mailDeliveriesPath,
    mailClaimsPath,
    notificationsOutboxPath,
  };
  const copiedLedgers = {};
  for (const [key, source] of Object.entries({
    mail_control_events: controlEventsPath,
    injection_receipts: injectionReceiptsPath,
    mail_deliveries: mailDeliveriesPath,
    mail_claims: mailClaimsPath,
    notifications_outbox: notificationsOutboxPath,
  })) {
    if (!existsSync(source)) continue;
    const destination = join(ledgerEvidenceDir, `${key}.jsonl`);
    await mkdir(resolve(destination, '..'), { recursive: true });
    await cp(source, destination);
    copiedLedgers[key] = destination;
  }
  fixture.runtime.mail_control_events = controlEventsPath;
  fixture.runtime.injection_receipts = injectionReceiptsPath;
  fixture.runtime.copied_ledgers = copiedLedgers;
  await buildProtocolCoverage(ledgerPaths);
  fixture.runtime.command_count = (await readFile(commandLog, 'utf8')).split('\n').filter(Boolean).length;
  fixture.assertions.push({
    code: 'three_agents_collaborated_by_mail',
    status: 'passed',
    detail: `${fixture.mail.length} delivered mail records across ${tasks.length} tasks`,
  });
}

async function main() {
  await mkdir(evidenceDir, { recursive: true });
  await maybeBuild();
  await prepareRuntime();
  await harness(['ensure']);
  await wikiHost(['ensure'], 'host-only runtime setup');
  await wikiHost(['page-create-all'], 'host-only fixture wiki setup');
  await startMcp();
  try {
    await exerciseWikiProjectSurface();
    await birthAgents();
    for (const task of tasks) await runTask(task);
    await collectFinalStatus();
  } finally {
    if (mcp) mcp.close();
  }
  await mkdir(resolve(fixturePath, '..'), { recursive: true });
  const fixtureJson = JSON.stringify(fixture, null, 2) + '\n';
  await writeFile(fixturePath, fixtureJson);
  await writeFile(join(evidenceDir, 'fixture.json'), fixtureJson);
  await writeFile(join(evidenceDir, 'fixture-path.txt'), `${fixturePath}\n`);
  console.log(
    JSON.stringify(
      {
        status: 'ok',
        fixture: fixturePath,
        evidence_dir: evidenceDir,
        runtime_root: runtimeRoot,
        agents: fixture.agents.map((agent) => ({ key: agent.key, agent_id: agent.agent_id })),
        mail_count: fixture.mail.length,
      },
      null,
      2
    )
  );
}

main().catch(async (error) => {
  try {
    await mkdir(evidenceDir, { recursive: true });
    await writeFile(
      join(evidenceDir, 'failure.json'),
      JSON.stringify({ status: 'error', code: error.code || 'error', message: error.message }, null, 2) + '\n'
    );
  } catch {
    // Best effort failure evidence only.
  }
  console.error(error.stack || error.message);
  process.exit(1);
});
