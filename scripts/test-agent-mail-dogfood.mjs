#!/usr/bin/env node
import { appendFile, cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const args = new Set(process.argv.slice(2));
const shouldBuild = args.has('--build');
const keepRuntime = args.has('--keep-runtime');
const timestamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
const shortStamp = timestamp.replace(/^.*T(\d{6})Z$/, '$1').toLowerCase();
const evidenceDir = resolve(
  process.env.ONECONTEXT_AGENT_MAIL_EVIDENCE_DIR ||
    join(repoRoot, 'test-results', `agent-mail-dogfood-${timestamp}`)
);
const devHome = resolve(
  process.env.ONECONTEXT_AGENT_MAIL_HOME ||
    join('/tmp', `1context-agent-mail-dogfood-${shortStamp}`)
);
const runtimeRoot = join(devHome, '1Context');
const wikiBin = resolve(
  process.env.ONECONTEXT_WIKI_CORE_BIN || join(repoRoot, 'target/debug/onecontext-wiki')
);
const commandLog = join(evidenceDir, 'commands.jsonl');
const failures = [];
const observations = [];

function usage() {
  console.log(`Usage: node scripts/test-agent-mail-dogfood.mjs [--build] [--keep-runtime]

Runs a disposable failure-seeking dogfood loop for the agent-mail protocol.

It creates a fake runtime under /tmp, sends labels-only talk, explicit mail,
Codex steering notifications, duplicate operations, bad recipients, stale
leases, and unauthorized delivery mutations.

Environment:
  ONECONTEXT_AGENT_MAIL_EVIDENCE_DIR  evidence output directory
  ONECONTEXT_AGENT_MAIL_HOME          fake home directory; user data is <home>/1Context
  ONECONTEXT_WIKI_CORE_BIN            Rust wiki CLI binary
`);
}

if (args.has('--help') || args.has('-h')) {
  usage();
  process.exit(0);
}

function sleep(ms) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms));
}

function assertInvariant(condition, code, detail) {
  if (condition) return;
  const failure = { code, detail };
  failures.push(failure);
  const error = new Error(`${code}: ${detail}`);
  error.failure = failure;
  throw error;
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

async function maybeBuild() {
  if (!shouldBuild) {
    if (!existsSync(wikiBin)) {
      throw new Error(`Missing ${wikiBin}. Run with --build or build onecontext-wiki first.`);
    }
    return;
  }
  const build = await run('cargo', ['build', '--package', 'onecontext-wiki-daemon']);
  await logCommand({ ...build, phase: 'build' });
  if (build.code !== 0) {
    throw new Error(`cargo build failed\n${build.stdout}\n${build.stderr}`);
  }
}

async function logCommand(entry) {
  await appendFile(commandLog, JSON.stringify(entry) + '\n');
}

function parseJson(stdout, label) {
  try {
    return JSON.parse(stdout);
  } catch (error) {
    throw new Error(`${label} did not emit JSON: ${error.message}\n${stdout}`);
  }
}

async function wiki(commandArgs, options = {}) {
  const entry = await run(wikiBin, ['--root', runtimeRoot, ...commandArgs]);
  const parsed = parseJson(entry.stdout, `onecontext-wiki ${commandArgs.join(' ')}`);
  const logged = {
    tool: 'onecontext-wiki',
    args: ['--root', runtimeRoot, ...commandArgs],
    code: entry.code,
    ms: entry.ms,
    status: parsed.status,
    operation: parsed.operation,
    error: parsed.error || null,
    json: parsed,
    stderr: entry.stderr,
  };
  await logCommand(logged);
  if (!options.allowFailure && entry.code !== 0) {
    throw new Error(
      `onecontext-wiki ${commandArgs.join(' ')} failed: ${JSON.stringify(parsed.error || parsed)}`
    );
  }
  return { code: entry.code, json: parsed };
}

async function expectWikiFailure(label, commandArgs, expectedFragment, code) {
  const result = await wiki(commandArgs, { allowFailure: true });
  assertInvariant(result.code !== 0, code, `${label} unexpectedly succeeded`);
  const message = result.json?.error?.message || JSON.stringify(result.json?.error || result.json);
  assertInvariant(
    !expectedFragment || message.includes(expectedFragment),
    `${code}_message`,
    `${label} error mismatch; wanted ${expectedFragment}, got ${message}`
  );
  observations.push({ label, expected_failure: true, message });
  return result;
}

async function writeJson(name, value) {
  await writeFile(join(evidenceDir, name), JSON.stringify(value, null, 2) + '\n');
}

async function writeText(name, value) {
  await writeFile(join(evidenceDir, name), value);
}

async function readJsonl(path) {
  if (!existsSync(path)) return [];
  const contents = await readFile(path, 'utf8');
  return contents
    .split('\n')
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line));
}

function latestBy(rows, key) {
  const latest = new Map();
  for (const row of rows) latest.set(row[key], row);
  return latest;
}

function assertNoBodyLeak(value, forbiddenBody, code) {
  if (!forbiddenBody) return;
  assertInvariant(!JSON.stringify(value).includes(forbiddenBody), code, JSON.stringify(value));
}

function assertNotificationEnvelope(notification, { agent, deliveryId, messageId, recipient, forbiddenBody, label }) {
  assertInvariant(notification.agent_id === agent.agent_id, `${label}_notification_agent`, JSON.stringify(notification));
  assertInvariant(notification.delivery_id === deliveryId, `${label}_notification_delivery`, JSON.stringify(notification));
  if (messageId) {
    assertInvariant(notification.message_id === messageId, `${label}_notification_message`, JSON.stringify(notification));
  }
  if (recipient) {
    assertInvariant(notification.recipient === recipient, `${label}_notification_recipient`, JSON.stringify(notification));
  }
  assertInvariant(!('body_markdown' in notification), `${label}_notification_body_field`, JSON.stringify(notification));
  assertNoBodyLeak(notification, forbiddenBody, `${label}_notification_body_leak`);
}

function requireInboxDelivery(inbox, deliveryId, expectedState, label) {
  const delivery = inbox.deliveries.find((row) => row.delivery_id === deliveryId);
  assertInvariant(delivery, `${label}_delivery_missing_from_inbox`, JSON.stringify(inbox));
  if (expectedState) {
    assertInvariant(delivery.state === expectedState, `${label}_delivery_state`, JSON.stringify(delivery));
  }
  return delivery;
}

function assertOpenInjection(opened, { agent, deliveryId, expectedBody, label }) {
  assertInvariant(opened.delivery.delivery_id === deliveryId, `${label}_open_wrong_delivery`, JSON.stringify(opened));
  assertInvariant(opened.message?.envelope?.message_id, `${label}_open_missing_message_id`, JSON.stringify(opened));
  assertInvariant(!('body_markdown' in opened.message), `${label}_open_body_top_level_leak`, JSON.stringify(opened.message));
  assertInvariant(
    opened.content_delivery?.transport === 'codex.thread.inject_items',
    `${label}_open_wrong_transport`,
    JSON.stringify(opened.content_delivery)
  );
  assertInvariant(
    opened.content_delivery?.method === 'thread/inject_items',
    `${label}_open_wrong_injection_method`,
    JSON.stringify(opened.content_delivery)
  );
  assertInvariant(
    opened.content_delivery?.thread_id === agent.transport.thread_id,
    `${label}_open_wrong_thread`,
    JSON.stringify(opened.content_delivery)
  );
  const item = opened.content_delivery.items?.[0];
  const text = item?.content?.[0]?.text || '';
  assertInvariant(item?.type === 'message', `${label}_open_item_type`, JSON.stringify(item));
  assertInvariant(item?.role === 'user', `${label}_open_item_role`, JSON.stringify(item));
  assertInvariant(text.includes('"kind": "1context.mail.opened"'), `${label}_open_item_kind`, text);
  assertInvariant(text.includes(deliveryId), `${label}_open_item_delivery_id`, text);
  assertInvariant(text.includes(opened.message.envelope.message_id), `${label}_open_item_message_id`, text);
  if (expectedBody) {
    assertInvariant(text.includes(expectedBody), `${label}_open_body_missing_from_injection`, text);
  }
  return text;
}

