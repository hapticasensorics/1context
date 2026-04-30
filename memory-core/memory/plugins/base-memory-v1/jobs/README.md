# Jobs

Jobs are reusable work contracts with human-readable ids. A run may carry more than one job id when that makes the work easier to understand.

A job usually chooses an agent and can add job-specific custom tools, permissions, inputs, outputs, prompt fragments, and completion states. Job ids should stay easy to say out loud. Concrete invocation values, such as date, hour, source path, or topic, are job params recorded only when an agent is hired. The ledger carries runtime provenance through `hired_agent_uuid` and the `hired_agent.created` birth certificate.

Job params are flat for v1:

```bash
uv run 1context hire --job memory.hourly --job-param date=2026-04-27 --job-param hour=13
```

When a single hire carries multiple job ids and the params would be ambiguous, prefix the key with the job id instead of adding another object model:

```bash
uv run 1context hire \
  --job memory.hourly \
  --job memory.concepts \
  --job-param memory.hourly.hour=13 \
  --job-param memory.concepts.slug=codex-harness
```

The runtime records those keys exactly. There is no hidden namespacing logic yet.

Some jobs are deterministic host jobs instead of hired-agent jobs. The first is
`memory.wiki.build_inputs`: it reads the markdown wiki workspace and concept
directory, then writes generated reader surfaces and a staging tree. It has no
agent, provider, or harness because the point is to make the corpus navigable
from existing state, not to ask another model to summarize it.

The wiki growth fabric now names the e08 role contracts as plugin jobs:

```text
memory.wiki.historian
memory.hourly.answerer
memory.wiki.for_you_curator
memory.wiki.context_curator
memory.wiki.librarian
memory.wiki.librarian_sweep
memory.wiki.biographer
memory.wiki.contradiction_flagger
memory.wiki.redactor
```

These are contract-complete before they are runner-complete: manifests,
agents, prompt paths, permissions, and completion states exist, but the generic
route-plan executor still needs to turn `role-route-plan.json` rows into
prepared hires.

Example shape:

```toml
id = "raw-memory-search"
version = "0.1.0"
agent = "memory-researcher"
tools = ["raw_data.query"]
inputs = ["sessions.db"]
outputs = ["memory.proposal"]
prompt_paths = ["prompts/jobs/raw-memory-search.md"]

[permissions]
read = ["memory/runtime/raw-data"]
write = ["memory/runtime/proposals"]
deny = ["answer_keys"]

[completion]
done = ["proposal_written"]
skip = ["no_relevant_memory"]
no_change = ["already_current"]
needs_approval = ["human_review_required"]
failure = ["missing_input", "tool_error"]
```

Completion states are vocabulary before enforcement. State machines should branch on these names when a real runner exists; until then they are part of the job contract and the birth certificate snapshot.
