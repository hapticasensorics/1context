# Prompts

Prompts are first-class configuration.

Keep prompt text in this folder instead of burying it inside TOML. TOML should name prompt files and say how a harness or agent uses them; Markdown should carry the actual instructions.

Use this folder for:

- Harness prompts that keep an agent oriented inside a harness such as Codex or Claude.
- Agent prompts that define role, taste, process, and domain behavior.
- Job or state-machine prompt fragments when a task needs reusable wording.

Prompts are not lived-experience. A prompt says how to behave. Lived-experience says what was previously done.

Prompts should be versioned by the plugin. Runtime experiments can copy or reference prompt files through the `hired_agent.created` birth certificate.
