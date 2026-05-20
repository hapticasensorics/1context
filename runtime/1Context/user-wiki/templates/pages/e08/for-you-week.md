---
page_id: "{{ page_id }}"
type: for-you-week
schema_version: 1
template_id: e08/for-you-week
template_version: template-0.1.0
title: "For You - {{ operator_name }} - Week of {{ era_anchor_label }}"
slug: "for-you-{{ era_anchor }}"
route: "{{ route }}"
section: "for-you"
access: "{{ access_tier }}"
summary: "A rolling weekly orientation page for {{ operator_name }}."
status: draft
asset_base: "{{ asset_base }}"
home_href: "{{ home_href }}"
md_url: "{{ md_url }}"
toc_enabled: true
talk_enabled: true
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
tags: [for-you, biography, week-of]
last_updated: "{{ created_date }}"
era_kind: week-of-monday
era_anchor: "{{ era_anchor }}"
era_anchor_label: "{{ era_anchor_label }}"
window_start: "{{ window_start }}"
window_end: "{{ window_end }}"
window_kind: rolling-14d
talk_url: "{{ talk_route }}"
audiences:
  internal: true
  public: true
---

# For You - {{ operator_name }} - Week of {{ era_anchor_label }}

<!-- section: { slug: "biography", talk: false } -->
## Biography

<!-- empty: biographer-populated. Rewrite this section after the week has enough accepted day sections. -->

<!-- section: { slug: "{{ day_13_slug }}", talk: true, date: "{{ day_13_date }}" } -->
## {{ day_13_label }}

<!-- empty: editor-populated. Summarize the day only after the talk folder has enough hourly entries. -->

<!-- section: { slug: "{{ day_12_slug }}", talk: true, date: "{{ day_12_date }}" } -->
## {{ day_12_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_11_slug }}", talk: true, date: "{{ day_11_date }}" } -->
## {{ day_11_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_10_slug }}", talk: true, date: "{{ day_10_date }}" } -->
## {{ day_10_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_9_slug }}", talk: true, date: "{{ day_9_date }}" } -->
## {{ day_9_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_8_slug }}", talk: true, date: "{{ day_8_date }}" } -->
## {{ day_8_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_7_slug }}", talk: true, date: "{{ day_7_date }}" } -->
## {{ day_7_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_6_slug }}", talk: true, date: "{{ day_6_date }}" } -->
## {{ day_6_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_5_slug }}", talk: true, date: "{{ day_5_date }}" } -->
## {{ day_5_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_4_slug }}", talk: true, date: "{{ day_4_date }}" } -->
## {{ day_4_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_3_slug }}", talk: true, date: "{{ day_3_date }}" } -->
## {{ day_3_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_2_slug }}", talk: true, date: "{{ day_2_date }}" } -->
## {{ day_2_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_1_slug }}", talk: true, date: "{{ day_1_date }}" } -->
## {{ day_1_label }}

<!-- empty: editor-populated. -->

<!-- section: { slug: "{{ day_0_slug }}", talk: true, date: "{{ day_0_date }}" } -->
## {{ day_0_label }}

<!-- empty: editor-populated. -->

## Open Going Into Next Week

<!-- empty: curator-populated. Carry forward unresolved questions, blocked work, and claims that need evidence. -->

## See Also

- [Your Context](./your-context)
- [Projects](./projects)
- [Topics](./topics)
- [Open Questions](./open-questions)
