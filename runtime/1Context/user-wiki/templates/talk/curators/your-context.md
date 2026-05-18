# Your Context Curator - Job Prompt Template

The system prompt is `context-engine/prompts/e08-for-you/agent-profile.md`.
Your job is to read proposals on the Your Context talk folder and apply accepted
changes to the Your Context article.

## Inputs

- Article: `{{ article_path }}`
- Talk folder: `{{ talk_folder }}`
- Talk metadata: `{{ talk_folder }}/_meta.yaml`
- Proposal glob: `{{ talk_folder }}/*.proposal.ycx-*.md`
- Existing decision glob: `{{ talk_folder }}/*.decided.*.md`

## Scope

You may read the article, the talk folder, prior decisions, and evidence files
explicitly cited by proposals when verification is needed.

Do not read unrelated talk folders, raw event stores, or other article bodies
unless the task explicitly widens scope.

## Processing Order

Process proposals in filename order. Filenames are timestamped, so this is the
chronological order of the page's history.

For each undecided proposal:

1. Read the proposal.
2. Identify the target section, proposed wording, and cited evidence.
3. Decide: apply, refine and apply, defer, reject, or supersede.
4. Edit the article only for applied or refined-and-applied proposals.
5. Add one decision entry to the talk folder.

## Article Sections

The default Your Context schema is:

- Working Style
- Coding Style
- Engineering Philosophy
- Preferences
- Taste
- Desires
- Recurring Ideas
- Habits
- Coworkers
- Infra And Tooling
- Standing Requests
- Notes For AI Agents
- Life Story

Replace an `<!-- empty: ... -->` placeholder only when a section receives its
first accepted content. After that, append or lightly merge. Do not collapse the
page into a fresh rewrite.

## Voice

Descriptive sections use third-person, present-tense, factual prose.
Prescriptive sections use direct collaborator guidance.

Keep claims traceable. Prefer compact references to talk entries, proposal ids,
or evidence ids over long quotations.

## Decision Entry Shape

Create a new markdown file:

```text
{{ now_utc_slug }}.decided.{{ proposal_slug }}.md
```

with frontmatter:

```yaml
---
kind: decided
author: "{{ agent_id }}"
created: "{{ now_utc }}"
parent: "{{ proposal_stem }}"
decision: "{{ apply|refine_apply|defer|reject|supersede }}"
status: closed
---
```

Body:

```markdown
## [DECIDED] {{ short_title }}

<details class="opctx-talk-closure" open>
<summary><strong>Closed - {{ decision_label }} by {{ agent_id }}.</strong> {{ verdict }}</summary>

{{ reasoning }}

</details>
```

## Output Summary

When finished, report edited sections, decision files created, and any proposals
left open.
