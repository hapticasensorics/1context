#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const root = takeFlag('--root') || process.env.ONECONTEXT_ROOT;
const wikiBin =
  takeFlag('--wiki-bin') ||
  process.env.ONECONTEXT_WIKI_CORE_BIN ||
  resolve(repoRoot, 'target/debug/onecontext-wiki');
const WIKI_TIMEOUT_MS = positiveInt(process.env.ONECONTEXT_MCP_WIKI_TIMEOUT_MS, 15_000);
const MAX_STDIO_BYTES = positiveInt(process.env.ONECONTEXT_MCP_WIKI_MAX_STDIO_BYTES, 1_048_576);
const MAX_PAYLOAD_TEXT_BYTES = positiveInt(process.env.ONECONTEXT_MCP_WIKI_MAX_PAYLOAD_TEXT_BYTES, 32_768);

if (!root) {
  writeProtocolError(null, -32602, 'onecontext wiki MCP server requires --root or ONECONTEXT_ROOT');
  process.exit(2);
}

const tools = [
  {
    name: 'wiki.list',
    description: 'List configured wiki pages and generated page inventory.',
    inputSchema: objectSchema({}, []),
  },
  {
    name: 'wiki.page.status',
    description: 'Read status metadata for a wiki page id or route.',
    inputSchema: objectSchema({
      page: stringSchema('Wiki page id or route.'),
    }, ['page']),
  },
  {
    name: 'wiki.agent.identify',
    description: 'Register or refresh a 1Context agent identity and lease.',
    inputSchema: objectSchema({
      thread_id: stringSchema('Runtime thread id for this agent.'),
      roles: arraySchema('Agent role/list/page addresses to grant.', stringSchema()),
      capabilities: arraySchema('Capability names to attach to this identity.', stringSchema()),
      ttl_seconds: integerSchema('Lease duration in seconds.', 60),
    }, ['thread_id']),
  },
  {
    name: 'wiki.agent.inbox',
    description: 'List actionable mail delivery envelopes visible to an agent.',
    inputSchema: objectSchema({
      agent_id: stringSchema('1Context agent id.'),
    }, ['agent_id']),
  },
  {
    name: 'wiki.mail.open',
    description: 'Open one delivery and return a body delivery request for host injection.',
    inputSchema: objectSchema({
      delivery_id: stringSchema('Delivery id to open.'),
      agent_id: stringSchema('1Context agent id.'),
    }, ['delivery_id', 'agent_id']),
  },
  {
    name: 'wiki.mail.claim',
    description: 'Claim one visible delivery before doing work.',
    inputSchema: objectSchema({
      delivery_id: stringSchema('Delivery id to claim.'),
      agent_id: stringSchema('1Context agent id.'),
    }, ['delivery_id', 'agent_id']),
  },
  {
    name: 'wiki.mail.mark',
    description: 'Mark a claimed delivery read, done, archived, or rejected.',
    inputSchema: objectSchema({
      delivery_id: stringSchema('Delivery id to mark.'),
      agent_id: stringSchema('1Context agent id.'),
      state: enumSchema(['read', 'done', 'archived', 'rejected']),
    }, ['delivery_id', 'agent_id', 'state']),
  },
  {
    name: 'wiki.notify.poll',
    description: 'Read pending notification hints for an agent.',
    inputSchema: objectSchema({
      agent_id: stringSchema('1Context agent id.'),
      cursor: stringSchema('Optional notification cursor.'),
    }, ['agent_id']),
  },
  {
    name: 'wiki.notify.ack',
    description: 'Acknowledge a notification hint after the delivery was handled.',
    inputSchema: objectSchema({
      notification_id: stringSchema('Notification id to acknowledge.'),
      agent_id: stringSchema('1Context agent id.'),
    }, ['notification_id', 'agent_id']),
  },
  {
    name: 'wiki.talk.append',
    description: 'Append talk context and optionally deliver it as durable mail.',
    inputSchema: objectSchema({
      page: stringSchema('Wiki page id.'),
      kind: stringSchema('Talk message kind.'),
      subject: stringSchema('Message subject.'),
      from: stringSchema('Sender address.'),
      body: stringSchema('Markdown body.'),
      to: arraySchema('Recipient addresses.', stringSchema()),
      cc: arraySchema('CC recipient addresses.', stringSchema()),
      reply_to: stringSchema('Message id being replied to.'),
      operation_id: stringSchema('Idempotency key.'),
      delivery_mode: enumSchema(['labels-only', 'mail']),
    }, ['page', 'subject', 'from', 'body']),
  },
];
const toolsByName = new Map(tools.map((tool) => [tool.name, tool]));

function takeFlag(name) {
  const index = args.indexOf(name);
  if (index === -1) return null;
  const value = args[index + 1];
  args.splice(index, 2);
  return value || null;
}

