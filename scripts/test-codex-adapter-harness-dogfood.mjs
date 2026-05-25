#!/usr/bin/env node
import { appendFile, mkdir, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const args = new Set(process.argv.slice(2));
const timestamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
const evidenceDir = resolve(
  process.env.ONECONTEXT_CODEX_ADAPTER_HARNESS_DOGFOOD_DIR ||
    join(repoRoot, 'test-results', `codex-adapter-harness-dogfood-${timestamp}`)
);
const requestDir = join(evidenceDir, 'requests');
const runtimeRoot = join(evidenceDir, 'runtime', '1Context');
const harnessBin = resolve(
  process.env.ONECONTEXT_AGENT_HARNESS_BIN ||
    join(repoRoot, 'target/debug/onecontext-agent-harness')
);
const adapterBin = resolve(
  process.env.ONECONTEXT_CODEX_ADAPTER_BIN ||
    join(repoRoot, 'target/debug/onecontext-codex-adapter')
);
const commandLog = join(evidenceDir, 'commands.jsonl');
const proofSummaryPath = join(evidenceDir, 'proof-summary.json');
const sensitivePrompt = 'DO-NOT-PERSIST-CODEX-ADAPTER-HARNESS-DOGFOOD-RAW-PROMPT';
const fixedTurnId = 'turn-codex-adapter-child-001';

const summary = {
  schema_version: 1,
  status: 'running',
  proof: 'codex-adapter-harness-dogfood',
  generated_at: new Date().toISOString(),
  evidence_dir: evidenceDir,
  runtime_root: runtimeRoot,
  expected_adapter_cli: {
    spawn_child: 'onecontext-codex-adapter spawn-child --root <1Context-root> --request-json <json>',
    record_proof: 'onecontext-codex-adapter record-proof --root <1Context-root> --request-json <json>',
  },
  assertions: {},
  commands: [],
  artifacts: {
    proof_summary: proofSummaryPath,
    command_log: commandLog,
  },
  redaction: {
    raw_prompt_sha256: sha256(sensitivePrompt),
    raw_prompt_persisted: false,
  },
};

function usage() {
  console.log(`Usage: node scripts/test-codex-adapter-harness-dogfood.mjs

Creates deterministic adapter+harness dogfood evidence under test-results.

Expected adapter CLI contract:
  onecontext-codex-adapter spawn-child --root <1Context-root> --request-json <json>
  onecontext-codex-adapter record-proof --root <1Context-root> --request-json <json>

Environment:
  ONECONTEXT_AGENT_HARNESS_BIN                       harness CLI binary
  ONECONTEXT_CODEX_ADAPTER_BIN                       adapter CLI binary
  ONECONTEXT_CODEX_ADAPTER_HARNESS_DOGFOOD_DIR       evidence output directory
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
    return value.includes(sensitivePrompt) ? value.replaceAll(sensitivePrompt, '[redacted]') : value;
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
      'Make the CLI emit a single JSON object for harness bridge dogfood commands.'
    );
  }
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

async function adapterHarness(subcommand, request) {
  const requestPath = await requestFile(`adapter-${subcommand}`, request);
  const useBinary = existsSync(adapterBin);
  const command = useBinary ? adapterBin : 'cargo';
  const bridgeArgs = [subcommand, '--root', runtimeRoot, '--request-json', JSON.stringify(request)];
  const loggedBridgeArgs = [subcommand, '--root', runtimeRoot, '--request-json', `@${requestPath}`];
  const fullArgs = useBinary
    ? bridgeArgs
    : ['run', '-q', '-p', 'onecontext-codex-adapter', '--', ...bridgeArgs];

  const entry = await run(command, fullArgs);
  if (entry.stdout.trim() === 'onecontext-codex-adapter scaffold') {
    fail(
      'adapter_harness_cli_missing',
      `adapter binary exists but did not implement ${subcommand}; it printed the scaffold placeholder`,
      'Implement the Codex adapter harness bridge CLI in crates/onecontext-codex-adapter/src/main.rs for `spawn-child` and `record-proof`.'
    );
  }

  if (entry.code !== 0 && !entry.stdout.trim()) {
    await logCommand({
      tool: 'onecontext-codex-adapter',
      via: useBinary ? 'binary' : 'cargo-run',
      args: loggedBridgeArgs,
      request_file: requestPath,
      code: entry.code,
      status: 'error',
      ms: entry.ms,
      stderr: entry.stderr,
    });
    fail(
      'adapter_harness_command_failed',
      `${subcommand} exited ${entry.code}: ${entry.stderr.trim() || 'no stderr'}`,
      'Run `cargo run -q -p onecontext-codex-adapter -- describe` and implement the expected spawn-child/record-proof CLI contract if it is missing.'
    );
  }

  const json = parseJson(entry.stdout, `onecontext-codex-adapter ${bridgeArgs.join(' ')}`);
  await logCommand({
    tool: 'onecontext-codex-adapter',
    via: useBinary ? 'binary' : 'cargo-run',
    args: loggedBridgeArgs,
    request_file: requestPath,
    code: entry.code,
    status: entry.code === 0 ? json.status || 'ok' : json.status || 'error',
    ms: entry.ms,
    stderr: entry.stderr,
  });
  if (entry.code !== 0) {
    fail(
      'adapter_harness_command_failed',
      `${bridgeArgs.join(' ')} failed: ${JSON.stringify(json.error || json)}`,
      'Return a harness unit or proof receipt from the adapter bridge command.'
    );
  }
  return json;
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

function proofObserved(statusPayload) {
  return new Set((statusPayload.proof_status?.observed || []).map(idValue));
}

function capabilityBinding(id, transport, toolNames, proofRequired) {
  return {
    id,
    transport,
    tool_names: toolNames,
    config: {
      external_source: `codex-adapter://dogfood/${id}`,
      harness_owns_content: false,
    },
    policy: {
      declared_by: 'codex-adapter-harness-dogfood',
      inherited_from_parent: false,
    },
    proof_required: proofRequired,
  };
}