async function recordOpenInjection(opened, { agent, deliveryId, expectedBody, label }) {
  const itemCount = opened.content_delivery.items?.length || 0;
  const recorded = (await wiki([
    'mail-record-injection',
    deliveryId,
    '--agent-id',
    agent.agent_id,
    '--thread-id',
    opened.content_delivery.thread_id,
    '--result',
    'ok',
    '--item-count',
    String(itemCount),
  ])).json;
  assertInvariant(
    recorded.operation === 'wiki.mail.record_injection',
    `${label}_injection_wrong_operation`,
    JSON.stringify(recorded)
  );
  assertInvariant(
    recorded.receipt.delivery_id === deliveryId,
    `${label}_injection_wrong_delivery`,
    JSON.stringify(recorded.receipt)
  );
  assertInvariant(
    recorded.receipt.agent_id === agent.agent_id,
    `${label}_injection_wrong_agent`,
    JSON.stringify(recorded.receipt)
  );
  assertInvariant(
    recorded.receipt.thread_id === agent.transport.thread_id,
    `${label}_injection_wrong_thread`,
    JSON.stringify(recorded.receipt)
  );
  assertInvariant(
    recorded.receipt.app_server_method === 'thread/inject_items',
    `${label}_injection_wrong_method`,
    JSON.stringify(recorded.receipt)
  );
  assertInvariant(
    recorded.receipt.app_server_result === 'ok',
    `${label}_injection_wrong_result`,
    JSON.stringify(recorded.receipt)
  );
  assertInvariant(
    recorded.receipt.item_count === itemCount,
    `${label}_injection_wrong_item_count`,
    JSON.stringify(recorded.receipt)
  );
  assertInvariant(
    recorded.control_event.source === 'codex_app_server',
    `${label}_control_event_wrong_source`,
    JSON.stringify(recorded.control_event)
  );
  assertInvariant(
    recorded.control_event.mail_refs.delivery_id === deliveryId,
    `${label}_control_event_wrong_delivery`,
    JSON.stringify(recorded.control_event)
  );
  assertInvariant(
    recorded.control_event.decision.behavior === 'record_only',
    `${label}_control_event_wrong_decision`,
    JSON.stringify(recorded.control_event)
  );
  assertNoBodyLeak(recorded, expectedBody, `${label}_injection_receipt_body_leak`);
  return recorded;
}

async function assertAgentStatusByThread({
  agent,
  expectedLeaseState = 'active',
  expectedPendingDeliveryId = null,
  label,
}) {
  const status = (await wiki(['agent-status-by-thread', agent.transport.thread_id])).json;
  assertInvariant(status.agent_id === agent.agent_id, `${label}_status_thread_agent`, JSON.stringify(status));
  assertInvariant(status.lease_state === expectedLeaseState, `${label}_status_thread_lease`, JSON.stringify(status));
  if (expectedPendingDeliveryId) {
    assertInvariant(
      status.pending_notifications.some((notification) => notification.delivery_id === expectedPendingDeliveryId),
      `${label}_status_thread_missing_pending`,
      JSON.stringify(status)
    );
  }
  return status;
}

async function assertPublishBoundaryStill(initialPublish, label) {
  const current = (await wiki(['publish-status'])).json;
  assertInvariant(
    samePublishBoundary(initialPublish, current),
    `${label}_changed_publish_boundary`,
    `${label} changed publish-status boundary fields`
  );
  return current;
}

async function prepareRuntime() {
  await rm(devHome, { recursive: true, force: true });
  await mkdir(evidenceDir, { recursive: true });
  await mkdir(devHome, { recursive: true });
  await cp(join(repoRoot, 'runtime/1Context'), runtimeRoot, { recursive: true });
  await writeText('dev-home.txt', `${devHome}\n`);
  await writeText('runtime-root.txt', `${runtimeRoot}\n`);
}

async function copyIfExists(source, evidenceName) {
  if (!existsSync(source)) return false;
  await cp(source, join(evidenceDir, evidenceName), { recursive: true });
  return true;
}

async function collectEvidenceFiles() {
  const contextEngine = join(runtimeRoot, 'context-engine');
  await copyIfExists(join(contextEngine, 'mail'), 'mail-ledger');
  await copyIfExists(join(contextEngine, 'notifications'), 'notification-ledger');
  await copyIfExists(join(contextEngine, 'agents'), 'agent-directory');
  await copyIfExists(
    join(runtimeRoot, 'user-wiki/source/families/reference/topics/talk/topics.talk'),
    'topics-talk'
  );
}

