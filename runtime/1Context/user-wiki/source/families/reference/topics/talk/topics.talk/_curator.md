# Topics Curator - Job Prompt Template

The system prompt is `context-engine/packs/wiki-company-v1/prompts/agent-profile.md`.
Your job is to read proposals on a Topics talk folder and apply accepted
taxonomy changes to the Topics index article.

## Inputs

- Article: `user-wiki://source/families/reference/topics/source/topics.md`
- Talk folder: `user-wiki://source/families/reference/topics/talk/topics.talk`
- Concept page directory: `user-wiki://source/families/reference`
- Proposal glob: `user-wiki://source/families/reference/topics/talk/topics.talk/*.proposal.topic-*.md`
- Existing decision glob: `user-wiki://source/families/reference/topics/talk/topics.talk/*.decision.*.md`

## Scope

You maintain the index. You do not write concept page bodies unless the task
explicitly widens scope.

You may read the article, the talk folder, prior decisions, the list of concept
page filenames, and evidence files explicitly cited by proposals.

## Processing Order

Process proposals oldest first by filename.

For each undecided proposal:

1. Read the proposed topic, category, slug, description, and evidence.
2. Check whether the topic is durable enough to index.
3. Decide: apply, refine and apply, defer, reject, supersede, merge, split, or move.
4. Edit the Topics article only for accepted changes.
5. Add a decision entry to the talk folder.

## Article Sections

The default Topics schema is:

- Engineering
- Infrastructure
- Process
- Tools
- Domain
- People
- Organizations

Entries are usually one line:

```markdown
- [{{ Topic Name }}](concept/{{ topic_slug }}) - {{ one_line_description }}
```

If the concept page does not exist yet:

```markdown
- `{{ topic_slug }}` - {{ one_line_description }}. *(concept page pending)*
```

## Decision Entry Shape

Create a new markdown file:

```text
{{ now_utc_slug }}.decision.{{ proposal_slug }}.md
```

with frontmatter:

```yaml
---
kind: decision
author: "{{ agent_id }}"
created: "{{ now_utc }}"
parent: "{{ proposal_stem }}"
decision: "{{ apply|refine_apply|defer|reject|supersede|merge|split|move }}"
state: resolved
---
```

## Output Summary

When finished, report edited sections, decision files created, and any proposals
left open.
