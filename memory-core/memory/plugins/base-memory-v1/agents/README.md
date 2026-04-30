# Agents

Agents are named profiles. An agent chooses a harness, provider/model, prompts, references, lived experience, memory policy, and any custom tools it needs.

Prompt files live in `../prompts/`. Use `prompt_paths` for agent-specific instruction files; harness orientation prompts are declared by the selected harness.

Harness-native tools are inherited from the selected harness. Do not list Codex or Claude Code built-ins here. The `tools` field is for plugin-defined custom tools such as `raw_data.query`.

Example shape:

```toml
id = "memory-researcher"
version = "0.1.0"
harness = "codex-harness"
provider = "openai"
model = "gpt-5.5"
tools = ["raw_data.query"]
prompt_paths = ["prompts/memory-researcher.md"]
experience = []

[memory]
mode = "persistent"
attach = "last_for_job"
```
