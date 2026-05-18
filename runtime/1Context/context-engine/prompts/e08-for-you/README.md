# e08 For You Prompt Templates

These are public-safe default prompt templates derived from the e08 role set.
They define reusable agent jobs, not filled user data.

The folder name preserves the experiment lineage. The files themselves must
stay neutral: no operator names, private project names, local machine paths, or
historical examples from any one user's wiki.

Use these prompts as shadow defaults. A user's `~/1Context/context-engine/`
copy may edit, replace, or fork them without app updates overwriting those
edits.

Prompt templates:

- `agent-profile.md` - shared operating contract for wiki agents
- `hourly.md` - turn an observation window into a signed hourly entry
- `hourly-answerer.md` - answer a bounded question from one observation window
- `editor.md` - propose daily or weekly page improvements
- `for-you-curator.md` - curate a For You page from accepted talk material
- `historian.md` - find longer-running arcs and open historical questions
- `librarian.md` - promote repeated entities into context, project, and topic pages
- `biographer.md` - maintain biography and life-story sections
- `contradiction-flagger.md` - surface conflicting claims for review
- `redactor.md` - classify and prepare private-to-public redactions
