#!/usr/bin/env node
import { appendFile, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { createServer } from 'node:http';
import { request as httpRequest } from 'node:http';
import { join, relative, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const argv = process.argv.slice(2);
const args = new Set(argv);
const timestamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
const shortStamp = timestamp.replace(/^.*T(\d{6})Z$/, '$1').toLowerCase();
const evidenceDir = resolve(
  process.env.ONECONTEXT_CODEX_ADAPTER_LIVE_MAIL_DIR ||
    join(repoRoot, 'test-results', `codex-adapter-live-mail-flow-${timestamp}`)
);
const devHome = resolve(
  process.env.ONECONTEXT_CODEX_ADAPTER_LIVE_MAIL_HOME ||
    join('/tmp', `1context-codex-adapter-live-mail-flow-${shortStamp}`)
);
const runtimeRoot = join(devHome, '1Context');
const commandLog = join(evidenceDir, 'commands.jsonl');
const proofSummaryPath = join(evidenceDir, 'proof-summary.json');
const transcriptPath = join(evidenceDir, 'app-server-transcript.jsonl');
const appServerStderrPath = join(evidenceDir, 'app-server.stderr.log');
const dispatchHelperPath = join(evidenceDir, 'dispatch-to-live-codex.mjs');
const mailBodyPath = join(devHome, 'live-mail-body.md');
const codexBin = process.env.ONECONTEXT_CODEX_BIN || 'codex';
const wikiBin = resolve(process.env.ONECONTEXT_WIKI_CORE_BIN || join(repoRoot, 'target/debug/onecontext-wiki'));
const model = process.env.ONECONTEXT_CODEX_ADAPTER_LIVE_MODEL || process.env.CODEX_MODEL || 'gpt-5.4-mini';
const keepRuntime = args.has('--keep-runtime');
const shouldBuild = args.has('--build');

const roleAddress = `role://live.mail.${shortStamp}`;
const mailSubject = `Live Codex mail flow ${shortStamp}`;
const mailBody = `Live mail body ${shortStamp}: this body must enter Codex only through wiki.mail.open content_delivery and thread/inject_items.`;
const liveTurnPrompt =
  'Live mail flow dogfood: use the shell command tool to run exactly `sleep 8`, then return exactly LIVE_MAIL_FLOW_BACKGROUND_OK.';
const redactions = new Set([mailBody, liveTurnPrompt]);

const summary = {
  schema_version: 1,
  status: 'running',
  proof: 'codex-adapter-live-mail-flow',
  generated_at: new Date().toISOString(),
  evidence_dir: evidenceDir,
  runtime_root: runtimeRoot,
  codex_bin: codexBin,
  wiki_bin: wikiBin,
  model,
  phases: {},
  assertions: {},
  commands: [],
  artifacts: {
    proof_summary: proofSummaryPath,
    command_log: commandLog,
    transcript_jsonl: transcriptPath,
    app_server_stderr: appServerStderrPath,
    dispatch_helper: dispatchHelperPath,
  },
  redaction: {
    mail_body_sha256: sha256(mailBody),
    background_prompt_sha256: sha256(liveTurnPrompt),
    steering_text_sha256: null,
    mail_body_persisted_in_summary: false,
    background_prompt_persisted_in_summary: false,
    steering_text_persisted_in_summary: false,
  },
};

function usage() {
  console.log(`Usage: node scripts/test-codex-adapter-live-mail-flow.mjs [--build] [--keep-runtime]

Starts a real Codex app-server, creates a real 1Context mail delivery, has
wiki.notify.dispatch invoke a local dispatcher command, and forwards that
notification into the active Codex turn with turn/steer. The opened mail body
is then delivered with thread/inject_items.

Environment:
  ONECONTEXT_CODEX_BIN                         Codex CLI binary (default: codex)
  ONECONTEXT_WIKI_CORE_BIN                     wiki CLI binary
  ONECONTEXT_CODEX_ADAPTER_LIVE_MAIL_DIR       evidence output directory
  ONECONTEXT_CODEX_ADAPTER_LIVE_MAIL_HOME      disposable 1Context home
  ONECONTEXT_CODEX_ADAPTER_LIVE_MODEL          thread/start model
`);
}

if (args.has('--help') || args.has('-h')) {
  usage();
  process.exit(0);
}

function sha256(value) {
  return `sha256:${createHash('sha256').update(value, 'utf8').digest('hex')}`;
}

function fail(code, detail, repairHint) {
  const error = new Error(`${code}: ${detail}`);
  error.code = code;
  error.detail = detail;
  error.repairHint = repairHint;
  throw error;
}

function assertInvariant(condition, code, detail) {
  if (!condition) fail(code, detail);
}

function redactForLog(value) {
  if (typeof value === 'string') {
    let redacted = value;
    for (const secret of redactions) {
      if (secret) redacted = redacted.replaceAll(secret, `[redacted:${sha256(secret)}]`);
    }
    return redacted;
  }
  if (Array.isArray(value)) return value.map(redactForLog);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, redactForLog(item)]));
  }
  return value;
}

