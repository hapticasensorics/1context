#!/usr/bin/env node
import { appendFile, mkdir, readdir, readFile, stat, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const argv = process.argv.slice(2);
const args = new Set(argv);
const timestamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
const evidenceDir = resolve(
  process.env.ONECONTEXT_CODEX_ADAPTER_LIVE_DOGFOOD_DIR ||
    join(repoRoot, 'test-results', `codex-adapter-live-server-dogfood-${timestamp}`)
);
const requestDir = join(evidenceDir, 'requests');
const runtimeRoot = join(evidenceDir, 'runtime', '1Context');
const commandLog = join(evidenceDir, 'commands.jsonl');
const proofSummaryPath = join(evidenceDir, 'proof-summary.json');
const transcriptPath = join(evidenceDir, 'app-server-transcript.jsonl');
const appServerStderrPath = join(evidenceDir, 'app-server.stderr.log');
const codexBin = process.env.ONECONTEXT_CODEX_BIN || 'codex';
const model = process.env.ONECONTEXT_CODEX_ADAPTER_LIVE_MODEL || process.env.CODEX_MODEL || 'gpt-5.4-mini';
const harnessBin = resolve(
  process.env.ONECONTEXT_AGENT_HARNESS_BIN || join(repoRoot, 'target/debug/onecontext-agent-harness')
);
const adapterBin = resolve(
  process.env.ONECONTEXT_CODEX_ADAPTER_BIN || join(repoRoot, 'target/debug/onecontext-codex-adapter')
);
const modelTurnEnabled =
  !args.has('--skip-model-turn') && process.env.ONECONTEXT_CODEX_ADAPTER_SKIP_LIVE_TURN !== '1';
const threadOpsEnabled =
  !args.has('--skip-thread-ops') && process.env.ONECONTEXT_CODEX_ADAPTER_LIVE_THREAD_OPS !== '0';
const fixedTurnId = 'turn-codex-live-child-001';
const liveTurnPrompt =
  'Live adapter dogfood: use the shell command tool to run exactly `sleep 5`, then return exactly LIVE_CODEX_ADAPTER_DOGFOOD_OK.';
const injectedContext = 'ONECONTEXT_CODEX_ADAPTER_LIVE_INJECTED_CONTEXT';

const summary = {
  schema_version: 1,
  status: 'running',
  proof: 'codex-adapter-live-server-dogfood',
  generated_at: new Date().toISOString(),
  evidence_dir: evidenceDir,
  runtime_root: runtimeRoot,
  codex_bin: codexBin,
  model,
  gates: {
    thread_ops_enabled: threadOpsEnabled,
    model_turn_enabled: modelTurnEnabled,
    model_turn_default: true,
    model_turn_skip: 'ONECONTEXT_CODEX_ADAPTER_SKIP_LIVE_TURN=1 or --skip-model-turn',
  },
  phases: {},
  assertions: {},
  commands: [],
  artifacts: {
    proof_summary: proofSummaryPath,
    command_log: commandLog,
    transcript_jsonl: transcriptPath,
    app_server_stderr: appServerStderrPath,
  },
  redaction: {
    raw_prompt_sha256: sha256(liveTurnPrompt),
    injected_context_sha256: sha256(injectedContext),
    raw_prompt_persisted_in_summary: false,
    injected_context_persisted_in_summary: false,
  },
};

function usage() {
  console.log(`Usage: node scripts/test-codex-adapter-live-server-dogfood.mjs [--skip-model-turn] [--skip-thread-ops]

Starts a real Codex app-server over stdio, records newline-delimited JSON-RPC,
and bridges redacted live evidence into the 1Context agent harness.

Environment:
  ONECONTEXT_CODEX_BIN                         Codex CLI binary (default: codex)
  ONECONTEXT_CODEX_ADAPTER_BIN                 adapter CLI binary
  ONECONTEXT_AGENT_HARNESS_BIN                 harness CLI binary
  ONECONTEXT_CODEX_ADAPTER_LIVE_DOGFOOD_DIR    evidence output directory
  ONECONTEXT_CODEX_ADAPTER_LIVE_MODEL          thread/start model
  ONECONTEXT_CODEX_ADAPTER_LIVE_THREAD_OPS=0   skip thread/start and loaded/list
  ONECONTEXT_CODEX_ADAPTER_SKIP_LIVE_TURN=1    skip turn/start and turn/steer
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
    return value
      .replaceAll(liveTurnPrompt, '[redacted-live-turn-prompt]')
      .replaceAll(injectedContext, '[redacted-live-injected-context]');
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
    fail(
      'invalid_json',
      `${label} did not emit JSON: ${error.message}. stdout=${JSON.stringify(stdout.slice(0, 300))}`,
      'Make the CLI emit one JSON object on stdout for this dogfood runner.'
    );
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
    args: redacted.args,
    code: redacted.code,
    status: redacted.status,
    ms: redacted.ms,
  });
  await appendFile(commandLog, JSON.stringify(redacted) + '\n');
}

async function requestFile(label, payload) {
  const path = join(requestDir, `${String(summary.commands.length + 1).padStart(2, '0')}-${label}.json`);
  await writeFile(path, JSON.stringify(payload, null, 2) + '\n');
  return path;
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
    ms: entry.ms,
    stderr: entry.stderr,
    error: json.error || null,
  });
  if (entry.code !== 0 || json.status !== 'ok') {
    fail('harness_command_failed', `${commandArgs.join(' ')} failed: ${JSON.stringify(json.error || json)}`);
  }
  return json;
}

async function adapter(subcommand, commandArgs = [], request = null) {
  const requestPath = request ? await requestFile(`adapter-${subcommand}`, request) : null;
  const useBinary = existsSync(adapterBin);
  const command = useBinary ? adapterBin : 'cargo';
  const bridgeArgs = [subcommand, ...commandArgs];
  const loggedArgs = [...bridgeArgs];
  if (request) {
    bridgeArgs.push('--request-json', JSON.stringify(request));
    loggedArgs.push('--request-json', `@${requestPath}`);
  }
  const fullArgs = useBinary ? bridgeArgs : ['run', '-q', '-p', 'onecontext-codex-adapter', '--', ...bridgeArgs];

  const entry = await run(command, fullArgs);
  const json = parseJson(entry.stdout, `onecontext-codex-adapter ${subcommand}`);
  await logCommand({
    tool: 'onecontext-codex-adapter',
    via: useBinary ? 'binary' : 'cargo-run',
    args: loggedArgs,
    request_file: requestPath,
    code: entry.code,
    status: entry.code === 0 ? json.status || 'ok' : json.status || 'error',
    ms: entry.ms,
    stderr: entry.stderr,
    error: json.error || null,
  });
  if (entry.code !== 0) {
    fail('adapter_command_failed', `${subcommand} failed: ${JSON.stringify(json.error || json)}`);
  }
  return json;
}

async function liveServerPlan() {
  const planArgs = [
    '--evidence-dir',
    evidenceDir,
    '--runtime-root',
    runtimeRoot,
    '--codex-bin',
    codexBin,
  ];
  if (!modelTurnEnabled) planArgs.push('--skip-model-turns');
  return adapter('live-server-plan', planArgs);
}

async function runSchemaCommand(plan) {
  const command = plan.schema_command?.[0] || codexBin;
  const commandArgs = plan.schema_command?.slice(1) || [
    'app-server',
    'generate-json-schema',
    '--experimental',
    '--out',
    join(evidenceDir, 'generated-schemas'),
  ];
  const entry = await run(command, commandArgs, { timeoutMs: 120_000 });
  await logCommand({
    tool: 'codex',
    phase: 'generate_schema',
    args: [command, ...commandArgs],
    code: entry.code,
    status: entry.code === 0 ? 'ok' : 'error',
    ms: entry.ms,
    stdout_sha256: sha256(entry.stdout),
    stderr: entry.stderr,
    timed_out: entry.timed_out,
  });
  if (entry.code !== 0) {
    fail(
      'schema_generation_failed',
      `codex app-server generate-json-schema exited ${entry.code}: ${entry.stderr.trim() || entry.stdout.trim()}`,
      'Run the same schema command from commands.jsonl and confirm the installed Codex CLI supports app-server schema generation.'
    );
  }
}

async function listFilesRecursive(root) {
  const out = [];
  async function walk(dir) {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(path);
      } else if (entry.isFile()) {
        out.push(path);
      }
    }
  }
  await walk(root);
  return out.sort();
}

async function schemaEvidence(schemaDir, requiredMethods) {
  const files = await listFilesRecursive(schemaDir);
  const jsonFiles = files.filter((file) => file.endsWith('.json'));
  const methodRefs = Object.fromEntries(requiredMethods.map((method) => [method, []]));
  for (const file of jsonFiles) {
    const content = await readFile(file, 'utf8');
    for (const method of requiredMethods) {
      if (content.includes(method)) {
        methodRefs[method].push(relative(evidenceDir, file));
      }
    }
  }
  const stats = await Promise.all(files.map((file) => stat(file)));
  const byteCount = stats.reduce((sum, item) => sum + item.size, 0);
  return {
    schema_dir: schemaDir,
    file_count: files.length,
    json_file_count: jsonFiles.length,
    total_bytes: byteCount,
    required_methods: requiredMethods,
    method_refs: Object.fromEntries(
      Object.entries(methodRefs).map(([method, refs]) => [method, refs.slice(0, 5)])
    ),
    methods_observed: Object.fromEntries(
      Object.entries(methodRefs).map(([method, refs]) => [method, refs.length > 0])
    ),
  };
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
    this.startedAt = Date.now();
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
      const payload = {
        ts: new Date().toISOString(),
        direction: 'process',
        event: 'exit',
        code,
        signal,
      };
      appendFile(transcriptPath, JSON.stringify(payload) + '\n').catch(() => {});
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
    const requestId = id || `live-dogfood-${this.nextId++}`;
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
      const timeout = setTimeout(() => {
        this.notificationWaiters = this.notificationWaiters.filter((waiter) => waiter !== waiterEntry);
        reject(new Error(`timeout waiting for ${method} notification after ${timeoutMs}ms`));
      }, timeoutMs);
      const waiterEntry = {
        method,
        predicate,
        resolve: (message) => {
          clearTimeout(timeout);
          resolveNotification(message);
        },
        reject: (error) => {
          clearTimeout(timeout);
          reject(error);
        },
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
      name: 'onecontext-codex-adapter-live-dogfood',
      title: '1Context Codex Adapter Live Dogfood',
      version: '0.1.0-live-dogfood',
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
    baseInstructions: 'You are a live 1Context Codex adapter dogfood worker.',
    developerInstructions:
      'Keep output short. Do not mutate files unless explicitly asked by the live dogfood turn.',
    config: {
      onecontext_adapter_dogfood: true,
    },
    persistExtendedHistory: true,
  };
}

function extractThreadId(response) {
  const result = response?.result || {};
  return (
    result.threadId ||
    result.thread_id ||
    result.id ||
    result.thread?.threadId ||
    result.thread?.id ||
    response?.params?.threadId ||
    null
  );
}

function extractTurnId(response) {
  const result = response?.result || {};
  return result.turnId || result.turn_id || result.id || result.turn?.turnId || result.turn?.id || null;
}

function extractUnit(payload, label) {
  const unit =
    (payload?.unit_id && payload?.certificate ? payload : null) ||
    payload.unit ||
    payload.child_unit ||
    payload.agent_unit ||
    payload.result?.unit ||
    payload.result?.child_unit ||
    payload.harness?.unit;
  assertInvariant(unit && typeof unit === 'object', 'missing_unit_payload', `${label} did not return a harness unit`);
  return unit;
}

function idValue(value) {
  return typeof value === 'string' ? value : value?.[0] || value?.id || value?.unit_id || value?.value;
}

function capabilityBinding(id, transport, toolNames, proofRequired) {
  return {
    id,
    transport,
    tool_names: toolNames,
    config: {
      external_source: `codex-app-server://live-dogfood/${id}`,
      harness_owns_content: false,
    },
    policy: {
      declared_by: 'codex-adapter-live-server-dogfood',
      inherited_from_parent: false,
    },
    proof_required: proofRequired,
  };
}

