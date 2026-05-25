#!/usr/bin/env node
import { existsSync } from 'node:fs';
import { readdir, readFile } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const args = process.argv.slice(2);
const expectedMcpTools = [
  'wiki.agent.identify',
  'wiki.agent.inbox',
  'wiki.mail.open',
  'wiki.mail.claim',
  'wiki.mail.mark',
  'wiki.notify.poll',
  'wiki.notify.ack',
  'wiki.list',
  'wiki.page.status',
  'wiki.talk.append',
];
const visibleWorkTools = [
  'wiki.list',
  'wiki.page.status',
  'wiki.agent.identify',
  'wiki.agent.inbox',
  'wiki.mail.open',
  'wiki.mail.claim',
  'wiki.mail.mark',
  'wiki.notify.poll',
  'wiki.notify.ack',
  'wiki.talk.append',
];
const allowedHostOnlyWikiOps = new Set([
  'wiki.ensure',
  'wiki.page.create_all',
  'wiki.mail.record_injection',
]);

if (args.includes('--help') || args.includes('-h')) {
  console.log(`Usage: node scripts/verify-agent-mail-triad-mcp-realism.mjs [evidence-dir]

Verifies an Agent Mail Triad evidence directory. With no directory, verifies the
latest test-results/agent-mail-triad-demo-* directory containing both
mcp-protocol.jsonl and commands.jsonl.`);
  process.exit(0);
}

const evidenceDir = args[0] ? resolve(args[0]) : await latestEvidenceDir();
const mcpLogPath = join(evidenceDir, 'mcp-protocol.jsonl');
const commandLogPath = join(evidenceDir, 'commands.jsonl');
const failures = [];

const mcpEntries = await readJsonl(mcpLogPath);
const commandEntries = await readJsonl(commandLogPath);
const fixture = await readFixtureSnapshot(evidenceDir);
const runFailure = await readRunFailure(evidenceDir);
const commandTrail = await readCommandTrail(commandEntries);

if (runFailure) {
  failures.push(`evidence directory contains failure.json: ${runFailure.code || 'unknown'} ${runFailure.message || ''}`.trim());
}

for (const [index, entry] of mcpEntries.entries()) {
  assertJsonRpcExchange(entry, index);
}

const toolsList = mcpEntries.find((entry) => entry.method === 'tools/list');
const discoveredTools = toolsList?.response?.result?.tools?.map((tool) => tool.name) || [];
assertSameSet(discoveredTools, expectedMcpTools, 'tools/list does not match the declared visible MCP surface');

const toolCalls = mcpEntries.filter((entry) => entry.method === 'tools/call');
for (const toolName of visibleWorkTools) {
  assert(
    toolCalls.some((entry) => entry.operation === toolName),
    `missing visible MCP work call: ${toolName}`
  );
}

for (const entry of toolCalls) {
  const result = entry.response?.result;
  assert(result && result.isError !== true, `MCP tool returned an error: ${entry.operation}`);
  assert(entry.request?.params?.name === entry.operation, `MCP operation/name mismatch: ${entry.operation}`);
}

const identities = new Map();
const byAgent = new Map();
for (const entry of toolCalls) {
  const argsObject = entry.request?.params?.arguments || {};
  if (entry.operation === 'wiki.agent.identify') {
    const agent = entry.response?.result?.structuredContent?.agent;
    if (agent?.agent_id) identities.set(agent.agent_id, agent);
  }
  if (argsObject.agent_id) {
    const counts = byAgent.get(argsObject.agent_id) || {};
    counts[entry.operation] = (counts[entry.operation] || 0) + 1;
    byAgent.set(argsObject.agent_id, counts);
  }
}

assert(identities.size === 3, `expected 3 MCP-identitied triad agents, found ${identities.size}`);
for (const [agentId] of identities) {
  const counts = byAgent.get(agentId) || {};
  for (const toolName of [
    'wiki.notify.poll',
    'wiki.agent.inbox',
    'wiki.mail.open',
    'wiki.mail.claim',
    'wiki.mail.mark',
    'wiki.notify.ack',
  ]) {
    assert(counts[toolName] > 0, `agent ${agentId} never used ${toolName} through MCP`);
  }
}

const delivered = new Map();
const opened = new Map();
const claimed = new Map();
const markedDone = new Map();
const acknowledged = new Map();

