# Wiki Interface

This folder is the memory-side adapter for wiki-facing work.

The important rule is simple: memory code should use the portable Rust wiki
core for wiki semantics. Python may write memory proposals and receipts, but it
should not own a second implementation of page placement, template fallback,
talk files, sitemap, or publish-status rules.

## Current Surface

- `core_client.WikiCoreClient` calls `onecontext-wiki` and returns JSON objects.
- `talk_append` accepts `body_markdown` or `body_file`, plus optional
  `reply_to` and `thread_id` values. Use
  `reply_to=<message_id>` for parented replies; use `thread_id=<thread_id>` only
  when an agent is intentionally targeting an existing thread without a specific
  parent message. Omitting both keeps the legacy subject-derived thread.
- `wiki_ensure`, `wiki_list`, `wiki_page_status`, `wiki_page_open`, `wiki_page_create`,
  `wiki_page_write_body`, `wiki_page_patch_body`, `wiki_page_delete`,
  `wiki_page_restore`, `wiki_reference_list`, `wiki_publish_status`, `wiki_publish`, `wiki_agent_identify`,
  `wiki_agent_register`, `wiki_agent_heartbeat`, `wiki_agent_retire`,
  `wiki_agent_whoami`, `wiki_agent_list`, `wiki_agent_status`,
  `wiki_agent_inbox`, `wiki_agent_claim`, `wiki_talk_append`, `wiki_mail_inbox`,
  `wiki_mail_subscribe`, `wiki_mail_unsubscribe`, `wiki_mail_subscriptions`,
  `wiki_mail_mark`, `wiki_mail_claim`, `wiki_mail_mark_all`, `wiki_page_watch`,
  `wiki_page_unwatch`, `wiki_page_assign_role`,
  `wiki_list_create`, `wiki_list_status`, `wiki_list_members`,
  `wiki_notify_poll`, and `wiki_notify_ack` are convenience wrappers over the
  same client.
- The class client also wraps the current V0 collaboration surface: agent
  register/identify/heartbeat/retire/whoami/list/status/inbox, talk append,
  agent-level claim, mail inbox/subscribe/unsubscribe/subscriptions/claim/mark/mark-all,
  page watch/unwatch/role assignment, list create/status/members, and notifications. `list_status`
  accepts `include_archived` and `include_snoozed` when an agent needs the
  hidden-message audit view for a list.
- `authoring.py` is transitional. It still writes route-plan, proposal,
  decision, preview, promotion, and legacy talk-entry records until those
  records are either moved into the Rust core or retired.
- `request_wiki_refresh` remains a daemon bridge for the current render queue.

## Consumer Notes

The adapter intentionally returns the Rust core receipt with minimal shaping.
For agent code, the most reliable loop is:

1. Use `wiki_page_create` with explicit `route`, `family_group`,
   `family_id`, `nav_section`, and `nav_order`.
2. Read the returned `page_status` and `hashes` for placement,
   template/custom/edit/render flags, tombstone state, validation,
   `next_action`, and the current `source_sha256`.
3. Call `wiki_page_open` when you need full editable resource handles, hashes,
   or the page's placement envelope. It returns `title`, `route`, `collection`,
   `type`, and a nested `page_status` so agents do not have to infer route
   metadata from handles.
4. Use `wiki_page_write_body` or `wiki_page_patch_body`. Pass inline
   `body_markdown`/`find`/`replace` for small edits, or
   `body_file`/`find_file`/`replace_file` when an agent is already working from
   markdown artifacts. Both receipts return fresh `page_status` and `hashes`,
   so chained patches can use the returned `hashes.source_sha256` directly.
5. Call `wiki_publish_status`, then `wiki_publish` when page content,
   tombstones, or `wiki.toml` changed.
6. Use `wiki_reference_list` after publish when an agent needs citeable
   page resources: inline images, downloadable files, web/wiki links, inline
   code blocks, and footnote-style citations from the rendered site.
7. Use `wiki_talk_append` with inline `body_markdown` for short notes or
   `body_file` for prepared markdown. The adapter requires exactly one body
   source, matching page body writes.
8. Treat talk, inbox, notifications, claims, and mail state changes as
   collaboration metadata. They should return `render_required=false` and keep
   `wiki_publish_status.next_action` at `none` unless page content, tombstones,
   or configuration also changed.
9. Recheck `wiki_page_status` or `wiki_list` after publish/delete/restore.

Dogfood note: current receipts are usable but still slightly uneven in naming.
`next_action` uses the compact token `publish`, and `content_state` uses
`edited` while `flags.user_edited` carries the boolean edit signal. When a CLI
call fails, catch `WikiCoreError`; its original JSON envelope stays available
as `payload`, while `operation`, `error_code`, `error_message`, and
`repair_hints` expose the fields an agent usually needs for the next retry.

## Ownership

This folder does not own:

- configured-page creation from `wiki.toml`
- source/talk folder placement
- page template fallback
- future mail, inbox, notification, or agent-directory state
- the JavaScript renderer
- Swift app hosting, last-good serving, and Apple-specific supervision
- local web APIs
- installed runtime defaults

Those behaviors are owned by `crates/onecontext-wiki-core`,
`crates/onecontext-wiki-daemon`, `wiki-engine/`, and the macOS host.
