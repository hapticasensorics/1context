# Context Engine Storage Manifest

- Status: authoritative storage contract for the Rust Context Engine
- Owner: `onecontext-context-engine`
- Validation:
  - `cargo test -p onecontext-context-engine context_engine_paths_follow_storage_manifest`
  - `cargo test -p onecontext-context-engine storage_layout_rejects_retired_runtime_roots`
  - `cargo test -p onecontext-context-engine artifact_store_rejects_mutating_existing_key`

This manifest chooses `context-engine/live/runs/<run-id>/` as the canonical envelope
for everything produced by one execution.

The split is:

- `live/runs/<run-id>/` owns run-scoped evidence, model outputs, source packets,
  turn attempts, page-write proof, publish proof, and compact run-local state.
- `live/mail/` owns global Agent Mail truth. A run indexes mail ids; it does not
  duplicate canonical message bodies as a second truth.
- `live/agents/` owns Agent Mail identity, registration, leases, and policy.
- `packs/` and `orchestrators/` own static configuration.
- `live/state/` is service state only: current-run pointer, process logs, app-server
  event stream, cleanup receipts, and migration receipts.

## Canonical Layout

```text
context-engine/
  indexes/

  packs/<pack-id>/
    agents/
    jobs/
    harnesses/
    prompts/

  orchestrators/<orchestrator-id>/

  live/
    agents/
      directory/
        agents.jsonl
        leases.jsonl
      policies/

    mail/
      messages/YYYY/MM/*.json
      bodies/YYYY/MM/*.md
      deliveries.jsonl
      claims.jsonl
      idempotency.jsonl
      injection-receipts.jsonl
      control-events.jsonl
      dead-letter.jsonl
      mailboxes/addr-*/inbox.jsonl
      threads/*.jsonl             # Audit mirrors only.
      .mutation.lock

    notifications/
      outbox.jsonl
      attempts.jsonl
      cursors/

    runs/
      <run-id>/
        run.json                  # Compact run-local state and lifecycle.
        source-packets/
          index.json
          <packet-id>.md
          <packet-id>.json
        turns/
          <operation-id>/
            attempt-0001/
              prompt-manifest.json
              final-message.md
              final-message.sha256
              completion.json
              adapter-events.jsonl
              tool-transcript.jsonl
              error.json
              stdout.log
              stderr.log
            attempt-0002/
              ...
        artifacts/
          _content/sha256/<prefix>/<sha256>.json
          <kind>/<artifact-id>.json
        publish/
          page-write-<page-id>.json
          publish-proof.json
        state/
          raw-ingest-cursor.json
          wiki-memory-cursor.json
          packet-cache-index.json
          dependency-graph.json
          queues/
          wiki-memory-cache/
        mail-index.jsonl          # Run-local index of message_id/delivery_id.
        receipt-hydration.json

    state/
      service/
        current-run.json
        run-events.jsonl
        wiki-memory-cache/        # Optional cross-run service cache only.
      codex-app-server/
        threads/
        events.jsonl
      process-logs/
        <run-id>/<process-id>/
          stdout.log
          stderr.log
          exit.json
      cleanup-receipts.jsonl
      migration-receipts.jsonl

    archive/
      failed-runs/<archive-id>/
        manifest.json
        hashes.sha256
        runs/<run-id>/
        mail-index.jsonl
        user-wiki-talk/
        repair-notes.md

    tmp/<run-id>/agents/<agent-id>/
```

## Forbidden Roots

These roots are retired and must not be written by new runs:

- `context-engine/artifacts`
- `context-engine/source-packets`
- `context-engine/agents`
- `context-engine/archive`
- `context-engine/decisions`
- `context-engine/inbox`
- `context-engine/jobs`
- `context-engine/ledgers`
- `context-engine/mail`
- `context-engine/notifications`
- `context-engine/observations`
- `context-engine/runs`
- `context-engine/state`
- `context-engine/tmp`
- `context-engine/wiki-memory-cache`
- `context-engine/prompts`
- `context-engine/proposals`

`update-wiki` must fail when these roots exist as current runtime output. Move
or archive them first.

## Run Envelope Rules

- A run id is normalized with the Rust path-safe run-id normalizer before it is
  used in a path.
- `runs/<run-id>/run.json` is the compact run-local state file. It records
  status, lifecycle reason, cursors, completed phases, artifact handles, and
  authoritative mail receipts.
- Run status values are `active`, `completed`, `interrupted`, `failed`, and
  `archived`. A stopped run must be marked terminal before cleanup moves it.
- `state/service/current-run.json` is only a discovery pointer for UI and
  agents. It names the current run id and `runs/<run-id>/run.json`.