async function assertLedgerEvidence({
  roleDeliveryId,
  curatorNotificationId,
  curatorBNotificationId,
  ackOnlyDeliveryId,
  ackOnlyNotificationId,
  failedDeliveryId,
  failedNotificationId,
  inactiveDeliveryId,
  inactiveNotificationId,
}) {
  const contextEngine = join(runtimeRoot, 'context-engine');
  const deliveries = await readJsonl(join(contextEngine, 'mail', 'deliveries.jsonl'));
  const injectionReceipts = await readJsonl(join(contextEngine, 'mail', 'injection-receipts.jsonl'));
  const controlEvents = await readJsonl(join(contextEngine, 'mail', 'control-events.jsonl'));
  const notifications = await readJsonl(join(contextEngine, 'notifications', 'outbox.jsonl'));
  const attempts = await readJsonl(join(contextEngine, 'notifications', 'attempts.jsonl'));
  const latestDeliveries = latestBy(deliveries, 'delivery_id');
  const latestNotifications = latestBy(notifications, 'notification_id');

  function assertRecordedInjection(deliveryId, label) {
    const receipt = injectionReceipts.find((row) => row.delivery_id === deliveryId && row.app_server_result === 'ok');
    assertInvariant(receipt, `${label}_injection_receipt_missing`, JSON.stringify(injectionReceipts));
    assertInvariant(receipt.app_server_method === 'thread/inject_items', `${label}_injection_method`, JSON.stringify(receipt));
    assertInvariant(receipt.item_count > 0, `${label}_injection_item_count`, JSON.stringify(receipt));
    const event = controlEvents.find((row) =>
      row.source === 'codex_app_server' &&
      row.mail_refs?.delivery_id === deliveryId &&
      row.decision?.behavior === 'record_only'
    );
    assertInvariant(event, `${label}_control_event_missing`, JSON.stringify(controlEvents));
    return { receipt, event };
  }

  assertInvariant(latestDeliveries.get(roleDeliveryId)?.state === 'done', 'ledger_role_delivery_not_done', JSON.stringify(latestDeliveries.get(roleDeliveryId)));
  assertInvariant(latestNotifications.get(curatorNotificationId)?.state === 'acknowledged', 'ledger_curator_notification_not_acknowledged', JSON.stringify(latestNotifications.get(curatorNotificationId)));
  assertInvariant(latestNotifications.get(curatorBNotificationId)?.state === 'suppressed', 'ledger_competing_notification_not_suppressed', JSON.stringify(latestNotifications.get(curatorBNotificationId)));
  assertRecordedInjection(roleDeliveryId, 'ledger_role_mail');

  assertInvariant(latestDeliveries.get(ackOnlyDeliveryId)?.state === 'done', 'ledger_ack_only_delivery_not_done', JSON.stringify(latestDeliveries.get(ackOnlyDeliveryId)));
  assertInvariant(latestNotifications.get(ackOnlyNotificationId)?.state === 'acknowledged', 'ledger_ack_only_notification_not_acknowledged', JSON.stringify(latestNotifications.get(ackOnlyNotificationId)));
  assertRecordedInjection(ackOnlyDeliveryId, 'ledger_ack_only');

  assertInvariant(latestDeliveries.get(failedDeliveryId)?.state === 'unread', 'ledger_failed_dispatch_delivery_changed', JSON.stringify(latestDeliveries.get(failedDeliveryId)));
  assertInvariant(latestNotifications.get(failedNotificationId)?.state === 'pending', 'ledger_failed_dispatch_notification_changed', JSON.stringify(latestNotifications.get(failedNotificationId)));
  assertInvariant(
    attempts.some((attempt) => attempt.notification_id === failedNotificationId && attempt.status === 'failed'),
    'ledger_failed_dispatch_attempt_missing',
    JSON.stringify(attempts)
  );

  assertInvariant(latestDeliveries.get(inactiveDeliveryId)?.state === 'unread', 'ledger_inactive_delivery_changed', JSON.stringify(latestDeliveries.get(inactiveDeliveryId)));
  assertInvariant(latestNotifications.get(inactiveNotificationId)?.state === 'pending', 'ledger_inactive_notification_changed', JSON.stringify(latestNotifications.get(inactiveNotificationId)));
  assertInvariant(
    attempts.some((attempt) => attempt.notification_id === inactiveNotificationId && attempt.status === 'dry_run'),
    'ledger_inactive_dispatch_attempt_missing',
    JSON.stringify(attempts)
  );

  return {
    deliveries: join(contextEngine, 'mail', 'deliveries.jsonl'),
    control_events: join(contextEngine, 'mail', 'control-events.jsonl'),
    injection_receipts: join(contextEngine, 'mail', 'injection-receipts.jsonl'),
    notification_outbox: join(contextEngine, 'notifications', 'outbox.jsonl'),
    notification_attempts: join(contextEngine, 'notifications', 'attempts.jsonl'),
  };
}

function publishBoundarySnapshot(status) {
  return {
    status: status.status,
    render_required: status.render_required,
    site_needs_publish: status.site_needs_publish,
    next_action: status.next_action,
    pages_needing_publish: status.pages_needing_publish || [],
    pages_missing_source: status.pages_missing_source || [],
    pages_missing_talk: status.pages_missing_talk || [],
  };
}

function samePublishBoundary(left, right) {
  return JSON.stringify(publishBoundarySnapshot(left)) === JSON.stringify(publishBoundarySnapshot(right));
}

async function identify(thread, roles, ttlSeconds = 3600) {
  const commandArgs = [
    'agent-identify',
    '--thread-id',
    thread,
    '--capability',
    'wiki.mail',
    '--ttl-seconds',
    String(ttlSeconds),
  ];
  for (const role of roles) commandArgs.splice(commandArgs.length - 4, 0, '--role', role);
  const { json } = await wiki(commandArgs);
  return json.agent;
}

async function appendTalk({
  operationId,
  subject,
  body,
  to,
  from = `agent://codex/dogfood-${shortStamp}`,
  deliveryMode,
  kind = 'proposal',
}) {
  const commandArgs = [
    'talk-append',
    '--page',
    'topics',
    '--kind',
    kind,
    '--subject',
    subject,
    '--from',
    from,
    '--body',
    body,
  ];
  if (operationId) commandArgs.push('--operation-id', operationId);
  if (deliveryMode) commandArgs.push('--delivery-mode', deliveryMode);
  for (const recipient of to) commandArgs.push('--to', recipient);
  const { json } = await wiki(commandArgs);
  return json;
}

function assertSteeringPayload({
  text,
  agentId,
  notificationId,
  deliveryId,
  messageId,
  expectedSubject,
  expectedPage,
  expectedKind,
  forbiddenBody,
  label,
}) {
  assertInvariant(text.startsWith('<steering source="1context"'), `${label}_steering_format`, text);
  assertInvariant(text.includes(`agent_id="${agentId}"`), `${label}_steering_agent_id`, text);
  assertInvariant(text.includes(`notification_id="${notificationId}"`), `${label}_steering_notification_id`, text);
  assertInvariant(
    text.includes(`wiki.agent.inbox(${agentId})`) || text.includes(`wiki.agent.inbox("${agentId}")`),
    `${label}_steering_inbox_call`,
    text
  );
  assertInvariant(text.includes(`delivery_id: ${deliveryId}`), `${label}_steering_delivery_id`, text);
  assertInvariant(text.includes(`message_id: ${messageId}`), `${label}_steering_message_id`, text);
  assertInvariant(text.includes('Suggested flow:'), `${label}_steering_suggested_flow`, text);
  assertInvariant(text.includes(`wiki.mail.open(${deliveryId})`), `${label}_steering_mail_open`, text);
  if (expectedSubject) {
    assertInvariant(text.includes(`subject: ${expectedSubject}`), `${label}_steering_subject`, text);
  }
  if (expectedPage) {
    assertInvariant(text.includes(`page: ${expectedPage}`), `${label}_steering_page`, text);
  }
  if (expectedKind) {
    assertInvariant(text.includes(`kind: ${expectedKind}`), `${label}_steering_kind`, text);
  }
  assertInvariant(!text.includes(forbiddenBody), `${label}_steering_body_leak`, text);
}

async function dispatchSteeringToCapture({
  agent,
  label,
  forbiddenBody,
  expectedSubject,
  expectedPage,
  expectedKind,
}) {
  const poll = (await wiki(['notify-poll', agent.agent_id])).json;
  assertInvariant(poll.notification_count === 1, `${label}_poll_count`, JSON.stringify(poll));
  const notification = poll.notifications[0];
  assertNotificationEnvelope(notification, {
    agent,
    deliveryId: notification.delivery_id,
    messageId: notification.message_id,
    recipient: notification.recipient,
    forbiddenBody,
    label,
  });
  const captureName = `captured-steering-${label}.txt`;
  const capturePath = join(evidenceDir, captureName);
  const dispatch = (await wiki([
    'notify-dispatch',
    agent.agent_id,
    '--steering-command',
    '/usr/bin/tee',
    '--steering-arg',
    capturePath,
    '--payload-format',
    'text',
    '--limit',
    '1',
  ])).json;
  assertInvariant(dispatch.attempt_count === 1, `${label}_dispatch_attempt_count`, JSON.stringify(dispatch));
  assertInvariant(dispatch.attempts[0].status === 'sent', `${label}_dispatch_not_sent`, JSON.stringify(dispatch.attempts[0]));
  assertNoBodyLeak(dispatch, forbiddenBody, `${label}_dispatch_body_leak`);
  const text = await readFile(capturePath, 'utf8');
  assertSteeringPayload({
    text,
    agentId: agent.agent_id,
    notificationId: notification.notification_id,
    deliveryId: notification.delivery_id,
    messageId: notification.message_id,
    expectedSubject,
    expectedPage,
    expectedKind,
    forbiddenBody,
    label,
  });
  return { notification, captureName, capturePath, dispatch };
}