function parseJson(stdout, label) {
  try {
    return JSON.parse(stdout);
  } catch (error) {
    fail('invalid_json', `${label} did not emit JSON: ${error.message}. stdout=${JSON.stringify(stdout.slice(0, 300))}`);
  }
}

async function run(command, commandArgs, options = {}) {
  const started = Date.now();
  const child = spawn(command, commandArgs, {
    cwd: repoRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
    ...options.spawn,
  });
  let stdout = '';
  let stderr = '';
  let timedOut = false;
  const timeoutMs = options.timeoutMs || 60_000;
  const timeout = setTimeout(() => {
    timedOut = true;
    child.kill('SIGTERM');
    setTimeout(() => child.kill('SIGKILL'), 2_000).unref();
  }, timeoutMs);
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
  clearTimeout(timeout);
  return {
    command,
    args: commandArgs,
    code,
    stdout,
    stderr,
    timed_out: timedOut,
    ms: Date.now() - started,
  };
}

async function logCommand(entry) {
  const redacted = redactForLog(entry);
  summary.commands.push({
    tool: redacted.tool,
    phase: redacted.phase || null,
    args: redacted.args,
    code: redacted.code,
    status: redacted.status,
    ms: redacted.ms,
  });
  await appendFile(commandLog, JSON.stringify(redacted) + '\n');
}

async function maybeBuildWiki() {
  if (!shouldBuild && existsSync(wikiBin)) return;
  if (!shouldBuild) {
    fail('missing_wiki_binary', `${wikiBin} does not exist`, 'Run this script with --build or build onecontext-wiki-daemon first.');
  }
  const build = await run('cargo', ['build', '--package', 'onecontext-wiki-daemon'], { timeoutMs: 120_000 });
  await logCommand({
    tool: 'cargo',
    phase: 'build_wiki',
    args: ['build', '--package', 'onecontext-wiki-daemon'],
    code: build.code,
    status: build.code === 0 ? 'ok' : 'error',
    ms: build.ms,
    stderr: build.stderr,
    timed_out: build.timed_out,
  });
  if (build.code !== 0) fail('wiki_build_failed', build.stderr || build.stdout);
}

async function wiki(commandArgs, options = {}) {
  const entry = await run(wikiBin, ['--root', runtimeRoot, ...commandArgs], { timeoutMs: options.timeoutMs || 60_000 });
  const json = parseJson(entry.stdout, `onecontext-wiki ${commandArgs.join(' ')}`);
  collectDynamicRedactions(json);
  await logCommand({
    tool: 'onecontext-wiki',
    args: ['--root', runtimeRoot, ...commandArgs],
    code: entry.code,
    status: entry.code === 0 ? json.status || 'ok' : json.status || 'error',
    operation: json.operation,
    ms: entry.ms,
    json,
    stderr: entry.stderr,
    timed_out: entry.timed_out,
  });
  if (!options.allowFailure && entry.code !== 0) {
    fail('wiki_command_failed', `${commandArgs.join(' ')} failed: ${JSON.stringify(json.error || json)}`);
  }
  return json;
}