- All runtime timestamps are RFC3339 UTC with `Z`.
- `tmp/<run-id>/agents/<agent-id>` is disposable per-agent scratch. Agents in
  the same run must not share a writable scratch directory.

## Source Packets

- Source packets live only under `runs/<run-id>/source-packets`.
- `source-packets/index.json` records packet ids, content hashes, source window,
  event/session counts, cache keys, and links to run-local cursor files.
- `runs/<run-id>/state/raw-ingest-cursor.json` tracks native source import
  progress.
- `runs/<run-id>/state/wiki-memory-cursor.json` tracks bounded packet/company
  progress.
- `runs/<run-id>/state/packet-cache-index.json` tracks packet hash cache state.
- A packet cache hit is valid only when source-packet hash and prompt-provenance
  hash still match.
- Only raw-history roles may read source packet bodies. Downstream roles read
  compact artifacts, mail, talk, and prior wiki pages.

## Mail And Talk

- `mail/` is global Agent Mail truth. The tuple `message_id + delivery_id` owns
  identity.
- `runs/<run-id>/mail-index.jsonl` records run-local references to mail ids,
  delivery ids, claim ids, talk projection paths, and hydration status. It is an
  index, not a second mail store.
- Role mailboxes and direct agent mailboxes have different semantics:
  `role://...` may be claimed by any eligible live lease, while `agent://...`
  targets one durable agent identity.
- Agent leases in `agents/directory/leases.jsonl` are separate from harness turn
  state. Stale leases block notification polling, claim, done, and push
  eligibility. Recovery is append-only through new lease, retirement,
  dead-letter, or retry records.
- Published talk linkage is explicit: projected talk entries record page id,
  route, Agent Mail message id, delivery ids, and talk file path. Hydration must
  work from mail id to talk entry and from talk entry back to mail id.
- Generated talk entries must be distinguishable from human edits with
  frontmatter such as `generated: true`, agent id, run id, message id, and
  delivery id.

## Turns And Artifacts

- Turn attempts are append-only. A retry allocates `attempt-0002`,
  `attempt-0003`, and so on.
- Final-message receipts are path plus hash. A receipt is incomplete without
  `final_message_path`, `final_message_sha256`, and `final_message_origin`.
- `final_message_origin` distinguishes `model_output` from harness-synthesized
  fallback text such as empty-output or timeout summaries.
- Failed turns write `error.json` and whatever process/adapter evidence exists
  in that attempt directory.
- Run artifacts are immutable after creation. Repeating the same key with the
  same payload is idempotent; changed content needs a new key or attempt.
- `runs/<run-id>/artifacts/_content/sha256` is the content-addressed option for
  proof that must survive path moves.
- User-visible drafts and internal proof can both live under
  `runs/<run-id>/artifacts`, but their artifact metadata must identify the tier,
  page id or packet id, content hash, and source job.

## Publish Trail

- Page-write receipts live under `runs/<run-id>/publish`.
- Promotion from artifact to wiki mutation is a receipt chain: source artifact
  id/hash, editor/curator decision, page-write receipt, old/new page hashes, and
  publish proof.
- Accepted page bodies and human-edited talk remain in `user-wiki/source`.
  They are not cleaned as regenerated run output.

## Cleanup And Archive

- Failed runs move as whole run envelopes to
  `archive/failed-runs/<archive-id>/runs/<run-id>/`.
- Cleanup/archive must first prove quiescence: no active context-engine process
  for the run, no held mail mutation lock, no active harness turn, and no
  pending notification attempt for that run.
- `runs/<run-id>/receipt-hydration.json` records hydration of run artifacts,
  mail ids, talk projections, page-write receipts, and publish proof.
- `state/cleanup-receipts.jsonl` records cleanup/archive attempts, including
  failed moves, partial moves, retry idempotency keys, and repair hints.
- `state/migration-receipts.jsonl` records compatibility moves such as retired
  `agents/harness` material to `state/harness` or archive: old path, new path,
  reason, hash, actor, and timestamp.
- Do not hard-delete failed evidence unless it has an archive manifest and
  hashes.

## Size And Compaction

- Prompt manifests, tool transcripts, source packets, mail bodies, process logs,
  and app-server event streams must have writer-enforced size caps.
- Oversized records are represented by a redacted/truncated body with original
  hash, byte count, omitted byte count, and content-addressed spill object when
  retention is required.
- Append-only ledgers may be compacted only after receipt hydration proves all
  referenced mail, run artifacts, talk entries, page writes, and publish proofs
  still resolve or have archived hashes.