async function completeDeliveryFromInbox({ agent, deliveryId, notificationId, expectedBody, label }) {
  const inbox = (await wiki(['agent-inbox', agent.agent_id])).json;
  requireInboxDelivery(inbox, deliveryId, null, label);
  const opened = (await wiki(['mail-open', deliveryId, '--agent-id', agent.agent_id])).json;
  assertOpenInjection(opened, { agent, deliveryId, expectedBody, label });
  const injectionRecord = await recordOpenInjection(opened, { agent, deliveryId, expectedBody, label });
  const claimed = (await wiki(['mail-claim', deliveryId, '--agent-id', agent.agent_id])).json;
  assertInvariant(claimed.delivery.state === 'claimed', `${label}_claim_failed`, JSON.stringify(claimed));
  const marked = (await wiki(['mail-mark', deliveryId, '--agent-id', agent.agent_id, '--state', 'done'])).json;
  assertInvariant(marked.delivery.state === 'done', `${label}_mark_done_failed`, JSON.stringify(marked));
  await wiki(['notify-ack', notificationId, '--agent-id', agent.agent_id]);
  const afterAck = (await wiki(['notify-poll', agent.agent_id])).json;
  assertInvariant(afterAck.notification_count === 0, `${label}_ack_did_not_clear_notification`, JSON.stringify(afterAck));
  return {
    inbox_count_before: inbox.deliveries.length,
    injection_id: injectionRecord.receipt.injection_id,
    control_event_id: injectionRecord.control_event.control_event_id,
  };
}

