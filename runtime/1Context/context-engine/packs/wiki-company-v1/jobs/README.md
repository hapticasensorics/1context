# Jobs

Jobs are reusable work contracts with human-readable ids. They choose an agent,
prompt fragments, permissions, expected inputs, expected outputs, and completion
states.

Runtime params belong in the Context Engine update request, mail thread, or
source packet that requested the work. They do not belong in the static job
definition.

Context Engine resolves a job by composing:

```text
job contract
+ agent identity/defaults
+ prompt bundle
+ current wiki/mail context
+ bounded source packet
= harness-born agent turn
```

Completion vocabulary:

```text
done
skip
no_change
needs_approval
failure
```

Jobs should describe what the turn is doing. The harness decides how the Codex
app-server turn is executed, and the agent profile decides who is doing it.
