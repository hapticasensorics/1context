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
talk_for: "page://mail-proof"
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