function parentBirthRequest() {
  return {
    unit_id: 'agent-codex-parent-001',
    role: 'codex adapter harness dogfood parent',
    model: 'gpt-5-codex',
    identity: {
      display_name: 'Codex Adapter Harness Dogfood Parent',
    },
    instructions: {
      system: 'Spawn governed Codex children only through the harness boundary.',
    },
    runtime: {
      adapter: 'codex_cli',
      thread_id: 'thread-codex-parent-001',
      session_id: 'session-codex-parent-001',
    },
    capabilities: [
      capabilityBinding('codex-adapter-harness-bridge', 'local_test', ['harness.spawn_child'], []),
    ],
    visibility: 'private',
    metadata: {
      title: 'Codex adapter harness dogfood parent',
      owner: 'agent-harness-dogfood',
      tags: ['dogfood', 'codex-adapter', 'parent'],
    },
  };
}

function childSpawnRequest() {
  return {
    parent_unit_id: 'agent-codex-parent-001',
    unit_id: 'agent-codex-child-001',
    spawn_request_id: 'codex-adapter-spawn-child-001',
    role: 'governed codex child dogfood',
    model: 'gpt-5-codex',
    identity: {
      display_name: 'Governed Codex Child',
    },
    instructions: {
      system: 'Operate with only declared harness capabilities and persist proof as redacted evidence.',
    },
    runtime: {
      adapter: 'codex_cli',
      thread_id: 'thread-codex-child-001',
      session_id: 'session-codex-child-001',
      room_id: 'room-codex-adapter-dogfood-child',
    },
    capabilities: [
      capabilityBinding(
        'codex-child-thread-bridge',
        'codex_app_server_dynamic_tool',
        ['thread/inject_items', 'agent.harness.record_proof'],
        ['transport_identity', 'context_injection', 'tool_conformance']
      ),
    ],
    visibility: 'private',
    metadata: {
      title: 'Governed Codex child from adapter bridge',
      owner: 'agent-harness-dogfood',
      tags: ['dogfood', 'codex-adapter', 'child'],
    },
  };
}

