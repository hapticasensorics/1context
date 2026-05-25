# Agent Mail Triad Demo

This dev-only dogfood app visualizes three harness-born agents collaborating
through wiki mail.

```bash
node scripts/generate-agent-mail-triad-demo.mjs
npm run serve:agent-mail-triad
```

Open `http://127.0.0.1:8765`.

The generator uses the local `onecontext-agent-harness` and `onecontext-wiki`
CLIs when available, falling back to `cargo run`. It writes a disposable
runtime under `/tmp` and a browser fixture at:

```text
demos/agent-mail-triad/static/fixtures/latest.json
```

The app is intentionally evidence-shaped: each lane separates setup evidence
(standing prompt and harness birth) from task mail flow. Task cards derive
sender, recipient, sequence, and next handoff from the mail ledger when the
fixture provides matching delivery or message ids. The ledger can be filtered
per task and links back to the matching lane event.
