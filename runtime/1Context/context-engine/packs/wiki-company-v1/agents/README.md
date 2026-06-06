# Agents

Agents are named profiles. An agent chooses a harness, provider/model, prompts, references, lived experience, memory policy, and any custom tools it needs.

Prompt files live in `../prompts/`. Use `prompt_paths` for agent-specific instruction files; harness orientation prompts are declared by the selected harness.

Harness-native tools are inherited from the selected harness. Do not list Codex
app-server built-ins here. The `tools` field is for pack-defined custom tools
or additional capabilities that the job must explicitly request.

Example shape:

```toml
id = "memory-researcher"
version = "0.1.0"
harness = "codex-app-server"
provider = "codex"
model = "gpt-5.5"
tools = ["raw_data.query"]
prompt_paths = ["prompts/memory-researcher.md"]
experience = []

[memory]
mode = "persistent"
attach = "last_for_job"
```
