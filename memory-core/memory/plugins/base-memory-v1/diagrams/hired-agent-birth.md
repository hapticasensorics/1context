## What This Map Shows

This diagram explains the birth event for one concrete hired agent. A hired agent is not just a model name. It is the resolved sum of job ids, concrete job params, agent config, harness, provider/model, prompts, custom tools, host grants, account choices, lived-experience, and linking policy.

## How to read it

- Job ids are human language. They say why the agent is being hired.
- Job params are invocation values. They say which hour, topic, source, branch, or artifact this particular hire is touching.
- The resolved config snapshot collects the pieces that define the agent at birth.
- The linker applies the attach mode: create a new runtime experience, reuse the last one for the job, attach manually, or run with no experience.
- The runtime experience stores native harness memory and the copied lived-experience seed.
- The ledger writes one `hired_agent.created` event with the new `hired_agent_uuid`.

## Why it matters

This is the provenance layer. Instead of tracking many unrelated ids, 1context uses one runtime identity, `hired_agent_uuid`, and records everything that created that identity in the birth certificate.
