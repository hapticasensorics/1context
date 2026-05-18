---
page_id: "{{ page_id }}"
type: this-week
schema_version: 1
template_id: e08/this-week
template_version: template-0.1.0
title: "This Week"
slug: this-week
section: site
access: "{{ access_tier }}"
summary: "A generated digest of recent wiki changes, decisions, promoted topics, contradictions, and open questions."
status: draft
asset_base: "{{ asset_base }}"
home_href: "{{ home_href }}"
md_url: "/{{ slug }}.md"
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
tags: [this-week, generated, recent-changes]
last_updated: "{{ created_date }}"
window_start: "{{ window_start }}"
window_end: "{{ window_end }}"
---

# This Week

_Generated digest covering {{ window_start }} to {{ window_end }}._

## Lead

<!-- empty: biographer-or-renderer-populated. Quote or summarize the strongest accepted weekly throughline. -->

## Promoted Topics

<!-- empty: renderer-populated. List newly created or materially expanded topic pages. -->

## Project Changes

<!-- empty: renderer-populated. List project state changes, newly active work, completions, pauses, and archival decisions. -->

## Decisions

<!-- empty: renderer-populated. Count and link accepted, deferred, rejected, withdrawn, and superseded proposals. -->

## Contradictions And Concerns

<!-- empty: renderer-populated. List active contradiction and concern entries that still need review. -->

## Open Questions

<!-- empty: renderer-populated. Summarize the current open-question count and link to the full worklist. -->

## See Also

- [For You](./for-you)
- [Open Questions](./open-questions)
- [Projects](./projects)
- [Topics](./topics)