function parentBirthRequest() {
  return {
    unit_id: 'agent-codex-live-parent-001',
    role: 'codex adapter live server dogfood parent',
    model,
    identity: {
      display_name: 'Codex Adapter Live Server Dogfood Parent',
    },
    instructions: {
      system: 'Spawn governed Codex children only through the adapter harness boundary.',
    },
    runtime: {
      adapter: 'codex_app_server',
      listen_url: 'stdio://',
    },
    capabilities: [
      capabilityBinding('codex-live-adapter-harness-bridge', 'local_test', ['harness.spawn_child'], []),
    ],
    visibility: 'private',
    metadata: {
      title: 'Codex adapter live server dogfood parent',
      owner: 'codex-adapter-live-server-dogfood',
      tags: ['dogfood', 'codex-adapter', 'live-server', 'parent'],
    },
  };
}

function childSpawnRequest(requiredProof) {
  return {
    parent_unit_id: 'agent-codex-live-parent-001',
    unit_id: 'agent-codex-live-child-001',
    spawn_request_id: 'codex-adapter-live-spawn-child-001',
    role: 'governed live codex app-server child dogfood',
    model,
    identity: {
      display_name: 'Governed Live Codex App Server Child',
    },
    instructions: {
      system: 'Persist only redacted live app-server proof evidence through the 1Context harness.',
    },
    runtime: {
      adapter: 'codex_app_server',
      listen_url: 'stdio://',
      transcript_jsonl: transcriptPath,
    },
    capabilities: [
      capabilityBinding(
        'codex-live-app-server-thread-bridge',
        'codex_app_server_dynamic_tool',
        ['initialize', 'thread/start', 'thread/loaded/list', 'thread/inject_items', 'turn/start', 'turn/steer'],
        requiredProof
      ),
    ],
    visibility: 'private',
    metadata: {
      title: 'Governed Codex child from live app-server dogfood',
      owner: 'codex-adapter-live-server-dogfood',
      tags: ['dogfood', 'codex-adapter', 'live-server', 'child'],
    },
  };
}