for (const [index, entry] of toolCalls.entries()) {
  const argsObject = entry.request?.params?.arguments || {};
  const content = entry.response?.result?.structuredContent || {};
  if (entry.operation === 'wiki.talk.append') {
    assert(argsObject.body === '[redacted]', `talk append body was not redacted in MCP protocol log at tool call ${index}`);
    assert(argsObject.delivery_mode === 'mail', `talk append did not request mail delivery at tool call ${index}`);
    assert(content.delivery_mode === 'mail', `talk append response was not mail delivery at tool call ${index}`);
    assert(content.mail_delivery?.status === 'delivered', `talk append did not deliver mail at tool call ${index}`);
    for (const attempt of content.mail_delivery?.attempts || []) {
      if (attempt.delivery_id) {
        delivered.set(attempt.delivery_id, {
          index,
          recipient: attempt.recipient,
          message_id: content.message_id,
          thread_id: content.thread_id,
        });
      }
    }
  } else if (entry.operation === 'wiki.mail.open') {
    const deliveryId = argsObject.delivery_id;
    opened.set(deliveryId, {
      index,
      agent_id: argsObject.agent_id,
      item_count: content.content_delivery?.item_count || 0,
      message_id: content.message?.envelope?.message_id,
      thread_id: content.content_delivery?.thread_id,
    });
    const delivery = content.content_delivery || {};
    assert(
      delivery.status === 'requires_host_injection' &&
        delivery.method === 'thread/inject_items' &&
        delivery.item_count > 0,
      `mail.open ${deliveryId} did not return a host injection payload`
    );
    assert(
      delivery.items === '[redacted injection item text]',
      `mail.open ${deliveryId} leaked or omitted the redacted injection item marker`
    );
    assert(
      entry.response?.result?.content?.[0]?.text === `[redacted tool output: content_delivery.item_count=${delivery.item_count}]`,
      `mail.open ${deliveryId} content text was not redacted to item_count metadata`
    );
  } else if (entry.operation === 'wiki.mail.claim') {
    claimed.set(argsObject.delivery_id, { index, agent_id: argsObject.agent_id });
  } else if (entry.operation === 'wiki.mail.mark' && argsObject.state === 'done') {
    markedDone.set(argsObject.delivery_id, { index, agent_id: argsObject.agent_id });
  } else if (entry.operation === 'wiki.notify.ack') {
    const notification = content.notification || {};
    acknowledged.set(notification.delivery_id, { index, agent_id: argsObject.agent_id });
  }
}

const injections = new Map();
for (const [index, entry] of commandEntries.entries()) {
  if (entry.tool === 'onecontext-wiki-host-only') {
    assert(
      allowedHostOnlyWikiOps.has(entry.operation),
      `unexpected host-only wiki operation: ${entry.operation}`
    );
    if (entry.operation === 'wiki.mail.record_injection') {
      const commandIndex = entry.args?.indexOf('mail-record-injection') ?? -1;
      const deliveryId = commandIndex === -1 ? null : entry.args?.[commandIndex + 1];
      const itemCount = Number(entry.args?.[entry.args.indexOf('--item-count') + 1] || 0);
      if (deliveryId) {
        injections.set(deliveryId, {
          index,
          itemCount,
          agent_id: flagValue(entry.args, '--agent-id'),
          thread_id: flagValue(entry.args, '--thread-id'),
          result: flagValue(entry.args, '--result'),
          entry,
        });
      }
    }
    continue;
  }
  if (entry.operation?.startsWith('wiki.') && entry.tool !== 'mcp-jsonrpc') {
    failures.push(`wiki operation bypassed MCP stdio: ${entry.operation} via ${entry.tool || 'unknown'}`);
  }
}

