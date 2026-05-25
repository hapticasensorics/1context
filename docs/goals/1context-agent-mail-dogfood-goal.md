# Milestone: Agent Mail Dogfood Harness

## Goal

Make the agent-mail protocol prove itself through a deterministic dogfood loop,
not a polite smoke test. The harness should exercise the system as an agent
would use it: identify, receive mail, get a wakeup, inspect inbox state, claim
work, mark it done, retry safely, and hit bad paths that should be refused.

This milestone is deliberately failure-seeking. A good run should produce
portable evidence under `test-results/`, and a bad run should name the protocol
invariant that failed.

## Done When

- A repo-local harness creates a disposable `1Context` runtime and never edits
  the real user-data tree.
- The harness covers labels-only talk, explicit role mail, notification poll,
  steering dispatch, notification ack, inbox visibility, claim, mark, snooze,
  duplicate operation ids, bad recipients, heartbeat, retire, stale agents, and
  unauthorized agents.
- The harness writes command logs, summary JSON, captured steering payloads,
  notification outbox rows, and attempt ledgers into ignored evidence files.
- The harness captures steering payloads for three distinct receiving agents and
  then has each agent read inbox, claim, mark, and ack its own delivery.
- Wrong-role agents and expired leases cannot claim, mark, snooze, or receive
  live notification dispatch.
- Retired agents cannot read unified inboxes or mutate deliveries.
- Talk/mail coordination does not force page publication; page content changes
  remain the publication boundary.

## Checklist

### 1. Baseline

- [x] Rust wiki core exposes agent identity, inbox, delivery state, notification
  polling, notification ack, and Codex steering payload construction.
- [x] Rust daemon CLI exposes `agent-identify`, `agent-inbox`, `mail-claim`,
  `mail-mark`, `mail-snooze`, `notify-poll`, `notify-ack`, and
  `notify-dispatch`.
- [x] Agent lifecycle has explicit status, heartbeat, and retire commands
  instead of relying on identify as a hidden lease-refresh path.
  Evidence: `npm run test:agent-mail:dogfood -- --build` passed with
  heartbeat/status/retire probes in
  `test-results/agent-mail-dogfood-20260521T091133Z/summary.json`.
- [x] Talk append defaults to labels-only and only creates delivery rows when
  `--delivery-mode mail` is explicit.
- [x] Delivery mutation enforces both live lease and recipient visibility.
  Evidence: `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`
  passed on 2026-05-21.
- [x] Unified inbox reads refuse retired agents instead of treating stale
  directory roles as durable access grants.
- [x] Notification polling reconciles open role mail after a stale agent
  heartbeats back into service.

### 2. Harness

- [x] Add `scripts/test-agent-mail-dogfood.mjs`.
- [x] Add a package script so the harness has one stable entrypoint.
- [x] Capture per-command JSON, steering output, notification ledgers, and a
  compact `summary.json`.
- [x] Make the harness fail with classified errors for authorization leaks,
  stale lease wakeups, duplicate idempotency regressions, or publication
  boundary regressions.
  Evidence: first failure captured
  `test-results/agent-mail-dogfood-20260521T085600Z/failure.json` with
  `authorization_bypass_claim`.

### 3. Closed-Loop Proof

- [x] Run the harness once before fixes and record any discovered protocol
  failures.
- [x] Patch high-signal failures in the smallest safe slice.
  Evidence: wrong-role delivery mutation now returns `cannot access`; stale
  lease notification/claim now returns `stale lease`.
- [x] Run targeted Rust tests for the mail core.
  Evidence: `cargo test -p onecontext-wiki-core agent_mail -- --nocapture`
  passed with 28 mail tests.
- [x] Re-run the dogfood harness to completion and record the evidence path.
  Evidence:
  `test-results/agent-mail-dogfood-20260521T085709Z/summary.json`.
- [x] Extend the dogfood run to prove stale-agent recovery via heartbeat and
  retired-agent refusal.
  Evidence:
  `test-results/agent-mail-dogfood-20260521T091133Z/summary.json`.
- [x] Extend the dogfood run to prove three independent receiving agents get
  concrete Codex steering hooks and can complete their own inbox work.
  Evidence:
  `test-results/agent-mail-dogfood-20260524T073416Z/summary.json` plus
  `captured-steering-squad-alpha.txt`, `captured-steering-squad-beta.txt`, and
  `captured-steering-squad-gamma.txt`.
- [x] Make the steering recipe real by adding `wiki.mail.open` and proving the
  dogfood loop opens the authorized delivery body before claim/mark/ack.
  Evidence: `test-results/agent-mail-dogfood-20260524T073416Z/commands.jsonl`
  has `mail-open` rows for curator, alpha, beta, and gamma deliveries.
- [x] Move opened body delivery onto the Codex injection lane.
  Evidence: `test-results/agent-mail-dogfood-20260525T033337Z/commands.jsonl`
  has `mail-open` rows whose top-level `message` object is bodyless
  (`body_sha256`/`body_bytes` only) and whose `content_delivery.method` is
  `thread/inject_items` with the authorized body inside the injected item.
- [x] Suppress competing same-role wakeups after one agent claims shared work.
  Evidence: `test-results/agent-mail-dogfood-20260524T073909Z/commands.jsonl`
  shows curator B has zero notifications after curator A claims the role
  delivery, and `notification-ledger/outbox.jsonl` records the competing
  notification as `suppressed`.

### 4. Exit

- [x] Update this checklist with final evidence.
- [x] Leave docs and harness names aligned with the agent-facing API.
- [x] Confirm no real user-data tree or shipped runtime was modified by the
  dogfood run.
  Evidence: harness copies `runtime/1Context` into `/tmp` and removes the fake
  home after a successful run unless `--keep-runtime` is passed.

## Notes

- Current baseline: the dogfood matrix is now codified as a repeatable harness
  with a passing evidence bundle.
- Additional verification: `cargo test -p onecontext-wiki-daemon -- --nocapture`
  and `swift test --package-path macos --filter WikiCoreRPCBridgeTests` passed
  on 2026-05-21, and `git diff --check` passed for the touched files.