async function main() {
  await mkdir(evidenceDir, { recursive: true });
  await maybeBuild();
  await prepareRuntime();

  await wiki(['ensure']);
  await wiki(['page-create-all']);
  const initialPublish = (await wiki(['publish-status'])).json;
  observations.push({ label: 'initial_publish', snapshot: publishBoundarySnapshot(initialPublish) });

  const curatorA = await identify(`dogfood-curator-a-${shortStamp}`, ['role://topics.curator']);
  const curatorB = await identify(`dogfood-curator-b-${shortStamp}`, ['role://topics.curator']);
  const outsider = await identify(`dogfood-outsider-${shortStamp}`, ['role://projects.curator']);
  const inactive = await identify(`dogfood-inactive-${shortStamp}`, ['role://topics.inactive'], 1);
  const ackOnly = await identify(`dogfood-ack-only-${shortStamp}`, ['role://dogfood.ack']);
  const listWatcher = await identify(`dogfood-list-${shortStamp}`, ['list://dogfood.reviewers']);
  const squadAgents = [
    {
      label: 'alpha',
      role: 'role://dogfood.alpha',
      agent: await identify(`dogfood-alpha-${shortStamp}`, ['role://dogfood.alpha']),
    },
    {
      label: 'beta',
      role: 'role://dogfood.beta',
      agent: await identify(`dogfood-beta-${shortStamp}`, ['role://dogfood.beta']),
    },
    {
      label: 'gamma',
      role: 'role://dogfood.gamma',
      agent: await identify(`dogfood-gamma-${shortStamp}`, ['role://dogfood.gamma']),
    },
  ];
  const curatorStatus = (await wiki(['agent-status', curatorA.agent_id])).json;
  assertInvariant(
    curatorStatus.agent.granted_roles.includes('role://topics.curator'),
    'agent_status_missing_role',
    JSON.stringify(curatorStatus)
  );
  assertInvariant(
    listWatcher.granted_roles.includes('list://dogfood.reviewers'),
    'list_watcher_missing_grant',
    JSON.stringify(listWatcher)
  );
  const curatorThreadStatus = await assertAgentStatusByThread({
    agent: curatorA,
    expectedLeaseState: 'active',
    label: 'curator_a_initial',
  });
  const unknownThreadStatus = (await wiki(['agent-status-by-thread', `unknown-thread-${shortStamp}`])).json;
  assertInvariant(unknownThreadStatus.lease_state === 'unknown', 'unknown_thread_status_lease', JSON.stringify(unknownThreadStatus));
  observations.push({
    label: 'agents_identified',
    curator_a: curatorA.agent_id,
    curator_b: curatorB.agent_id,
    outsider: outsider.agent_id,
    inactive: inactive.agent_id,
    ack_only: ackOnly.agent_id,
    list_watcher: listWatcher.agent_id,
    squad: Object.fromEntries(squadAgents.map((row) => [row.label, row.agent.agent_id])),
    curator_thread_status: curatorThreadStatus.lease_state,
    unknown_thread_status: unknownThreadStatus.lease_state,
  });

  const labelsOnly = await appendTalk({
    operationId: `dogfood-labels-${shortStamp}`,
    subject: `Labels-only dogfood ${shortStamp}`,
    body: 'Labels-only talk should be visible in talk files without creating mail delivery rows.',
    to: ['role://topics.curator'],
    kind: 'question',
  });
  assertInvariant(labelsOnly.delivery_mode === 'labels_only', 'labels_only_mode', labelsOnly.delivery_mode);
  assertInvariant(!labelsOnly.mail_delivery, 'labels_only_mail_delivery', 'labels-only talk created mail delivery');
  assertInvariant(labelsOnly.render_required === false, 'labels_only_render_required', 'labels-only talk asked for render');

  const afterLabelsPublish = (await wiki(['publish-status'])).json;
  assertInvariant(
    samePublishBoundary(initialPublish, afterLabelsPublish),
    'talk_changed_publish_boundary',
    'labels-only talk changed publish-status boundary fields'
  );

  const roleMail = await appendTalk({
    operationId: `dogfood-role-${shortStamp}`,
    subject: `Role mail dogfood ${shortStamp}`,
    body: 'Role mail body should live in mail storage, not in the Codex steering payload.',
    to: ['role://topics.curator'],
    deliveryMode: 'mail',
  });
  assertInvariant(roleMail.mail_delivery?.status === 'delivered', 'role_mail_not_delivered', JSON.stringify(roleMail.mail_delivery));
  assertInvariant(roleMail.mail_delivery.attempt_count === 1, 'role_mail_attempt_count', roleMail.mail_delivery.attempt_count);
  const roleDeliveryId = roleMail.mail_delivery.attempts[0].delivery_id;

  const curatorPoll = (await wiki(['notify-poll', curatorA.agent_id])).json;
  assertInvariant(curatorPoll.notification_count === 1, 'curator_poll_count', JSON.stringify(curatorPoll));
  const notificationId = curatorPoll.notifications[0].notification_id;
  assertNotificationEnvelope(curatorPoll.notifications[0], {
    agent: curatorA,
    deliveryId: roleDeliveryId,
    messageId: roleMail.message_id,
    recipient: 'role://topics.curator',
    forbiddenBody: 'Role mail body should live in mail storage, not in the Codex steering payload.',
    label: 'active_curator_a',
  });
  const curatorThreadStatusAfterMail = await assertAgentStatusByThread({
    agent: curatorA,
    expectedLeaseState: 'active',
    expectedPendingDeliveryId: roleDeliveryId,
    label: 'curator_a_after_role_mail',
  });
  const curatorBPollBeforeClaim = (await wiki(['notify-poll', curatorB.agent_id])).json;
  assertInvariant(
    curatorBPollBeforeClaim.notification_count === 1,
    'active_curator_b_poll_count',
    JSON.stringify(curatorBPollBeforeClaim)
  );
  assertNotificationEnvelope(curatorBPollBeforeClaim.notifications[0], {
    agent: curatorB,
    deliveryId: roleDeliveryId,
    messageId: roleMail.message_id,
    recipient: 'role://topics.curator',
    forbiddenBody: 'Role mail body should live in mail storage, not in the Codex steering payload.',
    label: 'active_curator_b',
  });

  const steeringCapture = join(evidenceDir, 'captured-steering.txt');
  const dispatch = (await wiki([
    'notify-dispatch',
    curatorA.agent_id,
    '--steering-command',
    '/usr/bin/tee',
    '--steering-arg',
    steeringCapture,
    '--payload-format',
    'text',
  ])).json;
  assertInvariant(dispatch.attempt_count === 1, 'dispatch_attempt_count', JSON.stringify(dispatch));
  assertInvariant(dispatch.attempts[0].status === 'sent', 'dispatch_not_sent', JSON.stringify(dispatch.attempts[0]));
  assertNoBodyLeak(
    dispatch,
    'Role mail body should live in mail storage, not in the Codex steering payload.',
    'dispatch_body_leak'
  );
  const steeringText = await readFile(steeringCapture, 'utf8');
  assertInvariant(steeringText.startsWith('<steering source="1context"'), 'steering_format', steeringText);
  assertInvariant(steeringText.includes(`delivery_id: ${roleDeliveryId}`), 'steering_delivery_id', steeringText);
  assertInvariant(steeringText.includes(`message_id: ${roleMail.message_id}`), 'steering_message_id', steeringText);
  assertInvariant(steeringText.includes('page: topics /topics'), 'steering_page', steeringText);
  assertInvariant(steeringText.includes('kind: proposal'), 'steering_kind', steeringText);
  assertInvariant(steeringText.includes(`subject: Role mail dogfood ${shortStamp}`), 'steering_subject', steeringText);
  assertInvariant(steeringText.includes(`wiki.mail.open(${roleDeliveryId})`), 'steering_mail_open', steeringText);
  assertInvariant(!steeringText.includes('Role mail body'), 'steering_body_leak', steeringText);

  const curatorInboxAfterSteering = (await wiki(['agent-inbox', curatorA.agent_id])).json;
  assertInvariant(
    curatorInboxAfterSteering.deliveries.some((row) => row.delivery_id === roleDeliveryId),
    'steering_mail_missing_from_inbox',
    'steering dispatch removed underlying delivery from inbox'
  );
  const openedRoleMail = (await wiki(['mail-open', roleDeliveryId, '--agent-id', curatorA.agent_id])).json;
  assertOpenInjection(openedRoleMail, {
    agent: curatorA,
    deliveryId: roleDeliveryId,
    expectedBody: 'Role mail body should live in mail storage, not in the Codex steering payload.',
    label: 'role_mail',
  });
  const roleMailInjection = await recordOpenInjection(openedRoleMail, {
    agent: curatorA,
    deliveryId: roleDeliveryId,
    expectedBody: 'Role mail body should live in mail storage, not in the Codex steering payload.',
    label: 'role_mail',
  });

  await expectWikiFailure(
    'outsider claim',
    ['mail-claim', roleDeliveryId, '--agent-id', outsider.agent_id],
    'cannot access',
    'authorization_bypass_claim'
  );
  await expectWikiFailure(
    'outsider mark',
    ['mail-mark', roleDeliveryId, '--agent-id', outsider.agent_id, '--state', 'done'],
    'cannot access',
    'authorization_bypass_mark'
  );
  await expectWikiFailure(
    'outsider snooze',
    ['mail-snooze', roleDeliveryId, '--agent-id', outsider.agent_id, '--until', '2099-01-01T00:00:00Z'],
    'cannot access',
    'authorization_bypass_snooze'
  );

  const claimed = (await wiki(['mail-claim', roleDeliveryId, '--agent-id', curatorA.agent_id])).json;
  assertInvariant(claimed.delivery.state === 'claimed', 'authorized_claim_failed', JSON.stringify(claimed));
  const curatorBPollAfterClaim = (await wiki(['notify-poll', curatorB.agent_id])).json;
  assertInvariant(
    curatorBPollAfterClaim.notification_count === 0,
    'claimed_role_delivery_left_competing_notification',
    JSON.stringify(curatorBPollAfterClaim)
  );
  await expectWikiFailure(
    'authorized competitor claim',
    ['mail-claim', roleDeliveryId, '--agent-id', curatorB.agent_id],
    'already claimed',
    'claim_arbiter_failed'
  );
  const marked = (await wiki(['mail-mark', roleDeliveryId, '--agent-id', curatorA.agent_id, '--state', 'done'])).json;
  assertInvariant(marked.delivery.state === 'done', 'authorized_mark_failed', JSON.stringify(marked));
  await wiki(['notify-ack', notificationId, '--agent-id', curatorA.agent_id]);
  const afterAckPoll = (await wiki(['notify-poll', curatorA.agent_id])).json;
  assertInvariant(afterAckPoll.notification_count === 0, 'ack_did_not_clear_notification', JSON.stringify(afterAckPoll));

  const duplicateSame = await appendTalk({
    operationId: `dogfood-role-${shortStamp}`,
    subject: `Role mail dogfood ${shortStamp}`,
    body: 'Role mail body should live in mail storage, not in the Codex steering payload.',
    to: ['role://topics.curator'],
    deliveryMode: 'mail',
  });
  assertInvariant(
    duplicateSame.mail_delivery?.acceptance === 'duplicate_same_payload',
    'duplicate_same_payload_not_idempotent',
    JSON.stringify(duplicateSame.mail_delivery)
  );
  assertInvariant(
    duplicateSame.mail_delivery.attempts[0].status === 'already_delivered',
    'duplicate_same_payload_redelivered',
    JSON.stringify(duplicateSame.mail_delivery)
  );

  const duplicateChanged = await appendTalk({
    operationId: `dogfood-role-${shortStamp}`,
    subject: `Role mail dogfood ${shortStamp}`,
    body: 'Changed body should not be accepted under the same operation id.',
    to: ['role://topics.curator'],
    deliveryMode: 'mail',
  });
  assertInvariant(
    duplicateChanged.status === 'appended_delivery_failed',
    'duplicate_changed_payload_accepted',
    JSON.stringify(duplicateChanged)
  );
  assertInvariant(
    duplicateChanged.mail_delivery?.error?.includes('duplicate idempotency key'),
    'duplicate_changed_payload_error',
    JSON.stringify(duplicateChanged.mail_delivery)
  );

  const ackOnlyBody = `Ack-only body must stay unread until mail is claimed ${shortStamp}`;
  const ackOnlyMail = await appendTalk({
    operationId: `dogfood-ack-only-${shortStamp}`,
    subject: `Ack-only dogfood ${shortStamp}`,
    body: ackOnlyBody,
    to: ['role://dogfood.ack'],
    deliveryMode: 'mail',
    kind: 'question',
  });
  assertInvariant(ackOnlyMail.mail_delivery?.status === 'delivered', 'ack_only_mail_not_delivered', JSON.stringify(ackOnlyMail));
  const ackOnlyDeliveryId = ackOnlyMail.mail_delivery.attempts[0].delivery_id;
  const ackOnlyPoll = (await wiki(['notify-poll', ackOnly.agent_id])).json;
  assertInvariant(ackOnlyPoll.notification_count === 1, 'ack_only_poll_count', JSON.stringify(ackOnlyPoll));
  const ackOnlyNotification = ackOnlyPoll.notifications[0];
  assertNotificationEnvelope(ackOnlyNotification, {
    agent: ackOnly,
    deliveryId: ackOnlyDeliveryId,
    messageId: ackOnlyMail.message_id,
    recipient: 'role://dogfood.ack',
    forbiddenBody: ackOnlyBody,
    label: 'ack_only',
  });
  const ackOnlyAck = (await wiki(['notify-ack', ackOnlyNotification.notification_id, '--agent-id', ackOnly.agent_id])).json;
  assertInvariant(ackOnlyAck.notification.state === 'acknowledged', 'ack_only_ack_failed', JSON.stringify(ackOnlyAck));
  const ackOnlyPollAfterAck = (await wiki(['notify-poll', ackOnly.agent_id])).json;
  assertInvariant(ackOnlyPollAfterAck.notification_count === 0, 'ack_only_notification_still_pollable', JSON.stringify(ackOnlyPollAfterAck));
  const ackOnlyInboxAfterAck = (await wiki(['agent-inbox', ackOnly.agent_id])).json;
  requireInboxDelivery(ackOnlyInboxAfterAck, ackOnlyDeliveryId, 'unread', 'ack_only_after_ack');
  const ackOnlyOpenedAfterAck = (await wiki(['mail-open', ackOnlyDeliveryId, '--agent-id', ackOnly.agent_id])).json;
  assertInvariant(ackOnlyOpenedAfterAck.delivery.state === 'unread', 'ack_only_open_changed_delivery', JSON.stringify(ackOnlyOpenedAfterAck));
  assertOpenInjection(ackOnlyOpenedAfterAck, {
    agent: ackOnly,
    deliveryId: ackOnlyDeliveryId,
    expectedBody: ackOnlyBody,
    label: 'ack_only',
  });
  const ackOnlyInjection = await recordOpenInjection(ackOnlyOpenedAfterAck, {
    agent: ackOnly,
    deliveryId: ackOnlyDeliveryId,
    expectedBody: ackOnlyBody,
    label: 'ack_only',
  });
  const ackOnlyClaimed = (await wiki(['mail-claim', ackOnlyDeliveryId, '--agent-id', ackOnly.agent_id])).json;
  assertInvariant(ackOnlyClaimed.delivery.state === 'claimed', 'ack_only_claim_failed', JSON.stringify(ackOnlyClaimed));
  const ackOnlyMarked = (await wiki(['mail-mark', ackOnlyDeliveryId, '--agent-id', ackOnly.agent_id, '--state', 'done'])).json;
  assertInvariant(ackOnlyMarked.delivery.state === 'done', 'ack_only_mark_failed', JSON.stringify(ackOnlyMarked));
  await assertPublishBoundaryStill(initialPublish, 'ack_only_mail');

  const pageMailboxBody = `Dogfood page mailbox body ${shortStamp}`;
  const pageMailboxMail = await appendTalk({
    operationId: `dogfood-page-mailbox-${shortStamp}`,
    subject: `Dogfood page mailbox ${shortStamp}`,
    body: pageMailboxBody,
    to: ['page://topics'],
    deliveryMode: 'mail',
    kind: 'question',
  });
  assertInvariant(
    pageMailboxMail.mail_delivery?.status === 'delivered',
    'page_mailbox_not_delivered',
    JSON.stringify(pageMailboxMail.mail_delivery)
  );
  assertInvariant(
    pageMailboxMail.mail_delivery.attempts[0].recipient === 'mailbox://page/topics',
    'page_mailbox_recipient_not_canonical',
    JSON.stringify(pageMailboxMail.mail_delivery)
  );
  assertNoBodyLeak(pageMailboxMail.mail_delivery, pageMailboxBody, 'page_mailbox_receipt_body_leak');

  const routeCoverage = [];
  for (const route of [
    {
      label: 'list',
      agent: listWatcher,
      recipient: 'list://dogfood.reviewers',
      expectedRecipient: 'list://dogfood.reviewers',
      expectedGrant: 'list://dogfood.reviewers',
    },
  ]) {
    assertInvariant(
      route.agent.granted_roles.includes(route.expectedGrant),
      `${route.label}_route_grant_missing`,
      JSON.stringify(route.agent)
    );
    const body = `Dogfood ${route.label} route private body ${shortStamp}`;
    const mail = await appendTalk({
      operationId: `dogfood-${route.label}-route-${shortStamp}`,
      subject: `Dogfood ${route.label} route ${shortStamp}`,
      body,
      to: [route.recipient],
      deliveryMode: 'mail',
      kind: 'question',
    });
    assertInvariant(mail.mail_delivery?.status === 'delivered', `${route.label}_route_not_delivered`, JSON.stringify(mail.mail_delivery));
    assertInvariant(
      mail.mail_delivery.attempts[0].recipient === route.expectedRecipient,
      `${route.label}_route_recipient`,
      JSON.stringify(mail.mail_delivery)
    );
    const deliveryId = mail.mail_delivery.attempts[0].delivery_id;
    const steering = await dispatchSteeringToCapture({
      agent: route.agent,
      label: `${route.label}-route`,
      forbiddenBody: body,
      expectedSubject: `Dogfood ${route.label} route ${shortStamp}`,
      expectedPage: 'topics /topics',
      expectedKind: 'question',
    });
    const completion = await completeDeliveryFromInbox({
      agent: route.agent,
      deliveryId,
      notificationId: steering.notification.notification_id,
      expectedBody: body,
      label: `${route.label}-route`,
    });
    routeCoverage.push({
      label: route.label,
      recipient: route.expectedRecipient,
      agent_id: route.agent.agent_id,
      delivery_id: deliveryId,
      notification_id: steering.notification.notification_id,
      capture: steering.captureName,
      inbox_count_before: completion.inbox_count_before,
    });
  }
  observations.push({ label: 'list_page_route_coverage', runs: routeCoverage });

  const squadRuns = [];
  for (const member of squadAgents) {
    const body = `Dogfood squad ${member.label} private body ${shortStamp}`;
    const mail = await appendTalk({
      operationId: `dogfood-squad-${member.label}-${shortStamp}`,
      subject: `Dogfood squad ${member.label} ${shortStamp}`,
      body,
      to: [member.role],
      deliveryMode: 'mail',
      kind: 'question',
    });
    assertInvariant(mail.mail_delivery?.status === 'delivered', `${member.label}_mail_not_delivered`, JSON.stringify(mail.mail_delivery));
    const deliveryId = mail.mail_delivery.attempts[0].delivery_id;
    if (member.label === 'beta') {
      await expectWikiFailure(
        'squad cross-agent claim',
        ['mail-claim', deliveryId, '--agent-id', squadAgents[0].agent.agent_id],
        'cannot access',
        'squad_cross_agent_claim_allowed'
      );
    }
    const steering = await dispatchSteeringToCapture({
      agent: member.agent,
      label: `squad-${member.label}`,
      forbiddenBody: body,
      expectedSubject: `Dogfood squad ${member.label} ${shortStamp}`,
      expectedPage: 'topics /topics',
      expectedKind: 'question',
    });
    const completion = await completeDeliveryFromInbox({
      agent: member.agent,
      deliveryId,
      notificationId: steering.notification.notification_id,
      expectedBody: body,
      label: `squad-${member.label}`,
    });
    squadRuns.push({
      label: member.label,
      role: member.role,
      agent_id: member.agent.agent_id,
      delivery_id: deliveryId,
      notification_id: steering.notification.notification_id,
      capture: steering.captureName,
      inbox_count_before: completion.inbox_count_before,
    });
  }
  observations.push({ label: 'squad_steering_round', runs: squadRuns });

  const failedSteeringMail = await appendTalk({
    operationId: `dogfood-steering-fail-${shortStamp}`,
    subject: `Steering failure dogfood ${shortStamp}`,
    body: 'This pending notification should survive a failed steering command.',
    to: ['role://topics.curator'],
    deliveryMode: 'mail',
  });
  const failedDispatch = (await wiki([
    'notify-dispatch',
    curatorA.agent_id,
    '--steering-command',
    '/usr/bin/false',
    '--payload-format',
    'text',
  ])).json;
  assertInvariant(failedDispatch.attempt_count >= 1, 'failed_dispatch_no_attempt', JSON.stringify(failedDispatch));
  assertInvariant(
    failedDispatch.attempts.every((attempt) => attempt.status === 'failed'),
    'failed_dispatch_status',
    JSON.stringify(failedDispatch.attempts)
  );
  assertNoBodyLeak(
    failedDispatch,
    'This pending notification should survive a failed steering command.',
    'failed_dispatch_body_leak'
  );
  const failedDeliveryId = failedSteeringMail.mail_delivery.attempts[0].delivery_id;
  const failedAttempt = failedDispatch.attempts.find((attempt) => attempt.payload.delivery_id === failedDeliveryId);
  assertInvariant(failedAttempt, 'failed_dispatch_missing_target_attempt', JSON.stringify(failedDispatch));
  assertInvariant(failedAttempt.notification_id, 'failed_dispatch_missing_notification_id', JSON.stringify(failedAttempt));
  const afterFailedDispatchPoll = (await wiki(['notify-poll', curatorA.agent_id])).json;
  assertInvariant(
    afterFailedDispatchPoll.notifications.some((notification) =>
      notification.delivery_id === failedDeliveryId &&
      notification.notification_id === failedAttempt.notification_id
    ),
    'failed_dispatch_consumed_notification',
    JSON.stringify(afterFailedDispatchPoll)
  );
  const failedInboxAfterDispatch = (await wiki(['agent-inbox', curatorA.agent_id])).json;
  requireInboxDelivery(failedInboxAfterDispatch, failedDeliveryId, 'unread', 'failed_dispatch_preserved_delivery');
  await assertPublishBoundaryStill(initialPublish, 'failed_dispatch');

  const badRecipient = await appendTalk({
    operationId: `dogfood-bad-recipient-${shortStamp}`,
    subject: `Bad recipient dogfood ${shortStamp}`,
    body: 'The talk source should remain, but mail delivery should fail.',
    to: ['not-an-address'],
    deliveryMode: 'mail',
  });
  assertInvariant(badRecipient.status === 'appended_delivery_failed', 'bad_recipient_accepted', JSON.stringify(badRecipient));
  assertInvariant(badRecipient.mail_delivery?.status === 'failed', 'bad_recipient_status', JSON.stringify(badRecipient.mail_delivery));

  await sleep(1500);
  const inactiveMail = await appendTalk({
    operationId: `dogfood-inactive-${shortStamp}`,
    subject: `Inactive agent dogfood ${shortStamp}`,
    body: 'A notification created after lease expiration must remain dispatchable by the supervisor.',
    to: ['role://topics.inactive'],
    deliveryMode: 'mail',
  });
  assertInvariant(inactiveMail.mail_delivery?.status === 'delivered', 'inactive_mail_delivery_failed', JSON.stringify(inactiveMail));
  const inactiveDeliveryId = inactiveMail.mail_delivery.attempts[0].delivery_id;
  await expectWikiFailure(
    'inactive notify poll',
    ['notify-poll', inactive.agent_id],
    'stale',
    'inactive_agent_self_poll_allowed'
  );
  await expectWikiFailure(
    'inactive claim',
    ['mail-claim', inactiveDeliveryId, '--agent-id', inactive.agent_id],
    'stale',
    'inactive_agent_claim_allowed'
  );
  const inactiveDispatch = (await wiki([
    'notify-dispatch',
    inactive.agent_id,
    '--dry-run',
  ])).json;
  assertInvariant(inactiveDispatch.attempt_count === 1, 'inactive_dispatch_attempt_count', JSON.stringify(inactiveDispatch));
  assertInvariant(
    inactiveDispatch.attempts[0].payload.delivery_id === inactiveDeliveryId,
    'inactive_dispatch_wrong_delivery',
    JSON.stringify(inactiveDispatch)
  );
  assertInvariant(inactiveDispatch.attempts[0].status === 'dry_run', 'inactive_dispatch_not_dry_run', JSON.stringify(inactiveDispatch));
  assertNoBodyLeak(
    inactiveDispatch,
    'A notification created after lease expiration must remain dispatchable by the supervisor.',
    'inactive_dispatch_body_leak'
  );
  observations.push({
    label: 'inactive_supervisor_dispatch',
    agent_id: inactive.agent_id,
    delivery_id: inactiveDispatch.attempts[0].payload.delivery_id,
    notification_id: inactiveDispatch.attempts[0].notification_id,
    status: inactiveDispatch.attempts[0].status,
  });
  const heartbeat = (await wiki(['agent-heartbeat', inactive.agent_id, '--ttl-seconds', '60'])).json;
  assertInvariant(heartbeat.operation === 'wiki.agent.heartbeat', 'heartbeat_failed', JSON.stringify(heartbeat));
  const afterHeartbeatStatus = (await wiki(['agent-status', inactive.agent_id])).json;
  assertInvariant(
    afterHeartbeatStatus.latest_lease?.agent_id === inactive.agent_id,
    'heartbeat_status_missing_lease',
    JSON.stringify(afterHeartbeatStatus)
  );
  const afterHeartbeatPoll = (await wiki(['notify-poll', inactive.agent_id])).json;
  assertInvariant(
    afterHeartbeatPoll.notifications.some((notification) =>
      notification.delivery_id === inactiveDeliveryId
    ),
    'heartbeat_did_not_restore_pending_notification',
    JSON.stringify(afterHeartbeatPoll)
  );
  assertNotificationEnvelope(afterHeartbeatPoll.notifications.find((notification) => notification.delivery_id === inactiveDeliveryId), {
    agent: inactive,
    deliveryId: inactiveDeliveryId,
    messageId: inactiveMail.message_id,
    recipient: 'role://topics.inactive',
    forbiddenBody: 'A notification created after lease expiration must remain dispatchable by the supervisor.',
    label: 'inactive_after_heartbeat',
  });
  const retired = (await wiki(['agent-retire', inactive.agent_id])).json;
  assertInvariant(retired.agent.state === 'retired', 'retire_failed', JSON.stringify(retired));
  const retiredStatus = (await wiki(['agent-status', inactive.agent_id])).json;
  assertInvariant(retiredStatus.agent.state === 'retired', 'retire_status_failed', JSON.stringify(retiredStatus));
  const retiredThreadStatus = await assertAgentStatusByThread({
    agent: inactive,
    expectedLeaseState: 'retired',
    expectedPendingDeliveryId: inactiveDeliveryId,
    label: 'inactive_retired',
  });
  await expectWikiFailure(
    'retired notify poll',
    ['notify-poll', inactive.agent_id],
    'not active',
    'retired_agent_notification_dispatchable'
  );
  await expectWikiFailure(
    'retired inbox',
    ['agent-inbox', inactive.agent_id],
    'not active',
    'retired_agent_inbox_readable'
  );
  await expectWikiFailure(
    'retired claim',
    ['mail-claim', inactiveDeliveryId, '--agent-id', inactive.agent_id],
    'not active',
    'retired_agent_claim_allowed'
  );

  const finalPublish = (await wiki(['publish-status'])).json;
  assertInvariant(
    samePublishBoundary(initialPublish, finalPublish),
    'mail_changed_publish_boundary',
    'mail and notification operations changed publish-status boundary fields'
  );

  await assertLedgerEvidence({
    roleDeliveryId,
    curatorNotificationId: notificationId,
    curatorBNotificationId: curatorBPollBeforeClaim.notifications[0].notification_id,
    ackOnlyDeliveryId,
    ackOnlyNotificationId: ackOnlyNotification.notification_id,
    failedDeliveryId,
    failedNotificationId: failedAttempt.notification_id,
    inactiveDeliveryId,
    inactiveNotificationId: inactiveDispatch.attempts[0].notification_id,
  });

  await collectEvidenceFiles();
  await writeJson('summary.json', {
    status: 'ok',
    timestamp,
    runtime_root: runtimeRoot,
    evidence_dir: evidenceDir,
    agents: {
      curator_a: curatorA.agent_id,
      curator_b: curatorB.agent_id,
      outsider: outsider.agent_id,
      inactive: inactive.agent_id,
      ack_only: ackOnly.agent_id,
      list_watcher: listWatcher.agent_id,
      squad: Object.fromEntries(squadAgents.map((row) => [row.label, row.agent.agent_id])),
    },
    checked: {
      labels_only: labelsOnly.message_id,
      role_delivery_id: roleDeliveryId,
      notification_id: notificationId,
      active_curator_b_notification_id: curatorBPollBeforeClaim.notifications[0].notification_id,
      role_mail_injection_id: roleMailInjection.receipt.injection_id,
      role_mail_control_event_id: roleMailInjection.control_event.control_event_id,
      curator_thread_pending_after_role_mail: curatorThreadStatusAfterMail.pending_notification_count,
      ack_only_delivery_id: ackOnlyDeliveryId,
      ack_only_notification_id: ackOnlyNotification.notification_id,
      ack_only_injection_id: ackOnlyInjection.receipt.injection_id,
      ack_only_control_event_id: ackOnlyInjection.control_event.control_event_id,
      page_mailbox_delivery_id: pageMailboxMail.mail_delivery.attempts[0].delivery_id,
      page_mailbox_recipient: pageMailboxMail.mail_delivery.attempts[0].recipient,
      route_coverage: routeCoverage,
      squad_steering: squadRuns,
      duplicate_same_message_id: duplicateSame.message_id,
      duplicate_changed_status: duplicateChanged.status,
      failed_steering_delivery_id: failedDeliveryId,
      failed_steering_notification_id: failedAttempt.notification_id,
      bad_recipient_status: badRecipient.status,
      inactive_delivery_id: inactiveDeliveryId,
      inactive_notification_id: inactiveDispatch.attempts[0].notification_id,
      heartbeat_agent_id: heartbeat.agent_id,
      retired_agent_state: retired.agent.state,
      retired_thread_lease_state: retiredThreadStatus.lease_state,
      publish_boundary: publishBoundarySnapshot(finalPublish),
    },
    evidence_files: {
      commands: join(evidenceDir, 'commands.jsonl'),
      summary: join(evidenceDir, 'summary.json'),
      mail_ledger: join(evidenceDir, 'mail-ledger'),
      mail_control_events: join(evidenceDir, 'mail-ledger', 'control-events.jsonl'),
      mail_injection_receipts: join(evidenceDir, 'mail-ledger', 'injection-receipts.jsonl'),
      notification_ledger: join(evidenceDir, 'notification-ledger'),
      agent_directory: join(evidenceDir, 'agent-directory'),
      default_steering_capture: join(evidenceDir, 'captured-steering.txt'),
      route_and_squad_captures: [
        ...routeCoverage.map((row) => join(evidenceDir, row.capture)),
        ...squadRuns.map((row) => join(evidenceDir, row.capture)),
      ],
    },
    proof_cases: [
      'active role delivery creates durable notifications for both active curators',
      'steering and notification control-plane outputs omit full message bodies',
      'mail-open returns a bodyless message summary and a thread/inject_items content_delivery request carrying the authorized body',
      'mail-open host injection receipts are recorded in mail/control-events.jsonl and mail/injection-receipts.jsonl',
      'agent-status-by-thread resolves Codex thread transport to durable agent identity and pending mail digest',
      'notify ack alone clears the wakeup hint but leaves delivery unread',
      'role and list route grants receive mail and steering; page-mailbox delivery canonicalizes page://topics',
      'failed dispatch records failed attempt while preserving pending notification and unread delivery',
      'mail created after lease expiration is supervisor-dispatchable but self-poll/claim is lease-gated',
      'heartbeat restores stale pending notification visibility',
      'claim suppresses competing same-role notification',
      'retired agent cannot poll inbox or mutate delivery state',
      'mail and notification operations leave publish-status boundary unchanged',
    ],
    observations,
  });
}

try {
  await main();
  if (!keepRuntime) {
    await rm(devHome, { recursive: true, force: true });
  }
  console.log(`agent-mail dogfood ok: ${evidenceDir}`);
} catch (error) {
  await mkdir(evidenceDir, { recursive: true });
  await collectEvidenceFiles().catch(() => {});
  await writeJson('failure.json', {
    status: 'failed',
    timestamp,
    runtime_root: runtimeRoot,
    evidence_dir: evidenceDir,
    message: error.message,
    failure: error.failure || null,
    failures,
    observations,
    stack: error.stack,
  }).catch(() => {});
  console.error(`agent-mail dogfood failed: ${error.message}`);
  console.error(`evidence: ${evidenceDir}`);
  process.exit(1);
}
