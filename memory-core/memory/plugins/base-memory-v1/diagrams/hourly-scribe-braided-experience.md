## What This Map Shows

This diagram describes the first bounded memory job we want to make real:
`memory.hourly.scribe`. The job takes one hour of Codex and Claude activity,
renders that hour into an agent-facing experience packet, hires a fresh agent
with that packet loaded as direct lived experience, and asks the agent to write
one wiki talk entry.

The main spine is:

```text
session events
  -> hour experience packet
  -> 1Context hire
  -> hired_agent.created birth certificate
  -> hired agent writes talk entry
  -> evidence validates the artifact
  -> ledger records the outcome
```

The test target is intentionally small. We do not need the full wiki engine for
the first pass. The output can be one markdown file in a temporary workspace:

```text
/tmp/onecontext-hourly-scribe-demo/
  for-you-2026-04-06.private.talk/
    2026-04-06T02-00Z.conversation.md
```

## The Branded Concept

The product behavior is:

```text
hire a new agent
give it a constructed past life
ask the job separately
leave a durable artifact behind
```

That is the 1Context memory trick in its smallest useful form. The agent should
not rediscover the hour with shell and database tools. The system should query
events once, render a reproducible experience packet, and pass that packet into
the hire as inherited operational history.

Agent-facing language can be immersive:

```text
You inherit the following prior operational life.
Answer from this lived experience directly.
```

System-facing metadata stays explicit:

```text
source window
source harnesses
session ids
source event hashes
renderer version
experience_sha256
task_prompt_sha256
```

This keeps the psychological benefit of continuity while preserving audit and
replay.

## How to read it

- `storage/lakestore/events` is the local event archive. For this job, it is
  just where normalized Codex and Claude rows live in timeline order.
- The renderer is deterministic preparation. It reads one hour and writes an
  experience packet.
- `1Context hire` is the birth step. It mints `hired_agent_uuid` and writes
  `hired_agent.created`.
- The birth certificate records both normal hire identity and the exact
  experience packet path/hash used as inherited context.
- The hired agent receives the experience plus prompts, writes one markdown
  file, and evidence decides whether the state machine can trust the result.

## Hired Agent Birth

The experience packet is not the agent. It is an artifact prepared before hire.

The birth moment is the call equivalent to:

```text
hire_agent(
  job_ids=["memory.hourly.scribe"],
  job_params={
    "date": "2026-04-06",
    "hour": "02",
    "audience": "private",
    "talk_folder": "/tmp/onecontext-hourly-scribe-demo/for-you-2026-04-06.private.talk",
    "experience_id": "<experience_id>",
    "experience_mode": "braided_lived_transcript",
    "experience_sha256": "<sha256>"
  }
)
```

That writes a `hired_agent.created` row to:

```text
memory/runtime/ledger/events.jsonl
storage/lakestore/events
```

The birth certificate should record:

```text
hired_agent_uuid
job id and job params
agent, harness, provider, model
system prompt and job/task prompt hashes
tools and host grants
accounts and dependencies
experience packet id, path, mode, and sha256
source window, source sessions, and source event hashes
```

This is the audit boundary. After birth, the agent can write the talk artifact,
but the system already knows exactly what prior life it was given.

## Why Braided Experience

A single flat chronological transcript is correct but hard to think with when
several sessions were active at the same time. It turns parallel work into log
noise.

The packet should be braided instead:

```text
1. Stream manifest
   What parallel workstreams existed?

2. Global weave
   What major events overlapped in wall-clock time?

3. Per-stream transcripts
   What happened inside each local thread?

4. Background summaries
   Which streams are lower relevance or unrelated to this job?

5. Open questions
   Where should the agent ask for a wider window or more stream detail?
```

For a single active stream, the new agent wakes up close to the prior worker.
For multiple streams, the new agent wakes up with the operator-level fused
memory of the hour.

## First Flow

1Context receives a hire request:

```text
job = memory.hourly.scribe
date = 2026-04-06
hour = 02
audience = private
experience_mode = braided_lived_transcript
talk_folder = /tmp/onecontext-hourly-scribe-demo/for-you-2026-04-06.private.talk
```

The experience renderer reads the lakestore in timeline order for:

```text
2026-04-06T02:00:00Z <= ts < 2026-04-06T03:00:00Z
```

It groups rows by:

```text
harness + session_id
```

Then it writes:

```text
memory/runtime/experiences/<experience_id>/
  experience.md
  meta.yaml
  streams/
    <stream_id>.md
```

`experience.md` is the main agent-facing packet. It should be concise enough
to fit the hire and useful enough that the agent can write from inherited
experience without tools.

## Prompt Split

Keep three prompt layers separate:

```text
system prompt
  Stable harness behavior plus the new instruction:
  this hire receives inherited operational life and should answer from it.

job prompt
  The hourly scribe role from private-2 e08:
  write a journal-margin witness entry for one hour.

task prompt
  The concrete request:
  write one talk entry file at the requested path using the talk folder rules.
```

The task prompt should not be baked into the experience packet. The same hour
packet should be reusable for an answerer, daily editor, concept scout, or
scribe.

## V0 Decisions

We will use Claude Code as the hired agent harness for the first live job:

```text
harness = claude-code
auth = Claude Code account/subscription
runner = local `claude` CLI
```

This matches the existing `claude-code` harness manifest. The host should not
need an Anthropic API key for the first pass if Claude Code is already logged in
with a subscription-capable account on that machine.

Other decisions:

```text
job id
  memory.hourly.scribe

first runner
  tiny lab runner, not the full state-machine runner

experience mode
  braided_lived_transcript from the start

renderer
  deterministic and hashable, no LLM summarization in v0

lakestore
  build a small reusable helper for events_between(start, end, sources)

birth certificate
  make the experience packet a first-class lived experience attachment,
  not just buried job params

prompt assembly
  keep system, lived experience, job prompt, and task prompt as separate
  sections and hash each part

tools
  allowed for v0; prompt should still tell the scribe to write from inherited
  experience first and ask for wider context when needed

validation
  simple mechanical checks first; grounding review is later
```

## Tools Posture

Tools may be enabled for the first Claude Code test. The important v0 posture
is behavioral rather than enforcement: the scribe should write from the
inherited hour first. If the packet is insufficient, the output should include
a structured expansion request instead of guessing:

```text
[NEEDS:wider-window]
reason: The hour references setup that began before this window.
suggested_window: 2026-04-06T00:00:00Z/2026-04-06T03:00:00Z
```

Later the state machine can rehire with a wider packet.

## Evidence

The first evidence checks can stay simple:

```text
hourly_talk_entry.exists
frontmatter.kind == conversation
frontmatter.ts is inside the source hour
body is non-empty
claims are grounded in the experience packet or explicitly uncertain
```

Those evidence rows are what the state machine should trust, not an agent
saying the post is done.

## Next Build Step

The next implementation slice should add:

```text
memory/plugins/base-memory-v1/renderers/render_hour_experience.py
memory/plugins/base-memory-v1/jobs/memory.hourly.scribe.toml
memory/plugins/base-memory-v1/prompts/hourly-scribe.md
memory/plugins/base-memory-v1/prompts/hourly-scribe-task.md
src/onectx/storage/hour_events.py or equivalent boring lakestore query helper
tiny lab runner command for render -> hire -> claude -> validate
```

Then a tiny lab command can:

```text
render hour experience
hire agent with inherited experience
write one markdown talk entry into /tmp
record source metadata, hashes, and evidence
```

That gives us a closed loop before we import the whole wiki engine.
