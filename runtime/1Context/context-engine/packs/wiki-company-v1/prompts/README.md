# Prompts

Prompts are first-class configuration.

Keep prompt text in this folder instead of burying it inside TOML. TOML should name prompt files and say how a harness or agent uses them; Markdown should carry the actual instructions.

Use this folder for:

- Harness prompts that keep an agent oriented inside the Codex app-server harness.
- Agent prompts that define role, taste, process, and domain behavior.
- Job or state-machine prompt fragments when a task needs reusable wording.

Prompts are not lived-experience. A prompt says how to behave. Lived-experience says what was previously done.

Prompts should be versioned by this pack. Runtime turns should reference prompt
files through the harness birth certificate instead of copying loose prompt
trees into top-level `context-engine/` folders.
