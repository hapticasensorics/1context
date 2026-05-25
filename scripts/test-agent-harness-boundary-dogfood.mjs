#!/usr/bin/env node
import { appendFile, mkdir, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const args = new Set(process.argv.slice(2));
const keepRuntime = args.has('--keep-runtime');
const timestamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
const shortStamp = timestamp.replace(/^.*T(\d{6})Z$/, '$1').toLowerCase();
const evidenceDir = resolve(
  process.env.ONECONTEXT_AGENT_HARNESS_DOGFOOD_EVIDENCE_DIR ||
    join(repoRoot, 'test-results', `agent-harness-boundary-dogfood-${timestamp}`)
);
const devHome = resolve(
  process.env.ONECONTEXT_AGENT_HARNESS_DOGFOOD_HOME ||
    join('/tmp', `1context-agent-harness-boundary-${shortStamp}`)
);
const runtimeRoot = join(devHome, '1Context');
const harnessBin = resolve(
  process.env.ONECONTEXT_AGENT_HARNESS_BIN ||
    join(repoRoot, 'target/debug/onecontext-agent-harness')
);
const commandLog = join(evidenceDir, 'commands.jsonl');
const fixedIssuedAt = '2026-05-25T00:00:00.000Z';
const externalMailBody =
  'External-only dogfood mail body. The harness fixture must never store this sentence.';

function usage() {
  console.log(`Usage: node scripts/test-agent-harness-boundary-dogfood.mjs [--keep-runtime]

Builds a deterministic lane-5 dogfood proof fixture for the Agent Harness.

The script exercises the current scaffold commands, then validates that one
  no-tool agent, one capability-bound agent, and three separate-room agents can
  be represented by harness certificates and metadata without harness-owned mail
  bodies. Where commands are implemented, it records adapter evidence for MCP
  toolsets plus Codex skills/plugins/connectors/apps, runs a turn with usage,
  asks for proof/usage status, queries a transport plan, and retires one
  disposable unit. The mail body appears only in the external capability fixture.

Environment:
  ONECONTEXT_AGENT_HARNESS_BIN                   harness CLI binary
  ONECONTEXT_AGENT_HARNESS_DOGFOOD_EVIDENCE_DIR evidence output directory
  ONECONTEXT_AGENT_HARNESS_DOGFOOD_HOME         disposable fake home
`);
}

if (args.has('--help') || args.has('-h')) {
  usage();
  process.exit(0);
}

function fail(code, detail) {
  const error = new Error(`${code}: ${detail}`);
  error.code = code;
  throw error;
}

function assertInvariant(condition, code, detail) {
  if (!condition) fail(code, detail);
}

const knownFeatureGatedCommands = new Map([
  [
    'record-adapter-event',
    'onecontext-agent-harness-core does not expose durable proof or adapter event intake APIs yet',
  ],
  [
    'start-turn',
    'onecontext-agent-harness-core does not expose turn lifecycle mutation APIs yet',
  ],
  [
    'complete-turn',
    'onecontext-agent-harness-core does not expose turn lifecycle mutation APIs yet',
  ],
  [
    'transport-plan',
    'onecontext-agent-harness-core does not expose transport planning APIs yet',
  ],
]);

function sha256(value) {
  return `sha256:${createHash('sha256').update(value, 'utf8').digest('hex')}`;
}

function parseJson(stdout, label) {
  try {
    return JSON.parse(stdout);
  } catch (error) {
    fail('invalid_json', `${label} did not emit JSON: ${error.message}\n${stdout}`);
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
  await appendFile(commandLog, JSON.stringify(entry) + '\n');
}

async function harness(commandArgs, options = {}) {
  const useBinary = existsSync(harnessBin);
  const command = useBinary ? harnessBin : 'cargo';
  const fullArgs = useBinary
    ? ['--root', runtimeRoot, ...commandArgs]
    : ['run', '-q', '-p', 'onecontext-agent-harness-daemon', '--', '--root', runtimeRoot, ...commandArgs];
  const entry = await run(command, fullArgs);
  const json = parseJson(entry.stdout, `onecontext-agent-harness ${commandArgs.join(' ')}`);
  await logCommand({
    tool: 'onecontext-agent-harness',
    via: useBinary ? 'binary' : 'cargo-run',
    args: ['--root', runtimeRoot, ...commandArgs],
    code: entry.code,
    ms: entry.ms,
    status: json.status,
    error: json.error || null,
    stderr: entry.stderr,
  });
  if (!options.allowFailure && entry.code !== 0) {
    fail('harness_command_failed', `${commandArgs.join(' ')} failed: ${JSON.stringify(json.error || json)}`);
  }
  return { code: entry.code, json };
}

function commandNameFromArgs(commandArgs) {
  return commandArgs[0];
}

function acceptImplementedOrKnownGap(command, result) {
  if (result.json.status === 'ok') {
    return {
      command,
      status: 'ok',
      real_behavior: true,
      feature_gate: null,
    };
  }
  if (result.json.status === 'scaffold' && knownFeatureGatedCommands.has(command)) {
    const expectedReason = knownFeatureGatedCommands.get(command);
    const actualReason = result.json.feature_gate?.reason;
    assertInvariant(
      actualReason === expectedReason,
      'unexpected_scaffold_feature_gate',
      JSON.stringify({ command, expectedReason, actualReason, result: result.json })
    );
    return {
      command,
      status: 'scaffold',
      real_behavior: false,
      feature_gate: result.json.feature_gate,
    };
  }
  fail('unexpected_harness_gap', JSON.stringify({ command, result: result.json }));
}

async function harnessFrontier(commandArgs) {
  const result = await harness(commandArgs);
  return {
    result,
    outcome: acceptImplementedOrKnownGap(commandNameFromArgs(commandArgs), result),
  };
}

function stringSet(value) {
  if (!Array.isArray(value)) return new Set();
  return new Set(
    value.map((item) => {
      if (typeof item === 'string') return item;
      if (item && typeof item === 'object') return item.category || item.name || item.kind || String(item);
      return String(item);
    })
  );
}

function proofCategorySet(statusPayload, field) {
  return stringSet(statusPayload?.proof_status?.[field]);
}

function assertProofObserved(statusPayload, categories) {
  const observed = proofCategorySet(statusPayload, 'observed');
  for (const category of categories) {
    assertInvariant(
      observed.has(category),
      'persisted_proof_category_missing',
      JSON.stringify({ category, observed: [...observed], proof_status: statusPayload?.proof_status })
    );
  }
}

function assertUsageObserved(statusPayload, expectedUsage) {
  const lifecycle = statusPayload.lifecycle || {};
  const usage = statusPayload.usage || {};
  assertInvariant(lifecycle.turns_started >= 1, 'turns_started_not_persisted', JSON.stringify(lifecycle));
  assertInvariant(lifecycle.turns_completed >= 1, 'turns_completed_not_persisted', JSON.stringify(lifecycle));
  assertInvariant(
    usage.input_tokens >= expectedUsage.input_tokens,
    'input_tokens_not_persisted',
    JSON.stringify(usage)
  );
  assertInvariant(
    usage.output_tokens >= expectedUsage.output_tokens,
    'output_tokens_not_persisted',
    JSON.stringify(usage)
  );
  assertInvariant(
    usage.total_tokens >= expectedUsage.total_tokens,
    'total_tokens_not_persisted',
    JSON.stringify(usage)
  );
}

const capabilityOwners = new Map([
  ['toolset-mail', 'onecontext mail system'],
  ['toolset-wiki', 'onecontext wiki system'],
  ['skill-openai-docs', 'codex skills registry'],
  ['plugin-agent-unit', 'codex plugin registry'],
  ['connector-1context-memory', 'codex connector registry'],
  ['app-codex-desktop', 'codex app registry'],
]);

function capabilityBinding(id, transport, toolNames, externalSource, proofRequired) {
  return {
    id,
    transport,
    tool_names: toolNames,
    config: {
      external_source: externalSource,
      external_owner: capabilityOwners.get(id) || 'external capability registry',
      harness_owns_content: false,
      harness_records: ['binding_id', 'tool_names', 'proof_status', 'adapter_evidence_refs'],
    },
    policy: {
      content_storage: 'external_only',
      body_storage: 'forbidden_in_harness',
      allow_native_extra_tools: false,
    },
    proof_required: proofRequired,
  };
}

function certificate({
  unitId,
  certificateId,
  callId,
  role,
  model,
  capabilities,
  metadata,
  generatedIds,
  parentUnitId,
  spawnRequestId,
}) {
  return {
    kind: 'onecontext.agent_harness.birth_certificate',
    version: 1,
    certificate_id: certificateId,
    call_id: callId,
    issued_at: fixedIssuedAt,
    unit_id: unitId,
    role,
    model,
    identity: {
      display_name: metadata.title,
      product_dogfood: true,
    },
    instructions: {
      boundary: 'Use only declared external capabilities; do not persist external content in harness state.',
    },
    runtime: {
      adapter: 'codex_app_server',
      room_id: generatedIds.room_id,
      session_history_id: generatedIds.session_history_id,
    },
    capabilities,
    birth_inputs: {
      requested_role: role,
      requested_capability_ids: capabilities.map((capability) => capability.id),
      requested_room_id: generatedIds.room_id,
    },
    generated_ids: generatedIds,
    inheritance_policy: {
      ambient_config: 'deny_unless_declared',
      codex_home: 'isolated',
      project_docs: 'disabled_unless_declared',
      tools: 'declared_capabilities_only',
    },
    lineage: {
      parent_unit_id: parentUnitId || null,
      root_unit_id: parentUnitId || unitId,
      spawn_request_id: spawnRequestId || null,
    },
  };
}

function unitFixture(index, options) {
  const unitId = options.unitId;
  const generatedIds = {
    namespace_id: `hns-${String(index).padStart(3, '0')}`,
    session_id: `hsn-${String(index).padStart(3, '0')}`,
    session_history_id: `history-${unitId}`,
    room_id: options.roomId,
    thread_id: `thread-${unitId}`,
  };
  const cert = certificate({
    unitId,
    certificateId: `cert-${String(index).padStart(3, '0')}`,
    callId: `call-${String(index).padStart(3, '0')}`,
    role: options.role,
    model: options.model || 'gpt-5-codex',
    capabilities: options.capabilities || [],
    metadata: options.metadata,
    generatedIds,
    parentUnitId: options.parentUnitId,
    spawnRequestId: options.spawnRequestId,
  });
  return {
    kind: 'onecontext.agent_harness.unit_fixture',
    unit_id: unitId,
    certificate: cert,
    namespace_path: `context-engine/agents/harness/units/${unitId}`,
    lifecycle_state: 'ready',
    session_id: generatedIds.session_id,
    session_history: {
      id: generatedIds.session_history_id,
      room_id: generatedIds.room_id,
      turns: [],
    },
    metadata: {
      visibility: options.metadata.visibility || 'private',
      availability: 'active',
      title: options.metadata.title,
      owner: '1context-product-dogfood',
      tags: options.metadata.tags,
      called_at: fixedIssuedAt,
      last_active_at: fixedIssuedAt,
      retired_at: null,
      retirement_reason: null,
      parent_unit_id: options.parentUnitId || null,
      spawn_request_id: options.spawnRequestId || null,
      turns_started: 0,
      turns_completed: 0,
      active_turn_id: null,
      input_tokens: 0,
      output_tokens: 0,
      total_tokens: 0,
      total_duration_ms: 0,
    },
    receipts: [
      {
        id: `receipt-${unitId}-birth`,
        at: fixedIssuedAt,
        unit_id: unitId,
        kind: 'birth',
        summary: 'Birth certificate planned for lane-5 dogfood fixture.',
        evidence: {
          certificate_id: cert.certificate_id,
          namespace_path: `context-engine/agents/harness/units/${unitId}`,
        },
      },
    ],
  };
}

function unitPlans() {
  const mailCapability = capabilityBinding(
    'toolset-mail',
    'mcp',
    ['agent.mail.send', 'agent.mail.inbox', 'agent.mail.open'],
    'mcp://onecontext/toolsets/mail',
    ['transport_identity', 'tool_allowlist_conformance', 'context_injection', 'wake_steering']
  );
  const wikiCapability = capabilityBinding(
    'toolset-wiki',
    'mcp',
    ['wiki.page.read', 'wiki.page.search', 'wiki.page.link'],
    'mcp://onecontext/toolsets/wiki',
    ['transport_identity', 'tool_allowlist_conformance']
  );
  const skillCapability = capabilityBinding(
    'skill-openai-docs',
    'codex_skill',
    [],
    'codex://skills/openai-docs',
    ['skill_registry']
  );
  const pluginCapability = capabilityBinding(
    'plugin-agent-unit',
    'codex_plugin',
    ['agent.harness.status', 'agent.harness.record-adapter-event'],
    'codex://plugins/1context-agent-unit',
    ['plugin_registry', 'tool_conformance']
  );
  const connectorCapability = capabilityBinding(
    'connector-1context-memory',
    'codex_connector',
    [],
    'codex://connectors/1context-memory-db',
    ['connector_registry']
  );
  const appCapability = capabilityBinding(
    'app-codex-desktop',
    'codex_app',
    [],
    'codex://apps/codex-desktop',
    ['app_registry']
  );
  return [
    {
      unitId: 'agent-no-tool-001',
      role: 'no-tool control agent',
      roomId: 'room-control',
      metadata: {
        title: 'No-tool control',
        tags: ['dogfood', 'no-product-toolsets'],
      },
    },
    {
      unitId: 'agent-mail-wiki-001',
      role: 'capability-bound agent',
      roomId: 'room-capability-bound',
      capabilities: [
        mailCapability,
        wikiCapability,
        skillCapability,
        pluginCapability,
        connectorCapability,
        appCapability,
      ],
      metadata: {
        title: 'Capability-bound agent',
        tags: [
          'dogfood',
          'toolset-mail',
          'toolset-wiki',
          'codex-skill',
          'codex-plugin',
          'codex-connector',
          'codex-app',
        ],
      },
    },
    {
      unitId: 'agent-room-red-001',
      role: 'separate room worker',
      roomId: 'room-red',
      parentUnitId: 'agent-mail-wiki-001',
      spawnRequestId: 'spawn-room-red-001',
      metadata: {
        title: 'Room red worker',
        tags: ['dogfood', 'separate-room', 'spawned-child'],
      },
    },
    {
      unitId: 'agent-room-green-001',
      role: 'separate room worker',
      roomId: 'room-green',
      parentUnitId: 'agent-mail-wiki-001',
      spawnRequestId: 'spawn-room-green-001',
      metadata: {
        title: 'Room green worker',
        tags: ['dogfood', 'separate-room', 'spawned-child'],
      },
    },
    {
      unitId: 'agent-room-blue-001',
      role: 'separate room worker',
      roomId: 'room-blue',
      parentUnitId: 'agent-mail-wiki-001',
      spawnRequestId: 'spawn-room-blue-001',
      metadata: {
        title: 'Room blue worker',
        tags: ['dogfood', 'separate-room', 'spawned-child'],
      },
    },
  ];
}

function harnessBirthRequest(plan) {
  return {
    unit_id: plan.unitId,
    parent_unit_id: plan.parentUnitId,
    spawn_request_id: plan.spawnRequestId,
    role: plan.role,
    model: plan.model || 'gpt-5-codex',
    identity: {
      display_name: plan.metadata.title,
      product_dogfood: true,
    },
    instructions: {
      boundary: 'Use only declared external capabilities; do not persist external content in harness state.',
    },
    runtime: {
      adapter: 'codex_app_server',
      room_id: plan.roomId,
      thread_hint: `thread-${plan.unitId}`,
      session_history_hint: `history-${plan.unitId}`,
    },
    capabilities: plan.capabilities || [],
    visibility: plan.metadata.visibility || 'private',
    metadata: {
      title: plan.metadata.title,
      owner: '1context-product-dogfood',
      tags: plan.metadata.tags,
    },
  };
}

async function birthScenarioUnits() {
  const units = [];
  for (const plan of unitPlans()) {
    const request = harnessBirthRequest(plan);
    const birth = await harness(['birth', '--request-json', JSON.stringify(request)]);
    assertInvariant(birth.json.status === 'ok', 'birth_status', JSON.stringify(birth.json));
    assertInvariant(birth.json.unit_id === plan.unitId, 'birth_unit_id', JSON.stringify(birth.json));
    assertInvariant(
      birth.json.unit?.certificate?.unit_id === plan.unitId,
      'birth_certificate_unit_id',
      JSON.stringify(birth.json)
    );
    const unit = birth.json.unit;
    unit.session_history = {
      id: unit.certificate.runtime.session_history_hint,
      room_id: unit.certificate.runtime.room_id,
      turns: [],
    };
    units.push(unit);
  }
  return units;
}

function buildScenario(units) {
  const byId = new Map(units.map((unit) => [unit.unit_id, unit]));
  const mailWikiUnit = byId.get('agent-mail-wiki-001');
  const redRoomUnit = byId.get('agent-room-red-001');
  assertInvariant(mailWikiUnit, 'missing_mail_wiki_unit', 'agent-mail-wiki-001');
  assertInvariant(redRoomUnit, 'missing_red_room_unit', 'agent-room-red-001');

  const mailDigest = sha256(externalMailBody);
  const adapterEvents = [
    {
      id: 'adapter-event-mail-wake-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'codex_app_server',
      kind: 'runtime_wakeup_accepted',
      status: 'accepted',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
        transport_attempt_id: 'transport-attempt-mail-001',
      },
      evidence: {
        capability_id: 'toolset-mail',
        external_source: 'mcp://onecontext/toolsets/mail',
        wake_target: 'room-red',
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-mail-transport-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'mcp',
      kind: 'transport_identity_observed',
      status: 'observed',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
      },
      evidence: {
        capability_id: 'toolset-mail',
        external_source: 'mcp://onecontext/toolsets/mail',
        generated_ids: mailWikiUnit.certificate.generated_ids,
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-mail-allowlist-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'mcp',
      kind: 'tool_allowlist_checked',
      status: 'accepted',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
      },
      evidence: {
        capability_id: 'toolset-mail',
        allowed_tools: ['agent.mail.send', 'agent.mail.inbox', 'agent.mail.open'],
        external_source: 'mcp://onecontext/toolsets/mail',
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-skill-registry-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'codex_skill',
      kind: 'skill_registry_observed',
      status: 'observed',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
      },
      evidence: {
        capability_id: 'skill-openai-docs',
        external_source: 'codex://skills/openai-docs',
        registry_scope: 'codex_home_skills',
        enabled: true,
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-plugin-registry-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'codex_plugin',
      kind: 'plugin_registry_observed',
      status: 'observed',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
      },
      evidence: {
        capability_id: 'plugin-agent-unit',
        external_source: 'codex://plugins/1context-agent-unit',
        registry_scope: 'codex_plugin_manifest',
        enabled: true,
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-connector-registry-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'codex_connector',
      kind: 'connector_registry_observed',
      status: 'observed',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
      },
      evidence: {
        capability_id: 'connector-1context-memory',
        external_source: 'codex://connectors/1context-memory-db',
        registry_scope: 'codex_connector_manifest',
        enabled: true,
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-app-registry-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'codex_app',
      kind: 'app_registry_observed',
      status: 'observed',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
      },
      evidence: {
        capability_id: 'app-codex-desktop',
        external_source: 'codex://apps/codex-desktop',
        registry_scope: 'host_app_inventory',
        enabled: true,
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-mail-send-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'mcp',
      kind: 'tool_call_observed',
      status: 'accepted',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
        delivery_id: 'delivery-dogfood-001',
        message_id: 'mail-message-dogfood-001',
        tool_call_id: 'tool-call-mail-send-001',
      },
      evidence: {
        capability_id: 'toolset-mail',
        tool_name: 'agent.mail.send',
        external_source: 'mcp://onecontext/toolsets/mail',
        sender_unit_id: 'agent-mail-wiki-001',
        recipient_unit_id: 'agent-room-red-001',
        labels: ['dogfood', 'capability-boundary-proof'],
        body_ref: 'mail://messages/mail-message-dogfood-001',
        body_digest: mailDigest,
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-mail-sender-injection-001',
      at: fixedIssuedAt,
      unit_id: 'agent-mail-wiki-001',
      adapter: 'codex_app_server',
      kind: 'context_injection_executed',
      status: 'accepted',
      correlation: {
        thread_id: mailWikiUnit.certificate.runtime.thread_hint,
        session_id: mailWikiUnit.session_id,
        turn_id: 'turn-agent-mail-wiki-001',
        delivery_id: 'delivery-dogfood-001',
        message_id: 'mail-message-dogfood-001',
      },
      evidence: {
        capability_id: 'toolset-mail',
        external_source: 'mcp://onecontext/toolsets/mail',
        delivery_kind: 'labels_and_message_ref_only',
        labels: ['dogfood', 'capability-boundary-proof'],
        body_ref: 'mail://messages/mail-message-dogfood-001',
        body_digest: mailDigest,
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
    {
      id: 'adapter-event-mail-injection-001',
      at: fixedIssuedAt,
      unit_id: 'agent-room-red-001',
      adapter: 'codex_app_server',
      kind: 'context_injection_executed',
      status: 'accepted',
      correlation: {
        thread_id: redRoomUnit.certificate.runtime.thread_hint,
        session_id: redRoomUnit.session_id,
        turn_id: 'turn-agent-room-red-001',
        delivery_id: 'delivery-dogfood-001',
        message_id: 'mail-message-dogfood-001',
      },
      evidence: {
        capability_id: 'toolset-mail',
        external_source: 'mcp://onecontext/toolsets/mail',
        delivery_kind: 'labels_and_message_ref_only',
        labels: ['dogfood', 'capability-boundary-proof'],
        body_ref: 'mail://messages/mail-message-dogfood-001',
        body_digest: mailDigest,
      },
      redaction: {
        raw_prompts_redacted: true,
        mail_bodies_redacted: true,
        tool_outputs_redacted: true,
        secrets_redacted: true,
      },
    },
  ];

  mailWikiUnit.receipts.push({
    id: 'receipt-mail-wiki-capability-bound',
    at: fixedIssuedAt,
    unit_id: 'agent-mail-wiki-001',
    kind: 'capability_bound',
    summary: 'Toolsets, skills, plugins, connectors, and app bindings are external capabilities.',
    evidence: {
      capability_ids: mailWikiUnit.certificate.capabilities.map((capability) => capability.id),
      external_sources: mailWikiUnit.certificate.capabilities.map(
        (capability) => capability.config.external_source
      ),
    },
  });
  mailWikiUnit.receipts.push({
    id: 'receipt-mail-delivery-observed',
    at: fixedIssuedAt,
    unit_id: 'agent-mail-wiki-001',
    kind: 'proof_observed',
    summary: 'Mail delivery observed by adapter evidence; body remains external.',
    evidence: {
      adapter_event_id: 'adapter-event-mail-send-001',
      message_id: 'mail-message-dogfood-001',
      delivery_id: 'delivery-dogfood-001',
      body_ref: 'mail://messages/mail-message-dogfood-001',
      body_digest: mailDigest,
      stores_mail_body: false,
    },
  });

  return {
    kind: 'onecontext.agent_harness.boundary_dogfood_fixture',
    version: 1,
    generated_at: fixedIssuedAt,
    invariant:
      'Harness artifacts may hold certificates, ids, parent-child lineage, lifecycle metadata, capability bindings, proof status, and adapter evidence, but never external mail/wiki/memory content bodies.',
    units,
    adapter_events: adapterEvents,
    proof_status: {
      'agent-no-tool-001': {
        product_toolsets: [],
        status: 'ready',
      },
      'agent-mail-wiki-001': {
        product_toolsets: ['toolset-mail', 'toolset-wiki'],
        codex_attachments: [
          'skill-openai-docs',
          'plugin-agent-unit',
          'connector-1context-memory',
          'app-codex-desktop',
        ],
        status: 'degraded_until_runtime_events_replay',
        observed_categories: [
          'transport_identity',
          'tool_conformance',
          'context_injection',
          'steering',
          'skill_registry',
          'plugin_registry',
          'connector_registry',
          'app_registry',
        ],
      },
      'agent-room-red-001': {
        parent_unit_id: 'agent-mail-wiki-001',
        room_id: redRoomUnit.certificate.runtime.room_id,
        status: 'ready',
      },
      'agent-room-green-001': {
        parent_unit_id: 'agent-mail-wiki-001',
        room_id: byId.get('agent-room-green-001')?.certificate.runtime.room_id,
        status: 'ready',
      },
      'agent-room-blue-001': {
        parent_unit_id: 'agent-mail-wiki-001',
        room_id: byId.get('agent-room-blue-001')?.certificate.runtime.room_id,
        status: 'ready',
      },
    },
  };
}

async function recordAdapterEvents(adapterEvents) {
  const records = [];
  for (const event of adapterEvents) {
    const { result, outcome } = await harnessFrontier([
      'record-adapter-event',
      '--request-json',
      JSON.stringify(event),
    ]);
    records.push({
      event_id: event.id,
      kind: event.kind,
      unit_id: event.unit_id,
      command_status: outcome.status,
      real_behavior: outcome.real_behavior,
      feature_gate: outcome.feature_gate,
      response: result.json,
    });
  }
  const realCount = records.filter((record) => record.real_behavior).length;
  assertInvariant(
    realCount === 0 || realCount === adapterEvents.length,
    'partial_adapter_event_recording',
    JSON.stringify(records.map(({ event_id, command_status }) => ({ event_id, command_status })))
  );
  return {
    status: realCount === adapterEvents.length ? 'recorded' : 'feature_gated',
    real_behavior: realCount === adapterEvents.length,
    records,
  };
}

async function exerciseTurnLifecycle(unitId) {
  const turnId = `turn-${unitId}`;
  const expectedUsage = {
    input_tokens: 123,
    output_tokens: 45,
    total_tokens: 168,
    duration_ms: 3210,
  };
  const startRequest = {
    unit_id: unitId,
    turn_id: turnId,
    reason: 'lane-5 dogfood proof turn',
    at: fixedIssuedAt,
  };
  const start = await harnessFrontier([
    'start-turn',
    '--request-json',
    JSON.stringify(startRequest),
  ]);
  const completeRequest = {
    unit_id: unitId,
    turn_id: turnId,
    status: 'completed',
    at: fixedIssuedAt,
    usage: expectedUsage,
    input_tokens: expectedUsage.input_tokens,
    output_tokens: expectedUsage.output_tokens,
    total_tokens: expectedUsage.total_tokens,
    duration_ms: expectedUsage.duration_ms,
  };
  const complete = await harnessFrontier([
    'complete-turn',
    '--request-json',
    JSON.stringify(completeRequest),
  ]);
  assertInvariant(
    start.outcome.real_behavior === complete.outcome.real_behavior,
    'partial_turn_lifecycle_implementation',
    JSON.stringify({ start: start.outcome, complete: complete.outcome })
  );
  return {
    turn_id: turnId,
    expected_usage: expectedUsage,
    status: complete.outcome.real_behavior ? 'completed' : 'feature_gated',
    real_behavior: complete.outcome.real_behavior,
    start: start.result.json,
    complete: complete.result.json,
  };
}

async function queryAgentStatuses(units) {
  const statuses = {};
  for (const unit of units) {
    const statusResult = await harness([
      'agent-status',
      '--request-json',
      JSON.stringify({ unit_id: unit.unit_id }),
    ]);
    statuses[unit.unit_id] = statusResult.json;
    assertInvariant(statusResult.json.status === 'ok', 'agent_status_ok', JSON.stringify(statusResult.json));
    assertInvariant(statusResult.json.unit_id === unit.unit_id, 'agent_status_unit_id', JSON.stringify(statusResult.json));
  }
  return statuses;
}

async function queryTransportPlan(unitId) {
  const { result, outcome } = await harnessFrontier([
    'transport-plan',
    '--request-json',
    JSON.stringify({
      unit_id: unitId,
      requested_transports: [
        'mcp',
        'codex_app_server_dynamic_tool',
        'codex_skill',
        'codex_plugin',
        'codex_connector',
        'codex_app',
        'host_hook',
      ],
      proof_categories: [
        'transport_identity',
        'tool_conformance',
        'context_injection',
        'steering',
        'skill_registry',
        'plugin_registry',
        'connector_registry',
        'app_registry',
      ],
    }),
  ]);
  assertInvariant(
    result.json.transport_plan || outcome.real_behavior,
    'transport_plan_missing',
    JSON.stringify(result.json)
  );
  return {
    status: outcome.real_behavior ? 'planned' : 'feature_gated',
    real_behavior: outcome.real_behavior,
    response: result.json,
  };
}

async function retireDisposableUnit(units) {
  const disposable = units.find((unit) => unit.unit_id === 'agent-room-blue-001');
  assertInvariant(disposable, 'missing_disposable_retire_unit', 'agent-room-blue-001');
  assertInvariant(
    disposable.certificate.capabilities.length === 0,
    'retire_unit_not_disposable',
    JSON.stringify(disposable.certificate.capabilities)
  );
  const retire = await harness([
    'retire',
    '--request-json',
    JSON.stringify({
      unit_id: disposable.unit_id,
      reason: 'lane-5 dogfood disposable room cleanup',
    }),
  ]);
  assertInvariant(retire.json.status === 'ok', 'retire_status', JSON.stringify(retire.json));
  assertInvariant(retire.json.unit?.metadata?.availability === 'retired', 'retire_availability', JSON.stringify(retire.json));
  return {
    unit_id: disposable.unit_id,
    real_behavior: true,
    response: retire.json,
    updated_unit: retire.json.unit,
  };
}

function externalCapabilityFixture() {
  return {
    kind: 'onecontext.external_capability_fixture',
    owner: 'toolset-mail',
    note: 'This file deliberately represents the mail system, not harness-owned state.',
    messages: [
      {
        message_id: 'mail-message-dogfood-001',
        delivery_id: 'delivery-dogfood-001',
        sender_unit_id: 'agent-mail-wiki-001',
        recipient_unit_id: 'agent-room-red-001',
        labels: ['dogfood', 'capability-boundary-proof'],
        body_markdown: externalMailBody,
      },
    ],
  };
}

function assertUnique(values, code) {
  const set = new Set(values);
  assertInvariant(set.size === values.length, code, JSON.stringify(values));
}

function assertNoHarnessBodyStorage(value, path = '$') {
  const forbiddenKeys = new Set(['body', 'body_markdown', 'mail_body', 'mail_bodies', 'page_body']);
  if (Array.isArray(value)) {
    value.forEach((item, index) => assertNoHarnessBodyStorage(item, `${path}[${index}]`));
    return;
  }
  if (value && typeof value === 'object') {
    for (const [key, child] of Object.entries(value)) {
      assertInvariant(!forbiddenKeys.has(key), 'harness_body_field_present', `${path}.${key}`);
      assertNoHarnessBodyStorage(child, `${path}.${key}`);
    }
    return;
  }
  if (typeof value === 'string') {
    assertInvariant(!value.includes(externalMailBody), 'harness_body_literal_present', path);
  }
}

function validateScenario(scenario) {
  assertInvariant(scenario.units.length === 5, 'unit_count', `expected 5 units, got ${scenario.units.length}`);
  assertUnique(
    scenario.units.map((unit) => unit.unit_id),
    'unique_unit_ids'
  );
  assertUnique(
    scenario.units.map((unit) => unit.certificate.certificate_id),
    'unique_certificate_ids'
  );
  assertUnique(
    scenario.units.map((unit) => unit.namespace_path),
    'unique_namespaces'
  );
  assertUnique(
    scenario.units.map((unit) => unit.session_history.id),
    'unique_session_histories'
  );

  const byId = new Map(scenario.units.map((unit) => [unit.unit_id, unit]));
  const noTool = byId.get('agent-no-tool-001');
  assertInvariant(noTool.certificate.capabilities.length === 0, 'no_tool_agent_has_tools', JSON.stringify(noTool.certificate.capabilities));

  const mailWiki = byId.get('agent-mail-wiki-001');
  const capabilityIds = mailWiki.certificate.capabilities.map((capability) => capability.id).sort();
  const expectedCapabilityIds = [
    'app-codex-desktop',
    'connector-1context-memory',
    'plugin-agent-unit',
    'skill-openai-docs',
    'toolset-mail',
    'toolset-wiki',
  ];
  assertInvariant(
    JSON.stringify(capabilityIds) === JSON.stringify(expectedCapabilityIds),
    'mail_wiki_capabilities',
    JSON.stringify(capabilityIds)
  );
  const sourcePrefixesByTransport = {
    mcp: 'mcp://',
    codex_skill: 'codex://skills/',
    codex_plugin: 'codex://plugins/',
    codex_connector: 'codex://connectors/',
    codex_app: 'codex://apps/',
  };
  for (const capability of mailWiki.certificate.capabilities) {
    const expectedPrefix = sourcePrefixesByTransport[capability.transport];
    assertInvariant(expectedPrefix, 'capability_transport', JSON.stringify(capability));
    assertInvariant(
      capability.config.external_source.startsWith(expectedPrefix),
      'capability_external_source',
      JSON.stringify(capability)
    );
    assertInvariant(capability.config.harness_owns_content === false, 'capability_harness_ownership', JSON.stringify(capability));
    assertInvariant(capability.policy.content_storage === 'external_only', 'capability_content_storage', JSON.stringify(capability));
  }

  const roomAgents = scenario.units.filter((unit) => unit.metadata.tags.includes('separate-room'));
  assertInvariant(roomAgents.length === 3, 'separate_room_count', `expected 3, got ${roomAgents.length}`);
  assertUnique(
    roomAgents.map((unit) => unit.certificate.runtime.room_id),
    'unique_room_ids'
  );
  for (const roomAgent of roomAgents) {
    assertInvariant(
      roomAgent.certificate.lineage?.parent_unit_id === 'agent-mail-wiki-001',
      'child_agent_parent_lineage',
      JSON.stringify(roomAgent.certificate.lineage)
    );
    assertInvariant(
      roomAgent.certificate.lineage?.root_unit_id === 'agent-mail-wiki-001',
      'child_agent_root_lineage',
      JSON.stringify(roomAgent.certificate.lineage)
    );
    assertInvariant(
      roomAgent.metadata.parent_unit_id === 'agent-mail-wiki-001',
      'child_agent_metadata_parent',
      JSON.stringify(roomAgent.metadata)
    );
  }

  const mailProof = scenario.adapter_events.find((event) => event.id === 'adapter-event-mail-send-001');
  assertInvariant(mailProof, 'missing_mail_adapter_event', 'adapter-event-mail-send-001');
  assertInvariant(mailProof.evidence.body_ref.startsWith('mail://'), 'mail_body_ref', JSON.stringify(mailProof.evidence));
  assertInvariant(mailProof.evidence.body_digest === sha256(externalMailBody), 'mail_body_digest', JSON.stringify(mailProof.evidence));
  assertInvariant(mailProof.redaction.mail_bodies_redacted === true, 'mail_redaction_flag', JSON.stringify(mailProof.redaction));

  assertNoHarnessBodyStorage(scenario);
}

async function writeJson(name, value) {
  await writeFile(join(evidenceDir, name), JSON.stringify(value, null, 2) + '\n');
}

async function main() {
  await rm(devHome, { recursive: true, force: true });
  await mkdir(runtimeRoot, { recursive: true });
  await mkdir(evidenceDir, { recursive: true });

  const ensure = await harness(['ensure']);
  const status = await harness(['status']);
  const describe = await harness(['describe']);

  assertInvariant(ensure.json.status === 'ok', 'ensure_status', JSON.stringify(ensure.json));
  assertInvariant(status.json.harness_root_exists === true, 'status_harness_root', JSON.stringify(status.json));
  assertInvariant(
    describe.json.external_product_layers.includes('toolset-mail') &&
      describe.json.external_product_layers.includes('toolset-wiki') &&
      describe.json.external_product_layers.includes('memory-db') &&
      describe.json.external_product_layers.includes('mcp-server-implementations') &&
      describe.json.external_product_layers.includes('codex-skills') &&
      describe.json.external_product_layers.includes('codex-plugins') &&
      describe.json.external_product_layers.includes('codex-connectors') &&
      describe.json.external_product_layers.includes('codex-apps'),
    'describe_external_layers',
    JSON.stringify(describe.json.external_product_layers)
  );

  let units = await birthScenarioUnits();
  const inventory = await harness(['agents']);
  assertInvariant(
    inventory.json.agents?.counts?.active === 5,
    'inventory_active_count',
    JSON.stringify(inventory.json)
  );

  const scenario = buildScenario(units);
  const agentStatusesBeforeFrontier = await queryAgentStatuses(units);
  const turnLifecycle = await exerciseTurnLifecycle('agent-mail-wiki-001');
  const adapterEvidence = await recordAdapterEvents(scenario.adapter_events);
  const transportPlan = await queryTransportPlan('agent-mail-wiki-001');
  const mailWikiStatusAfterFrontier = await harness([
    'agent-status',
    '--request-json',
    JSON.stringify({ unit_id: 'agent-mail-wiki-001' }),
  ]);
  assertInvariant(
    mailWikiStatusAfterFrontier.json.status === 'ok',
    'agent_status_after_frontier_ok',
    JSON.stringify(mailWikiStatusAfterFrontier.json)
  );
  if (turnLifecycle.real_behavior) {
    assertUsageObserved(mailWikiStatusAfterFrontier.json, turnLifecycle.expected_usage);
  }
  if (adapterEvidence.real_behavior) {
    assertProofObserved(mailWikiStatusAfterFrontier.json, [
      'transport_identity',
      'tool_conformance',
      'context_injection',
      'steering',
      'skill_registry',
      'plugin_registry',
      'connector_registry',
      'app_registry',
    ]);
  }
  const retire = await retireDisposableUnit(units);
  units = units.map((unit) =>
    unit.unit_id === retire.unit_id
      ? {
          ...retire.updated_unit,
          session_history: unit.session_history,
        }
      : unit
  );
  scenario.units = units;
  scenario.proof_status[retire.unit_id] = {
    room_id: retire.updated_unit.certificate.runtime.room_id,
    status: 'retired_disposable_unit',
  };
  const inventoryAfterRetire = await harness(['agents']);
  assertInvariant(
    inventoryAfterRetire.json.agents?.counts?.active === 4,
    'inventory_active_after_retire_count',
    JSON.stringify(inventoryAfterRetire.json)
  );
  assertInvariant(
    inventoryAfterRetire.json.agents?.counts?.retired === 1,
    'inventory_retired_count',
    JSON.stringify(inventoryAfterRetire.json)
  );
  const agentStatuses = await queryAgentStatuses(units);
  const external = externalCapabilityFixture();
  validateScenario(scenario);
  assertInvariant(
    external.messages[0].body_markdown === externalMailBody,
    'external_fixture_body_missing',
    JSON.stringify(external)
  );

  const summary = {
    status: 'ok',
    evidence_dir: evidenceDir,
    runtime_root: runtimeRoot,
    scaffold_commands: {
      ensure: ensure.json.status,
      status: status.json.status,
      describe: describe.json.status,
      birth: 'ok',
      agents: inventory.json.status,
      agent_status: 'ok',
      start_turn: turnLifecycle.status,
      complete_turn: turnLifecycle.status,
      record_adapter_event: adapterEvidence.status,
      transport_plan: transportPlan.status,
      retire: 'ok',
    },
    represented_agents: scenario.units.map((unit) => ({
      unit_id: unit.unit_id,
      certificate_id: unit.certificate.certificate_id,
      namespace_path: unit.namespace_path,
      room_id: unit.certificate.runtime.room_id,
      session_history_id: unit.session_history.id,
      parent_unit_id: unit.certificate.lineage?.parent_unit_id || null,
      root_unit_id: unit.certificate.lineage?.root_unit_id || unit.unit_id,
      capability_ids: unit.certificate.capabilities.map((capability) => capability.id),
    })),
    boundary_proof: {
      harness_mail_body_fields: 'forbidden',
      external_mail_body_fixture: 'present outside harness artifacts',
      mail_body_digest: sha256(externalMailBody),
      external_capability_bindings: [
        'mcp://onecontext/toolsets/mail',
        'mcp://onecontext/toolsets/wiki',
        'codex://skills/openai-docs',
        'codex://plugins/1context-agent-unit',
        'codex://connectors/1context-memory-db',
        'codex://apps/codex-desktop',
      ],
    },
    real_behavior: {
      births: true,
      agent_status: true,
      turn_lifecycle_and_usage: turnLifecycle.real_behavior,
      adapter_event_persistence: adapterEvidence.real_behavior,
      proof_status_from_persisted_events: adapterEvidence.real_behavior,
      transport_plan: transportPlan.real_behavior,
      retire_disposable_unit: retire.real_behavior,
    },
    retired_disposable_unit: retire.unit_id,
    turn_lifecycle: {
      unit_id: 'agent-mail-wiki-001',
      turn_id: turnLifecycle.turn_id,
      status: turnLifecycle.status,
      expected_usage: turnLifecycle.expected_usage,
    },
    gaps_waiting_on_implementation: [
      ...(adapterEvidence.real_behavior
        ? []
        : ['Persist adapter events through agent.harness.record-adapter-event instead of scaffold receipts.']),
      ...(adapterEvidence.real_behavior
        ? []
        : ['Use persisted adapter events in agent.harness.agent-status proof status.']),
      ...(turnLifecycle.real_behavior
        ? []
        : ['Add turn lifecycle and usage mutation receipts.']),
      ...(transportPlan.real_behavior ? [] : ['Replace transport-plan scaffold receipt with durable transport planning output.']),
    ],
  };

  await writeJson('harness-describe.json', describe.json);
  await writeJson('harness-agents.json', inventory.json);
  await writeJson('harness-agents-after-retire.json', inventoryAfterRetire.json);
  await writeJson('harness-agent-statuses-before-frontier.json', agentStatusesBeforeFrontier);
  await writeJson('harness-agent-statuses.json', agentStatuses);
  await writeJson('turn-lifecycle.json', turnLifecycle);
  await writeJson('adapter-event-recording.json', adapterEvidence);
  await writeJson('agent-status-after-frontier.json', mailWikiStatusAfterFrontier.json);
  await writeJson('transport-plan.json', transportPlan);
  await writeJson('harness-retire.json', retire.response);
  await writeJson('dogfood-boundary-scenario.json', scenario);
  await writeJson('external-capability-fixture.json', external);
  await writeJson('proof-summary.json', summary);

  console.log(JSON.stringify(summary, null, 2));
}

main()
  .catch(async (error) => {
    try {
      await writeJson('failure.json', {
        status: 'error',
        code: error.code || 'agent_harness_boundary_dogfood_failed',
        message: error.message,
      });
    } catch {
      // Ignore evidence write failures while reporting the primary error.
    }
    console.error(error.stack || error.message);
    process.exitCode = 1;
  })
  .finally(async () => {
    if (!keepRuntime) {
      await rm(devHome, { recursive: true, force: true });
    }
  });
