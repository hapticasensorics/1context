# Attention Runner Validation Loop

This page describes the deterministic loop for judging the first attention runner against the current one-minute fixture.

## Fixture

- Session manifest: `docs/assets/attention-capture-mockup/attention-debug-20260524-215739/attention-dashboard-session.json`
- Candidate frame set: `frames-2fps`, about 120 candidates across a 59.978 second run
- Runner output schema: `attention-ledger.v3`

## Smoke Test

Run the package test before judging a new algorithm change:

```bash
cargo test -p onecontext-attention-runner
```

The integration test runs `onecontext-attention-runner` against the fixture, writes output to a temp JSON file, parses it, and checks the minimum dashboard contract:

- `version` is `attention-ledger.v3`
- `raw_buffer_audit` is non-empty and stays near 120 candidates for the 2fps fixture
- `saved_states` is non-empty
- `algorithms` contains a summary with candidate and save counts
- every saved state carries `proof_bundle.raw_event_refs` and raw provenance paths

## Manual Judge Loop

Generate a fresh output when you want to judge behavior by eye:

```bash
cargo run -p onecontext-attention-runner -- \
  --session docs/assets/attention-capture-mockup/attention-debug-20260524-215739/attention-dashboard-session.json \
  --out docs/assets/attention-capture-mockup/attention-debug-20260524-215739/attention-filter-output.json
```

Then open the attention dashboard for the same session, scrub the one-minute video, and label decisions:

- `must_save`: the runner should have saved this moment.
- `missed_save`: a dropped raw-buffer candidate should have become a saved state.
- `bad_save`: a saved state is noise, misleading, redundant, or not memory-worthy.

After labeling, change the algorithm, regenerate the output, and rerun `cargo test -p onecontext-attention-runner`. The smoke test proves the output still loads in dashboard-shaped JSON; Paul’s labels are the source of truth for whether the attention policy got better.

## What This Proves

The loop proves the runner can consume the current fixture deterministically, emit parseable `attention-ledger.v3` JSON, preserve the candidate buffer the dashboard needs, produce at least one saved state, and attach raw event proof refs to saved states.

It does not prove the saves are good, the regions are correct, or the policy has acceptable recall. Those judgments come from comparing the saved states and raw buffer against Paul’s `must_save`, `missed_save`, and `bad_save` labels.