function collectDynamicRedactions(value) {
  if (!value || typeof value !== 'object') return;
  if (typeof value.steering_text === 'string') {
    redactions.add(value.steering_text);
    summary.redaction.steering_text_sha256 = sha256(value.steering_text);
  }
  if (Array.isArray(value)) {
    for (const item of value) collectDynamicRedactions(item);
    return;
  }
  for (const item of Object.values(value)) collectDynamicRedactions(item);
}

class AppServerRpc {
  constructor(command, commandArgs) {
    this.command = command;
    this.commandArgs = commandArgs;
    this.nextId = 1;
    this.buffer = '';
    this.pending = new Map();
    this.notifications = [];
    this.notificationWaiters = [];
    this.transcriptSummary = {
      request_methods: {},
      response_count: 0,
      error_count: 0,
      notification_methods: {},
    };
  }

  async start() {
    this.child = spawn(this.command, this.commandArgs, {
      cwd: repoRoot,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    await appendFile(
      transcriptPath,
      JSON.stringify({
        ts: new Date().toISOString(),
        direction: 'process',
        event: 'spawn',
        command: [this.command, ...this.commandArgs],
      }) + '\n'
    );
    this.child.stdout.on('data', (chunk) => this.receive(chunk.toString('utf8')));
    this.child.stderr.on('data', (chunk) => {
      appendFile(appServerStderrPath, redactForLog(chunk.toString('utf8'))).catch(() => {});
    });
    this.child.on('exit', (code, signal) => {
      appendFile(
        transcriptPath,
        JSON.stringify({ ts: new Date().toISOString(), direction: 'process', event: 'exit', code, signal }) + '\n'
      ).catch(() => {});
      for (const pending of this.pending.values()) {
        pending.reject(new Error(`app-server exited before response: code=${code} signal=${signal}`));
      }
      this.pending.clear();
      for (const waiter of this.notificationWaiters) {
        waiter.reject(new Error(`app-server exited before notification: code=${code} signal=${signal}`));
      }
      this.notificationWaiters = [];
    });
  }

  receive(chunk) {
    this.buffer += chunk;
    while (this.buffer.includes('\n')) {
      const index = this.buffer.indexOf('\n');
      const rawLine = this.buffer.slice(0, index);
      this.buffer = this.buffer.slice(index + 1);
      const line = rawLine.trim();
      if (!line) continue;
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        appendFile(
          transcriptPath,
          JSON.stringify({ ts: new Date().toISOString(), direction: 'recv_raw', line: redactForLog(line) }) + '\n'
        ).catch(() => {});
        continue;
      }
      this.observe(message);
      appendFile(
        transcriptPath,
        JSON.stringify({ ts: new Date().toISOString(), direction: 'recv', message: redactForLog(message) }) + '\n'
      ).catch(() => {});
      if (message.id !== undefined && this.pending.has(String(message.id))) {
        const pending = this.pending.get(String(message.id));
        this.pending.delete(String(message.id));
        pending.resolve(message);
      } else if (message.method) {
        this.notifications.push(message);
        this.resolveNotificationWaiters(message);
      }
    }
  }

  observe(message) {
    if (message.method) {
      const bucket = message.id === undefined ? this.transcriptSummary.notification_methods : this.transcriptSummary.request_methods;
      bucket[message.method] = (bucket[message.method] || 0) + 1;
    }
    if (message.result !== undefined) this.transcriptSummary.response_count += 1;
    if (message.error !== undefined) this.transcriptSummary.error_count += 1;
  }

  async send(method, params, id = null, timeoutMs = 30_000) {
    const requestId = id || `live-mail-${this.nextId++}`;
    const message = { id: requestId, method, params };
    this.observe(message);
    await appendFile(
      transcriptPath,
      JSON.stringify({ ts: new Date().toISOString(), direction: 'send', message: redactForLog(message) }) + '\n'
    );
    const responsePromise = new Promise((resolveResponse, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(String(requestId));
        reject(new Error(`timeout waiting for ${method} response after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending.set(String(requestId), {
        resolve: (messageResponse) => {
          clearTimeout(timeout);
          resolveResponse(messageResponse);
        },
        reject: (error) => {
          clearTimeout(timeout);
          reject(error);
        },
      });
    });
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
    return responsePromise;
  }

  waitForNotification(method, predicate = () => true, timeoutMs = 30_000) {
    const existing = this.notifications.find((message) => {
      try {
        return message.method === method && predicate(message);
      } catch {
        return false;
      }
    });
    if (existing) return Promise.resolve(existing);

    return new Promise((resolveNotification, reject) => {
      const waiterEntry = { method, predicate, resolve: null, reject: null };
      const timeout = setTimeout(() => {
        this.notificationWaiters = this.notificationWaiters.filter((waiter) => waiter !== waiterEntry);
        reject(new Error(`timeout waiting for ${method} notification after ${timeoutMs}ms`));
      }, timeoutMs);
      waiterEntry.resolve = (message) => {
        clearTimeout(timeout);
        resolveNotification(message);
      };
      waiterEntry.reject = (error) => {
        clearTimeout(timeout);
        reject(error);
      };
      this.notificationWaiters.push(waiterEntry);
    });
  }

  resolveNotificationWaiters(message) {
    const remaining = [];
    for (const waiter of this.notificationWaiters) {
      let matches = false;
      try {
        matches = message.method === waiter.method && waiter.predicate(message);
      } catch (error) {
        waiter.reject(error);
        continue;
      }
      if (matches) {
        waiter.resolve(message);
      } else {
        remaining.push(waiter);
      }
    }
    this.notificationWaiters = remaining;
  }

  async stop() {
    if (!this.child) return;
    const child = this.child;
    if (child.exitCode === null && child.signalCode === null) {
      child.stdin.end();
      await new Promise((resolveStop) => {
        const timeout = setTimeout(() => {
          child.kill('SIGTERM');
          setTimeout(() => child.kill('SIGKILL'), 2_000).unref();
          resolveStop();
        }, 1_000);
        child.once('close', () => {
          clearTimeout(timeout);
          resolveStop();
        });
      });
    }
  }
}

function initializeParams() {
  return {
    clientInfo: {
      name: 'onecontext-codex-adapter-live-mail-flow',
      title: '1Context Codex Adapter Live Mail Flow',
      version: '0.1.0-live-mail-flow',
    },
    capabilities: {
      experimentalApi: true,
    },
  };
}

function threadStartParams() {
  return {
    cwd: repoRoot,
    model,
    approvalPolicy: 'never',
    sandbox: 'workspace-write',
    ephemeral: true,
    baseInstructions: 'You are a live 1Context Codex mail dogfood worker.',
    developerInstructions:
      'Keep output short. Treat steering as a notification only; open mail before acting. Do not mutate files unless explicitly asked.',
    config: {
      onecontext_adapter_live_mail_flow: true,
    },
    persistExtendedHistory: true,
  };
}

function extractThreadId(response) {
  const result = response?.result || {};
  return result.threadId || result.thread_id || result.id || result.thread?.threadId || result.thread?.id || null;
}

function extractTurnId(response) {
  const result = response?.result || {};
  return result.turnId || result.turn_id || result.id || result.turn?.turnId || result.turn?.id || null;
}

function assertNotification(notification, { agent, deliveryId, messageId }) {
  assertInvariant(notification.agent_id === agent.agent_id, 'notification_agent_mismatch', JSON.stringify(notification));
  assertInvariant(notification.delivery_id === deliveryId, 'notification_delivery_mismatch', JSON.stringify(notification));
  assertInvariant(notification.message_id === messageId, 'notification_message_mismatch', JSON.stringify(notification));
  assertInvariant(!JSON.stringify(notification).includes(mailBody), 'notification_body_leak', JSON.stringify(notification));
}

function assertOpenInjection(opened, { agent, deliveryId }) {
  assertInvariant(opened.delivery?.delivery_id === deliveryId, 'open_wrong_delivery', JSON.stringify(opened));
  assertInvariant(opened.content_delivery?.transport === 'codex.thread.inject_items', 'open_wrong_transport', JSON.stringify(opened.content_delivery));
  assertInvariant(opened.content_delivery?.method === 'thread/inject_items', 'open_wrong_method', JSON.stringify(opened.content_delivery));
  assertInvariant(opened.content_delivery?.thread_id === agent.transport.thread_id, 'open_wrong_thread', JSON.stringify(opened.content_delivery));
  const item = opened.content_delivery.items?.[0];
  const text = item?.content?.[0]?.text || '';
  assertInvariant(item?.type === 'message', 'open_item_type', JSON.stringify(item));
  assertInvariant(item?.role === 'user', 'open_item_role', JSON.stringify(item));
  assertInvariant(text.includes('"kind": "1context.mail.opened"'), 'open_item_kind', text);
  assertInvariant(text.includes(deliveryId), 'open_item_delivery_id', text);
  assertInvariant(text.includes(mailBody), 'open_body_missing_from_injection', text);
}

async function writeDispatchHelper() {
  const source = `#!/usr/bin/env node
import { request } from 'node:http';

const url = process.argv[2];
if (!url) {
  console.error('missing dispatch bridge URL');
  process.exit(2);
}

let body = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
  body += chunk;
});
process.stdin.on('end', () => {
  const req = request(url, {
    method: 'POST',
    headers: {
      'content-type': 'text/plain; charset=utf-8',
      'content-length': Buffer.byteLength(body),
    },
  }, (res) => {
    let responseBody = '';
    res.setEncoding('utf8');
    res.on('data', (chunk) => {
      responseBody += chunk;
    });
    res.on('end', () => {
      if (res.statusCode >= 200 && res.statusCode < 300) {
        process.stdout.write(responseBody);
        process.exit(0);
      }
      process.stderr.write(responseBody || 'dispatch bridge rejected steering payload');
      process.exit(1);
    });
  });
  req.on('error', (error) => {
    console.error(error.message);
    process.exit(1);
  });
  req.end(body);
});
`;
  await writeFile(dispatchHelperPath, source, { mode: 0o755 });
}

async function startSteeringBridge({ rpc, threadId, expectedTurnId }) {
  const deliveries = [];
  let server;
  await new Promise((resolveListen, reject) => {
    server = createServer(async (req, res) => {
      try {
        if (req.method !== 'POST') {
          res.writeHead(405, { 'content-type': 'application/json' });
          res.end(JSON.stringify({ error: 'method_not_allowed' }));
          return;
        }
        let steeringText = '';
        req.setEncoding('utf8');
        req.on('data', (chunk) => {
          steeringText += chunk;
        });
        req.on('end', async () => {
          try {
            redactions.add(steeringText);
            summary.redaction.steering_text_sha256 = sha256(steeringText);
            const response = await rpc.send(
              'turn/steer',
              {
                threadId,
                expectedTurnId,
                input: [{ type: 'text', text: steeringText }],
              },
              'mail-turn-steer-1',
              120_000
            );
            if (response.error) {
              deliveries.push({ status: 'failed', steering_text_sha256: sha256(steeringText), error: response.error });
              res.writeHead(502, { 'content-type': 'application/json' });
              res.end(JSON.stringify({ status: 'failed', error: response.error }));
              return;
            }
            deliveries.push({
              status: 'passed',
              steering_text_sha256: sha256(steeringText),
              response_sha256: sha256(JSON.stringify(response)),
            });
            res.writeHead(200, { 'content-type': 'application/json' });
            res.end(JSON.stringify({ status: 'ok', turn_id: response.result?.turnId || response.result?.turn_id || null }));
          } catch (error) {
            deliveries.push({ status: 'failed', error: error.message });
            res.writeHead(500, { 'content-type': 'application/json' });
            res.end(JSON.stringify({ status: 'failed', error: error.message }));
          }
        });
      } catch (error) {
        res.writeHead(500, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ status: 'failed', error: error.message }));
      }
    });
    server.on('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  const { port } = server.address();
  return {
    url: `http://127.0.0.1:${port}/dispatch`,
    deliveries,
    close: () =>
      new Promise((resolveClose, rejectClose) => {
        server.close((error) => (error ? rejectClose(error) : resolveClose()));
      }),
  };
}

async function writeSummary(status = summary.status) {
  summary.status = status;
  const serialized = JSON.stringify(summary);
  summary.redaction.mail_body_persisted_in_summary = serialized.includes(mailBody);
  summary.redaction.background_prompt_persisted_in_summary = serialized.includes(liveTurnPrompt);
  summary.redaction.steering_text_persisted_in_summary = [...redactions].some(
    (secret) => secret && secret !== mailBody && secret !== liveTurnPrompt && serialized.includes(secret)
  );
  await writeFile(proofSummaryPath, JSON.stringify(redactForLog(summary), null, 2) + '\n');
}

async function main() {
  await mkdir(evidenceDir, { recursive: true });
  if (!keepRuntime) await rm(devHome, { recursive: true, force: true });
  await mkdir(runtimeRoot, { recursive: true });
  await mkdir(devHome, { recursive: true });
  await writeFile(mailBodyPath, mailBody);
  await writeDispatchHelper();
  await maybeBuildWiki();

  const rpc = new AppServerRpc(codexBin, ['app-server', '--listen', 'stdio://']);
  let steeringBridge = null;
  let initializeResponse = null;
  let threadStartResponse = null;
  let threadId = null;
  let turnId = null;
  let agent = null;
  let deliveryId = null;
  let messageId = null;

  try {
    await wiki(['ensure']);
    summary.phases.wiki_ensure = { status: 'passed' };

    await rpc.start();
    summary.phases.spawn_app_server = { status: 'passed' };

    initializeResponse = await rpc.send('initialize', initializeParams(), 'initialize-1', 30_000);
    assertInvariant(!initializeResponse.error, 'initialize_json_rpc_error', JSON.stringify(initializeResponse.error));
    summary.phases.initialize = { status: 'passed' };

    threadStartResponse = await rpc.send('thread/start', threadStartParams(), 'thread-start-1', 30_000);
    assertInvariant(!threadStartResponse.error, 'thread_start_json_rpc_error', JSON.stringify(threadStartResponse.error));
    threadId = extractThreadId(threadStartResponse);
    assertInvariant(threadId, 'thread_start_missing_thread_id', JSON.stringify(threadStartResponse));
    summary.phases.thread_start = { status: 'passed', thread_id_present: true };

    const identify = await wiki([
      'agent-identify',
      '--thread-id',
      threadId,
      '--role',
      roleAddress,
      '--capability',
      'wiki.mail',
      '--ttl-seconds',
      '600',
    ]);
    agent = identify.agent;
    assertInvariant(agent?.agent_id, 'agent_identify_missing_agent', JSON.stringify(identify));
    summary.phases.agent_identify = { status: 'passed', agent_id: agent.agent_id, role: roleAddress };

    const sent = await wiki([
      'mail-send',
      '--from',
      'agent://codex/live-mail-supervisor',
      '--to',
      roleAddress,
      '--subject',
      mailSubject,
      '--body-file',
      mailBodyPath,
      '--kind',
      'message',
      '--idempotency-key',
      `live-mail-flow-${shortStamp}`,
    ]);
    assertInvariant(
      sent.delivery_attempt_count === 1 && sent.delivery_attempts?.[0]?.status === 'delivered',
      'mail_not_delivered',
      JSON.stringify(sent)
    );
    deliveryId = sent.delivery_attempts[0].delivery_id;
    messageId = sent.message?.message_id;
    assertInvariant(messageId, 'mail_send_missing_message_id', JSON.stringify(sent.message));
    summary.phases.mail_send = { status: 'passed', delivery_id: deliveryId, message_id: messageId };

    const poll = await wiki(['notify-poll', agent.agent_id]);
    assertInvariant(poll.notification_count === 1, 'notify_poll_count', JSON.stringify(poll));
    const notification = poll.notifications[0];
    assertNotification(notification, { agent, deliveryId, messageId });
    summary.phases.notify_poll = { status: 'passed', notification_id: notification.notification_id };

    const turnStartResponse = await rpc.send(
      'turn/start',
      {
        threadId,
        effort: 'low',
        input: [{ type: 'text', text: liveTurnPrompt }],
      },
      'background-turn-start-1',
      120_000
    );
    assertInvariant(!turnStartResponse.error, 'background_turn_start_error', JSON.stringify(turnStartResponse.error));
    turnId = extractTurnId(turnStartResponse);
    assertInvariant(turnId, 'background_turn_missing_turn_id', JSON.stringify(turnStartResponse));
    await rpc.waitForNotification(
      'turn/started',
      (message) => message.params?.threadId === threadId && message.params?.turn?.id === turnId,
      60_000
    );
    summary.phases.background_turn_start = { status: 'passed', turn_id_present: true, turn_started_notification: true };

    steeringBridge = await startSteeringBridge({ rpc, threadId, expectedTurnId: turnId });
    const dispatch = await wiki([
      'notify-dispatch',
      agent.agent_id,
      '--steering-command',
      process.execPath,
      '--steering-arg',
      dispatchHelperPath,
      '--steering-arg',
      steeringBridge.url,
      '--payload-format',
      'text',
      '--limit',
      '1',
    ], { timeoutMs: 120_000 });
    assertInvariant(dispatch.attempt_count === 1, 'dispatch_attempt_count', JSON.stringify(dispatch));
    assertInvariant(dispatch.attempts[0].status === 'sent', 'dispatch_not_sent', JSON.stringify(dispatch.attempts[0]));
    assertInvariant(steeringBridge.deliveries[0]?.status === 'passed', 'live_turn_steer_not_observed', JSON.stringify(steeringBridge.deliveries));
    assertInvariant(!JSON.stringify(dispatch).includes(mailBody), 'dispatch_body_leak', JSON.stringify(dispatch));
    summary.phases.notify_dispatch_to_live_codex = {
      status: 'passed',
      attempt_id: dispatch.attempts[0].attempt_id,
      steering_text_sha256: steeringBridge.deliveries[0].steering_text_sha256,
      turn_steer_response_sha256: steeringBridge.deliveries[0].response_sha256,
    };

    const opened = await wiki(['mail-open', deliveryId, '--agent-id', agent.agent_id]);
    assertOpenInjection(opened, { agent, deliveryId });
    summary.phases.mail_open = {
      status: 'passed',
      content_delivery_method: opened.content_delivery.method,
      item_count: opened.content_delivery.items.length,
    };

    const injectResponse = await rpc.send(
      'thread/inject_items',
      {
        threadId,
        items: opened.content_delivery.items,
      },
      'mail-thread-inject-items-1',
      30_000
    );
    assertInvariant(!injectResponse.error, 'thread_inject_items_error', JSON.stringify(injectResponse.error));
    summary.phases.thread_inject_items = { status: 'passed', response_sha256: sha256(JSON.stringify(injectResponse)) };

    const recordInjection = await wiki([
      'mail-record-injection',
      deliveryId,
      '--agent-id',
      agent.agent_id,
      '--thread-id',
      threadId,
      '--result',
      'ok',
      '--item-count',
      String(opened.content_delivery.items.length),
    ]);
    assertInvariant(recordInjection.receipt?.app_server_result === 'ok', 'record_injection_failed', JSON.stringify(recordInjection));
    summary.phases.mail_record_injection = { status: 'passed', injection_id: recordInjection.receipt.injection_id };

    const claimed = await wiki(['mail-claim', deliveryId, '--agent-id', agent.agent_id]);
    assertInvariant(claimed.delivery?.state === 'claimed', 'mail_claim_failed', JSON.stringify(claimed));
    const marked = await wiki(['mail-mark', deliveryId, '--agent-id', agent.agent_id, '--state', 'done']);
    assertInvariant(marked.delivery?.state === 'done', 'mail_mark_done_failed', JSON.stringify(marked));
    const ack = await wiki(['notify-ack', notification.notification_id, '--agent-id', agent.agent_id]);
    assertInvariant(ack.notification?.state === 'acknowledged', 'notify_ack_failed', JSON.stringify(ack));
    const afterAck = await wiki(['notify-poll', agent.agent_id]);
    assertInvariant(afterAck.notification_count === 0, 'notify_ack_did_not_clear', JSON.stringify(afterAck));
    summary.phases.mail_complete = { status: 'passed', delivery_state: 'done', notification_state: 'acknowledged' };

    const statusByThread = await wiki(['agent-status-by-thread', threadId]);
    assertInvariant(statusByThread.agent_id === agent.agent_id, 'status_by_thread_agent_mismatch', JSON.stringify(statusByThread));
    assertInvariant(statusByThread.pending_notification_count === 0, 'status_by_thread_pending_not_clear', JSON.stringify(statusByThread));
    summary.phases.agent_status_by_thread = { status: 'passed', lease_state: statusByThread.lease_state, pending_notification_count: 0 };

    summary.mail_flow = {
      thread_id: threadId,
      turn_id: turnId,
      agent_id: agent.agent_id,
      role: roleAddress,
      delivery_id: deliveryId,
      message_id: messageId,
      notification_id: notification.notification_id,
      dispatch_attempt_id: dispatch.attempts[0].attempt_id,
      injection_id: recordInjection.receipt.injection_id,
    };
    summary.transcript_summary = rpc.transcriptSummary;
    summary.assertions.notify_dispatch_used_real_turn_steer = 'passed';
    summary.assertions.mail_open_body_injected_via_thread_inject_items = 'passed';
    summary.assertions.delivery_done_and_notification_acknowledged = 'passed';
    summary.assertions.redacted_summary = 'passed';

    const summaryText = JSON.stringify(summary);
    assertInvariant(!summaryText.includes(mailBody), 'mail_body_in_summary', 'proof summary would contain raw mail body');
    assertInvariant(!summaryText.includes(liveTurnPrompt), 'background_prompt_in_summary', 'proof summary would contain raw background prompt');
    for (const secret of redactions) {
      if (secret && secret !== mailBody && secret !== liveTurnPrompt) {
        assertInvariant(!summaryText.includes(secret), 'steering_text_in_summary', 'proof summary would contain raw steering text');
      }
    }

    await writeSummary('passed');
    console.log(`codex adapter live mail flow proof passed: ${proofSummaryPath}`);
  } finally {
    if (steeringBridge) await steeringBridge.close().catch(() => {});
    await rpc.stop();
  }
}

main().catch(async (error) => {
  summary.status = 'failed';
  summary.failure = {
    code: error.code || 'unexpected_error',
    message: error.message,
    detail: error.detail || null,
    repair_hint: error.repairHint || null,
  };
  try {
    await writeSummary('failed');
  } catch {}
  console.error(`${summary.failure.code}: ${summary.failure.message}`);
  console.error(`Evidence: ${proofSummaryPath}`);
  process.exit(1);
});
