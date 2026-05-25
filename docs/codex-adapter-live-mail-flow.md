# Codex Adapter Live Mail Flow Dogfood

This dogfood runner proves that 1Context mail can wake a real Codex app-server
thread and deliver the opened body through the host injection lane.

Run:

```bash
node scripts/test-codex-adapter-live-mail-flow.mjs
```

Use `--build` if `target/debug/onecontext-wiki` is missing.

## Proven Flow

1. Start a real `codex app-server --listen stdio://`.
2. Create an ephemeral Codex thread with model `gpt-5.4-mini` by default.
3. Register a live wiki agent identity whose transport points at that thread.
4. Send real 1Context mail to the agent's role address.
5. Poll the durable notification queue.
6. Start a live low-effort Codex turn and wait for the matching
   `turn/started` notification.
7. Run `wiki.notify.dispatch` with a local dispatcher command. The command
   forwards the steering payload to the dogfood runner, which calls live
   `turn/steer` with the active turn id.
8. Open the delivery with `wiki.mail.open`.
9. Deliver the authorized opened body to Codex through live
   `thread/inject_items`.
10. Record the injection receipt, claim and mark the delivery done, then ack
    the notification.

The proof summary stores ids, hashes, statuses, and transcript counts. It
does not store the raw mail body, background turn prompt, or steering text.

Latest known proof:

```text
test-results/codex-adapter-live-mail-flow-20260525T112717Z/proof-summary.json
```

