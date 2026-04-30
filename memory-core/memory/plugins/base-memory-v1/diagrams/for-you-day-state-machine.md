## What This Map Shows

This is the first concrete `ai_state_machine` definition in 1context:
`state_machines/for_you_day.py`.

It models one day of the For You memory loop. A day starts by discovering
active hours, fans out hourly witness jobs, waits until those jobs are done or
skipped, then runs daily review jobs.

## How to read it

- The day has a small local state: `pending`, `discovering_hours`,
  `writing_hourlies`, `reviewing`, `complete`, or `blocked`.
- Hourly witnesses run in parallel, one per active hour.
- Hourly completion is evidence-based: each active hour produces a talk entry
  or a skip.
- The daily editor reads the hourlies and writes a synthesis/reply.
- The concept scout reads the same material and proposes concepts or questions.
- User messages, activity ticks, job failure, and approval needs are events,
  not hidden prompt text.

## Why it matters

This keeps the control layer separate from agent creativity. Agents decide how
to write and reason inside a job; the state machine decides when jobs wake, what
they may read/write, and what evidence closes the day.
