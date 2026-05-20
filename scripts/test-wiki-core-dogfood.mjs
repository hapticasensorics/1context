#!/usr/bin/env node
import { createConnection } from 'node:net';
import { createWriteStream } from 'node:fs';
import { cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { request } from 'node:http';
import { basename, join, resolve } from 'node:path';
import { spawn } from 'node:child_process';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);
const args = new Set(process.argv.slice(2));
const shouldBuild = args.has('--build');
const keepRuntime = args.has('--keep-runtime');
const leavePublished = args.has('--leave-published');
const timestamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
const shortStamp = timestamp.replace(/^.*T(\d{6})Z$/, '$1').toLowerCase();
const evidenceDir = resolve(
  process.env.ONECONTEXT_WIKI_DOGFOOD_EVIDENCE_DIR ||
    join(repoRoot, 'test-results', `wiki-core-dogfood-${timestamp}`)
);
const devHome = resolve(
  process.env.ONECONTEXT_WIKI_DOGFOOD_HOME ||
    join('/tmp', `1cw-dogfood-${shortStamp}`)
);
const runtimeRoot = join(devHome, '1Context');
const socketPath = resolve(
  process.env.ONECONTEXT_WIKI_DOGFOOD_SOCKET ||
    join('/tmp', `1cw-dogfood-${shortStamp}.sock`)
);
const wikiCoreBin = resolve(process.env.ONECONTEXT_WIKI_CORE_BIN || join(repoRoot, 'target/debug/onecontext-wiki'));
const daemonBin = resolve(process.env.ONECONTEXT_WIKI_DOGFOOD_DAEMON_BIN || join(repoRoot, 'macos/.build/debug/1contextd'));

const children = [];
const rpcLog = [];

function usage() {
  console.log(`Usage: node scripts/test-wiki-core-dogfood.mjs [--build] [--keep-runtime] [--leave-published]

Runs a disposable live-daemon wiki dogfood loop:
  create/edit/list/watch/role/publish/talk/mail/notify/http/delete.

Use --leave-published --keep-runtime when you want rendered files to remain
available for manual or in-app browser inspection. The script still stops its
temporary HTTP server on exit; restart serve-site against the emitted app_mirror
path for later browser checks. Without --leave-published, the script tombstones
the page and verifies the route disappears before stopping the server.

Environment:
  ONECONTEXT_WIKI_DOGFOOD_EVIDENCE_DIR  evidence output directory
  ONECONTEXT_WIKI_DOGFOOD_HOME          fake home directory; user data is <home>/1Context
  ONECONTEXT_WIKI_DOGFOOD_SOCKET        daemon socket path; defaults to short /tmp path
  ONECONTEXT_WIKI_CORE_BIN              Rust wiki core binary
  ONECONTEXT_WIKI_DOGFOOD_DAEMON_BIN    debug 1contextd binary
`);
}

if (args.has('--help') || args.has('-h')) {
  usage();
  process.exit(0);
}

function run(command, commandArgs, options = {}) {
  return new Promise((resolvePromise, reject) => {
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
    child.on('error', reject);
    child.on('close', (code) => {
      if (code === 0) {
        resolvePromise({ stdout, stderr });
      } else {
        reject(new Error(`${command} ${commandArgs.join(' ')} exited ${code}\n${stdout}\n${stderr}`));
      }
    });
  });
}

async function maybeBuild() {
  if (!shouldBuild) {
    const missing = [];
    if (!existsSync(wikiCoreBin)) missing.push(wikiCoreBin);
    if (!existsSync(daemonBin)) missing.push(daemonBin);
    if (missing.length > 0) {
      throw new Error(`Missing dogfood binary/binaries:\n${missing.join('\n')}\nRun with --build or build them first.`);
    }
    return;
  }
  await run('cargo', ['build', '--package', 'onecontext-wiki-daemon']);
  await run('swift', ['build', '--package-path', 'macos', '--product', '1contextd', '--product', '1context']);
}

async function waitFor(predicate, timeoutMs, label) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (await predicate()) return;
    await new Promise((resolveTimeout) => setTimeout(resolveTimeout, 100));
  }
  throw new Error(`Timed out waiting for ${label}`);
}