function proofPlan({ family, kind, summaryText, unitId, eventKind, status, correlation, evidence }) {
  return {
    target: 'agent_harness',
    family,
    kind,
    summary: summaryText,
    redacted: true,
    policy: {
      allowed: true,
      reason: 'allowed',
      message: 'live dogfood proof contains redacted evidence only',
    },
    harness_request: {
      unit_id: unitId,
      adapter: 'codex_app_server',
      kind: eventKind,
      status,
      correlation,
      evidence,
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    created_at: '2026-05-25T00:00:00Z',
  };
}

function liveProofEvents({
  childUnit,
  initializeResponse,
  schema,
  threadStart,
  loadedList,
  injectItems,
  turnStart,
  turnSteer,
}) {
  const generatedIds = childUnit.certificate?.generated_ids || {
    unit_id: childUnit.unit_id,
    certificate_id: childUnit.certificate?.certificate_id,
    session_id: childUnit.session_id,
  };
  const threadId = threadStart.thread_id || 'thread-not-started';
  const sessionId = childUnit.session_id || childUnit.certificate?.generated_ids?.session_id || 'session-not-emitted';
  const turnId = turnStart.turn_id || fixedTurnId;
  const methodVisibility = Object.entries(schema.methods_observed || {}).map(([method, observed]) => ({
    method,
    observed,
    schema_refs: schema.method_refs?.[method] || [],
  }));
  const events = [
    proofPlan({
      family: 'event_mirror',
      kind: 'codex.live.transport_identity_observed',
      summaryText: 'live Codex app-server stdio transport initialized and harness child identity correlated',
      unitId: childUnit.unit_id,
      eventKind: 'transport_identity_observed',
      status: 'observed',
      correlation: {
        thread_id: threadId,
        session_id: idValue(sessionId),
        turn_id: turnId,
        expected_turn_id: turnId,
        transport_attempt_id: 'codex-live-app-server-stdio-001',
      },
      evidence: {
        generated_ids: generatedIds,
        listen_url: 'stdio://',
        initialize_result_sha256: sha256(JSON.stringify(initializeResponse.result || {})),
        transcript_jsonl: relative(repoRoot, transcriptPath),
        redacted: true,
      },
    }),
    proofPlan({
      family: 'toolset_visibility',
      kind: 'codex.live.schema_method_visibility',
      summaryText: 'live Codex app-server generated schemas expose required adapter methods',
      unitId: childUnit.unit_id,
      eventKind: 'tool_allowlist_checked',
      status: Object.values(schema.methods_observed || {}).every(Boolean) ? 'accepted' : 'missing',
      correlation: {
        thread_id: threadId,
        session_id: idValue(sessionId),
        turn_id: turnId,
        transport_attempt_id: 'codex-live-schema-methods-001',
      },
      evidence: {
        schema_dir: relative(repoRoot, schema.schema_dir),
        json_file_count: schema.json_file_count,
        method_visibility: methodVisibility,
        hidden_host_tool_count: 0,
        redacted: true,
      },
    }),
  ];

  if (threadStart.status === 'passed') {
    events.push(
      proofPlan({
        family: 'event_mirror',
        kind: 'codex.live.thread_start_observed',
        summaryText: 'live Codex app-server thread/start completed without a model turn',
        unitId: childUnit.unit_id,
        eventKind: 'tool_call_observed',
        status: 'observed',
        correlation: {
          thread_id: threadId,
          session_id: idValue(sessionId),
          turn_id: turnId,
          transport_attempt_id: 'codex-live-thread-start-001',
        },
        evidence: {
          method: 'thread/start',
          response_sha256: sha256(JSON.stringify(threadStart.response || {})),
          thread_id_present: Boolean(threadStart.thread_id),
          redacted: true,
        },
      })
    );
  }

  if (loadedList.status === 'passed') {
    events.push(
      proofPlan({
        family: 'event_mirror',
        kind: 'codex.live.thread_loaded_list_observed',
        summaryText: 'live Codex app-server thread/loaded/list completed without a model turn',
        unitId: childUnit.unit_id,
        eventKind: 'agent_heartbeat_observed',
        status: 'observed',
        correlation: {
          thread_id: threadId,
          session_id: idValue(sessionId),
          turn_id: turnId,
          transport_attempt_id: 'codex-live-thread-loaded-list-001',
        },
        evidence: {
          method: 'thread/loaded/list',
          response_sha256: sha256(JSON.stringify(loadedList.response || {})),
          redacted: true,
        },
      })
    );
  }

  if (injectItems.status === 'passed') {
    events.push(
      proofPlan({
        family: 'injection',
        kind: 'codex.live.thread_inject_items_observed',
        summaryText: 'live Codex app-server thread/inject_items completed with redacted context',
        unitId: childUnit.unit_id,
        eventKind: 'context_injection_executed',
        status: 'accepted',
        correlation: {
          thread_id: threadId,
          session_id: idValue(sessionId),
          turn_id: turnId,
          transport_attempt_id: 'codex-live-thread-inject-items-001',
        },
        evidence: {
          method: 'thread/inject_items',
          response_sha256: sha256(JSON.stringify(injectItems.response || {})),
          injected_context_sha256: sha256(injectedContext),
          injected_context: '[redacted]',
          redacted: true,
        },
      })
    );
  }

  if (turnStart.status === 'passed' || turnSteer.status === 'passed') {
    events.push(
      proofPlan({
        family: 'wake',
        kind: 'codex.live.turn_steering_observed',
        summaryText: 'live Codex app-server turn and steering operations completed',
        unitId: childUnit.unit_id,
        eventKind: 'runtime_wakeup_accepted',
        status: 'accepted',
        correlation: {
          thread_id: threadId,
          session_id: idValue(sessionId),
          turn_id: turnId,
          expected_turn_id: turnId,
          transport_attempt_id: 'codex-live-turn-default-001',
        },
        evidence: {
          turn_start_status: turnStart.status,
          turn_steer_status: turnSteer.status,
          prompt_sha256: sha256(liveTurnPrompt),
          raw_prompt: '[redacted]',
          redacted: true,
        },
      })
    );
  }

  return events;
}

function assertParentBirth(parent) {
  assertInvariant(parent.unit_id === 'agent-codex-live-parent-001', 'parent_unit_id_mismatch', JSON.stringify(parent.unit_id));
  assertInvariant(
    ['born', 'ready', 'Born', 'Ready'].includes(parent.lifecycle_state),
    'parent_not_born',
    JSON.stringify(parent.lifecycle_state)
  );
  summary.assertions.parent_unit_birth = 'passed';
}

function assertChildLineage(child) {
  const lineage = child.certificate?.lineage || {};
  assertInvariant(child.unit_id === 'agent-codex-live-child-001', 'child_unit_id_mismatch', JSON.stringify(child.unit_id));
  assertInvariant(
    idValue(lineage.parent_unit_id) === 'agent-codex-live-parent-001',
    'child_parent_lineage_mismatch',
    JSON.stringify(lineage)
  );
  assertInvariant(
    lineage.spawn_request_id === 'codex-adapter-live-spawn-child-001',
    'child_spawn_request_lineage_mismatch',
    JSON.stringify(lineage)
  );
  summary.assertions.child_spawn_via_adapter_bridge = 'passed';
}

async function writeSummary(status = summary.status) {
  summary.status = status;
  summary.redaction.raw_prompt_persisted_in_summary = JSON.stringify(summary).includes(liveTurnPrompt);
  summary.redaction.injected_context_persisted_in_summary = JSON.stringify(summary).includes(injectedContext);
  await writeFile(proofSummaryPath, JSON.stringify(redactForLog(summary), null, 2) + '\n');
}

async function main() {
  await mkdir(requestDir, { recursive: true });
  await mkdir(runtimeRoot, { recursive: true });

  const plan = await liveServerPlan();
  summary.plan = plan;
  assertInvariant(
    plan.execution_policy?.allow_model_consuming_turns === modelTurnEnabled,
    'model_turn_plan_mismatch',
    `runner modelTurnEnabled=${modelTurnEnabled}, plan allow_model_consuming_turns=${JSON.stringify(plan.execution_policy?.allow_model_consuming_turns)}`
  );
  summary.assertions.model_turn_plan_matches_runner = 'passed';
  summary.artifacts.schema_dir = plan.artifacts?.schema_dir || join(evidenceDir, 'generated-schemas');
  summary.phases.live_server_plan = { status: 'passed' };

  await runSchemaCommand(plan);
  const schema = await schemaEvidence(summary.artifacts.schema_dir, plan.required_methods || []);
  summary.schema = schema;
  summary.phases.generate_schema = { status: 'passed' };
  assertInvariant(schema.json_file_count > 0, 'schema_files_missing', `no JSON schema files found in ${summary.artifacts.schema_dir}`);
  summary.assertions.schema_artifacts_written = 'passed';
  summary.assertions.required_methods_visible = Object.values(schema.methods_observed).every(Boolean) ? 'passed' : 'failed';
  assertInvariant(
    summary.assertions.required_methods_visible === 'passed',
    'required_method_schema_missing',
    JSON.stringify(schema.methods_observed)
  );

  const appServerCommand = plan.codex_command?.[0] || codexBin;
  const appServerArgs = plan.codex_command?.slice(1) || ['app-server', '--listen', 'stdio://'];
  const rpc = new AppServerRpc(appServerCommand, appServerArgs);
  let initializeResponse;
  const threadStart = { status: 'skipped', reason: 'thread ops disabled' };
  const loadedList = { status: 'skipped', reason: 'thread ops disabled' };
  const turnStart = {
    status: 'skipped',
    reason: 'model-turn operations skipped by ONECONTEXT_CODEX_ADAPTER_SKIP_LIVE_TURN=1 or --skip-model-turn',
  };
  const turnSteer = {
    status: 'skipped',
    reason: 'model-turn operations skipped by ONECONTEXT_CODEX_ADAPTER_SKIP_LIVE_TURN=1 or --skip-model-turn',
  };
  const injectItems = {
    status: 'skipped',
    reason: 'thread/inject_items requires a live thread id from thread/start',
  };

  try {
    await rpc.start();
    await logCommand({
      tool: 'codex',
      phase: 'spawn_app_server',
      args: [appServerCommand, ...appServerArgs],
      code: 0,
      status: 'spawned',
      ms: 0,
    });
    summary.phases.spawn_app_server = { status: 'passed' };

    initializeResponse = await rpc.send('initialize', initializeParams(), 'initialize-1', 30_000);
    summary.initialize = {
      response_sha256: sha256(JSON.stringify(initializeResponse)),
      has_result: initializeResponse.result !== undefined,
      has_error: initializeResponse.error !== undefined,
    };
    summary.phases.initialize = initializeResponse.error ? { status: 'failed', error: initializeResponse.error } : { status: 'passed' };
    assertInvariant(!initializeResponse.error, 'initialize_json_rpc_error', JSON.stringify(initializeResponse.error));
    summary.assertions.initialize_responded = 'passed';

    if (threadOpsEnabled) {
      const response = await rpc.send('thread/start', threadStartParams(), 'thread-start-1', 30_000);
      if (response.error) {
        threadStart.status = 'failed';
        threadStart.error = response.error;
        summary.phases.thread_start = { status: 'failed', error: response.error };
      } else {
        threadStart.status = 'passed';
        threadStart.response = response;
        threadStart.thread_id = extractThreadId(response);
        summary.phases.thread_start = {
          status: 'passed',
          thread_id_present: Boolean(threadStart.thread_id),
        };
      }

      const listResponse = await rpc.send('thread/loaded/list', { limit: 10 }, 'thread-loaded-list-1', 30_000);
      if (listResponse.error) {
        loadedList.status = 'failed';
        loadedList.error = listResponse.error;
        summary.phases.thread_loaded_list = { status: 'failed', error: listResponse.error };
      } else {
        loadedList.status = 'passed';
        loadedList.response = listResponse;
        summary.phases.thread_loaded_list = { status: 'passed' };
      }

      if (threadStart.thread_id) {
        const injectResponse = await rpc.send(
          'thread/inject_items',
          {
            threadId: threadStart.thread_id,
            items: [
              {
                type: 'message',
                role: 'user',
                content: [
                  {
                    type: 'input_text',
                    text: injectedContext,
                  },
                ],
              },
            ],
          },
          'thread-inject-items-1',
          30_000
        );
        if (injectResponse.error) {
          injectItems.status = 'failed';
          injectItems.error = injectResponse.error;
          summary.phases.thread_inject_items = { status: 'failed', error: injectResponse.error };
        } else {
          injectItems.status = 'passed';
          injectItems.response = injectResponse;
          summary.phases.thread_inject_items = { status: 'passed' };
        }
      } else {
        injectItems.reason = 'thread/start did not return a thread id';
        summary.phases.thread_inject_items = injectItems;
      }
    } else {
      summary.phases.thread_start = threadStart;
      summary.phases.thread_loaded_list = loadedList;
      summary.phases.thread_inject_items = injectItems;
    }

    if (modelTurnEnabled && threadStart.thread_id) {
      const response = await rpc.send(
        'turn/start',
        {
          threadId: threadStart.thread_id,
          effort: 'low',
          input: [{ type: 'text', text: liveTurnPrompt }],
        },
        'turn-start-1',
        120_000
      );
      if (response.error) {
        turnStart.status = 'failed';
        turnStart.error = response.error;
      } else {
        turnStart.status = 'passed';
        turnStart.response = response;
        turnStart.turn_id = extractTurnId(response) || fixedTurnId;
        const startedNotification = await rpc.waitForNotification(
          'turn/started',
          (message) =>
            message.params?.threadId === threadStart.thread_id &&
            message.params?.turn?.id === turnStart.turn_id,
          60_000
        );
        turnStart.started_notification_sha256 = sha256(JSON.stringify(startedNotification));
      }
      summary.phases.turn_start = {
        status: turnStart.status,
        turn_id_present: Boolean(turnStart.turn_id),
        turn_started_notification: Boolean(turnStart.started_notification_sha256),
        error: turnStart.error || null,
      };

      if (turnStart.status === 'passed') {
        const steerResponse = await rpc.send(
          'turn/steer',
          {
            threadId: threadStart.thread_id,
            expectedTurnId: turnStart.turn_id || fixedTurnId,
            input: [{ type: 'text', text: liveTurnPrompt }],
          },
          'turn-steer-1',
          120_000
        );
        if (steerResponse.error) {
          turnSteer.status = 'failed';
          turnSteer.error = steerResponse.error;
        } else {
          turnSteer.status = 'passed';
          turnSteer.response = steerResponse;
        }
      } else {
        turnSteer.status = 'skipped';
        turnSteer.reason = 'turn/start did not pass';
      }
      summary.phases.turn_steer = {
        status: turnSteer.status,
        error: turnSteer.error || null,
      };
    } else {
      if (modelTurnEnabled && !threadStart.thread_id) {
        turnStart.reason = 'model turn enabled, but no thread id was returned by thread/start';
        turnSteer.reason = turnStart.reason;
      }
      summary.phases.turn_start = turnStart;
      summary.phases.turn_steer = turnSteer;
    }
    summary.transcript_summary = rpc.transcriptSummary;
  } finally {
    await rpc.stop();
  }

  assertInvariant(initializeResponse?.result !== undefined, 'initialize_result_missing', 'initialize did not return result');
  summary.assertions.transcript_jsonl_written = existsSync(transcriptPath) ? 'passed' : 'failed';
  if (threadOpsEnabled) {
    assertInvariant(threadStart.status === 'passed', 'thread_start_failed', JSON.stringify(threadStart.error || threadStart));
    assertInvariant(threadStart.thread_id, 'thread_start_missing_thread_id', JSON.stringify(threadStart.response || threadStart));
    assertInvariant(loadedList.status === 'passed', 'thread_loaded_list_failed', JSON.stringify(loadedList.error || loadedList));
    assertInvariant(injectItems.status === 'passed', 'thread_inject_items_failed', JSON.stringify(injectItems.error || injectItems));
  }
  if (modelTurnEnabled) {
    assertInvariant(turnStart.status === 'passed', 'turn_start_failed', JSON.stringify(turnStart.error || turnStart));
    assertInvariant(turnSteer.status === 'passed', 'turn_steer_failed', JSON.stringify(turnSteer.error || turnSteer));
  }

  await harness(['ensure']);
  const parentBirth = await harness(['birth'], parentBirthRequest());
  const parentUnit = extractUnit(parentBirth, 'parent birth');
  assertParentBirth(parentUnit);
  summary.phases.harness_parent_birth = { status: 'passed' };

  const requiredProof = modelTurnEnabled
    ? ['transport_identity', 'context_injection', 'tool_conformance', 'steering']
    : ['transport_identity', 'context_injection', 'tool_conformance'];
  const childSpawn = await adapter('spawn-child', ['--root', runtimeRoot], childSpawnRequest(requiredProof));
  const childUnit = extractUnit(childSpawn, 'adapter child spawn');
  assertChildLineage(childUnit);
  summary.phases.harness_child_birth = { status: 'passed' };

  await harness(['start-turn'], {
    unit_id: 'agent-codex-live-child-001',
    turn_id: fixedTurnId,
    reason: 'live app-server adapter dogfood proof',
    expected_transport: 'codex_app_server_dynamic_tool',
    context: {
      initialize_result_sha256: sha256(JSON.stringify(initializeResponse.result || {})),
      schema_dir: relative(repoRoot, summary.artifacts.schema_dir),
      transcript_jsonl: relative(repoRoot, transcriptPath),
    },
  });

  const recordedProofs = [];
  for (const event of liveProofEvents({
    childUnit,
    initializeResponse,
    schema,
    threadStart,
    loadedList,
    injectItems,
    turnStart,
    turnSteer,
  })) {
    const result = await adapter('record-proof', ['--root', runtimeRoot], event);
    recordedProofs.push({
      kind: event.kind,
      event_kind: event.harness_request.kind,
      status: event.harness_request.status,
      bridge_status: result.status || 'ok',
      redacted: true,
    });
  }
  summary.recorded_proofs = recordedProofs;
  summary.phases.record_harness_proof = { status: 'passed', count: recordedProofs.length };

  await harness(['complete-turn'], {
    unit_id: 'agent-codex-live-child-001',
    turn_id: fixedTurnId,
    outcome: 'completed',
    usage: {
      input_tokens: 0,
      output_tokens: 0,
      total_tokens: 0,
    },
    duration_ms: 1,
    metadata: {
      proof: 'codex-adapter-live-server-dogfood',
      model_turn_enabled: modelTurnEnabled,
    },
  });

  const childStatus = await harness(['agent-status'], { unit_id: 'agent-codex-live-child-001' });
  summary.parent = {
    unit_id: parentUnit.unit_id,
    certificate_id: parentUnit.certificate?.certificate_id,
    lifecycle_state: parentUnit.lifecycle_state,
  };
  summary.child = {
    unit_id: childStatus.unit_id,
    proof_status: childStatus.proof_status,
    adapter_evidence: {
      persisted_event_count: childStatus.adapter_evidence?.persisted_event_count,
      kinds: (childStatus.adapter_evidence?.events || []).map((event) => event.kind),
    },
  };
  summary.assertions.harness_proof_events_recorded =
    childStatus.adapter_evidence?.persisted_event_count >= recordedProofs.length ? 'passed' : 'failed';
  summary.assertions.model_turn_operations =
    modelTurnEnabled && turnStart.status === 'passed' && turnSteer.status === 'passed'
      ? 'passed'
      : `skipped: ${turnStart.reason || turnSteer.reason || 'model turn was not completed'}`;
  summary.assertions.thread_inject_items = `skipped: ${injectItems.reason}`;
  if (injectItems.status === 'passed') {
    summary.assertions.thread_inject_items = 'passed';
  }
  assertInvariant(
    !JSON.stringify(summary).includes(liveTurnPrompt),
    'raw_prompt_in_summary',
    'proof-summary.json would contain the raw live turn prompt'
  );
  assertInvariant(
    !JSON.stringify(summary).includes(injectedContext),
    'raw_injected_context_in_summary',
    'proof-summary.json would contain the raw injected context'
  );

  await writeSummary('passed');
  console.log(`codex adapter live app-server dogfood proof passed: ${proofSummaryPath}`);
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
    await mkdir(evidenceDir, { recursive: true });
    await writeSummary('failed');
  } catch (writeError) {
    console.error(`failed to write proof summary: ${writeError.message}`);
  }
  console.error(error.message);
  if (error.repairHint) console.error(`Repair: ${error.repairHint}`);
  console.error(`Evidence: ${proofSummaryPath}`);
  process.exit(1);
});
