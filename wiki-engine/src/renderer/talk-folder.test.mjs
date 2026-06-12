import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';

import { renderTalkFolder } from './talk-folder.mjs';

function writeFixtureFile(path, contents) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents);
}

test('talk-folder renderer surfaces mail metadata and attachment links', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-talk-mail-meta-'));
  try {
    const folder = resolve(tmp, 'mail-proof.talk');
    mkdirSync(resolve(folder, 'attachments/talkmsg_ds_meta'), { recursive: true });
    writeFixtureFile(resolve(folder, '_meta.yaml'), `title: Talk - Mail Proof
slug: mail-proof.talk
section: reference
access: private
talk_route: /mail-proof/talk
`);
    writeFixtureFile(
      resolve(folder, '2026-05-20T14-37Z.proposal.render-mail-metadata.md'),
      `---
id: "talkmsg_ds_meta"
kind: "proposal"
author: "agent://worker-ds/author"
created: "2026-05-20T14:37:00Z"
talk_for: "mailbox://page/mail-proof"
thread: "thread_mail_proof_render"
subject: "Render mail metadata"
state: open
recipients:
  - "agent://worker-ds/curator"
  - "list://worker-ds.reviewers"
attachments:
  - filename: "handoff.eml"
    media_type: "text/plain"
    path: "attachments/talkmsg_ds_meta/handoff.eml"
    handle: "user-wiki://page/mail-proof/talk/attachments/talkmsg_ds_meta/handoff.eml"
    caption: "Mail handoff"
    alt_text: "Mail handoff alternate text"
---

The rendered talk page should expose this inbox handoff.

## Attachments

- [handoff.eml](/mail-proof/talk/attachments/talkmsg_ds_meta/handoff.eml) (text/plain) - Mail handoff
`
    );

    const { bodyHtml, mdAssembled, entries } = renderTalkFolder(folder);

    assert.equal(entries[0].ts, '2026-05-20T14:37:00Z');
    assert.match(bodyHtml, /talkmsg_ds_meta/);
    assert.match(bodyHtml, /thread_mail_proof_render/);
    assert.match(bodyHtml, /agent:\/\/worker-ds\/curator/);
    assert.match(bodyHtml, /list:\/\/worker-ds\.reviewers/);
    assert.match(bodyHtml, /href="\/mail-proof\/talk\/attachments\/talkmsg_ds_meta\/handoff\.eml"/);
    assert.match(bodyHtml, /text\/plain/);
    assert.match(bodyHtml, /Mail handoff alternate text/);
    assert.equal(
      (bodyHtml.match(/href="\/mail-proof\/talk\/attachments\/talkmsg_ds_meta\/handoff\.eml"/g) || []).length,
      1
    );
    assert.match(mdAssembled, /Message: talkmsg_ds_meta/);
    assert.match(mdAssembled, /To: agent:\/\/worker-ds\/curator, list:\/\/worker-ds\.reviewers/);
    assert.match(mdAssembled, /\[handoff\.eml]\(\/mail-proof\/talk\/attachments\/talkmsg_ds_meta\/handoff\.eml\) \(text\/plain\) — Mail handoff; alt: Mail handoff alternate text/);
    assert.equal(
      (mdAssembled.match(/\[handoff\.eml]\(\/mail-proof\/talk\/attachments\/talkmsg_ds_meta\/handoff\.eml\)/g) || []).length,
      1
    );
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('talk-folder renderer surfaces handle-only attachments', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-talk-handle-attachment-'));
  try {
    const folder = resolve(tmp, 'topics.talk');
    writeFixtureFile(resolve(folder, '_meta.yaml'), `title: Talk - Topics
slug: topics.talk
section: reference
access: private
talk_route: /topics/talk
`);
    writeFixtureFile(
      resolve(folder, '2026-05-20T15-00Z.proposal.handle-only.md'),
      `---
id: "talkmsg_handle_only"
kind: "proposal"
author: "agent://worker-ds/author"
created: "2026-05-20T15:00:00Z"
talk_for: "mailbox://page/topics"
thread: "thread_handle_only"
subject: "Handle only attachment"
state: open
attachments:
  - handle: "user-wiki://page/topics/talk/attachments/talkmsg_handle_only/evidence.txt"
    media_type: "text/plain"
    caption: "Evidence"
---

The attachment has only the durable handle from mail.
`
    );

    const { bodyHtml, mdAssembled, entries } = renderTalkFolder(folder);

    assert.equal(entries[0].attachments[0].filename, 'evidence.txt');
    assert.equal(entries[0].attachments[0].path, 'attachments/talkmsg_handle_only/evidence.txt');
    assert.match(bodyHtml, /href="\/topics\/talk\/attachments\/talkmsg_handle_only\/evidence\.txt"/);
    assert.match(mdAssembled, /\[evidence\.txt]\(\/topics\/talk\/attachments\/talkmsg_handle_only\/evidence\.txt\)/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('talk-folder renderer preserves orphaned replies', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-talk-orphan-reply-'));
  try {
    const folder = resolve(tmp, 'topics.talk');
    writeFixtureFile(resolve(folder, '_meta.yaml'), `title: Talk - Topics
slug: topics.talk
section: reference
access: private
talk_route: /topics/talk
`);
    writeFixtureFile(
      resolve(folder, '2026-05-20T15-30Z.reply.missing-parent.md'),
      `---
id: "talkmsg_orphan_reply"
kind: "reply"
author: "agent://worker-ds/author"
created: "2026-05-20T15:30:00Z"
talk_for: "mailbox://page/topics"
thread: "thread_missing_parent"
parent: "talkmsg_parent_that_is_not_here"
subject: "Missing parent reply"
state: open
---

This reply must remain visible even when the parent id cannot hydrate.
`
    );

    const { bodyHtml } = renderTalkFolder(folder);

    assert.match(bodyHtml, /Orphaned replies/);
    assert.match(bodyHtml, /talkmsg_parent_that_is_not_here/);
    assert.match(bodyHtml, /This reply must remain visible/);
    assert.doesNotMatch(bodyHtml, /No discussion yet/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});
