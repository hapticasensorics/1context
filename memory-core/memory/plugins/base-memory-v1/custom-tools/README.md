# Custom Tools

Custom tools are plugin-defined callable capabilities. They are different from the native tools already provided by a harness such as Codex or Claude Code.

Harness-native tools are declared on the harness and are assumed when an agent chooses that harness. They should stay broad and semantic, such as `workspace.read`, `workspace.write`, `patch.apply`, or `shell.exec`, so Codex and Claude can improve their internal tool implementations without breaking this plugin.

Custom tools are exact contracts. Use them when a prompt alone is too vague or too weak to enforce a capability, such as querying a raw-data database.

## Folder Contract

```text
custom-tools.toml  custom tool contracts
<tool-id>/         optional portable implementation, schemas, examples, or notes
```

## Contract Fields

```text
id             stable capability name, such as raw_data.query
kind           command, mcp, or api
description    what the tool does
entrypoint      command/module/server when the plugin provides the implementation
input_schema    relative path to a JSON schema when input is structured
output_schema   relative path to a JSON schema when output is structured
permissions     semantic permissions this tool exercises
dependencies    dependency ids from ../dependencies/dependencies.toml
accounts        root account ids from ../../../../accounts.toml when a tool needs host auth
```

Provider accounts used to run models are host-only by default. Custom-tool dependencies are different: declare plugin needs such as local databases in `../dependencies/dependencies.toml`. If a custom tool needs host auth, reference a root account id in `accounts`. Expose the custom tool only when an agent or job requests it and the host grants it.

Agents and jobs list custom tools directly:

```toml
tools = [
  "raw_data.query",
]
```

There are no tool packs in this seed. A little repetition in agent and job definitions is better than another abstraction before the state machines are real.