async function startProcess(command, commandArgs, name, env = {}) {
  const stdout = createWriteStream(join(evidenceDir, `${name}.stdout.log`));
  const stderr = createWriteStream(join(evidenceDir, `${name}.stderr.log`));
  const child = spawn(command, commandArgs, {
    cwd: repoRoot,
    env: { ...process.env, ...env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  children.push(child);
  child.stdout.pipe(stdout);
  child.stderr.pipe(stderr);
  child.on('exit', (code, signal) => {
    stderr.write(`\n[${name}] exited code=${code} signal=${signal}\n`);
  });
  return child;
}

async function call(method, params = {}) {
  return new Promise((resolveCall) => {
    const id = rpcLog.length + 1;
    const started = Date.now();
    const client = createConnection(socketPath);
    let data = '';
    client.on('connect', () => {
      client.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    });
    client.on('data', (chunk) => {
      data += chunk.toString('utf8');
      if (data.includes('\n')) client.end();
    });
    client.on('error', (error) => {
      const entry = { id, method, params, transport_error: error.message, ms: Date.now() - started };
      rpcLog.push(entry);
      resolveCall(entry);
    });
    client.on('end', () => {
      let response;
      try {
        response = JSON.parse(data.trim());
      } catch (error) {
        response = { parse_error: error.message, raw: data };
      }
      const entry = {
        id,
        method,
        params,
        response,
        ok: Boolean(response.result && !response.error),
        ms: Date.now() - started,
      };
      rpcLog.push(entry);
      resolveCall(entry);
    });
  });
}

function errorMessage(entry) {
  const error = entry.response?.error || entry.transport_error;
  if (!error) return '';
  if (typeof error === 'string') return error;
  return error.message || JSON.stringify(error);
}

function result(entry) {
  if (!entry.response?.result) {
    throw new Error(`${entry.method} failed: ${errorMessage(entry) || JSON.stringify(entry)}`);
  }
  return entry.response.result;
}

async function expectError(label, method, params, expected) {
  const entry = await call(method, params);
  const message = errorMessage(entry);
  if (!message) throw new Error(`${label} unexpectedly succeeded`);
  if (expected && !message.includes(expected)) {
    throw new Error(`${label} error mismatch. Wanted ${expected}, got ${message}`);
  }
  return { label, method, message };
}

async function httpGet(urlPath) {
  const base = await readFile(join(evidenceDir, 'serve-url.txt'), 'utf8');
  const url = new URL(urlPath, base.trim());
  return new Promise((resolveGet, reject) => {
    const req = request(url, { method: 'GET' }, (res) => {
      let body = '';
      res.setEncoding('utf8');
      res.on('data', (chunk) => {
        body += chunk;
      });
      res.on('end', () => {
        resolveGet({
          url: url.toString(),
          status: res.statusCode,
          content_type: res.headers['content-type'] || '',
          body,
        });
      });
    });
    req.on('error', reject);
    req.end();
  });
}

async function writeEvidence(name, value) {
  await writeFile(join(evidenceDir, name), JSON.stringify(value, null, 2) + '\n');
}

async function prepareRuntime() {
  await rm(devHome, { recursive: true, force: true });
  await rm(socketPath, { force: true });
  await mkdir(devHome, { recursive: true });
  await cp(join(repoRoot, 'runtime/1Context'), runtimeRoot, { recursive: true });
  await writeFile(join(evidenceDir, 'dev-home.txt'), devHome + '\n');
  await writeFile(join(evidenceDir, 'runtime-root.txt'), runtimeRoot + '\n');
}

async function startDaemon() {
  await startProcess(daemonBin, [], '1contextd', {
    ONECONTEXT_DEV_RUNTIME_HOME: devHome,
    ONECONTEXT_DEV_SOCKET_PATH: socketPath,
    ONECONTEXT_WIKI_CORE_BIN: wikiCoreBin,
  });
  await waitFor(() => existsSync(socketPath), 10_000, `daemon socket ${socketPath}`);
  await writeFile(join(evidenceDir, 'socket.txt'), socketPath + '\n');
}

async function startStaticServer(appMirror) {
  const portFile = join(evidenceDir, 'serve-port.txt');
  await rm(portFile, { force: true });
  await rm(join(evidenceDir, 'serve-url.txt'), { force: true });
  await startProcess('node', ['wiki-engine/tools/serve-site.mjs', appMirror], 'serve-site', {
    PORT: '0',
    PORT_FILE: portFile,
  });
  await waitFor(() => existsSync(portFile), 10_000, 'static wiki server port');
  const port = (await readFile(portFile, 'utf8')).trim();
  const serveUrl = `http://127.0.0.1:${port}`;
  await writeFile(join(evidenceDir, 'serve-url.txt'), serveUrl + '\n');
  return serveUrl;
}

async function main() {
  await mkdir(evidenceDir, { recursive: true });
  await maybeBuild();
  await prepareRuntime();
  await startDaemon();

  const pageId = `dogfood-harness-${shortStamp}`;
  const route = `/${pageId}`;
  const listAddress = `list://${pageId}.reviewers`;
  const inputDir = join(evidenceDir, 'inputs');
  await mkdir(inputDir, { recursive: true });
  const bodyFile = join(inputDir, 'page-body.md');
  const findFile = join(inputDir, 'find.md');
  const replaceFile = join(inputDir, 'replace.md');
  const talkBodyFile = join(inputDir, 'talk-body.md');
  const attachmentFile = join(inputDir, 'handoff.eml');
  await writeFile(bodyFile, [
    '# Dogfood Harness',
    '',
    'This page was created by the reusable wiki dogfood runner.',
    '',
    'Related pages: [Topics](/topics), [Projects](/projects), [Your Context](/your-context).',
    '',
    'Patch anchor: pending.',
    '',
  ].join('\n'));
  await writeFile(findFile, 'Patch anchor: pending.');
  await writeFile(replaceFile, 'Patch anchor: replaced by hash-checked patch.');
  await writeFile(talkBodyFile, [
    'The reusable dogfood runner is checking page creation, mail delivery, talk rendering, and deletion.',
    '',
    'This message should be visible on the rendered talk route after a force publish.',
    '',
  ].join('\n'));
  await writeFile(attachmentFile, [
    'From: dogfood@local',
    'To: curator@local',
    'Subject: Dogfood harness attachment',
    '',
    'This .eml attachment should open inline as text/plain.',
    '',
  ].join('\n'));

  const summary = {
    schema_version: 1,
    evidence_dir: evidenceDir,
    dev_home: devHome,
    runtime_root: runtimeRoot,
    page_id: pageId,
    route,
    list_address: listAddress,
    results: {},
    expected_errors: {},
  };

  summary.results.health = result(await call('health'));
  summary.results.validate = result(await call('wiki.validate'));
  summary.results.initial_list = result(await call('wiki.list'));
  const initialPages = new Map(summary.results.initial_list.pages.map((page) => [page.id, page]));
  for (const runtimeDefaultId of ['for-you', 'your-context', 'projects', 'topics']) {
    const page = initialPages.get(runtimeDefaultId);
    if (!page) throw new Error(`Missing runtime default page in wiki.list: ${runtimeDefaultId}`);
    if (page.origin !== 'runtime_default' || !page.flags.runtime_default || page.flags.custom_created) {
      throw new Error(`Runtime default origin proof failed for ${runtimeDefaultId}: ${JSON.stringify({
        origin: page.origin,
        flags: page.flags,
      })}`);
    }
    if (!page.template?.relative_path || !page.template?.sha256) {
      throw new Error(`Runtime default template proof failed for ${runtimeDefaultId}: ${JSON.stringify(page.template)}`);
    }
  }
  for (const generatedPageId of ['home', 'this-week', 'open-questions']) {
    const page = initialPages.get(generatedPageId);
    if (!page) throw new Error(`Missing generated site page in wiki.list: ${generatedPageId}`);
    if (page.kind !== 'generated_site_page' || page.flags.source_backed || page.origin !== 'generated_site_page') {
      throw new Error(`Generated site page inventory proof failed for ${generatedPageId}: ${JSON.stringify({
        kind: page.kind,
        origin: page.origin,
        flags: page.flags,
      })}`);
    }
    if (!page.template?.relative_path) {
      throw new Error(`Generated site page template proof failed for ${generatedPageId}: ${JSON.stringify(page.template)}`);
    }
  }
  if (
    summary.results.initial_list.page_count !== summary.results.initial_list.pages.length ||
    summary.results.initial_list.source_page_count < 4 ||
    summary.results.initial_list.generated_page_count < 3
  ) {
    throw new Error(`wiki.list page counts are inconsistent: ${JSON.stringify({
      page_count: summary.results.initial_list.page_count,
      source_page_count: summary.results.initial_list.source_page_count,
      generated_page_count: summary.results.initial_list.generated_page_count,
      pages_length: summary.results.initial_list.pages.length,
    })}`);
  }
  summary.results.generated_home_status = result(await call('wiki.page.status', { page: 'home' }));
  if (summary.results.generated_home_status.kind !== 'generated_site_page') {
    throw new Error(`Generated page status proof failed: ${JSON.stringify(summary.results.generated_home_status)}`);
  }
  if (!summary.results.generated_home_status.template?.relative_path) {
    throw new Error(`Generated page status template proof failed: ${JSON.stringify(summary.results.generated_home_status.template)}`);
  }
  summary.expected_errors.missing_patch_find = await expectError(
    'missing patch find',
    'wiki.page.patch_body',
    { page: 'topics' },
    'params.find'
  );
  summary.expected_errors.missing_talk_body = await expectError(
    'missing talk body',
    'wiki.talk.append',
    { page: 'missing-page', kind: 'proposal', subject: 'Missing body', from: 'agent://dogfood', toRole: 'curator' },
    'body'
  );
  summary.expected_errors.invalid_template = await expectError(
    'invalid template',
    'wiki.page.create',
    { id: `${pageId}-bad`, title: 'Bad Template', route: `${route}-bad`, template: '../bad.md' },
    'template path escapes templates/'
  );

  const author = result(await call('wiki.agent.identify', {
    threadId: `thread-${pageId}-author`,
    role: 'role://dogfood-author',
    capability: 'dogfood',
  }));
  const curator = result(await call('wiki.agent.identify', {
    threadId: `thread-${pageId}-curator`,
    role: 'role://curator',
    capability: 'review',
  }));
  summary.results.author = author;
  summary.results.curator = curator;

  summary.results.page_create = result(await call('wiki.page.create', {
    id: pageId,
    title: 'Dogfood Harness',
    route,
    slug: pageId,
    familyGroup: 'dogfood',
    familyGroupTitle: 'Dogfood',
    familyId: 'harness',
    familyTitle: 'Harness',
    type: 'context',
    template: 'pages/context-page.md',
    talkConventionsTemplate: 'talk/conventions/topics.md',
    talkCuratorTemplate: 'talk/curators/topics.md',
    summary: 'Reusable daemon dogfood proof page.',
    navSection: 'hidden',
    navOrder: '99',
  }));
  if (
    summary.results.page_create.page_status?.origin !== 'created_from_template' ||
    summary.results.page_create.page_status?.flags?.runtime_default ||
    !summary.results.page_create.page_status?.flags?.custom_created ||
    !summary.results.page_create.page_status?.template?.relative_path ||
    !summary.results.page_create.page_status?.template?.sha256
  ) {
    throw new Error(`Custom page origin proof failed: ${JSON.stringify(summary.results.page_create.page_status)}`);
  }
  summary.results.page_write = result(await call('wiki.page-write-body', { page: pageId, bodyFile }));
  const opened = result(await call('wiki.page.open', { page: pageId }));
  summary.results.page_open_after_write = opened;
  summary.expected_errors.stale_hash = await expectError(
    'stale hash',
    'wiki.page-patch-body',
    { page: pageId, findFile, replaceFile, expectedSourceSha256: 'stale' },
    'source hash mismatch'
  );
  summary.results.page_patch = result(await call('wiki.page-patch-body', {
    page: pageId,
    findFile,
    replaceFile,
    expectedSourceSha256: opened.edit.expected_source_sha256,
  }));
  summary.results.list_create = result(await call('wiki.list-create', {
    address: listAddress,
    title: 'Dogfood Harness Reviewers',
    description: 'Disposable review list for dogfood proof.',
    page: route,
    owner: author.agent_id,
  }));
  summary.results.lists_by_route = result(await call('wiki.lists', { page: route }));
  summary.results.lists_by_id = result(await call('wiki.lists', { page: pageId }));
  if (summary.results.lists_by_route.list_count !== 1 || summary.results.lists_by_id.list_count !== 1) {
    throw new Error(`List route/id canonicalization proof failed for ${pageId}`);
  }
  summary.results.assign_role = result(await call('wiki.page-assign-role', {
    page: route,
    agentId: curator.agent_id,
    role: 'curator',
    kind: 'proposal',
  }));
  summary.results.watch = result(await call('wiki.page-watch', {
    page: route,
    agentId: author.agent_id,
    list: listAddress,
    kind: 'proposal',
  }));
  summary.results.list_status = result(await call('wiki.list-status', { list: listAddress }));
  summary.results.list_members = result(await call('wiki.list-members', { list: listAddress }));
  const listCreatedAt = summary.results.list_create.list?.created_at;
  if (
    !listCreatedAt ||
    summary.results.lists_by_route.lists?.[0]?.created_at !== listCreatedAt ||
    summary.results.lists_by_id.lists?.[0]?.created_at !== listCreatedAt ||
    summary.results.list_status.list?.created_at !== listCreatedAt ||
    summary.results.list_members.list?.created_at !== listCreatedAt
  ) {
    throw new Error(`List created_at summary proof failed for ${listAddress}`);
  }
  summary.results.publish = result(await call('wiki.publish', { trigger: 'wiki-core-dogfood-script', force: true }));
  summary.results.talk_append = result(await call('wiki.talk-append', {
    page: route,
    message: {
      kind: 'proposal',
      subject: 'Dogfood harness review',
      fromAddress: author.primary_address,
      toRoles: ['curator'],
      cc: [listAddress],
      bodyFile: talkBodyFile,
      attachments: [{ path: attachmentFile }],
    },
  }));
  summary.results.publish_status_after_talk = result(await call('wiki.publish.status'));
  summary.results.curator_inbox = result(await call('wiki.agent.inbox', { agentId: curator.agent_id }));
  summary.results.list_inbox = result(await call('wiki.mail-inbox', { recipient: listAddress }));
  summary.results.mail_read = result(await call('wiki.mail-read', { messageId: summary.results.talk_append.message_id }));
  summary.results.notify_poll = result(await call('wiki.notify-poll', { agentId: curator.agent_id }));
  if (
    summary.results.curator_inbox.threads?.[0]?.attachment_count !== 1 ||
    summary.results.curator_inbox.messages?.[0]?.attachments?.length !== 1
  ) {
    throw new Error(`Agent inbox attachment summary proof failed: ${JSON.stringify(summary.results.curator_inbox.threads?.[0])}`);
  }
  const notification = summary.results.notify_poll.notifications?.[0];
  if (notification?.attachment_count !== 1) {
    throw new Error(`Notification attachment summary proof failed: ${JSON.stringify(notification)}`);
  }
  const notificationId = notification?.notification_id;
  if (!notificationId) {
    throw new Error(`Notification id missing from poll result: ${JSON.stringify(notification)}`);
  }
  summary.results.notify_ack = result(await call('wiki.notify-ack', { agentId: curator.agent_id, notificationId }));
  summary.results.notify_poll_after_ack = result(await call('wiki.notify-poll', { agentId: curator.agent_id }));
  if (summary.results.notify_poll_after_ack.notification_count !== 0) {
    throw new Error(`Notification ack did not clear curator wakeup: ${JSON.stringify(summary.results.notify_poll_after_ack)}`);
  }
  summary.results.claim = result(await call('wiki.agent.claim', {
    agentId: curator.agent_id,
    messageId: summary.results.talk_append.message_id,
  }));
  summary.results.mark_all = result(await call('wiki.mail-mark-all', {
    messageId: summary.results.talk_append.message_id,
    state: 'done',
  }));
  summary.results.publish_after_talk = result(await call('wiki.publish', {
    trigger: 'wiki-core-dogfood-script-talk-render',
    force: true,
  }));

  const appMirror = join(devHome, 'Library/Application Support/1Context/wiki-site/current');
  summary.app_mirror = appMirror;
  await writeFile(join(evidenceDir, 'app-mirror.txt'), appMirror + '\n');
  const serveUrl = await startStaticServer(appMirror);
  const messageId = summary.results.talk_append.message_id;
  const attachmentPath = `${route}/talk/attachments/${messageId}/${basename(attachmentFile)}`;
  const routeChecks = [
    { path: route, mustContain: ['Dogfood Harness', 'reusable wiki dogfood runner', 'Patch anchor: replaced by hash-checked patch'] },
    {
      path: `${route}/talk`,
      mustContain: ['Dogfood harness review', 'This message should be visible', basename(attachmentFile), 'text/plain'],
      mustNotContain: ['application/octet-stream'],
    },
    { path: attachmentPath, mustContain: ['Subject: Dogfood harness attachment', 'open inline as text/plain'] },
  ];
  const httpProof = { serve_url: serveUrl, checks: [] };
  for (const check of routeChecks) {
    const response = await httpGet(check.path);
    const ok = response.status === 200 &&
      check.mustContain.every((needle) => response.body.includes(needle)) &&
      (check.mustNotContain || []).every((needle) => !response.body.includes(needle));
    httpProof.checks.push({
      path: check.path,
      status: response.status,
      content_type: response.content_type,
      ok,
      must_contain: check.mustContain,
      must_not_contain: check.mustNotContain || [],
      excerpt: response.body.slice(0, 500),
    });
    if (!ok) throw new Error(`HTTP proof failed for ${check.path}`);
  }
  if (!httpProof.checks.find((check) => check.path === attachmentPath)?.content_type.includes('text/plain')) {
    throw new Error('Attachment did not serve as text/plain');
  }

  if (leavePublished) {
    httpProof.delete_check = {
      skipped: true,
      reason: '--leave-published keeps the rendered page available for browser inspection',
    };
  } else {
    summary.results.page_delete = result(await call('wiki.page.delete', { page: pageId, mode: 'tombstone' }));
    summary.results.publish_after_delete = result(await call('wiki.publish', {
      trigger: 'wiki-core-dogfood-script-delete',
      force: true,
    }));
    summary.results.final_page_status = result(await call('wiki.page.status', { page: pageId }));
    const missingRoute = await httpGet(route);
    httpProof.delete_check = {
      path: route,
      status: missingRoute.status,
      ok: missingRoute.body.includes('missing route'),
      excerpt: missingRoute.body.slice(0, 200),
    };
    if (!httpProof.delete_check.ok) throw new Error(`Delete proof failed for ${route}`);
  }

  await writeEvidence('http-proof.json', httpProof);
  await writeEvidence('rpc-log.json', rpcLog);
  await writeEvidence('summary.json', summary);
  console.log(JSON.stringify({
    status: 'ok',
    evidence_dir: evidenceDir,
    page_id: pageId,
    route,
    served_during_run_url: serveUrl,
    app_mirror: appMirror,
    server_stops_on_exit: true,
    runtime_kept: keepRuntime,
    message_id: messageId,
    leave_published: leavePublished,
    delete_route_missing: Boolean(httpProof.delete_check.ok),
  }, null, 2));
}

async function cleanup() {
  for (const child of children.reverse()) {
    if (!child.killed) child.kill('SIGTERM');
  }
  await new Promise((resolveCleanup) => setTimeout(resolveCleanup, 150));
  for (const child of children.reverse()) {
    if (!child.killed) child.kill('SIGKILL');
  }
  if (!keepRuntime) {
    await rm(devHome, { recursive: true, force: true });
  }
}

process.on('SIGINT', async () => {
  await cleanup();
  process.exit(130);
});
process.on('SIGTERM', async () => {
  await cleanup();
  process.exit(143);
});

main()
  .catch(async (error) => {
    await writeEvidence('rpc-log.json', rpcLog).catch(() => {});
    console.error(error.stack || error.message);
    process.exitCode = 1;
  })
  .finally(cleanup);