assert(delivered.size > 0, 'no wiki.talk.append mail deliveries were observed');
for (const [deliveryId, delivery] of delivered) {
  const open = opened.get(deliveryId);
  const claim = claimed.get(deliveryId);
  const mark = markedDone.get(deliveryId);
  const ack = acknowledged.get(deliveryId);
  const injection = injections.get(deliveryId);
  const openCommand = commandTrail.find((event) => event.operation === 'wiki.mail.open' && event.delivery_id === deliveryId);
  const claimCommand = commandTrail.find((event) => event.operation === 'wiki.mail.claim' && event.delivery_id === deliveryId);
  const doneCommand = commandTrail.find((event) => event.operation === 'wiki.mail.mark' && event.delivery_id === deliveryId && event.state === 'done');
  assert(open, `delivered mail was never opened through MCP: ${deliveryId}`);
  assert(claim, `opened mail was never claimed through MCP: ${deliveryId}`);
  assert(mark, `claimed mail was never marked done through MCP: ${deliveryId}`);
  assert(ack, `completed mail was never acknowledged through MCP notification ack: ${deliveryId}`);
  assert(injection?.itemCount > 0, `mail.open did not get a host injection receipt: ${deliveryId}`);
  assert(openCommand, `commands.jsonl did not record the MCP mail.open request file for ${deliveryId}`);
  assert(claimCommand, `commands.jsonl did not record the MCP mail.claim request file for ${deliveryId}`);
  assert(doneCommand, `commands.jsonl did not record the MCP mail.mark done request file for ${deliveryId}`);
  if (open && claim) assert(open.index < claim.index, `claim happened before open for ${deliveryId}`);
  if (claim && mark) assert(claim.index < mark.index, `done mark happened before claim for ${deliveryId}`);
  if (mark && ack) assert(mark.index < ack.index, `notification ack happened before done mark for ${deliveryId}`);
  if (open) assert(open.index > delivery.index, `mail was opened before it was delivered: ${deliveryId}`);
  if (open && injection) {
    assert(injection.agent_id === open.agent_id, `host injection receipt agent did not match mail.open agent for ${deliveryId}`);
    assert(injection.thread_id === open.thread_id, `host injection receipt thread did not match mail.open injection thread for ${deliveryId}`);
    assert(injection.result === 'ok', `host injection receipt was not successful for ${deliveryId}`);
    assert(injection.itemCount === open.item_count, `host injection item count did not match mail.open item count for ${deliveryId}`);
  }
  if (openCommand && injection) {
    assert(openCommand.index < injection.index, `host injection was recorded before MCP mail.open in commands.jsonl for ${deliveryId}`);
  }
  if (claimCommand && injection) {
    assert(injection.index < claimCommand.index, `host injection was not recorded before MCP mail.claim in commands.jsonl for ${deliveryId}`);
  }
}

if (fixture?.protocol_coverage) {
  const coverage = fixture.protocol_coverage;
  const gaps = coverage.gaps || [];
  assert(coverage.status === 'passed', `fixture protocol coverage is not passed: ${coverage.status}`);
  assert(gaps.length === 0, `fixture protocol coverage reported gaps: ${JSON.stringify(gaps)}`);
  assert(coverage.expected_mail_count === delivered.size, 'fixture expected mail count does not match delivered mail count');
  assert(coverage.project_page?.page_id === 'projects', 'fixture did not exercise the projects wiki page');
  assert(coverage.project_page?.route === '/projects', 'fixture projects page route was not /projects');
  for (const surfaceId of [
    'wiki_list_inventory',
    'wiki_page_status_projects',
    'project_talk_links',
    'ledger_claimed_events',
    'ledger_done_events',
    'ledger_injection_receipts',
    'ledger_acknowledged_notifications',
  ]) {
    const surface = coverage.surfaces?.find((item) => item.id === surfaceId);
    assert(surface?.status === 'passed', `fixture coverage surface missing or not passed: ${surfaceId}`);
  }
}

const summary = {
  evidence_dir: evidenceDir,
  mcp_tool_calls: toolCalls.length,
  agents: identities.size,
  delivered_mail: delivered.size,
  opened_mail: opened.size,
  injection_receipts: injections.size,
  protocol_coverage: fixture?.protocol_coverage?.status || 'checked-by-logs',
};

if (failures.length > 0) {
  console.error(JSON.stringify({ status: 'failed', summary, failures }, null, 2));
  process.exit(1);
}

console.log(JSON.stringify({ status: 'ok', summary }, null, 2));