function proofEvent(kind, status, evidence) {
  const familyByKind = {
    transport_identity_observed: 'event_mirror',
    context_injection_executed: 'injection',
    tool_allowlist_checked: 'toolset_visibility',
  };
  return {
    target: 'agent_harness',
    family: familyByKind[kind] || 'event_mirror',
    kind: `codex.${kind}`,
    summary: `codex adapter dogfood observed ${kind}`,
    redacted: true,
    policy: {
      allowed: true,
      reason: 'allowed',
      message: 'adapter dogfood proof contains redacted evidence only',
    },
    harness_request: {
      unit_id: 'agent-codex-child-001',
      adapter: 'codex_cli',
      kind,
      status,
      correlation: {
        thread_id: 'thread-codex-child-001',
        session_id: 'session-codex-child-001',
        turn_id: fixedTurnId,
        expected_turn_id: fixedTurnId,
        transport_attempt_id: `adapter-dogfood-${kind}`,
      },
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

function proofEvents(childUnit) {
  const generatedIds = {
    unit_id: childUnit.unit_id,
    certificate_id: childUnit.certificate?.certificate_id,
    session_id: childUnit.session_id,
  };
  return [
    proofEvent('transport_identity_observed', 'observed', {
      generated_ids: generatedIds,
      declared_transport: 'codex_app_server_dynamic_tool',
      redacted: true,
    }),
    proofEvent('context_injection_executed', 'accepted', {
      injection_job_id: 'inject-codex-adapter-dogfood-001',
      item_count: 1,
      raw_prompt_sha256: sha256(sensitivePrompt),
      raw_prompt: '[redacted]',
      redacted: true,
    }),
    proofEvent('tool_allowlist_checked', 'accepted', {
      declared_tools: ['thread/inject_items', 'agent.harness.record_proof'],
      denied_native_extra_tools: [],
      redacted: true,
    }),
  ];
}

function assertParentBirth(parent) {
  assertInvariant(parent.unit_id === 'agent-codex-parent-001', 'parent_unit_id_mismatch', JSON.stringify(parent.unit_id));
  assertInvariant(
    ['born', 'ready', 'Born', 'Ready'].includes(parent.lifecycle_state),
    'parent_not_born',
    JSON.stringify(parent.lifecycle_state)
  );
  assertInvariant(
    idValue(parent.certificate?.lineage?.root_unit_id) === 'agent-codex-parent-001',
    'parent_root_lineage_mismatch',
    JSON.stringify(parent.certificate?.lineage)
  );
  summary.assertions.parent_unit_birth = 'passed';
}

function assertChildLineage(child) {
  const lineage = child.certificate?.lineage || {};
  assertInvariant(child.unit_id === 'agent-codex-child-001', 'child_unit_id_mismatch', JSON.stringify(child.unit_id));
  assertInvariant(
    idValue(lineage.parent_unit_id) === 'agent-codex-parent-001',
    'child_parent_lineage_mismatch',
    JSON.stringify(lineage)
  );
  assertInvariant(
    idValue(lineage.root_unit_id) === 'agent-codex-parent-001',
    'child_root_lineage_mismatch',
    JSON.stringify(lineage)
  );
  assertInvariant(
    lineage.spawn_request_id === 'codex-adapter-spawn-child-001',
    'child_spawn_request_lineage_mismatch',
    JSON.stringify(lineage)
  );
  summary.assertions.child_spawn_via_adapter_bridge = 'passed';
  summary.assertions.child_certificate_lineage = 'passed';
}

function assertProofStatus(status) {
  const observed = proofObserved(status);
  for (const category of ['transport_identity', 'context_injection', 'tool_conformance']) {
    assertInvariant(
      observed.has(category),
      'proof_category_missing',
      JSON.stringify({ category, observed: [...observed], proof_status: status.proof_status })
    );
  }
  assertInvariant(
    status.proof_status?.gate_status === 'satisfied',
    'proof_gate_not_satisfied',
    JSON.stringify(status.proof_status)
  );
  assertInvariant(
    status.adapter_evidence?.persisted_event_count >= 3,
    'adapter_evidence_not_recorded',
    JSON.stringify(status.adapter_evidence)
  );
  summary.assertions.proof_status_and_evidence_recording = 'passed';
}

async function writeSummary(status = summary.status) {
  summary.status = status;
  summary.redaction.raw_prompt_persisted = JSON.stringify(summary).includes(sensitivePrompt);
  await writeFile(proofSummaryPath, JSON.stringify(summary, null, 2) + '\n');
}

async function main() {
  await mkdir(requestDir, { recursive: true });
  await mkdir(runtimeRoot, { recursive: true });

  await harness(['ensure']);
  const parentBirth = await harness(['birth'], parentBirthRequest());
  const parentUnit = extractUnit(parentBirth, 'parent birth');
  assertParentBirth(parentUnit);

  const childSpawn = await adapterHarness('spawn-child', childSpawnRequest());
  const childUnit = extractUnit(childSpawn, 'adapter child spawn');
  assertChildLineage(childUnit);

  await harness(['start-turn'], {
    unit_id: 'agent-codex-child-001',
    turn_id: fixedTurnId,
    reason: 'adapter harness dogfood proof',
    expected_transport: 'codex_app_server_dynamic_tool',
    context: {
      redacted_prompt_sha256: sha256(sensitivePrompt),
    },
  });

  const recordedProofs = [];
  for (const event of proofEvents(childUnit)) {
    const result = await adapterHarness('record-proof', event);
    recordedProofs.push({
      kind: event.kind,
      status: event.harness_request.status,
      bridge_status: result.status || 'ok',
      redacted: true,
    });
  }
  summary.recorded_proofs = recordedProofs;

  await harness(['complete-turn'], {
    unit_id: 'agent-codex-child-001',
    turn_id: fixedTurnId,
    outcome: 'completed',
    usage: {
      input_tokens: 37,
      output_tokens: 19,
      total_tokens: 56,
    },
    duration_ms: 123,
    metadata: {
      proof: 'codex-adapter-harness-dogfood',
    },
  });

  const childStatus = await harness(['agent-status'], { unit_id: 'agent-codex-child-001' });
  assertProofStatus(childStatus);

  summary.parent = {
    unit_id: parentUnit.unit_id,
    certificate_id: parentUnit.certificate?.certificate_id,
    lifecycle_state: parentUnit.lifecycle_state,
  };
  summary.child = {
    unit_id: childStatus.unit_id,
    certificate_id: childStatus.certificate?.certificate_id,
    lifecycle: childStatus.lifecycle,
    lineage: childStatus.lineage,
    proof_status: childStatus.proof_status,
    adapter_evidence: {
      persisted_event_count: childStatus.adapter_evidence?.persisted_event_count,
      kinds: (childStatus.adapter_evidence?.events || []).map((event) => event.kind),
    },
  };
  summary.assertions.redacted_proof_summary_artifact = 'passed';
  assertInvariant(
    !JSON.stringify(summary).includes(sensitivePrompt),
    'raw_prompt_in_summary',
    'proof-summary.json would contain the raw prompt sentinel'
  );

  await writeSummary('passed');
  console.log(`codex adapter harness dogfood proof passed: ${proofSummaryPath}`);
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