function positiveInt(value, fallback) {
  const parsed = Number.parseInt(String(value ?? ''), 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function stringSchema(description = '') {
  return { type: 'string', description };
}

function integerSchema(description = '', minimum = 0) {
  return { type: 'integer', minimum, description };
}

function arraySchema(description, items) {
  return { type: 'array', items, default: [], description };
}

function enumSchema(values) {
  return { type: 'string', enum: values };
}

function objectSchema(properties, required) {
  return {
    type: 'object',
    additionalProperties: false,
    properties,
    required,
  };
}

function response(id, result) {
  return { jsonrpc: '2.0', id, result };
}

function errorResponse(id, code, message, data = undefined) {
  return {
    jsonrpc: '2.0',
    id,
    error: {
      code,
      message,
      ...(data === undefined ? {} : { data }),
    },
  };
}

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function writeProtocolError(id, code, message, data) {
  write(errorResponse(id, code, message, data));
}

function toolResult(payload, isError = false) {
  return {
    content: [
      {
        type: 'text',
        text: JSON.stringify(payload),
      },
    ],
    structuredContent: payload,
    isError,
  };
}

function required(argsObject, key) {
  const value = argsObject?.[key];
  if (typeof value !== 'string' || !value.trim()) {
    throw new Error(`Missing required string argument: ${key}`);
  }
  return value;
}

function optionalArray(argsObject, key) {
  const value = argsObject?.[key];
  if (value == null) return [];
  if (!Array.isArray(value)) throw new Error(`${key} must be an array`);
  return value.map(String).filter(Boolean);
}

function optionalString(argsObject, key) {
  const value = argsObject?.[key];
  return typeof value === 'string' && value.trim() ? value : null;
}

function optionalInt(argsObject, key) {
  const value = argsObject?.[key];
  if (value == null) return null;
  const parsed = Number.parseInt(String(value), 10);
  if (!Number.isFinite(parsed)) throw new Error(`${key} must be an integer`);
  return parsed;
}

function commandSpec(args, cleanup = async () => {}) {
  return { args, cleanup };
}

async function writeTempBodyFile(body) {
  const tempDir = await mkdtemp(join(tmpdir(), 'onecontext-mcp-talk-'));
  const bodyPath = join(tempDir, 'body.md');
  try {
    await writeFile(bodyPath, body, { encoding: 'utf8', mode: 0o600 });
  } catch (error) {
    await rm(tempDir, { recursive: true, force: true }).catch(() => {});
    throw error;
  }
  return {
    path: bodyPath,
    cleanup: async () => {
      try {
        await rm(tempDir, { recursive: true, force: true });
      } catch {
        // Best-effort cleanup only; never fail the tool response because tmp removal failed.
      }
    },
  };
}

async function commandForTool(name, toolArgs) {
  switch (name) {
    case 'wiki.list':
      return commandSpec(['list']);
    case 'wiki.page.status':
      return commandSpec(['page-status', required(toolArgs, 'page')]);
    case 'wiki.agent.identify': {
      const command = ['agent-identify', '--thread-id', required(toolArgs, 'thread_id')];
      for (const role of optionalArray(toolArgs, 'roles')) command.push('--role', role);
      for (const capability of optionalArray(toolArgs, 'capabilities')) command.push('--capability', capability);
      const ttl = optionalInt(toolArgs, 'ttl_seconds');
      if (ttl) command.push('--ttl-seconds', String(ttl));
      return commandSpec(command);
    }
    case 'wiki.agent.inbox':
      return commandSpec(['agent-inbox', required(toolArgs, 'agent_id')]);
    case 'wiki.mail.open':
      return commandSpec(['mail-open', required(toolArgs, 'delivery_id'), '--agent-id', required(toolArgs, 'agent_id')]);
    case 'wiki.mail.claim':
      return commandSpec(['mail-claim', required(toolArgs, 'delivery_id'), '--agent-id', required(toolArgs, 'agent_id')]);
    case 'wiki.mail.mark':
      return commandSpec([
        'mail-mark',
        required(toolArgs, 'delivery_id'),
        '--agent-id',
        required(toolArgs, 'agent_id'),
        '--state',
        required(toolArgs, 'state'),
      ]);
    case 'wiki.notify.poll': {
      const command = ['notify-poll', required(toolArgs, 'agent_id')];
      const cursor = optionalString(toolArgs, 'cursor');
      if (cursor) command.push('--cursor', cursor);
      return commandSpec(command);
    }
    case 'wiki.notify.ack':
      return commandSpec(['notify-ack', required(toolArgs, 'notification_id'), '--agent-id', required(toolArgs, 'agent_id')]);
    case 'wiki.talk.append': {
      const body = required(toolArgs, 'body');
      const to = optionalArray(toolArgs, 'to');
      const cc = optionalArray(toolArgs, 'cc');
      const page = required(toolArgs, 'page');
      const subject = required(toolArgs, 'subject');
      const from = required(toolArgs, 'from');
      const kind = optionalString(toolArgs, 'kind') || 'proposal';
      const replyTo = optionalString(toolArgs, 'reply_to');
      const operationId = optionalString(toolArgs, 'operation_id');
      const deliveryMode = optionalString(toolArgs, 'delivery_mode');
      const bodyFile = await writeTempBodyFile(body);
      const command = [
        'talk-append',
        '--page',
        page,
        '--kind',
        kind,
        '--subject',
        subject,
        '--from',
        from,
        '--body-file',
        bodyFile.path,
      ];
      for (const recipient of to) command.push('--to', recipient);
      for (const recipient of cc) command.push('--cc', recipient);
      if (replyTo) command.push('--reply-to', replyTo);
      if (operationId) command.push('--operation-id', operationId);
      if (deliveryMode) command.push('--delivery-mode', deliveryMode);
      return commandSpec(command, bodyFile.cleanup);
    }
    default:
      throw new Error(`Unknown tool: ${name}`);
  }
}

function validateJsonRpcMessage(message) {
  if (!message || typeof message !== 'object' || Array.isArray(message)) {
    return 'JSON-RPC message must be an object';
  }
  if (message.jsonrpc !== '2.0') {
    return 'JSON-RPC message must include jsonrpc: "2.0"';
  }
  if (typeof message.method !== 'string' || !message.method.trim()) {
    return 'JSON-RPC message must include a string method';
  }
  if (
    Object.prototype.hasOwnProperty.call(message, 'id') &&
    message.id !== null &&
    typeof message.id !== 'string' &&
    typeof message.id !== 'number'
  ) {
    return 'JSON-RPC id must be a string, number, or null';
  }
  return null;
}

function validateToolArguments(tool, toolArgs) {
  const schema = tool.inputSchema || {};
  if (!toolArgs || typeof toolArgs !== 'object' || Array.isArray(toolArgs)) {
    throw new Error(`${tool.name} arguments must be an object`);
  }
  const properties = schema.properties || {};
  for (const key of schema.required || []) {
    if (!Object.prototype.hasOwnProperty.call(toolArgs, key)) {
      throw new Error(`Missing required argument: ${key}`);
    }
  }
  if (schema.additionalProperties === false) {
    for (const key of Object.keys(toolArgs)) {
      if (!Object.prototype.hasOwnProperty.call(properties, key)) {
        throw new Error(`Unexpected argument: ${key}`);
      }
    }
  }
  for (const [key, value] of Object.entries(toolArgs)) {
    const property = properties[key];
    if (!property || value == null) continue;
    if (property.type === 'string') {
      if (typeof value !== 'string') throw new Error(`${key} must be a string`);
      if (property.enum && !property.enum.includes(value)) {
        throw new Error(`${key} must be one of: ${property.enum.join(', ')}`);
      }
    } else if (property.type === 'integer') {
      if (!Number.isInteger(value)) throw new Error(`${key} must be an integer`);
      if (Number.isFinite(property.minimum) && value < property.minimum) {
        throw new Error(`${key} must be >= ${property.minimum}`);
      }
    } else if (property.type === 'array') {
      if (!Array.isArray(value)) throw new Error(`${key} must be an array`);
      const itemType = property.items?.type;
      if (itemType) {
        for (const [index, item] of value.entries()) {
          if (typeof item !== itemType) throw new Error(`${key}[${index}] must be a ${itemType}`);
        }
      }
    }
  }
}

async function runWiki(commandArgs) {
  const useBinary = existsSync(wikiBin);
  const command = useBinary ? wikiBin : 'cargo';
  const fullArgs = useBinary
    ? ['--root', root, ...commandArgs]
    : ['run', '-q', '-p', 'onecontext-wiki-daemon', '--', '--root', root, ...commandArgs];
  let child;
  try {
    child = spawn(command, fullArgs, { cwd: repoRoot, stdio: ['ignore', 'pipe', 'pipe'] });
  } catch (error) {
    return toolExecutionError(`failed to spawn wiki CLI: ${error.message}`);
  }

  let stdout = '';
  let stderr = '';
  let stdoutBytes = 0;
  let stderrBytes = 0;
  let timedOut = false;
  let outputExceeded = null;
  let killTimer = null;

  const terminateChild = () => {
    child.kill('SIGTERM');
    if (!killTimer) {
      killTimer = setTimeout(() => child.kill('SIGKILL'), 1_000);
      killTimer.unref?.();
    }
  };

  const appendLimited = (stream, chunk) => {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    const currentBytes = stream === 'stdout' ? stdoutBytes : stderrBytes;
    if (currentBytes + buffer.length > MAX_STDIO_BYTES) {
      const remaining = Math.max(0, MAX_STDIO_BYTES - currentBytes);
      if (remaining > 0) {
        const text = buffer.subarray(0, remaining).toString('utf8');
        if (stream === 'stdout') stdout += text;
        else stderr += text;
      }
      if (stream === 'stdout') stdoutBytes = MAX_STDIO_BYTES;
      else stderrBytes = MAX_STDIO_BYTES;
      outputExceeded ||= stream;
      terminateChild();
      return;
    }
    if (stream === 'stdout') {
      stdout += buffer.toString('utf8');
      stdoutBytes += buffer.length;
    } else {
      stderr += buffer.toString('utf8');
      stderrBytes += buffer.length;
    }
  };

  child.stdout.on('data', (chunk) => appendLimited('stdout', chunk));
  child.stderr.on('data', (chunk) => appendLimited('stderr', chunk));

  const timeout = setTimeout(() => {
    timedOut = true;
    terminateChild();
  }, WIKI_TIMEOUT_MS);
  timeout.unref?.();

  const close = await new Promise((resolveClose) => {
    child.on('error', (error) => resolveClose({ error }));
    child.on('close', (code, signal) => resolveClose({ code, signal }));
  });
  clearTimeout(timeout);
  if (killTimer) clearTimeout(killTimer);

  if (close.error) {
    return toolExecutionError(`wiki CLI process error: ${close.error.message}`, stderr);
  }
  if (timedOut) {
    return toolExecutionError(`wiki CLI timed out after ${WIKI_TIMEOUT_MS}ms`, stderr, 124);
  }
  if (outputExceeded) {
    return toolExecutionError(
      `wiki CLI ${outputExceeded} exceeded ${MAX_STDIO_BYTES} bytes`,
      stderr,
      124
    );
  }

  let parsed;
  try {
    parsed = JSON.parse(stdout);
  } catch {
    parsed = {
      status: 'error',
      stdout: truncateText(stdout),
      stderr: truncateText(stderr),
    };
  }
  return { code: close.code, parsed, stderr };
}

function truncateText(value) {
  const buffer = Buffer.from(value || '', 'utf8');
  if (buffer.length <= MAX_PAYLOAD_TEXT_BYTES) return value || '';
  return `${buffer.subarray(0, MAX_PAYLOAD_TEXT_BYTES).toString('utf8')}\n...[truncated]`;
}

function toolExecutionError(message, stderr = '', code = 1) {
  return {
    code,
    parsed: {
      status: 'error',
      message,
      stderr: truncateText(stderr),
    },
    stderr,
  };
}

async function handle(message) {
  const { id, method, params } = message;
  if (method === 'initialize') {
    return response(id, {
      protocolVersion: params?.protocolVersion || '2025-06-18',
      capabilities: { tools: {} },
      serverInfo: { name: 'onecontext', version: '0.1.0' },
    });
  }
  if (method === 'notifications/initialized') {
    return null;
  }
  if (method === 'tools/list') {
    return response(id, { tools });
  }
  if (method === 'tools/call') {
    const name = params?.name;
    const toolArgs = params?.arguments || {};
    if (typeof name !== 'string') {
      return errorResponse(id, -32602, 'tools/call requires params.name');
    }
    const tool = toolsByName.get(name);
    if (!tool) {
      return errorResponse(id, -32602, `Unknown tool: ${name}`);
    }
    let command;
    try {
      validateToolArguments(tool, toolArgs);
      command = await commandForTool(name, toolArgs);
    } catch (error) {
      return errorResponse(id, -32602, error.message);
    }
    try {
      const result = await runWiki(command.args);
      return response(id, toolResult(result.parsed, result.code !== 0));
    } finally {
      await command.cleanup();
    }
  }
  return errorResponse(id, -32601, `Unknown method: ${method}`);
}

const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
let messageQueue = Promise.resolve();

lines.on('line', async (line) => {
  if (!line.trim()) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch (error) {
    writeProtocolError(null, -32700, `Invalid JSON: ${error.message}`);
    return;
  }
  messageQueue = messageQueue.then(() => handleMessage(message));
});

async function handleMessage(message) {
  try {
    const protocolError = validateJsonRpcMessage(message);
    if (protocolError) {
      write(errorResponse(message?.id ?? null, -32600, protocolError));
      return;
    }
    if (!Object.prototype.hasOwnProperty.call(message, 'id')) {
      if (message.method === 'notifications/initialized') await handle(message);
      return;
    }
    const reply = await handle(message);
    if (reply) write(reply);
  } catch (error) {
    write(errorResponse(message.id ?? null, -32603, error.message));
  }
}