async function latestEvidenceDir() {
  const root = join(repoRoot, 'test-results');
  const entries = await readdir(root, { withFileTypes: true }).catch(() => []);
  const candidates = entries
    .filter((entry) => entry.isDirectory() && entry.name.startsWith('agent-mail-triad-demo-'))
    .map((entry) => join(root, entry.name))
    .filter((dir) => existsSync(join(dir, 'mcp-protocol.jsonl')) && existsSync(join(dir, 'commands.jsonl')))
    .sort((left, right) => basename(left).localeCompare(basename(right)));
  if (candidates.length === 0) {
    throw new Error('No agent-mail-triad-demo evidence directory with MCP and command logs was found.');
  }
  const completed = candidates.filter((dir) => {
    const hasFixture = existsSync(join(dir, 'fixture.json')) || existsSync(join(dir, 'fixture-path.txt'));
    return hasFixture && !existsSync(join(dir, 'failure.json'));
  });
  return (completed.length > 0 ? completed : candidates)[(completed.length > 0 ? completed : candidates).length - 1];
}

async function readJsonl(path) {
  const text = await readFile(path, 'utf8');
  return text
    .split(/\n+/)
    .filter(Boolean)
    .map((line, index) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        throw new Error(`${path}:${index + 1}: invalid JSONL: ${error.message}`);
      }
    });
}

async function readFixtureSnapshot(dir) {
  const snapshot = join(dir, 'fixture.json');
  if (existsSync(snapshot)) {
    return JSON.parse(await readFile(snapshot, 'utf8'));
  }

  const pointer = join(dir, 'fixture-path.txt');
  if (!existsSync(pointer)) return null;
  const fixturePath = (await readFile(pointer, 'utf8')).trim();
  if (!fixturePath || !existsSync(fixturePath)) return null;
  const candidate = JSON.parse(await readFile(fixturePath, 'utf8'));
  if (!basename(dir).includes(candidate.runtime?.run_id || '')) return null;
  return candidate;
}

async function readRunFailure(dir) {
  const path = join(dir, 'failure.json');
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(await readFile(path, 'utf8'));
  } catch (error) {
    return { code: 'invalid_failure_json', message: error.message };
  }
}

async function readCommandTrail(entries) {
  const trail = [];
  for (const [index, entry] of entries.entries()) {
    if (entry.tool !== 'mcp-jsonrpc') continue;
    if (entry.method !== 'tools/call') continue;
    const request = await readRequestFile(entry.request_file);
    const argsObject = request?.params?.arguments || {};
    assert(request?.jsonrpc === '2.0', `MCP command request file is not JSON-RPC 2.0: ${entry.request_file}`);
    assert(request?.method === 'tools/call', `MCP command request file method mismatch: ${entry.request_file}`);
    assert(request?.params?.name === entry.operation, `MCP command request file operation mismatch: ${entry.request_file}`);
    trail.push({
      index,
      operation: entry.operation,
      delivery_id: argsObject.delivery_id,
      notification_id: argsObject.notification_id,
      agent_id: argsObject.agent_id,
      state: argsObject.state,
    });
  }
  return trail;
}

async function readRequestFile(path) {
  if (!path || !existsSync(path)) return null;
  try {
    return JSON.parse(await readFile(path, 'utf8'));
  } catch (error) {
    failures.push(`${path}: invalid JSON request file: ${error.message}`);
    return null;
  }
}

function assertJsonRpcExchange(entry, index) {
  const label = `${entry.method || 'unknown'} at mcp-protocol.jsonl entry ${index + 1}`;
  assert(entry.request?.jsonrpc === '2.0', `request is not JSON-RPC 2.0 for ${label}`);
  assert(entry.request?.method === entry.method, `request method/log method mismatch for ${label}`);
  assert(entry.response?.jsonrpc === '2.0', `response is not JSON-RPC 2.0 for ${label}`);
  assert(!entry.response?.error, `response contains JSON-RPC error for ${label}: ${JSON.stringify(entry.response?.error)}`);
  if (Object.prototype.hasOwnProperty.call(entry.request || {}, 'id')) {
    assert(entry.response?.id === entry.request.id, `response id did not match request id for ${label}`);
  }
}

function flagValue(args, flag) {
  const index = args?.indexOf(flag) ?? -1;
  return index === -1 ? null : args[index + 1] || null;
}

function assert(condition, message) {
  if (!condition) failures.push(message);
}

function assertSameSet(actual, expected, message) {
  const actualSorted = [...actual].sort();
  const expectedSorted = [...expected].sort();
  assert(
    JSON.stringify(actualSorted) === JSON.stringify(expectedSorted),
    `${message}: expected ${JSON.stringify(expectedSorted)}, got ${JSON.stringify(actualSorted)}`
  );
}
